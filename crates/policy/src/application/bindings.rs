//! Role binding use cases with the self-escalation guards.
//!
//! `BindRole` refuses to bind an IAM-management role to the acting user
//! unless the actor is *bootstrap-sourced* — evaluated as an active,
//! effective binding to an immutable system role **at decision time**,
//! never a historical flag — or unless an identical active binding already
//! exists (idempotent re-bind convergence). Revoking one's own only binding
//! (self-DoS) is allowed but audited; recovery is the `[auth.bootstrap]`
//! version-bump path.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::application::error::PolicyApplicationError;
use crate::application::{
    validate_idempotency_key, validate_text_field, MAX_BINDINGS_PER_USER, MAX_REASON_LEN,
};
use crate::catalog::is_iam_management_permission;
use crate::domain::{ResourceScope, RoleBinding};
use crate::ports::{
    BindRoleCommit, BindingCommitOutcome, MutationActorKind, MutationContext,
    OrganizationScopePort, PolicyCommandPort, PolicyQueryPort, RevokeBindingCommit,
    SubjectStatusPort,
};

fn audit_context(
    actor_user_id: Uuid,
    reason: Option<String>,
) -> Result<MutationContext, PolicyApplicationError> {
    if let Some(reason) = &reason {
        validate_text_field(reason, MAX_REASON_LEN, "reason")
            .map_err(PolicyApplicationError::Validation)?;
    }
    Ok(MutationContext {
        actor_id: actor_user_id.to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason,
    })
}

/// Command for [`BindRole`].
#[derive(Debug, Clone)]
pub struct BindRoleCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Optional caller-chosen binding id.
    pub binding_id: Option<Uuid>,
    /// User to bind the role to.
    pub user_id: Uuid,
    /// Role to bind (tenant-owned or system).
    pub role_id: Uuid,
    /// Granted scope.
    pub scope: ResourceScope,
    /// Validity window start.
    pub effective_at: DateTime<Utc>,
    /// Validity window end (exclusive when set).
    pub expires_at: Option<DateTime<Utc>>,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Bind a role to a user inside the tenant.
pub struct BindRole {
    command: Arc<dyn PolicyCommandPort>,
    query: Arc<dyn PolicyQueryPort>,
    subject: Arc<dyn SubjectStatusPort>,
    org: Arc<dyn OrganizationScopePort>,
}

impl BindRole {
    #[must_use]
    pub fn new(
        command: Arc<dyn PolicyCommandPort>,
        query: Arc<dyn PolicyQueryPort>,
        subject: Arc<dyn SubjectStatusPort>,
        org: Arc<dyn OrganizationScopePort>,
    ) -> Self {
        Self {
            command,
            query,
            subject,
            org,
        }
    }

    pub async fn execute(
        &self,
        command: BindRoleCommand,
    ) -> Result<BindingCommitOutcome, PolicyApplicationError> {
        if command.tenant_id.is_nil()
            || command.user_id.is_nil()
            || command.role_id.is_nil()
            || command.actor_user_id.is_nil()
        {
            return Err(PolicyApplicationError::Validation(
                "tenant, user, role, and actor ids must not be nil".to_string(),
            ));
        }
        if let Some(expires_at) = command.expires_at {
            if expires_at <= command.effective_at {
                return Err(PolicyApplicationError::Validation(
                    "expires_at must be after effective_at".to_string(),
                ));
            }
        }
        if let Some(binding_id) = command.binding_id {
            if binding_id.is_nil() {
                return Err(PolicyApplicationError::Validation(
                    "binding id must not be nil".to_string(),
                ));
            }
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(PolicyApplicationError::Validation)?;

        // The target must be an active member of this tenant.
        if !self
            .subject
            .subject_status(command.tenant_id, command.user_id)
            .await?
            .is_active()
        {
            return Err(PolicyApplicationError::NotTenantMember);
        }

        // Role must be visible (tenant-owned or system) and active.
        let role = self
            .query
            .get_role(command.tenant_id, command.role_id)
            .await?
            .ok_or(PolicyApplicationError::NotFound)?;
        if !role.is_active() {
            return Err(PolicyApplicationError::Validation(
                "role is disabled and cannot be bound".to_string(),
            ));
        }

        // Org-unit scopes must address an active unit in this tenant.
        if let ResourceScope::OrganizationUnit { org_unit_id, .. } = &command.scope {
            if !self
                .org
                .unit_is_active(command.tenant_id, *org_unit_id)
                .await?
            {
                return Err(PolicyApplicationError::ScopeUnitUnavailable);
            }
        }

        let existing_bindings = self
            .query
            .list_bindings_for_user(command.tenant_id, command.user_id)
            .await?;

        if command.user_id == command.actor_user_id {
            self.assert_self_bind_allowed(&command, &existing_bindings)
                .await?;
        }

        // Per-user binding cap (write-side spam guard).
        if existing_bindings
            .iter()
            .filter(|binding| binding.is_active())
            .count()
            >= MAX_BINDINGS_PER_USER
        {
            return Err(PolicyApplicationError::TooManyBindings);
        }
        let audit = audit_context(command.actor_user_id, command.reason)?;

        Ok(self
            .command
            .bind_role(BindRoleCommit {
                tenant_id: command.tenant_id,
                binding_id: command.binding_id,
                user_id: command.user_id,
                role_id: command.role_id,
                scope: command.scope,
                effective_at: command.effective_at,
                expires_at: command.expires_at,
                audit,
                idempotency_key: command.idempotency_key,
                now: Utc::now(),
            })
            .await?)
    }

    /// Self-bind guard (domain-level invariant, not handler policy): an
    /// actor may never *newly* bind themselves to a role carrying IAM
    /// management permissions, unless an identical active binding already
    /// exists (idempotent re-bind) or they are bootstrap-sourced right now.
    async fn assert_self_bind_allowed(
        &self,
        command: &BindRoleCommand,
        existing_bindings: &[RoleBinding],
    ) -> Result<(), PolicyApplicationError> {
        let role_keys = self
            .query
            .get_role_permissions(command.tenant_id, command.role_id)
            .await?;
        let role_grants_management = role_keys
            .iter()
            .any(|key| is_iam_management_permission(key));
        if !role_grants_management {
            return Ok(());
        }
        // Idempotent re-bind: an identical *active* binding to the same
        // role already converges at the store — allow it.
        let already_bound = existing_bindings.iter().any(|binding| {
            binding.role_id() == command.role_id
                && binding.is_active()
                && binding.is_within_validity(Utc::now())
        });
        if already_bound {
            return Ok(());
        }
        if self
            .actor_is_bootstrap_sourced(command.tenant_id, command.actor_user_id)
            .await?
        {
            return Ok(());
        }
        Err(PolicyApplicationError::SelfEscalationDenied)
    }

    /// Bootstrap-sourced = an active, effective binding to an immutable
    /// system role *right now* (single-admin cold-start allowance). Never
    /// a historical flag: revoking the system binding ends the carve-out
    /// on the next request.
    async fn actor_is_bootstrap_sourced(
        &self,
        tenant_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<bool, PolicyApplicationError> {
        for binding in self
            .query
            .list_bindings_for_user(tenant_id, actor_user_id)
            .await?
        {
            if !binding.is_active() || !binding.is_within_validity(Utc::now()) {
                continue;
            }
            if let Some(role) = self.query.get_role(tenant_id, binding.role_id()).await? {
                if role.is_system() && role.is_active() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

/// Command for [`RevokeRoleBinding`].
#[derive(Debug, Clone)]
pub struct RevokeRoleBindingCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target binding.
    pub binding_id: Uuid,
    /// Optimistic version.
    pub expected_version: i64,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Revoke a binding. Deliberately has no self-guard: an admin revoking
/// their own only binding (self-DoS) is allowed and audited; recovery is
/// the bootstrap config version bump.
pub struct RevokeRoleBinding {
    command: Arc<dyn PolicyCommandPort>,
}

impl RevokeRoleBinding {
    #[must_use]
    pub fn new(command: Arc<dyn PolicyCommandPort>) -> Self {
        Self { command }
    }

    pub async fn execute(
        &self,
        command: RevokeRoleBindingCommand,
    ) -> Result<BindingCommitOutcome, PolicyApplicationError> {
        if command.tenant_id.is_nil()
            || command.binding_id.is_nil()
            || command.actor_user_id.is_nil()
        {
            return Err(PolicyApplicationError::Validation(
                "tenant, binding, and actor ids must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(PolicyApplicationError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(PolicyApplicationError::Validation)?;
        let audit = audit_context(command.actor_user_id, command.reason)?;

        Ok(self
            .command
            .revoke_binding(RevokeBindingCommit {
                tenant_id: command.tenant_id,
                binding_id: command.binding_id,
                expected_version: command.expected_version,
                audit,
                idempotency_key: command.idempotency_key,
                now: Utc::now(),
            })
            .await?)
    }
}
