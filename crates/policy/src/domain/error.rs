//! Domain errors for the Policy bounded context.

use thiserror::Error;

/// Errors that can occur inside the policy domain.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PolicyDomainError {
    /// The permission stable key violated the strict grammar.
    #[error("invalid permission stable key: {0:?}")]
    InvalidPermissionKey(String),

    /// The role stable key violated the grammar.
    #[error("invalid role stable key: {0:?}")]
    InvalidStableKey(String),

    /// The resource kind violated the grammar.
    #[error("invalid resource kind: {0:?}")]
    InvalidResourceKind(String),

    /// Description or display name blank / oversized.
    #[error("text field blank or oversized")]
    InvalidText,

    /// A caller-supplied or persisted identity was nil.
    #[error("policy identifiers must not be nil")]
    InvalidIdentity,

    /// The aggregate version was not positive.
    #[error("aggregate version must be positive")]
    InvalidVersion,

    /// A status transition the machine does not allow.
    #[error("invalid policy transition: {operation} from {status}")]
    InvalidTransition {
        /// Requested operation.
        operation: &'static str,
        /// Current status label.
        status: &'static str,
    },

    /// System roles are immutable: created only by the migration seed.
    #[error("system roles are immutable")]
    SystemRoleImmutable,

    /// Binding window invalid (`expires_at <= effective_at`).
    #[error("binding expires_at must be after effective_at")]
    InvalidValidityWindow,

    /// The permission set for one role exceeds the bounded cap.
    #[error("role permission set exceeds the bound of {max}")]
    TooManyPermissions {
        /// Bound that was exceeded.
        max: usize,
    },
}
