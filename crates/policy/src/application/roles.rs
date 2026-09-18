//! Role management use cases with the self-escalation guard.
//!
//! System roles are immutable at every layer; `SetRolePermissions` refuses
//! to add IAM-management permissions to a role the actor currently holds —
//! the domain has no notion of "trusting yourself to grant yourself".

use std::sync::Arc;

use uuid::Uuid;

use crate::application::error::PolicyApplicationError;
use crate::application::{
    validate_idempotency_key, validate_text_field, MAX_REASON_LEN, MAX_ROLES_PER_TENANT,
    MAX_ROLE_PERMISSIONS,
};
use crate::catalog::is_iam_management_permission;
use crate::domain::{PermissionKey, RoleDefinition, RoleStatus};
use crate::ports::{
    CreateRoleCommit, MutationActorKind, MutationContext, PermissionsCommitOutcome,
    PolicyCommandPort, PolicyQueryPort, RoleCommitOutcome, SetRolePermissionsCommit,
    UpdateRoleCommit,
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

/// Command for [`CreateRole`].
#[derive(Debug, Clone)]
pub struct CreateRoleCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Optional caller-chosen role id.
    pub role_id: Option<Uuid>,
    /// Stable key (unique per tenant).
    pub stable_key: String,
    /// Display name.
    pub display_name: String,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Create a tenant role (empty permission set; system roles are
/// migration-seeded only).
pub struct CreateRole {
    command: Arc<dyn PolicyCommandPort>,
    query: Arc<dyn PolicyQueryPort>,
}

impl CreateRole {
    #[must_use]
    pub fn new(command: Arc<dyn PolicyCommandPort>, query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { command, query }
    }

    pub async fn execute(
        &self,
        command: CreateRoleCommand,
    ) -> Result<RoleCommitOutcome, PolicyApplicationError> {
        if command.tenant_id.is_nil() || command.actor_user_id.is_nil() {
            return Err(PolicyApplicationError::Validation(
                "tenant and actor ids must not be nil".to_string(),
            ));
        }
        if let Some(role_id) = command.role_id {
            if role_id.is_nil() {
                return Err(PolicyApplicationError::Validation(
                    "role id must not be nil".to_string(),
                ));
            }
        }
        // Domain grammar is the authority; pre-check for 400-class errors.
        RoleDefinition::create_tenant_role(
            command.role_id.unwrap_or_else(Uuid::now_v7),
            command.tenant_id,
            &command.stable_key,
            &command.display_name,
            chrono::Utc::now(),
        )
        .map_err(|error| PolicyApplicationError::Validation(error.to_string()))?;
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(PolicyApplicationError::Validation)?;
        let audit = audit_context(command.actor_user_id, command.reason)?;

        if self.query.list_roles(command.tenant_id).await?.len() >= MAX_ROLES_PER_TENANT {
            return Err(PolicyApplicationError::Validation(format!(
                "tenant role cap ({MAX_ROLES_PER_TENANT}) reached"
            )));
        }

        Ok(self
            .command
            .create_role(CreateRoleCommit {
                tenant_id: command.tenant_id,
                role_id: command.role_id,
                stable_key: command.stable_key.trim().to_string(),
                display_name: command.display_name.trim().to_string(),
                audit,
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

/// Command for [`UpdateRole`].
#[derive(Debug, Clone)]
pub struct UpdateRoleCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target role.
    pub role_id: Uuid,
    /// New display name.
    pub display_name: Option<String>,
    /// New status.
    pub status: Option<RoleStatus>,
    /// Optimistic version.
    pub expected_version: i64,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Update tenant role metadata or status.
pub struct UpdateRole {
    command: Arc<dyn PolicyCommandPort>,
    query: Arc<dyn PolicyQueryPort>,
}

impl UpdateRole {
    #[must_use]
    pub fn new(command: Arc<dyn PolicyCommandPort>, query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { command, query }
    }

    pub async fn execute(
        &self,
        command: UpdateRoleCommand,
    ) -> Result<RoleCommitOutcome, PolicyApplicationError> {
        if command.tenant_id.is_nil() || command.role_id.is_nil() || command.actor_user_id.is_nil()
        {
            return Err(PolicyApplicationError::Validation(
                "tenant, role, and actor ids must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(PolicyApplicationError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        if command.display_name.is_none() && command.status.is_none() {
            return Err(PolicyApplicationError::Validation(
                "at least one field must be updated".to_string(),
            ));
        }
        if let Some(display_name) = &command.display_name {
            validate_text_field(display_name, 200, "display_name")
                .map_err(PolicyApplicationError::Validation)?;
            if display_name.trim().is_empty() {
                return Err(PolicyApplicationError::Validation(
                    "display_name must not be blank".to_string(),
                ));
            }
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(PolicyApplicationError::Validation)?;

        // Reject system targets with the right error class before the store
        // does (400-class contract).
        let role = self
            .query
            .get_role(command.tenant_id, command.role_id)
            .await?
            .ok_or(PolicyApplicationError::NotFound)?;
        if role.is_system() {
            return Err(PolicyApplicationError::RoleImmutable);
        }
        let audit = audit_context(command.actor_user_id, command.reason)?;

        Ok(self
            .command
            .update_role(UpdateRoleCommit {
                tenant_id: command.tenant_id,
                role_id: command.role_id,
                display_name: command.display_name,
                status: command.status,
                expected_version: command.expected_version,
                audit,
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

/// Command for [`SetRolePermissions`].
#[derive(Debug, Clone)]
pub struct SetRolePermissionsCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target role.
    pub role_id: Uuid,
    /// Complete replacement permission key set.
    pub permission_keys: Vec<String>,
    /// Optimistic version.
    pub expected_version: i64,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Transactional set replace of a role's permissions, guarded against
/// self-escalation: adding an IAM-management permission to a role the
/// actor is currently bound to is rejected outright.
pub struct SetRolePermissions {
    command: Arc<dyn PolicyCommandPort>,
    query: Arc<dyn PolicyQueryPort>,
}

impl SetRolePermissions {
    #[must_use]
    pub fn new(command: Arc<dyn PolicyCommandPort>, query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { command, query }
    }

    pub async fn execute(
        &self,
        command: SetRolePermissionsCommand,
    ) -> Result<PermissionsCommitOutcome, PolicyApplicationError> {
        if command.tenant_id.is_nil() || command.role_id.is_nil() || command.actor_user_id.is_nil()
        {
            return Err(PolicyApplicationError::Validation(
                "tenant, role, and actor ids must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(PolicyApplicationError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        if command.permission_keys.len() > MAX_ROLE_PERMISSIONS {
            return Err(PolicyApplicationError::TooManyPermissions);
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut keys = Vec::with_capacity(command.permission_keys.len());
        for raw in &command.permission_keys {
            let key = PermissionKey::parse(raw)
                .map_err(|error| PolicyApplicationError::Validation(error.to_string()))?;
            if !seen.insert(key.as_str().to_string()) {
                return Err(PolicyApplicationError::Validation(format!(
                    "duplicate permission key {:?}",
                    key.as_str()
                )));
            }
            keys.push(key.as_str().to_string());
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(PolicyApplicationError::Validation)?;

        let role = self
            .query
            .get_role(command.tenant_id, command.role_id)
            .await?
            .ok_or(PolicyApplicationError::NotFound)?;
        if role.is_system() {
            return Err(PolicyApplicationError::RoleImmutable);
        }

        // Self-escalation guard: additions of IAM-management permissions
        // are refused while the actor holds an active binding to this role.
        let current_keys = self
            .query
            .get_role_permissions(command.tenant_id, command.role_id)
            .await?;
        let current: std::collections::BTreeSet<&str> =
            current_keys.iter().map(String::as_str).collect();
        let adds_management = keys
            .iter()
            .any(|key| is_iam_management_permission(key) && !current.contains(key.as_str()));
        if adds_management {
            let holds_role = self
                .query
                .list_active_bindings_for_role(command.tenant_id, command.role_id)
                .await?
                .iter()
                .any(|binding| binding.user_id() == command.actor_user_id);
            if holds_role {
                return Err(PolicyApplicationError::SelfEscalationDenied);
            }
        }
        let audit = audit_context(command.actor_user_id, command.reason)?;

        Ok(self
            .command
            .set_role_permissions(SetRolePermissionsCommit {
                tenant_id: command.tenant_id,
                role_id: command.role_id,
                permission_keys: keys,
                expected_version: command.expected_version,
                audit,
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}
