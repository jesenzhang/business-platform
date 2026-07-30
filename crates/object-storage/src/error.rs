use thiserror::Error;

/// 对象存储错误类型
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Object not found: {0}")]
    NotFound(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("S3 operation error: {0}")]
    S3(String),

    #[error("Presigning error: {0}")]
    Presign(String),
}
