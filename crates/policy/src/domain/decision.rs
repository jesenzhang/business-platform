//! `PolicyDecision` and the bounded [`DecisionReason`] vocabulary.
//!
//! Decisions are safe to serialize to logs, metrics, and (the allow/deny
//! part) API responses: the reason enum is closed, and the matched binding /
//! permission / scope detail is for admin `ExplainDecision` only — it never
//! leaks internal state beyond stable identifiers.

use uuid::Uuid;

use super::permission::PermissionKey;
use super::scope::ResourceScope;

/// Why a decision came out the way it did. Bounded enum — never free-form
/// strings, so metrics and audit can enumerate every cause. Default deny:
/// anything that is not an explicit `Allow*` denies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecisionReason {
    /// An active, effective role binding grants the permission in scope.
    AllowRoleBinding,
    /// The bounded management-permission compatibility bridge grants a
    /// governance key (server-trusted claim, config-gated; see PLAN-0013
    /// preflight §5). Explainable separately from real bindings.
    AllowCompatManagementClaim,
    /// The resolved platform user does not exist (fail closed).
    DenyNoUser,
    /// The platform user is disabled.
    DenyUserDisabled,
    /// No tenant membership at all.
    DenyNoMembership,
    /// Tenant membership is suspended.
    DenyMembershipSuspended,
    /// No active, effective binding grants this permission in scope.
    DenyNoBinding,
    /// A candidate binding's validity window has passed.
    DenyBindingExpired,
    /// A candidate binding is not effective yet.
    DenyBindingNotYetEffective,
    /// A candidate binding was revoked.
    DenyBindingRevoked,
    /// The bound role is disabled.
    DenyRoleDisabled,
    /// The permission key is not in the catalog (unknown or retired).
    DenyUnknownPermission,
    /// The binding references a role that is not visible to this tenant.
    DenyCrossTenant,
    /// A candidate binding grants the permission but not for this resource.
    DenyScopeMismatch,
    /// The policy or bridged store failed; evaluation could not complete.
    /// **Never** an allow — an unavailable policy store denies.
    DenyInternal,
}

impl DecisionReason {
    #[must_use]
    pub const fn allowed(self) -> bool {
        matches!(
            self,
            Self::AllowRoleBinding | Self::AllowCompatManagementClaim
        )
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllowRoleBinding => "allow_role_binding",
            Self::AllowCompatManagementClaim => "allow_compat_management_claim",
            Self::DenyNoUser => "deny_no_user",
            Self::DenyUserDisabled => "deny_user_disabled",
            Self::DenyNoMembership => "deny_no_membership",
            Self::DenyMembershipSuspended => "deny_membership_suspended",
            Self::DenyNoBinding => "deny_no_binding",
            Self::DenyBindingExpired => "deny_binding_expired",
            Self::DenyBindingNotYetEffective => "deny_binding_not_yet_effective",
            Self::DenyBindingRevoked => "deny_binding_revoked",
            Self::DenyRoleDisabled => "deny_role_disabled",
            Self::DenyUnknownPermission => "deny_unknown_permission",
            Self::DenyCrossTenant => "deny_cross_tenant",
            Self::DenyScopeMismatch => "deny_scope_mismatch",
            Self::DenyInternal => "deny_internal",
        }
    }
}

/// One evaluated candidate binding, surfaced by `ExplainDecision` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingEvaluation {
    /// Evaluated binding id.
    pub binding_id: Uuid,
    /// Role behind the binding.
    pub role_id: Uuid,
    /// Binding status.
    pub binding_active: bool,
    /// Whether the validity window covers the decision instant.
    pub within_validity: bool,
    /// Whether evaluation reached the grant path: the bound role was
    /// visible and active *and* its permission set contains the key.
    pub role_grants_permission: bool,
    /// Outcome for this candidate.
    pub outcome: DecisionReason,
}

/// The result of one authorization evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// Allow or deny (the only two states that matter to handlers).
    pub allowed: bool,
    /// Bounded reason.
    pub reason: DecisionReason,
    /// Binding that produced an `AllowRoleBinding`.
    pub matched_binding: Option<Uuid>,
    /// Permission key that was decided on.
    pub matched_permission: Option<String>,
    /// Scope that matched (allow) or that the resource violated (mismatch).
    pub matched_scope: Option<ResourceScope>,
    /// Stable policy reference for audit correlation.
    pub policy_reference: String,
    /// Per-binding explanations (empty for plain `Authorize`).
    pub evaluations: Vec<BindingEvaluation>,
}

impl PolicyDecision {
    /// Deny with a bounded reason; safe for logs and client responses.
    #[must_use]
    pub fn deny(reason: DecisionReason) -> Self {
        debug_assert!(!reason.allowed());
        Self {
            allowed: false,
            reason,
            matched_binding: None,
            matched_permission: None,
            matched_scope: None,
            policy_reference: format!("policy:v1:{}", reason.as_str()),
            evaluations: Vec::new(),
        }
    }

    /// Allow granted by an active role binding.
    #[must_use]
    pub fn allow_binding(
        binding_id: Uuid,
        permission: &PermissionKey,
        scope: ResourceScope,
    ) -> Self {
        Self {
            allowed: true,
            reason: DecisionReason::AllowRoleBinding,
            matched_binding: Some(binding_id),
            matched_permission: Some(permission.as_str().to_string()),
            matched_scope: Some(scope),
            policy_reference: format!("policy:v1:binding/{binding_id}"),
            evaluations: Vec::new(),
        }
    }

    /// Allow granted by the bounded management-permission compat bridge.
    #[must_use]
    pub fn allow_compat(permission: &PermissionKey) -> Self {
        Self {
            allowed: true,
            reason: DecisionReason::AllowCompatManagementClaim,
            matched_binding: None,
            matched_permission: Some(permission.as_str().to_string()),
            matched_scope: Some(ResourceScope::Tenant),
            policy_reference: "policy:v1:compat-management-claim".to_string(),
            evaluations: Vec::new(),
        }
    }
}
