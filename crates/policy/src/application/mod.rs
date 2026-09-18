//! Policy application layer: use cases over ports.

mod authorize;
mod bindings;
mod error;
mod queries;
mod roles;

pub use authorize::{
    AuthorizationContext, Authorize, ExplainDecision, ManagementCompatGrant, ResourceTarget,
};

pub use bindings::{BindRole, BindRoleCommand, RevokeRoleBinding, RevokeRoleBindingCommand};
pub use error::PolicyApplicationError;
pub use queries::{GetRole, ListPermissions, ListRoleBindings, ListRoles};
pub use roles::{
    CreateRole, CreateRoleCommand, SetRolePermissions, SetRolePermissionsCommand, UpdateRole,
    UpdateRoleCommand,
};

/// Upper bound of permissions attachable to one role (bounded work per
/// decision: the evaluator iterates at most this many keys).
pub const MAX_ROLE_PERMISSIONS: usize = 200;

/// Upper bound of tenant roles per tenant (listing bound).
pub const MAX_ROLES_PER_TENANT: usize = 500;

/// Upper bound of bindings per user (write guard against binding spam).
pub const MAX_BINDINGS_PER_USER: usize = 100;

/// Upper bound of bindings per tenant for the management listing.
pub const MAX_BINDINGS_PER_TENANT: usize = 5_000;

/// Field cap for free-text mutation metadata.
pub const MAX_REASON_LEN: usize = 512;

/// Reject control characters (including NUL) and oversized free text.
pub(crate) fn validate_text_field(value: &str, max_len: usize, label: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) {
        return Err(format!("{label} must not contain control characters"));
    }
    if value.len() > max_len {
        return Err(format!("{label} must not exceed {max_len} characters"));
    }
    Ok(())
}

/// Validate a caller-supplied idempotency key (repo convention: 1..=255).
pub(crate) fn validate_idempotency_key(key: Option<&String>) -> Result<(), String> {
    if let Some(key) = key {
        let trimmed = key.trim();
        if trimmed.is_empty() || trimmed.len() > 255 {
            return Err("Idempotency-Key must contain 1 to 255 characters".to_string());
        }
    }
    Ok(())
}
