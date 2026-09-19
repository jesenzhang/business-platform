//! Domain errors for the Platform Identity bounded context.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur inside the identity domain.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityDomainError {
    /// A caller-supplied or persisted identity was nil.
    #[error("identity identifiers must not be nil")]
    InvalidIdentity,

    /// The external issuer was blank or exceeded the field cap.
    #[error("external issuer is invalid")]
    InvalidIssuer,

    /// The external subject was blank or exceeded the field cap.
    #[error("external subject is invalid")]
    InvalidSubject,

    /// The external identity and user id were inconsistent (nil etc.).
    #[error("external identity must reference a non-nil user id")]
    InvalidExternalIdentityLink,

    /// A membership aggregate carried an invalid identity combination.
    #[error("membership tenant and user identifiers must not be nil")]
    InvalidMembershipIdentity,

    /// The membership source label was blank.
    #[error("membership source is required")]
    InvalidMembershipSource,

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

    /// A lifecycle transition that the status machine does not allow.
    #[error("invalid identity lifecycle transition: {operation} from {status}")]
    InvalidTransition {
        /// The requested operation.
        operation: &'static str,
        /// The current status rendered as a stable label.
        status: &'static str,
    },

    /// The requested aggregate was not found.
    #[error("identity not found: {0}")]
    NotFound(Uuid),

    /// A persisted timestamp violated ordering constraints.
    #[error("persisted identity timestamps are inconsistent")]
    InvalidTimestamps,
}
