//! Policy domain: permission catalog entries, roles, bindings, scopes,
//! and the bounded decision vocabulary.

mod binding;
mod decision;
mod error;
mod permission;
mod role;
mod scope;
mod version;

pub use binding::{BindingStatus, RehydrateRoleBinding, RoleBinding, ValidityWindow};
pub use decision::{BindingEvaluation, DecisionReason, PolicyDecision};
pub use error::PolicyDomainError;
pub use permission::{PermissionDefinition, PermissionKey, MAX_PERMISSION_KEY_LEN};
pub use role::{
    validate_role_stable_key, RehydrateRoleDefinition, RoleDefinition, RoleStatus,
    MAX_ROLE_DISPLAY_NAME_LEN,
};
pub use scope::{validate_resource_kind, ResourceScope, MAX_RESOURCE_KIND_LEN};
pub use version::AggregateVersion;
