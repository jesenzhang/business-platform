//! `Authorize` / `ExplainDecision` — the default-DENY evaluation engine.
//!
//! Evaluation order (locked by PLAN-0013 preflight §5):
//! 1. subject status re-read from the identity bridge (suspension or
//!    disable denies on the very next request, unexpired JWT or not);
//! 2. permission must exist in the catalog (`DenyUnknownPermission`);
//! 3. the bounded compat bridge may grant only the seven governance keys,
//!    and only while the subject is Active (`AllowCompatManagementClaim`);
//! 4. active, effective role bindings in tenant — role visibility, role
//!    status, permission membership, then scope match. A scope only ever
//!    narrows; a request without a concrete resource can only ever be
//!    granted by a `Tenant` scope.
//!
//! Any store failure yields `DenyInternal` (never an allow). OIDC token
//! roles have no input path: the only authority inputs are
//! `AuthorizationContext` (resolved subject + server-trusted compat grants)
//! and stored policy data.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::application::error::PolicyApplicationError;
use crate::domain::{
    BindingEvaluation, DecisionReason, PermissionKey, PolicyDecision, ResourceScope, RoleBinding,
};
use crate::ports::{OrganizationScopePort, PolicyQueryPort, SubjectStatus, SubjectStatusPort};

/// The seven governance permissions the compat bridge may carry. This enum
/// is the *entire* claim-grant vocabulary: parsing is bounded, so an
/// unknown claim string can never name a new permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManagementCompatGrant {
    /// `audit.read`
    AuditRead,
    /// `integrity.read`
    IntegrityRead,
    /// `integrity.scan`
    IntegrityScan,
    /// `repair.dry-run`
    RepairDryRun,
    /// `repair.execute`
    RepairExecute,
    /// `repair.approve`
    RepairApprove,
    /// `repair.cancel`
    RepairCancel,
}

impl ManagementCompatGrant {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::AuditRead => "audit.read",
            Self::IntegrityRead => "integrity.read",
            Self::IntegrityScan => "integrity.scan",
            Self::RepairDryRun => "repair.dry-run",
            Self::RepairExecute => "repair.execute",
            Self::RepairApprove => "repair.approve",
            Self::RepairCancel => "repair.cancel",
        }
    }

    /// Parse a server-trusted claim value; unknown strings grant nothing.
    /// Surrounding whitespace is tolerated (claim transport), never
    /// widened.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "audit.read" => Some(Self::AuditRead),
            "integrity.read" => Some(Self::IntegrityRead),
            "integrity.scan" => Some(Self::IntegrityScan),
            "repair.dry-run" => Some(Self::RepairDryRun),
            "repair.execute" => Some(Self::RepairExecute),
            "repair.approve" => Some(Self::RepairApprove),
            "repair.cancel" => Some(Self::RepairCancel),
            _ => None,
        }
    }
}

/// The authenticated subject as re-established per request: resolved
/// platform identity plus the server-trusted compat grants. Built by the
/// authorization middleware — never from client headers.
#[derive(Debug, Clone)]
pub struct AuthorizationContext {
    /// Resolved platform user id.
    pub user_id: Uuid,
    /// Resolved tenant boundary.
    pub tenant_id: Uuid,
    /// Bounded governance compat grants (empty for normal subjects).
    pub compat_grants: BTreeSet<ManagementCompatGrant>,
}

impl AuthorizationContext {
    #[must_use]
    pub fn new(user_id: Uuid, tenant_id: Uuid) -> Self {
        Self {
            user_id,
            tenant_id,
            compat_grants: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn with_compat_grants(
        mut self,
        grants: impl IntoIterator<Item = ManagementCompatGrant>,
    ) -> Self {
        self.compat_grants = grants.into_iter().collect();
        self
    }
}

/// Trusted resource metadata for a decision. Values must come from server
/// storage (the owning use case loads the resource), never from the
/// request body. A request without a concrete resource passes `None`:
/// then only `Tenant`-scoped bindings can match — scopes never widen.
#[derive(Debug, Clone, Default)]
pub struct ResourceTarget {
    /// Resource kind, if the decision is about a concrete resource.
    pub kind: Option<String>,
    /// Concrete resource id, when the decision is per-resource.
    pub resource_id: Option<Uuid>,
    /// Owning organization unit, when the resource has one.
    pub org_unit_id: Option<Uuid>,
}

/// Deny-reason preference: higher wins when several candidates deny.
const fn deny_rank(reason: DecisionReason) -> u8 {
    match reason {
        DecisionReason::DenyNoBinding => 0,
        DecisionReason::DenyBindingRevoked
        | DecisionReason::DenyBindingExpired
        | DecisionReason::DenyBindingNotYetEffective => 1,
        DecisionReason::DenyScopeMismatch => 2,
        DecisionReason::DenyRoleDisabled => 3,
        DecisionReason::DenyCrossTenant => 4,
        _ => 5,
    }
}

/// Outcome of evaluating one candidate binding.
enum Candidate {
    /// This binding grants; carries the finished allow decision.
    Allowed(PolicyDecision),
    /// This binding does not grant; carries the explain inputs.
    Denied {
        reason: DecisionReason,
        role_grants: bool,
    },
    /// A backing store failed; the whole decision must deny
    /// (`DenyInternal`) — an evaluator never errors its way to an allow.
    InternalFailure,
}

/// Shared evaluation engine behind [`Authorize`] and [`ExplainDecision`].
struct Engine {
    policy: Arc<dyn PolicyQueryPort>,
    subject: Arc<dyn SubjectStatusPort>,
    org: Arc<dyn OrganizationScopePort>,
}

impl Engine {
    async fn decide(
        &self,
        ctx: &AuthorizationContext,
        raw_permission: &str,
        resource: Option<&ResourceTarget>,
        now: DateTime<Utc>,
        explain: bool,
    ) -> Result<PolicyDecision, PolicyApplicationError> {
        // 1. Subject facts, re-read on every decision.
        let Ok(status) = self
            .subject
            .subject_status(ctx.tenant_id, ctx.user_id)
            .await
        else {
            return Ok(PolicyDecision::deny(DecisionReason::DenyInternal));
        };
        let subject_deny = match status {
            SubjectStatus::MissingUser => Some(DecisionReason::DenyNoUser),
            SubjectStatus::UserDisabled => Some(DecisionReason::DenyUserDisabled),
            SubjectStatus::NoMembership => Some(DecisionReason::DenyNoMembership),
            SubjectStatus::MembershipSuspended => Some(DecisionReason::DenyMembershipSuspended),
            SubjectStatus::Active => None,
        };
        if let Some(reason) = subject_deny {
            return Ok(PolicyDecision::deny(reason));
        }

        // 2. Permission must exist in the catalog (grammar + membership).
        let Ok(key) = PermissionKey::parse(raw_permission) else {
            return Ok(PolicyDecision::deny(DecisionReason::DenyUnknownPermission));
        };
        let Ok(entry) = self.policy.get_permission(&key).await else {
            return Ok(PolicyDecision::deny(DecisionReason::DenyInternal));
        };
        if !entry.is_some_and(|entry| entry.is_active()) {
            return Ok(PolicyDecision::deny(DecisionReason::DenyUnknownPermission));
        }

        // 3. Bounded compat bridge: governance keys only, subject already
        //    Active; explainable and metriced as its own reason.
        if let Some(grant) = ManagementCompatGrant::parse(key.as_str()) {
            if ctx.compat_grants.contains(&grant) {
                return Ok(PolicyDecision::allow_compat(&key));
            }
        }

        // 4. Role bindings (all statuses, deterministic order).
        let Ok(bindings) = self
            .policy
            .list_bindings_for_user(ctx.tenant_id, ctx.user_id)
            .await
        else {
            return Ok(PolicyDecision::deny(DecisionReason::DenyInternal));
        };
        self.decide_from_bindings(ctx, &key, resource, &bindings, now, explain)
            .await
    }

    async fn decide_from_bindings(
        &self,
        ctx: &AuthorizationContext,
        key: &PermissionKey,
        resource: Option<&ResourceTarget>,
        bindings: &[RoleBinding],
        now: DateTime<Utc>,
        explain: bool,
    ) -> Result<PolicyDecision, PolicyApplicationError> {
        let mut evaluations = Vec::new();
        let mut best_deny = DecisionReason::DenyNoBinding;
        for binding in bindings {
            match self
                .evaluate_binding(ctx, key, resource, binding, now)
                .await
            {
                Candidate::Allowed(mut decision) => {
                    if explain {
                        evaluations.push(BindingEvaluation {
                            binding_id: binding.binding_id(),
                            role_id: binding.role_id(),
                            binding_active: true,
                            within_validity: true,
                            role_grants_permission: true,
                            outcome: DecisionReason::AllowRoleBinding,
                        });
                        decision.evaluations = evaluations;
                    }
                    return Ok(decision);
                }
                Candidate::Denied {
                    reason,
                    role_grants,
                } => {
                    if explain {
                        evaluations.push(BindingEvaluation {
                            binding_id: binding.binding_id(),
                            role_id: binding.role_id(),
                            binding_active: binding.is_active(),
                            within_validity: binding.is_within_validity(now),
                            role_grants_permission: role_grants,
                            outcome: reason,
                        });
                    }
                    if deny_rank(reason) > deny_rank(best_deny) {
                        best_deny = reason;
                    }
                }
                Candidate::InternalFailure => {
                    return Ok(PolicyDecision::deny(DecisionReason::DenyInternal));
                }
            }
        }
        let mut decision = PolicyDecision::deny(best_deny);
        if explain {
            decision.evaluations = evaluations;
        }
        Ok(decision)
    }

    /// Evaluate one candidate binding: validity window first, then role
    /// visibility (tenant-owned or system, never foreign), role status,
    /// permission membership, and finally the scope match.
    async fn evaluate_binding(
        &self,
        ctx: &AuthorizationContext,
        key: &PermissionKey,
        resource: Option<&ResourceTarget>,
        binding: &RoleBinding,
        now: DateTime<Utc>,
    ) -> Candidate {
        let denied = |reason: DecisionReason| Candidate::Denied {
            reason,
            role_grants: false,
        };
        if !binding.is_active() {
            return denied(DecisionReason::DenyBindingRevoked);
        }
        if now < binding.effective_at() {
            return denied(DecisionReason::DenyBindingNotYetEffective);
        }
        if binding
            .expires_at()
            .is_some_and(|expires_at| now >= expires_at)
        {
            return denied(DecisionReason::DenyBindingExpired);
        }
        let Ok(role) = self.policy.get_role(ctx.tenant_id, binding.role_id()).await else {
            return Candidate::InternalFailure;
        };
        let Some(role) = role else {
            return denied(DecisionReason::DenyCrossTenant);
        };
        if !role.is_active() {
            return denied(DecisionReason::DenyRoleDisabled);
        }
        let Ok(permission_keys) = self
            .policy
            .get_role_permissions(ctx.tenant_id, binding.role_id())
            .await
        else {
            return Candidate::InternalFailure;
        };
        if !permission_keys.iter().any(|stored| stored == key.as_str()) {
            return denied(DecisionReason::DenyNoBinding);
        }
        let Ok(matches) = self.scope_matches(binding, resource, ctx).await else {
            return Candidate::InternalFailure;
        };
        if !matches {
            return Candidate::Denied {
                reason: DecisionReason::DenyScopeMismatch,
                role_grants: true,
            };
        }
        Candidate::Allowed(PolicyDecision::allow_binding(
            binding.binding_id(),
            key,
            binding.scope().clone(),
        ))
    }

    /// Scope matching against the trusted resource target. Organization
    /// bridge failures propagate as errors — the caller maps them to
    /// `DenyInternal` (never an allow).
    async fn scope_matches(
        &self,
        binding: &RoleBinding,
        resource: Option<&ResourceTarget>,
        ctx: &AuthorizationContext,
    ) -> Result<bool, PolicyApplicationError> {
        match binding.scope() {
            ResourceScope::Tenant => Ok(true),
            ResourceScope::OrganizationUnit {
                org_unit_id,
                include_subtree,
            } => {
                let Some(resource_unit) = resource.and_then(|target| target.org_unit_id) else {
                    return Ok(false);
                };
                if resource_unit == *org_unit_id {
                    return self
                        .org
                        .unit_is_active(ctx.tenant_id, *org_unit_id)
                        .await
                        .map_err(PolicyApplicationError::from);
                }
                if !*include_subtree {
                    return Ok(false);
                }
                // Subtree use is scope use: a disabled bound unit stops
                // granting (same rule as the exact branch), and a disabled
                // unit hosting the resource stops being addressable — the
                // walk fails closed at both ends. Intermediate units on the
                // path are deliberately not re-validated per decision:
                // re-activating an intermediate node must not silently
                // re-open authority that the endpoint rule already
                // controls, and a mid-tree disable is handled where the
                // organization surfaces it (membership/staffing rules).
                let bound_active = self
                    .org
                    .unit_is_active(ctx.tenant_id, *org_unit_id)
                    .await
                    .map_err(PolicyApplicationError::from)?;
                if !bound_active {
                    return Ok(false);
                }
                let candidate_active = self
                    .org
                    .unit_is_active(ctx.tenant_id, resource_unit)
                    .await
                    .map_err(PolicyApplicationError::from)?;
                if !candidate_active {
                    return Ok(false);
                }
                self.org
                    .subtree_contains(ctx.tenant_id, *org_unit_id, resource_unit)
                    .await
                    .map_err(PolicyApplicationError::from)
            }
            ResourceScope::ResourceType { kind } => {
                Ok(resource.is_some_and(|target| target.kind.as_deref() == Some(kind.as_str())))
            }
            ResourceScope::Resource { kind, resource_id } => Ok(resource.is_some_and(|target| {
                target.kind.as_deref() == Some(kind.as_str())
                    && target.resource_id == Some(*resource_id)
            })),
        }
    }
}

/// Plain allow/deny authorization for application use cases.
pub struct Authorize {
    engine: Engine,
}

/// Admin-facing evaluation with per-candidate explanation output.
pub struct ExplainDecision {
    engine: Engine,
}

impl Authorize {
    #[must_use]
    pub fn new(
        policy: Arc<dyn PolicyQueryPort>,
        subject: Arc<dyn SubjectStatusPort>,
        org: Arc<dyn OrganizationScopePort>,
    ) -> Self {
        Self {
            engine: Engine {
                policy,
                subject,
                org,
            },
        }
    }

    /// Evaluate one permission for the context against an optional trusted
    /// resource. Store failures return `Deny(DenyInternal)` — never an
    /// allow; delivery maps that reason to a retryable 503.
    pub async fn check(
        &self,
        ctx: &AuthorizationContext,
        permission: &str,
        resource: Option<&ResourceTarget>,
    ) -> Result<PolicyDecision, PolicyApplicationError> {
        self.engine
            .decide(ctx, permission, resource, Utc::now(), false)
            .await
    }
}

impl ExplainDecision {
    #[must_use]
    pub fn new(
        policy: Arc<dyn PolicyQueryPort>,
        subject: Arc<dyn SubjectStatusPort>,
        org: Arc<dyn OrganizationScopePort>,
    ) -> Self {
        Self {
            engine: Engine {
                policy,
                subject,
                org,
            },
        }
    }

    /// Same evaluation as [`Authorize::check`], but every candidate binding
    /// is reported in `evaluations` (admin-only surface, gated by the
    /// `policy.explain` permission at delivery).
    pub async fn explain(
        &self,
        ctx: &AuthorizationContext,
        permission: &str,
        resource: Option<&ResourceTarget>,
    ) -> Result<PolicyDecision, PolicyApplicationError> {
        self.engine
            .decide(ctx, permission, resource, Utc::now(), true)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat_grant_parsing_is_bounded() {
        let cases = [
            ("audit.read", ManagementCompatGrant::AuditRead),
            ("integrity.read", ManagementCompatGrant::IntegrityRead),
            ("integrity.scan", ManagementCompatGrant::IntegrityScan),
            ("repair.dry-run", ManagementCompatGrant::RepairDryRun),
            ("repair.execute", ManagementCompatGrant::RepairExecute),
            ("repair.approve", ManagementCompatGrant::RepairApprove),
            ("repair.cancel", ManagementCompatGrant::RepairCancel),
        ];
        for (raw, grant) in cases {
            assert_eq!(ManagementCompatGrant::parse(raw), Some(grant));
            assert_eq!(grant.key(), raw);
        }
        // Anything else — including IAM keys, reserved keys, and injection
        // shapes — parses to nothing: the bridge cannot widen past the seven.
        for bad in [
            "identity.read",
            "policy.binding.manage",
            "contract.read",
            "audit.read; DROP",
            "AUDIT.READ",
            "audit",
            "",
        ] {
            assert!(
                ManagementCompatGrant::parse(bad).is_none(),
                "{bad:?} granted"
            );
        }
        // Transport whitespace tolerated, never widened.
        assert_eq!(
            ManagementCompatGrant::parse(" audit.read "),
            Some(ManagementCompatGrant::AuditRead)
        );
    }

    #[test]
    fn deny_rank_prefers_structural_causes() {
        assert!(
            deny_rank(DecisionReason::DenyCrossTenant)
                > deny_rank(DecisionReason::DenyRoleDisabled)
        );
        assert!(
            deny_rank(DecisionReason::DenyRoleDisabled)
                > deny_rank(DecisionReason::DenyScopeMismatch)
        );
        assert!(
            deny_rank(DecisionReason::DenyScopeMismatch)
                > deny_rank(DecisionReason::DenyBindingRevoked)
        );
        assert_eq!(deny_rank(DecisionReason::DenyNoBinding), 0);
    }
}
