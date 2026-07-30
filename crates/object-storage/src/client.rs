use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

use crate::error::StorageError;

/// 对象存储客户端 trait
#[async_trait]
pub trait ObjectStorageClient: Send + Sync {
    /// 上传对象
    async fn put_object(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), StorageError>;
    /// 获取对象
    async fn get_object(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    /// 删除对象
    async fn delete_object(&self, key: &str) -> Result<(), StorageError>;
    /// 生成预签名 URL
    async fn presigned_url(&self, key: &str, expires_secs: u64) -> Result<String, StorageError>;
    /// 检查对象是否存在
    async fn object_exists(&self, key: &str) -> Result<bool, StorageError>;
}

/// S3 兼容存储客户端配置
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub region: String,
}

/// S3 兼容存储客户端（MinIO / AWS S3）
#[derive(Debug)]
pub struct S3Client {
    config: S3Config,
    http: reqwest::Client,
}

impl S3Client {
    /// 创建 S3 客户端
    pub fn new(config: S3Config) -> Result<Self, StorageError> {
        if config.endpoint.is_empty() {
            return Err(StorageError::Config("endpoint cannot be empty".to_string()));
        }
        if config.bucket.is_empty() {
            return Err(StorageError::Config("bucket cannot be empty".to_string()));
        }

        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| StorageError::Connection(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self { config, http })
    }

    /// 构建对象 URL
    fn object_url(&self, key: &str) -> String {
        format!("{}/{}/{}", self.config.endpoint, self.config.bucket, key)
    }
}

#[async_trait]
impl ObjectStorageClient for S3Client {
    #[instrument(skip(self, data), fields(bucket = %self.config.bucket))]
    async fn put_object(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), StorageError> {
        let url = self.object_url(key);
        debug!(key, size = data.len(), "Uploading object to S3");

        // TODO: 实现 AWS Signature V4 签名
        let response = self
            .http
            .put(&url)
            .header("Content-Type", content_type)
            .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
            .body(data)
            .send()
            .await
            .map_err(|e| StorageError::Connection(format!("PUT {url} failed: {e}")))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(StorageError::Connection(format!(
                "PUT {url} returned {status}: {body}"
            )))
        }
    }

    #[instrument(skip(self), fields(bucket = %self.config.bucket))]
    async fn get_object(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.object_url(key);
        debug!(key, "Downloading object from S3");

        // TODO: 实现 AWS Signature V4 签名
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| StorageError::Connection(format!("GET {url} failed: {e}")))?;

        match response.status().as_u16() {
            200 => {
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|e| StorageError::Io(format!("Failed to read response body: {e}")))?;
                Ok(bytes.to_vec())
            }
            404 => Err(StorageError::NotFound(key.to_string())),
            status => Err(StorageError::Connection(format!(
                "GET {url} returned {status}"
            ))),
        }
    }

    #[instrument(skip(self), fields(bucket = %self.config.bucket))]
    async fn delete_object(&self, key: &str) -> Result<(), StorageError> {
        let url = self.object_url(key);
        debug!(key, "Deleting object from S3");

        // TODO: 实现 AWS Signature V4 签名
        let response = self
            .http
            .delete(&url)
            .send()
            .await
            .map_err(|e| StorageError::Connection(format!("DELETE {url} failed: {e}")))?;

        if response.status().is_success() || response.status().as_u16() == 404 {
            // S3 DELETE 对不存在的对象也返回成功
            Ok(())
        } else {
            let status = response.status();
            Err(StorageError::Connection(format!(
                "DELETE {url} returned {status}"
            )))
        }
    }

    #[instrument(skip(self), fields(bucket = %self.config.bucket))]
    async fn presigned_url(&self, key: &str, expires_secs: u64) -> Result<String, StorageError> {
        // TODO: 实现 AWS Signature V4 预签名 URL 生成
        // 当前返回基础 URL + 过期参数占位
        let url = self.object_url(key);
        debug!(key, expires_secs, "Generating presigned URL");

        Ok(format!(
            "{url}?X-Amz-Expires={expires_secs}&X-Amz-Signature=TODO"
        ))
    }

    #[instrument(skip(self), fields(bucket = %self.config.bucket))]
    async fn object_exists(&self, key: &str) -> Result<bool, StorageError> {
        let url = self.object_url(key);
        debug!(key, "Checking object existence");

        // TODO: 实现 AWS Signature V4 签名
        let response = self
            .http
            .head(&url)
            .send()
            .await
            .map_err(|e| StorageError::Connection(format!("HEAD {url} failed: {e}")))?;

        match response.status().as_u16() {
            200 => Ok(true),
            404 => Ok(false),
            status => Err(StorageError::Connection(format!(
                "HEAD {url} returned {status}"
            ))),
        }
    }
}

/// 本地文件系统存储客户端（开发环境使用）
#[derive(Debug)]
pub struct LocalStorageClient {
    base_dir: PathBuf,
}

impl LocalStorageClient {
    /// 创建本地存储客户端
    ///
    /// `base_dir` 为文件存储根目录，不存在时自动创建
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)
            .map_err(|e| StorageError::Io(format!("Failed to create storage dir: {e}")))?;
        Ok(Self { base_dir })
    }

    /// 获取对象的本地文件路径
    fn object_path(&self, key: &str) -> PathBuf {
        self.base_dir.join(key)
    }
}

#[async_trait]
impl ObjectStorageClient for LocalStorageClient {
    #[instrument(skip(self, data))]
    async fn put_object(
        &self,
        key: &str,
        data: Vec<u8>,
        _content_type: &str,
    ) -> Result<(), StorageError> {
        let path = self.object_path(key);
        debug!(key, path = %path.display(), size = data.len(), "Writing object to local storage");

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StorageError::Io(format!("Failed to create parent dir: {e}")))?;
        }

        std::fs::write(&path, &data).map_err(|e| {
            StorageError::Io(format!("Failed to write file {}: {e}", path.display()))
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_object(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.object_path(key);
        debug!(key, path = %path.display(), "Reading object from local storage");

        if !path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }

        std::fs::read(&path)
            .map_err(|e| StorageError::Io(format!("Failed to read file {}: {e}", path.display())))
    }

    #[instrument(skip(self))]
    async fn delete_object(&self, key: &str) -> Result<(), StorageError> {
        let path = self.object_path(key);
        debug!(key, path = %path.display(), "Deleting object from local storage");

        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                StorageError::Io(format!("Failed to delete file {}: {e}", path.display()))
            })?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn presigned_url(&self, key: &str, _expires_secs: u64) -> Result<String, StorageError> {
        // 本地存储返回 file:// URL
        let path = self.object_path(key);
        Ok(format!("file://{}", path.display()))
    }

    #[instrument(skip(self))]
    async fn object_exists(&self, key: &str) -> Result<bool, StorageError> {
        let path = self.object_path(key);
        Ok(path.exists())
    }
}
