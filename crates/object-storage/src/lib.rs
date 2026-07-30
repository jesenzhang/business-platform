//! 对象存储抽象层
//!
//! 提供 MinIO/S3 兼容的对象存储客户端抽象，支持：
//! - S3 兼容存储（MinIO、AWS S3 等），基于官方 `aws-sdk-s3`
//! - 本地文件系统存储（开发环境）
//!
//! 所有对象 key 均通过 [`ObjectKey`] 值对象校验，防止路径穿越攻击。

pub mod client;
pub mod error;
pub mod key;

pub use client::{LocalStorageClient, ObjectStorageClient, S3Client};
pub use error::StorageError;
pub use key::{ObjectKey, ObjectKeyError};
