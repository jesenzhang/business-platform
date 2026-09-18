//! Identity application layer: use cases over ports.
//!
//! Authorization is not decided here: this layer answers "who is the
//! authenticated caller on the platform, and are they an active member of
//! this tenant". What the caller may *do* is decided by the Policy context.

mod access;
mod bootstrap;
mod membership;
mod queries;
mod resolve;
mod user_status;

pub use access::{TenantAccess, TenantAccessChecker, TenantAccessError, TenantAccessReason};
pub use bootstrap::{
    bootstrap_service_actor, BootstrapAdministrator, BootstrapAdministratorConfig,
    BootstrapBindingError, BootstrapBindingPort, BootstrapError, BOOTSTRAP_ROLE_STABLE_KEY,
};
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
    EXTERNAL_IDENTITY_NAMESPACE,
};
pub use user_status::{ChangeUserStatus, ChangeUserStatusCommand, ChangeUserStatusError};

/// Field caps for free-text mutation metadata persisted into audit details.
pub const MAX_REASON_LEN: usize = 512;
/// Field cap for carried trace ids.
pub const MAX_TRACE_ID_LEN: usize = 256;

/// Reject control characters (including NUL) and oversized free text before
/// it reaches audit details or adapter columns.
pub(crate) fn validate_text_field(value: &str, max_len: usize, label: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) {
        return Err(format!("{label} must not contain control characters"));
    }
    if value.len() > max_len {
        return Err(format!("{label} must not exceed {max_len} characters"));
    }
    Ok(())
}

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
