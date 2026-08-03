//! Domain errors for the document bounded context.

use thiserror::Error;
use uuid::Uuid;

use super::entity::DocumentStatus;

/// Errors that can occur within the document domain.
#[derive(Debug, Error)]
pub enum DocumentDomainError {
    /// File size must be absent, zero, or positive.
    #[error("document size must not be negative")]
    InvalidSize,

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

    /// A state transition was requested that the lifecycle does not allow.
    #[error("invalid document status transition: {operation} from {status:?}")]
    InvalidStatusTransition {
        /// The requested lifecycle operation.
        operation: &'static str,
        /// The current status.
        status: DocumentStatus,
    },

    /// A persisted aggregate contains an invalid identity.
    #[error("document tenant and creator identifiers must not be nil")]
    InvalidIdentity,

    /// A persisted aggregate contains an invalid version.
    #[error("document version must be positive")]
    InvalidVersion,

    /// A persisted aggregate contains an invalid content revision.
    #[error("document content revision must be positive")]
    InvalidContentRevision,

    /// The storage key revision differs from the persisted content revision.
    #[error("document content reference revision does not match persisted revision")]
    ContentRevisionMismatch,

    /// A persisted aggregate contains timestamps in the wrong order.
    #[error("document updated_at must not be before created_at")]
    InvalidTimestamps,

    /// A persisted status value is not part of the lifecycle.
    #[error("unknown document status")]
    InvalidStatus,
}
