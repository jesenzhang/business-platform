//! 对象存储抽象层
//!
//! 提供 MinIO/S3 兼容的对象存储客户端抽象，支持：
//! - S3 兼容存储（MinIO、AWS S3 等）
//! - 本地文件系统存储（开发环境）

pub mod client;
pub mod error;

pub use client::{LocalStorageClient, ObjectStorageClient, S3Client};
pub use error::StorageError;
