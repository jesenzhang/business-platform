//! Shared application errors for organization use cases.

use thiserror::Error;

use crate::ports::OrganizationStoreError;

/// Errors of organization management use cases. Delivery maps these onto the
/// existing stable error codes.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OrganizationApplicationError {
    /// The request was malformed.
    #[error("validation failed: {0}")]
    Validation(String),
    /// The target aggregate was not found inside the tenant.
    #[error("organization aggregate not found")]
    NotFound,
    /// The aggregate key already exists.
    #[error("organization aggregate already exists")]
    AlreadyExists,
    /// Optimistic version conflict.
    #[error("version conflict")]
    VersionConflict,
    /// Parent placement invalid (unknown, cross-tenant, inactive, or too
    /// deep).
    #[error("invalid parent placement")]
    InvalidParent,
    /// Placement would create a cycle.
    #[error("organization tree cycle rejected")]
    Cycle,
    /// The organization unit is disabled.
    #[error("organization unit is disabled")]
    UnitDisabled,
    /// The target user is not an active member of the tenant.
    #[error("user is not an active tenant member")]
    NotTenantMember,
    /// Tenant resource cap exceeded.
    #[error("tenant organization resource cap exceeded")]
    TooManyResources,
    /// Idempotency key reuse with a different payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// Persistence is unavailable.
    #[error("organization persistence is unavailable")]
    Unavailable,
    /// Persistence failed.
    #[error("organization operation failed")]
    Failed,
}

impl From<OrganizationStoreError> for OrganizationApplicationError {
    fn from(error: OrganizationStoreError) -> Self {
        match error {
            OrganizationStoreError::NotFound => Self::NotFound,
            OrganizationStoreError::AlreadyExists => Self::AlreadyExists,
            OrganizationStoreError::VersionConflict => Self::VersionConflict,
            OrganizationStoreError::InvalidParent => Self::InvalidParent,
            OrganizationStoreError::Cycle => Self::Cycle,
            OrganizationStoreError::UnitDisabled => Self::UnitDisabled,
            OrganizationStoreError::TooManyResources => Self::TooManyResources,
            OrganizationStoreError::IdempotencyConflict => Self::IdempotencyConflict,
            OrganizationStoreError::Unavailable => Self::Unavailable,
            OrganizationStoreError::Failed => Self::Failed,
        }
    }
}
