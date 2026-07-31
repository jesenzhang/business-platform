//! Domain errors for the document bounded context.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur within the document domain.
#[derive(Debug, Error)]
pub enum DocumentDomainError {
    /// The original filename is empty or blank.
    #[error("filename must not be empty")]
    EmptyFilename,

    /// The content type (MIME) is empty or blank.
    #[error("content type must not be empty")]
    EmptyContentType,

    /// The object storage key is empty or blank.
    #[error("object key must not be empty")]
    EmptyObjectKey,

    /// The object storage key failed validation (path traversal, etc.).
    #[error("invalid object key: {0}")]
    InvalidObjectKey(String),

    /// The requested document was not found for the given tenant.
    #[error("document not found: {0}")]
    NotFound(Uuid),

    /// Optimistic locking version conflict.
    #[error("version conflict: expected {expected}, got {actual}")]
    VersionConflict {
        /// The version the caller expected.
        expected: i64,
        /// The version actually stored.
        actual: i64,
    },
}
