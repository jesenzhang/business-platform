//! Identity application layer: use cases over ports.
//!
//! Authorization is not decided here: this layer answers "who is the
//! authenticated caller on the platform, and are they an active member of
//! this tenant". What the caller may *do* is decided by the Policy context.

mod access;
mod membership;
mod queries;
mod resolve;
mod user_status;

pub use access::{TenantAccess, TenantAccessChecker, TenantAccessReason};
pub use membership::{
    ChangeTenantMembershipStatus, ChangeTenantMembershipStatusCommand,
    ChangeTenantMembershipStatusError, CreateMembershipTarget, CreateTenantMembership,
    CreateTenantMembershipCommand, CreateTenantMembershipError,
};
pub use queries::{
    GetTenantUser, IdentityPage, IdentityPageCursor, IdentityQueryError, ListMemberships,
    ListTenantUsers, IDENTITY_MAX_PAGE_SIZE,
};
pub use resolve::{
    ResolveAuthenticatedUser, ResolveAuthenticatedUserCommand, ResolveCallerError, ResolvedCaller,
};
pub use user_status::{ChangeUserStatus, ChangeUserStatusCommand, ChangeUserStatusError};

/// Validate a caller-supplied idempotency key (existing repo convention:
/// 1..=255 characters when present).
pub(crate) fn validate_idempotency_key(key: Option<&String>) -> Result<(), String> {
    if let Some(key) = key {
        let trimmed = key.trim();
        if trimmed.is_empty() || trimmed.len() > 255 {
            return Err("Idempotency-Key must contain 1 to 255 characters".to_string());
        }
    }
    Ok(())
}
