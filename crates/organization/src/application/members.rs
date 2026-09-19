//! Organization membership use cases (PLAN-0013 WP-02).

use std::sync::Arc;

use uuid::Uuid;

use crate::application::error::OrganizationApplicationError;
use crate::application::{validate_idempotency_key, validate_text_field, MAX_REASON_LEN};
use crate::domain::OrganizationMembershipType;
use crate::ports::{
    AddMemberCommit, MutationActorKind, MutationContext, OrganizationCommandPort,
    OrganizationQueryPort, RemoveMemberCommit, TenantMembershipReader,
};

/// Command for [`AddOrganizationMember`].
#[derive(Debug, Clone)]
pub struct AddOrganizationMemberCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// Platform user to attach.
    pub user_id: Uuid,
    /// Kind of membership.
    pub membership_type: OrganizationMembershipType,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Add a user to an organization unit.
pub struct AddOrganizationMember {
    command_port: Arc<dyn OrganizationCommandPort>,
    query_port: Arc<dyn OrganizationQueryPort>,
    tenant_reader: Arc<dyn TenantMembershipReader>,
}

impl AddOrganizationMember {
    #[must_use]
    pub fn new(
        command_port: Arc<dyn OrganizationCommandPort>,
        query_port: Arc<dyn OrganizationQueryPort>,
        tenant_reader: Arc<dyn TenantMembershipReader>,
    ) -> Self {
        Self {
            command_port,
            query_port,
            tenant_reader,
        }
    }

    pub async fn execute(
        &self,
        command: AddOrganizationMemberCommand,
    ) -> Result<crate::ports::MemberCommitOutcome, OrganizationApplicationError> {
        if command.tenant_id.is_nil()
            || command.unit_id.is_nil()
            || command.user_id.is_nil()
            || command.actor_user_id.is_nil()
        {
            return Err(OrganizationApplicationError::Validation(
                "tenant, unit, user, and actor ids must not be nil".to_string(),
            ));
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(OrganizationApplicationError::Validation)?;
        if let Some(reason) = &command.reason {
            validate_text_field(reason, MAX_REASON_LEN, "reason")
                .map_err(OrganizationApplicationError::Validation)?;
        }

        // The user must be an active tenant member before joining a unit
        // (identity owns that fact; organization asks, never reads tables).
        if !self
            .tenant_reader
            .is_active_member(command.tenant_id, command.user_id)
            .await?
        {
            return Err(OrganizationApplicationError::NotTenantMember);
        }
        let unit = self
            .query_port
            .get_unit(command.tenant_id, command.unit_id)
            .await?
            .ok_or(OrganizationApplicationError::NotFound)?;
        if !unit.is_active() {
            return Err(OrganizationApplicationError::UnitDisabled);
        }

        Ok(self
            .command_port
            .add_member(AddMemberCommit {
                tenant_id: command.tenant_id,
                unit_id: command.unit_id,
                user_id: command.user_id,
                membership_type: command.membership_type,
                audit: MutationContext {
                    actor_id: command.actor_user_id.to_string(),
                    actor_kind: MutationActorKind::User,
                    operation_id: Uuid::now_v7(),
                    trace_id: None,
                    reason: command.reason,
                },
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

/// Command for [`RemoveOrganizationMember`].
#[derive(Debug, Clone)]
pub struct RemoveOrganizationMemberCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// Platform user to detach.
    pub user_id: Uuid,
    /// Kind of membership.
    pub membership_type: OrganizationMembershipType,
    /// Optimistic version of the membership row.
    pub expected_version: i64,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Remove (deactivate) a user from an organization unit.
pub struct RemoveOrganizationMember {
    command_port: Arc<dyn OrganizationCommandPort>,
}

impl RemoveOrganizationMember {
    #[must_use]
    pub fn new(command_port: Arc<dyn OrganizationCommandPort>) -> Self {
        Self { command_port }
    }

    pub async fn execute(
        &self,
        command: RemoveOrganizationMemberCommand,
    ) -> Result<crate::ports::MemberCommitOutcome, OrganizationApplicationError> {
        if command.tenant_id.is_nil()
            || command.unit_id.is_nil()
            || command.user_id.is_nil()
            || command.actor_user_id.is_nil()
        {
            return Err(OrganizationApplicationError::Validation(
                "tenant, unit, user, and actor ids must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(OrganizationApplicationError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(OrganizationApplicationError::Validation)?;
        if let Some(reason) = &command.reason {
            validate_text_field(reason, MAX_REASON_LEN, "reason")
                .map_err(OrganizationApplicationError::Validation)?;
        }

        Ok(self
            .command_port
            .remove_member(RemoveMemberCommit {
                tenant_id: command.tenant_id,
                unit_id: command.unit_id,
                user_id: command.user_id,
                membership_type: command.membership_type,
                expected_version: command.expected_version,
                audit: MutationContext {
                    actor_id: command.actor_user_id.to_string(),
                    actor_kind: MutationActorKind::User,
                    operation_id: Uuid::now_v7(),
                    trace_id: None,
                    reason: command.reason,
                },
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OrganizationUnitType;
    use crate::testing::FakeOrganizationPorts;
    use std::sync::Arc;

    fn tenant() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn actor() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    fn user() -> Uuid {
        Uuid::from_bytes([3; 16])
    }

    fn seed_unit(ports: &FakeOrganizationPorts) -> Uuid {
        ports.seed_unit(
            crate::domain::OrganizationUnit::create(
                Uuid::now_v7(),
                tenant(),
                None,
                OrganizationUnitType::Company,
                "Acme",
                chrono::Utc::now(),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
        ports.seeded_unit_id()
    }

    #[tokio::test]
    async fn add_requires_active_tenant_membership_then_converges_on_readd() {
        let ports = FakeOrganizationPorts::default();
        let unit_id = seed_unit(&ports);
        let add = AddOrganizationMember::new(
            Arc::clone(&ports.command),
            Arc::clone(&ports.query),
            Arc::clone(&ports.reader),
        );

        // User is not a tenant member yet.
        let denied = add
            .execute(AddOrganizationMemberCommand {
                tenant_id: tenant(),
                unit_id,
                user_id: user(),
                membership_type: OrganizationMembershipType::Member,
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await;
        assert_eq!(denied, Err(OrganizationApplicationError::NotTenantMember));

        ports.set_active_member(tenant(), user());
        let added = add
            .execute(AddOrganizationMemberCommand {
                tenant_id: tenant(),
                unit_id,
                user_id: user(),
                membership_type: OrganizationMembershipType::Member,
                actor_user_id: actor(),
                idempotency_key: Some("add-1".to_string()),
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(added.membership.is_active());

        // Idempotent replay converges.
        let replay = add
            .execute(AddOrganizationMemberCommand {
                tenant_id: tenant(),
                unit_id,
                user_id: user(),
                membership_type: OrganizationMembershipType::Member,
                actor_user_id: actor(),
                idempotency_key: Some("add-1".to_string()),
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(replay.replayed);

        // Remove then re-add reactivates the history row.
        let remove = RemoveOrganizationMember::new(Arc::clone(&ports.command));
        let removed = remove
            .execute(RemoveOrganizationMemberCommand {
                tenant_id: tenant(),
                unit_id,
                user_id: user(),
                membership_type: OrganizationMembershipType::Member,
                expected_version: 1,
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!removed.membership.is_active());

        let readded = add
            .execute(AddOrganizationMemberCommand {
                tenant_id: tenant(),
                unit_id,
                user_id: user(),
                membership_type: OrganizationMembershipType::Member,
                actor_user_id: actor(),
                idempotency_key: Some("add-2".to_string()),
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(readded.membership.is_active());
        assert!(
            !readded.replayed,
            "reactivating a removed membership is a real mutation, not a replay"
        );
        assert_eq!(
            readded.membership.membership_id(),
            removed.membership.membership_id(),
            "re-add must converge onto the history row"
        );
    }
}
