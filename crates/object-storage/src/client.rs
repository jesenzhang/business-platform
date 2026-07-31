use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::presigning::PresigningConfig;
use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};
use http_body::Frame;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::{debug, instrument};

use crate::error::StorageError;
use crate::key::ObjectKey;

const MAX_SMALL_OBJECT_BYTES: usize = 16 * 1024 * 1024;

pub type ObjectStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send + Sync + 'static>>;

#[derive(Debug, Clone, Default)]
pub struct ObjectMetadata {
    pub content_length: u64,
    pub content_type: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub etag: Option<String>,
}

pub struct StoredObject {
    pub body: ObjectStream,
    pub metadata: ObjectMetadata,
}

#[async_trait]
pub trait ObjectStorageClient: Send + Sync {
    async fn put_stream(
        &self,
        key: &ObjectKey,
        body: ObjectStream,
        content_length: u64,
        content_type: &str,
        metadata: &BTreeMap<String, String>,
    ) -> Result<(), StorageError>;

    async fn open_stream(&self, key: &ObjectKey) -> Result<StoredObject, StorageError>;

    async fn head(&self, key: &ObjectKey) -> Result<ObjectMetadata, StorageError>;

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError>;

    async fn exists(&self, key: &ObjectKey) -> Result<bool, StorageError>;

    async fn presign(&self, key: &ObjectKey, expires_secs: u64) -> Result<String, StorageError>;

    async fn put_object(
        &self,
        key: &ObjectKey,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), StorageError> {
        if data.len() > MAX_SMALL_OBJECT_BYTES {
            return Err(StorageError::TooLarge(MAX_SMALL_OBJECT_BYTES));
        }
        let length = data.len() as u64;
        self.put_stream(
            key,
            Box::pin(stream::once(async move { Ok(data) })),
            length,
            content_type,
            &BTreeMap::new(),
        )
        .await
    }

    async fn get_object(&self, key: &ObjectKey) -> Result<Bytes, StorageError> {
        let object = self.open_stream(key).await?;
        let mut body = object.body;
        let mut bytes = Vec::with_capacity(
            usize::try_from(object.metadata.content_length).unwrap_or(MAX_SMALL_OBJECT_BYTES),
        );
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > MAX_SMALL_OBJECT_BYTES {
                return Err(StorageError::TooLarge(MAX_SMALL_OBJECT_BYTES));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Bytes::from(bytes))
    }

    async fn delete_object(&self, key: &ObjectKey) -> Result<(), StorageError> {
        self.delete(key).await
    }

    async fn object_exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        self.exists(key).await
    }

    async fn presigned_url(
        &self,
        key: &ObjectKey,
        expires_secs: u64,
    ) -> Result<String, StorageError> {
        self.presign(key, expires_secs).await
    }
}

fn is_not_found<E: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static>(
    error: &SdkError<E>,
) -> bool {
    matches!(error, SdkError::ServiceError(context) if context.raw().status().as_u16() == 404)
}

#[derive(Debug)]
pub struct S3Client {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Client {
    #[must_use]
    pub fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: &str,
    ) -> Self {
        use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
        let credentials = Credentials::new(access_key, secret_key, None, None, "static");
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .region(Region::new(region.to_string()))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();
        Self {
            client: aws_sdk_s3::Client::from_conf(config),
            bucket: bucket.to_string(),
        }
    }
}

#[async_trait]
impl ObjectStorageClient for S3Client {
    #[instrument(skip(self, body), fields(bucket = %self.bucket))]
    async fn put_stream(
        &self,
        key: &ObjectKey,
        body: ObjectStream,
        content_length: u64,
        content_type: &str,
        metadata: &BTreeMap<String, String>,
    ) -> Result<(), StorageError> {
        debug!(key = %key, content_length, "streaming upload to S3");
        let body = body.map(|chunk| {
            chunk
                .map(Frame::data)
                .map_err(|error: StorageError| std::io::Error::other(error.to_string()))
        });
        let body = http_body_util::StreamBody::new(body);
        let sdk_body = aws_smithy_types::body::SdkBody::from_body_1_x(body);
        let byte_stream = aws_smithy_types::byte_stream::ByteStream::new(sdk_body);
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .body(byte_stream)
            .content_length(
                i64::try_from(content_length).map_err(|_| StorageError::TooLarge(usize::MAX))?,
            )
            .content_type(content_type);
        for (name, value) in metadata {
            request = request.metadata(name, value);
        }
        request
            .send()
            .await
            .map_err(|error| StorageError::S3(format!("put_object failed: {error}")))?;
        Ok(())
    }

    async fn open_stream(&self, key: &ObjectKey) -> Result<StoredObject, StorageError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|error| {
                if is_not_found(&error) {
                    StorageError::NotFound(key.to_string())
                } else {
                    StorageError::S3(format!("get_object failed: {error}"))
                }
            })?;
        let metadata = ObjectMetadata {
            content_length: response
                .content_length()
                .unwrap_or_default()
                .max(0)
                .cast_unsigned(),
            content_type: response.content_type().map(ToOwned::to_owned),
            metadata: response
                .metadata()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            etag: response.e_tag().map(ToOwned::to_owned),
        };
        let body = ReaderStream::new(response.body.into_async_read())
            .map(|chunk| chunk.map_err(|error| StorageError::Io(error.to_string())));
        Ok(StoredObject {
            body: Box::pin(body),
            metadata,
        })
    }

    async fn head(&self, key: &ObjectKey) -> Result<ObjectMetadata, StorageError> {
        let response = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|error| {
                if is_not_found(&error) {
                    StorageError::NotFound(key.to_string())
                } else {
                    StorageError::S3(format!("head_object failed: {error}"))
                }
            })?;
        Ok(ObjectMetadata {
            content_length: response
                .content_length()
                .unwrap_or_default()
                .max(0)
                .cast_unsigned(),
            content_type: response.content_type().map(ToOwned::to_owned),
            metadata: response
                .metadata()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            etag: response.e_tag().map(ToOwned::to_owned),
        })
    }

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(|error| StorageError::S3(format!("delete_object failed: {error}")))?;
        Ok(())
    }

    async fn exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        match self.head(key).await {
            Ok(_) => Ok(true),
            Err(StorageError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn presign(&self, key: &ObjectKey, expires_secs: u64) -> Result<String, StorageError> {
        let config = PresigningConfig::expires_in(Duration::from_secs(expires_secs))
            .map_err(|error| StorageError::Presign(format!("invalid presign config: {error}")))?;
        self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .presigned(config)
            .await
            .map(|presigned| presigned.uri().to_string())
            .map_err(|error| StorageError::Presign(format!("presign failed: {error}")))
    }
}

#[derive(Debug)]
pub struct LocalStorageClient {
    base_dir: PathBuf,
}

impl LocalStorageClient {
    /// `LocalStorage` is for trusted development data. A symlink can still be
    /// swapped between canonicalization and open on platforms without
    /// openat-style no-follow primitives; use `MinIO` for hostile input.
    pub async fn new(base_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&base_dir)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        let base_dir = tokio::fs::canonicalize(base_dir)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        Ok(Self { base_dir })
    }

    fn object_path(&self, key: &ObjectKey) -> PathBuf {
        self.base_dir.join(key.as_str())
    }

    fn verify_under_root(&self, path: &Path) -> Result<(), StorageError> {
        if path.starts_with(&self.base_dir) {
            Ok(())
        } else {
            Err(StorageError::Config(
                "resolved path escapes storage root".to_string(),
            ))
        }
    }
}

#[async_trait]
impl ObjectStorageClient for LocalStorageClient {
    async fn put_stream(
        &self,
        key: &ObjectKey,
        mut body: ObjectStream,
        _content_length: u64,
        _content_type: &str,
        _metadata: &BTreeMap<String, String>,
    ) -> Result<(), StorageError> {
        let path = self.object_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| StorageError::Io(error.to_string()))?;
            self.verify_under_root(
                &tokio::fs::canonicalize(parent)
                    .await
                    .map_err(|error| StorageError::Io(error.to_string()))?,
            )?;
        }
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        while let Some(chunk) = body.next().await {
            file.write_all(&chunk?)
                .await
                .map_err(|error| StorageError::Io(error.to_string()))?;
        }
        file.flush()
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        Ok(())
    }

    async fn open_stream(&self, key: &ObjectKey) -> Result<StoredObject, StorageError> {
        let path = self.object_path(key);
        let canonical = tokio::fs::canonicalize(&path)
            .await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;
        self.verify_under_root(&canonical)?;
        let metadata = tokio::fs::metadata(&canonical)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        let file = tokio::fs::File::open(canonical)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        let body = ReaderStream::new(file)
            .map(|chunk| chunk.map_err(|error| StorageError::Io(error.to_string())));
        Ok(StoredObject {
            body: Box::pin(body),
            metadata: ObjectMetadata {
                content_length: metadata.len(),
                content_type: None,
                metadata: BTreeMap::new(),
                etag: None,
            },
        })
    }

    async fn head(&self, key: &ObjectKey) -> Result<ObjectMetadata, StorageError> {
        let object = self.open_stream(key).await?;
        Ok(object.metadata)
    }

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        let path = self.object_path(key);
        if let Ok(canonical) = tokio::fs::canonicalize(&path).await {
            self.verify_under_root(&canonical)?;
            tokio::fs::remove_file(canonical)
                .await
                .map_err(|error| StorageError::Io(error.to_string()))?;
        }
        Ok(())
    }

    async fn exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        match self.head(key).await {
            Ok(_) => Ok(true),
            Err(StorageError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn presign(&self, key: &ObjectKey, _expires_secs: u64) -> Result<String, StorageError> {
        Ok(format!("file://{}", self.object_path(key).display()))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_storage_streams_and_deletes() {
        let directory = tempfile::tempdir().expect("temp directory");
        let client = LocalStorageClient::new(directory.path())
            .await
            .expect("client");
        let key = ObjectKey::new("nested/file.bin").expect("key");
        let chunks = stream::iter([
            Ok(Bytes::from_static(b"hello ")),
            Ok(Bytes::from_static(b"stream")),
        ]);
        client
            .put_stream(
                &key,
                Box::pin(chunks),
                12,
                "application/octet-stream",
                &BTreeMap::new(),
            )
            .await
            .expect("write");
        assert!(client.exists(&key).await.expect("exists"));
        assert_eq!(
            client.get_object(&key).await.expect("read"),
            Bytes::from_static(b"hello stream")
        );
        client.delete(&key).await.expect("delete");
        assert!(!client.exists(&key).await.expect("exists"));
    }

    #[tokio::test]
    async fn local_storage_rejects_symlink_escape_when_supported() {
        let directory = tempfile::tempdir().expect("temp directory");
        let outside = tempfile::tempdir().expect("outside directory");
        let client = LocalStorageClient::new(directory.path())
            .await
            .expect("client");
        let link = directory.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).expect("symlink");
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(outside.path(), &link).is_err() {
            return;
        }
        let key = ObjectKey::new("link/escape.txt").expect("key");
        let result = client
            .put_object(&key, Bytes::from_static(b"escape"), "text/plain")
            .await;
        assert!(matches!(result, Err(StorageError::Config(_))));
    }
}
