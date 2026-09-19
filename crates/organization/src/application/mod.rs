//! Organization application layer: use cases over ports.

mod error;
mod members;
mod queries;
mod units;

pub use error::OrganizationApplicationError;
pub use members::{
    AddOrganizationMember, AddOrganizationMemberCommand, RemoveOrganizationMember,
    RemoveOrganizationMemberCommand,
};
pub use queries::{GetUnit, ListOrganizationTree, ListUnitMembers, ListUserMemberships};
pub use units::{
    CreateOrganizationUnit, CreateOrganizationUnitCommand, MoveOrganizationUnit,
    MoveOrganizationUnitCommand, UpdateOrganizationUnit, UpdateOrganizationUnitCommand,
};

/// Tenant resource cap for organization units (listing and placement share
/// it; keeps the tree listing and cycle checks bounded).
pub const MAX_UNITS_PER_TENANT: usize = 2_000;

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
