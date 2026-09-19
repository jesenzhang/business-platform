//! Identity, Organization, and Policy management DTOs (PLAN-0013 Stage 8).
//!
//! These shapes are the complete public surface of the minimal management
//! REST API. They mirror bounded domain views only: internal aggregates,
//! lease tokens, external claim payloads, and storage internals are never
//! representable here. Timestamps serialize as RFC 3339 strings exactly as
//! the existing `AuditEvent` contract.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One platform user as visible to tenant administrators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AdminUser {
    /// Stable platform user id.
    pub user_id: Uuid,
    /// Lifecycle status (`active` | `disabled`).
    pub status: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Aggregate version for optimistic concurrency.
    pub version: i64,
}

/// One tenant membership row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MembershipView {
    /// Membership id.
    pub membership_id: Uuid,
    /// Member user.
    pub user_id: Uuid,
    /// Membership status (`active` | `suspended`).
    pub status: String,
    /// When the user joined the tenant.
    pub joined_at: DateTime<Utc>,
    /// When the membership was suspended, if it is suspended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspended_at: Option<DateTime<Utc>>,
    /// Record origin (`bootstrap` | `admin` | `migration`).
    pub source: String,
    /// Aggregate version for optimistic concurrency.
    pub version: i64,
}

/// One permission catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PermissionView {
    /// Stable permission key.
    pub key: String,
    /// Human-readable description.
    pub description: String,
    /// Reserved keys have no consuming use case yet.
    pub reserved: bool,
    /// Retired (inactive) keys always evaluate as unknown.
    pub active: bool,
}

/// One role with its granted permission keys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RoleView {
    /// Role id.
    pub role_id: Uuid,
    /// Owning tenant; `null` for immutable system roles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<Uuid>,
    /// Stable key (unique per tenant, globally for system roles).
    pub stable_key: String,
    /// Display name.
    pub display_name: String,
    /// Role status (`active` | `disabled`).
    pub status: String,
    /// System roles are immutable and migration-seeded.
    pub system: bool,
    /// Granted permission keys.
    pub permission_keys: Vec<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Aggregate version for optimistic concurrency.
    pub version: i64,
}

/// The bounded scope model for role bindings (mirrors the policy
/// `ResourceScope` closed enum; there is deliberately no free-form shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ScopeView {
    /// The whole tenant.
    Tenant,
    /// One organization unit, optionally including its subtree.
    #[serde(rename_all = "snake_case")]
    OrgUnit {
        /// Bound unit id.
        org_unit_id: Uuid,
        /// Whether descendants are included. Defaults to false.
        #[serde(default)]
        include_subtree: bool,
    },
    /// All resources of one kind inside the tenant.
    #[serde(rename_all = "snake_case")]
    ResourceType {
        /// Resource kind (validated grammar).
        resource_kind: String,
    },
    /// One concrete resource.
    #[serde(rename_all = "snake_case")]
    Resource {
        /// Resource kind (validated grammar).
        resource_kind: String,
        /// The single resource id.
        resource_id: Uuid,
    },
}

/// One role binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RoleBindingView {
    /// Binding id.
    pub binding_id: Uuid,
    /// Bound user.
    pub user_id: Uuid,
    /// Bound role.
    pub role_id: Uuid,
    /// Granted scope.
    pub scope: ScopeView,
    /// Binding status (`active` | `revoked`).
    pub status: String,
    /// Validity window start.
    pub effective_at: DateTime<Utc>,
    /// Validity window end, exclusive when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Aggregate version for optimistic concurrency.
    pub version: i64,
}

/// One organization unit (flat; hierarchy is `parent_id`-derived).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OrganizationUnitView {
    /// Unit id.
    pub unit_id: Uuid,
    /// Parent unit, `null` at the tenant root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    /// Unit kind (`company` | `department` | `team`).
    pub unit_type: String,
    /// Display name.
    pub name: String,
    /// Unit status (`active` | `disabled`).
    pub status: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Aggregate version for optimistic concurrency.
    pub version: i64,
}

/// One organization unit membership row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OrganizationMemberView {
    /// Membership id.
    pub membership_id: Uuid,
    /// Member user.
    pub user_id: Uuid,
    /// Unit the membership attaches to.
    pub unit_id: Uuid,
    /// Membership kind (`member` | `leader`).
    pub membership_type: String,
    /// Membership status (`active` | `inactive`).
    pub status: String,
    /// When the user joined the unit.
    pub joined_at: DateTime<Utc>,
    /// When the membership was removed, if inactive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivated_at: Option<DateTime<Utc>>,
    /// Aggregate version for optimistic concurrency.
    pub version: i64,
}

/// One evaluated candidate binding inside an explanation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExplainEvaluation {
    /// Evaluated binding id.
    pub binding_id: Uuid,
    /// Role behind the binding.
    pub role_id: Uuid,
    /// Binding status.
    pub binding_active: bool,
    /// Whether the validity window covers the decision instant.
    pub within_validity: bool,
    /// Whether the bound role was visible, active, and grants the key.
    pub role_grants_permission: bool,
    /// Bounded outcome label for this candidate.
    pub outcome: String,
}

/// The full bounded explanation of one decision (admin-only surface).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExplainView {
    /// Allow or deny.
    pub allowed: bool,
    /// Bounded reason label.
    pub reason: String,
    /// Binding that produced an allow, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_binding: Option<Uuid>,
    /// Permission key that was decided on, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_permission: Option<String>,
    /// Scope that matched (allow) or that the resource violated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_scope: Option<ScopeView>,
    /// Stable policy reference for audit correlation.
    pub policy_reference: String,
    /// Per-binding evaluations (empty when no candidate binding existed).
    pub evaluations: Vec<ExplainEvaluation>,
}

/// Body for creating a tenant membership. Exactly one target form must be
/// supplied: an existing `user_id`, or an `issuer` + `subject` external
/// subject to provision and attach.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateTenantMembershipRequest {
    /// Existing platform user id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    /// Verified external issuer (paired with `subject`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// Verified external subject (paired with `issuer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Optional audit reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Body for a tenant membership status change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct MembershipStatusChangeRequest {
    /// Optimistic version observed by the caller.
    pub expected_version: i64,
}

/// Body for creating a tenant role. `permission_keys` may be empty; a
/// non-empty set is applied transactionally after creation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateRoleRequest {
    /// Stable key, unique within the tenant.
    pub stable_key: String,
    /// Display name.
    pub display_name: String,
    /// Initial permission keys (catalog members).
    #[serde(default)]
    pub permission_keys: Vec<String>,
}

/// Body for updating a tenant role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UpdateRoleRequest {
    /// New display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// New status (`active` | `disabled`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optimistic version.
    pub expected_version: i64,
}

/// Body for the transactional set replace of a role's permissions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct SetRolePermissionsRequest {
    /// Complete replacement permission key set.
    pub permission_keys: Vec<String>,
    /// Optimistic version.
    pub expected_version: i64,
}

/// Body for creating an organization unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateUnitRequest {
    /// Parent unit, `null` for a tenant root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    /// Unit kind (`company` | `department` | `team`).
    pub unit_type: String,
    /// Display name.
    pub name: String,
}

/// Body for updating an organization unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UpdateUnitRequest {
    /// New display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New status (`active` | `disabled`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optimistic version.
    pub expected_version: i64,
}

/// Body for reparenting an organization unit. `new_parent_id: null` moves
/// the unit to the tenant root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct MoveUnitRequest {
    /// New parent unit (`null` = tenant root).
    #[serde(default)]
    pub new_parent_id: Option<Uuid>,
    /// Optimistic version.
    pub expected_version: i64,
}

/// Body for adding a member to an organization unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct AddUnitMemberRequest {
    /// Membership kind (`member` | `leader`).
    pub membership_type: String,
}

/// Body for creating a role binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct BindingCreateRequest {
    /// User to bind.
    pub user_id: Uuid,
    /// Role to bind (tenant-owned or system).
    pub role_id: Uuid,
    /// Granted scope.
    pub scope: ScopeView,
    /// Validity window start; defaults to the decision instant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_at: Option<DateTime<Utc>>,
    /// Validity window end, exclusive when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Body for revoking a role binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct BindingRevokeRequest {
    /// Optimistic version.
    pub expected_version: i64,
}

/// Optional concrete resource for an explanation request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ExplainResource {
    /// Resource kind, if the decision is about a concrete resource kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Concrete resource id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<Uuid>,
    /// Owning organization unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_unit_id: Option<Uuid>,
}

/// Body for explaining one authorization decision for a target user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ExplainRequest {
    /// Target user whose decision is explained.
    pub user_id: Uuid,
    /// Permission key to evaluate.
    pub permission: String,
    /// Optional concrete resource context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<ExplainResource>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_uses_a_bounded_tagged_shape() {
        let encoded = serde_json::to_string(&ScopeView::Tenant).unwrap_or_default();
        assert_eq!(encoded, r#"{"scope":"tenant"}"#);
        let unit = ScopeView::OrgUnit {
            org_unit_id: Uuid::now_v7(),
            include_subtree: true,
        };
        let encoded = serde_json::to_string(&unit).unwrap_or_default();
        assert!(encoded.contains("\"scope\":\"org_unit\""));
        assert!(encoded.contains("\"include_subtree\":true"));
        let parsed: ScopeView =
            serde_json::from_str(r#"{"scope":"org_unit","org_unit_id":"018f0000-0000-7000-8000-000000000001","include_subtree":false}"#)
                .unwrap_or_else(|_| unreachable!("fixture scope must parse"));
        assert_eq!(parsed, unit_org_unit_false());
        // Unknown scope tags fail closed.
        assert!(serde_json::from_str::<ScopeView>(r#"{"scope":"everything"}"#).is_err());
    }

    fn unit_org_unit_false() -> ScopeView {
        ScopeView::OrgUnit {
            org_unit_id: Uuid::parse_str("018f0000-0000-7000-8000-000000000001")
                .unwrap_or_else(|_| unreachable!("fixture uuid must parse")),
            include_subtree: false,
        }
    }

    #[test]
    fn iam_dto_json_never_names_internal_fields() {
        let request = CreateTenantMembershipRequest {
            user_id: Some(Uuid::now_v7()),
            issuer: None,
            subject: None,
            reason: None,
        };
        let encoded = serde_json::to_string(&request).unwrap_or_default();
        for forbidden in [
            "object_key",
            "storage_key",
            "bucket",
            "internal_path",
            "password",
            "secret_key",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }
}
