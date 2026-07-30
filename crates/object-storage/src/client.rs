//! Object storage client trait and implementations.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::presigning::PresigningConfig;
use bytes::Bytes;
use tracing::{debug, instrument};

use crate::error::StorageError;
use crate::key::ObjectKey;

/// Object storage operations.
///
/// All keys are validated [`ObjectKey`] values, preventing path traversal
/// and other injection attacks at the type level.
#[async_trait]
pub trait ObjectStorageClient: Send + Sync {
    /// Upload an object with content type.
    async fn put_object(
        &self,
        key: &ObjectKey,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), StorageError>;

    /// Download an object.
    async fn get_object(&self, key: &ObjectKey) -> Result<Bytes, StorageError>;

    /// Delete an object. Idempotent: deleting a non-existent object succeeds.
    async fn delete_object(&self, key: &ObjectKey) -> Result<(), StorageError>;

    /// Check if an object exists.
    async fn object_exists(&self, key: &ObjectKey) -> Result<bool, StorageError>;

    /// Generate a presigned URL for temporary access.
    async fn presigned_url(
        &self,
        key: &ObjectKey,
        expires_secs: u64,
    ) -> Result<String, StorageError>;
}

/// Check whether an AWS SDK error represents a 404 Not Found response.
fn is_not_found<E: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static>(
    err: &SdkError<E>,
) -> bool {
    match err {
        SdkError::ServiceError(ctx) => ctx.raw().status().as_u16() == 404,
        _ => false,
    }
}

/// S3-compatible storage client backed by the official AWS SDK.
///
/// Uses path-style addressing for `MinIO` compatibility.
#[derive(Debug)]
pub struct S3Client {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Client {
    /// Create a new S3 client from explicit credentials and endpoint.
    ///
    /// Uses path-style addressing (`force_path_style`) so that the client
    /// works with `MinIO` and other S3-compatible stores out of the box.
    #[must_use]
    pub fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: &str,
    ) -> Self {
        use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};

        let creds = Credentials::new(access_key, secret_key, None, None, "static");

        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .region(Region::new(region.to_string()))
            .credentials_provider(creds)
            .force_path_style(true)
            .build();

        let client = aws_sdk_s3::Client::from_conf(config);

        Self {
            client,
            bucket: bucket.to_string(),
        }
    }
}

#[async_trait]
impl ObjectStorageClient for S3Client {
    #[instrument(skip(self, data), fields(bucket = %self.bucket))]
    async fn put_object(
        &self,
        key: &ObjectKey,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), StorageError> {
        debug!(key = %key, size = data.len(), "Uploading object to S3");

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .body(data.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| StorageError::S3(format!("put_object failed: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self), fields(bucket = %self.bucket))]
    async fn get_object(&self, key: &ObjectKey) -> Result<Bytes, StorageError> {
        debug!(key = %key, "Downloading object from S3");

        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| {
                if is_not_found(&e) {
                    StorageError::NotFound(key.to_string())
                } else {
                    StorageError::S3(format!("get_object failed: {e}"))
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| StorageError::Io(format!("failed to read response body: {e}")))?
            .into_bytes();

        Ok(bytes)
    }

    #[instrument(skip(self), fields(bucket = %self.bucket))]
    async fn delete_object(&self, key: &ObjectKey) -> Result<(), StorageError> {
        debug!(key = %key, "Deleting object from S3");

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|e| StorageError::S3(format!("delete_object failed: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self), fields(bucket = %self.bucket))]
    async fn object_exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        debug!(key = %key, "Checking object existence");

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) if is_not_found(&e) => Ok(false),
            Err(e) => Err(StorageError::S3(format!("head_object failed: {e}"))),
        }
    }

    #[instrument(skip(self), fields(bucket = %self.bucket))]
    async fn presigned_url(
        &self,
        key: &ObjectKey,
        expires_secs: u64,
    ) -> Result<String, StorageError> {
        debug!(key = %key, expires_secs, "Generating presigned URL");

        let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expires_secs))
            .map_err(|e| StorageError::Presign(format!("invalid presign config: {e}")))?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .presigned(presigning_config)
            .await
            .map_err(|e| StorageError::Presign(format!("presign failed: {e}")))?;

        Ok(presigned.uri().to_string())
    }
}

/// Local filesystem storage client (development environment).
///
/// All paths are validated against the storage root to prevent directory
/// traversal, providing defense-in-depth on top of [`ObjectKey`] validation.
#[derive(Debug)]
pub struct LocalStorageClient {
    base_dir: PathBuf,
}

impl LocalStorageClient {
    /// Create a local storage client rooted at `base_dir`.
    ///
    /// The directory is created if it does not exist and canonicalized
    /// so that all subsequent path checks are symlink-safe.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the directory cannot be created or canonicalized.
    pub async fn new(base_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&base_dir)
            .await
            .map_err(|e| StorageError::Io(format!("Failed to create storage dir: {e}")))?;
        let base_dir = tokio::fs::canonicalize(&base_dir)
            .await
            .map_err(|e| StorageError::Io(format!("Failed to canonicalize storage dir: {e}")))?;
        Ok(Self { base_dir })
    }

    /// Join the storage root with the object key to form the target path.
    fn object_path(&self, key: &ObjectKey) -> PathBuf {
        self.base_dir.join(key.as_str())
    }

    /// Verify that a canonicalized path is still under the storage root.
    fn verify_under_root(&self, canonical: &Path) -> Result<(), StorageError> {
        if !canonical.starts_with(&self.base_dir) {
            return Err(StorageError::Config(
                "resolved path escapes storage root".to_string(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl ObjectStorageClient for LocalStorageClient {
    #[instrument(skip(self, data))]
    async fn put_object(
        &self,
        key: &ObjectKey,
        data: Bytes,
        _content_type: &str,
    ) -> Result<(), StorageError> {
        let path = self.object_path(key);
        debug!(key = %key, path = %path.display(), size = data.len(), "Writing object to local storage");

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Io(format!("Failed to create parent dir: {e}")))?;
            let canonical_parent = tokio::fs::canonicalize(parent)
                .await
                .map_err(|e| StorageError::Io(format!("Failed to canonicalize parent dir: {e}")))?;
            self.verify_under_root(&canonical_parent)?;
        }

        tokio::fs::write(&path, &data).await.map_err(|e| {
            StorageError::Io(format!("Failed to write file {}: {e}", path.display()))
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_object(&self, key: &ObjectKey) -> Result<Bytes, StorageError> {
        let path = self.object_path(key);
        debug!(key = %key, path = %path.display(), "Reading object from local storage");

        let canonical = tokio::fs::canonicalize(&path)
            .await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;
        self.verify_under_root(&canonical)?;

        tokio::fs::read(&canonical)
            .await
            .map(Bytes::from)
            .map_err(|e| {
                StorageError::Io(format!("Failed to read file {}: {e}", canonical.display()))
            })
    }

    #[instrument(skip(self))]
    async fn delete_object(&self, key: &ObjectKey) -> Result<(), StorageError> {
        let path = self.object_path(key);
        debug!(key = %key, path = %path.display(), "Deleting object from local storage");

        // File does not exist → delete is idempotent, so we only act on Ok.
        if let Ok(canonical) = tokio::fs::canonicalize(&path).await {
            self.verify_under_root(&canonical)?;
            tokio::fs::remove_file(&canonical).await.map_err(|e| {
                StorageError::Io(format!(
                    "Failed to delete file {}: {e}",
                    canonical.display()
                ))
            })?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn presigned_url(
        &self,
        key: &ObjectKey,
        _expires_secs: u64,
    ) -> Result<String, StorageError> {
        let path = self.object_path(key);
        Ok(format!("file://{}", path.display()))
    }

    #[instrument(skip(self))]
    async fn object_exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        let path = self.object_path(key);
        Ok(tokio::fs::try_exists(&path).await.unwrap_or(false))
    }
}
