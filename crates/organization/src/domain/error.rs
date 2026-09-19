//! Domain errors for the minimal Organization bounded context.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur inside the organization domain.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OrganizationDomainError {
    /// A caller-supplied or persisted identity was nil.
    #[error("organization identifiers must not be nil")]
    InvalidIdentity,

    /// The unit name was blank or exceeded the field cap.
    #[error("organization unit name must contain 1 to {max} characters")]
    InvalidName {
        /// Field cap.
        max: usize,
    },

    /// The aggregate version was not positive.
    #[error("aggregate version must be positive")]
    InvalidVersion,

    /// Optimistic locking version conflict.
    #[error("version conflict: expected {expected}, got {actual}")]
    VersionConflict {
        /// The version the caller expected.
        expected: i64,
        /// The version actually stored.
        actual: i64,
    },

    /// A status transition the machine does not allow.
    #[error("invalid organization transition: {operation} from {status}")]
    InvalidTransition {
        /// Requested operation.
        operation: &'static str,
        /// Current status label.
        status: &'static str,
    },

    /// The requested aggregate was not found.
    #[error("organization aggregate not found: {0}")]
    NotFound(Uuid),
}
