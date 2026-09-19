//! Shared application errors for policy use cases.

use thiserror::Error;

use crate::ports::PolicyStoreError;

/// Errors of policy management use cases. Delivery maps these onto the
/// existing stable error codes.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PolicyApplicationError {
    /// The request was malformed.
    #[error("validation failed: {0}")]
    Validation(String),
    /// The target aggregate was not found inside the tenant.
    #[error("policy aggregate not found")]
    NotFound,
    /// The role/binding key already exists.
    #[error("policy aggregate already exists")]
    AlreadyExists,
    /// Optimistic version conflict.
    #[error("version conflict")]
    VersionConflict,
    /// A mutation targeted a system role.
    #[error("system roles are immutable")]
    RoleImmutable,
    /// A referenced permission key is not in the catalog.
    #[error("permission key is not in the catalog")]
    UnknownPermission,
    /// A role exceeds the bounded permission count.
    #[error("role permission bound exceeded")]
    TooManyPermissions,
    /// The per-user binding cap is reached.
    #[error("user binding cap exceeded")]
    TooManyBindings,
    /// The tenant binding cap is reached.
    #[error("tenant policy resource cap exceeded")]
    TooManyResources,
    /// Idempotency key reuse with a different payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// The target user is not an active member of the tenant.
    #[error("user is not an active tenant member")]
    NotTenantMember,
    /// A self-escalation guard rejected the mutation (binding an
    /// IAM-management role to oneself, or granting such permissions to a
    /// role one currently holds).
    #[error("self-escalation rejected: an actor cannot grant IAM-management authority to itself")]
    SelfEscalationDenied,
    /// An organization-unit scope referenced a unit that is not active (or
    /// not present) in the tenant.
    #[error("organization unit scope is unavailable in this tenant")]
    ScopeUnitUnavailable,
    /// Persistence is unavailable.
    #[error("policy persistence is unavailable")]
    Unavailable,
    /// Persistence failed.
    #[error("policy operation failed")]
    Failed,
}

impl From<PolicyStoreError> for PolicyApplicationError {
    fn from(error: PolicyStoreError) -> Self {
        match error {
            PolicyStoreError::NotFound => Self::NotFound,
            PolicyStoreError::AlreadyExists => Self::AlreadyExists,
            PolicyStoreError::VersionConflict => Self::VersionConflict,
            PolicyStoreError::RoleImmutable => Self::RoleImmutable,
            PolicyStoreError::UnknownPermission => Self::UnknownPermission,
            PolicyStoreError::TooManyPermissions => Self::TooManyPermissions,
            PolicyStoreError::TooManyResources => Self::TooManyResources,
            PolicyStoreError::IdempotencyConflict => Self::IdempotencyConflict,
            PolicyStoreError::Unavailable => Self::Unavailable,
            PolicyStoreError::Failed => Self::Failed,
        }
    }
}
