//! Organization unit management use cases (PLAN-0013 WP-02).

use std::sync::Arc;

use uuid::Uuid;

use crate::application::error::OrganizationApplicationError;
use crate::application::{
    validate_idempotency_key, validate_text_field, MAX_REASON_LEN, MAX_UNITS_PER_TENANT,
};
use crate::domain::{
    validate_tree_placement, OrganizationUnit, OrganizationUnitStatus, OrganizationUnitType,
};
use crate::ports::{
    CreateUnitCommit, MoveUnitCommit, MutationContext, OrganizationCommandPort,
    OrganizationQueryPort, UnitCommitOutcome, UpdateUnitCommit,
};

fn audit_context(
    actor_user_id: Uuid,
    reason: Option<String>,
) -> Result<MutationContext, OrganizationApplicationError> {
    if let Some(reason) = &reason {
        validate_text_field(reason, MAX_REASON_LEN, "reason")
            .map_err(OrganizationApplicationError::Validation)?;
    }
    Ok(MutationContext {
        actor_id: actor_user_id.to_string(),
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason,
    })
}

/// Command for [`CreateOrganizationUnit`].
#[derive(Debug, Clone)]
pub struct CreateOrganizationUnitCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Optional caller-chosen unit id (server generates one when absent).
    pub unit_id: Option<Uuid>,
    /// Parent unit, if any.
    pub parent_id: Option<Uuid>,
    /// Unit kind.
    pub unit_type: OrganizationUnitType,
    /// Display name.
    pub name: String,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Create a unit inside the tenant tree.
pub struct CreateOrganizationUnit {
    command_port: Arc<dyn OrganizationCommandPort>,
    query_port: Arc<dyn OrganizationQueryPort>,
}

impl CreateOrganizationUnit {
    #[must_use]
    pub fn new(
        command_port: Arc<dyn OrganizationCommandPort>,
        query_port: Arc<dyn OrganizationQueryPort>,
    ) -> Self {
        Self {
            command_port,
            query_port,
        }
    }

    pub async fn execute(
        &self,
        command: CreateOrganizationUnitCommand,
    ) -> Result<UnitCommitOutcome, OrganizationApplicationError> {
        if command.tenant_id.is_nil() || command.actor_user_id.is_nil() {
            return Err(OrganizationApplicationError::Validation(
                "tenant and actor ids must not be nil".to_string(),
            ));
        }
        if let Some(unit_id) = command.unit_id {
            if unit_id.is_nil() || unit_id == command.parent_id.unwrap_or(Uuid::nil()) {
                return Err(OrganizationApplicationError::Validation(
                    "unit id must not be nil or its own parent".to_string(),
                ));
            }
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(OrganizationApplicationError::Validation)?;
        let audit = audit_context(command.actor_user_id, command.reason)?;

        // Pre-flight placement checks (the store re-validates atomically).
        if let Some(parent_id) = command.parent_id {
            let parent = self
                .query_port
                .get_unit(command.tenant_id, parent_id)
                .await?
                .ok_or(OrganizationApplicationError::InvalidParent)?;
            if !parent.is_active() {
                return Err(OrganizationApplicationError::UnitDisabled);
            }
        }
        let snapshot = self.query_port.list_units(command.tenant_id).await?;
        if snapshot.len() >= MAX_UNITS_PER_TENANT {
            return Err(OrganizationApplicationError::TooManyResources);
        }
        let unit_id = command.unit_id.unwrap_or_else(Uuid::now_v7);
        let draft = OrganizationUnit::create(
            unit_id,
            command.tenant_id,
            command.parent_id,
            command.unit_type,
            &command.name,
            chrono::Utc::now(),
        )
        .map_err(|error| OrganizationApplicationError::Validation(error.to_string()))?;
        let tree: Vec<(Uuid, Option<Uuid>)> = snapshot
            .iter()
            .map(|unit| (unit.unit_id(), unit.parent_id()))
            .collect();
        validate_tree_placement(&tree, draft.unit_id(), draft.parent_id())
            .map_err(|_| OrganizationApplicationError::InvalidParent)?;

        Ok(self
            .command_port
            .create_unit(CreateUnitCommit {
                tenant_id: command.tenant_id,
                unit_id,
                parent_id: command.parent_id,
                unit_type: command.unit_type,
                name: command.name.trim().to_string(),
                audit,
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

/// Command for [`UpdateOrganizationUnit`].
#[derive(Debug, Clone)]
pub struct UpdateOrganizationUnitCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// New display name.
    pub name: Option<String>,
    /// New kind.
    pub unit_type: Option<OrganizationUnitType>,
    /// New status (active / disabled).
    pub status: Option<OrganizationUnitStatus>,
    /// Optimistic version.
    pub expected_version: i64,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Update unit metadata or status.
pub struct UpdateOrganizationUnit {
    command_port: Arc<dyn OrganizationCommandPort>,
}

impl UpdateOrganizationUnit {
    #[must_use]
    pub fn new(command_port: Arc<dyn OrganizationCommandPort>) -> Self {
        Self { command_port }
    }

    pub async fn execute(
        &self,
        command: UpdateOrganizationUnitCommand,
    ) -> Result<UnitCommitOutcome, OrganizationApplicationError> {
        if command.tenant_id.is_nil() || command.unit_id.is_nil() || command.actor_user_id.is_nil()
        {
            return Err(OrganizationApplicationError::Validation(
                "tenant, unit, and actor ids must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(OrganizationApplicationError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        if command.name.is_none() && command.unit_type.is_none() && command.status.is_none() {
            return Err(OrganizationApplicationError::Validation(
                "at least one field must be updated".to_string(),
            ));
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(OrganizationApplicationError::Validation)?;
        let audit = audit_context(command.actor_user_id, command.reason)?;

        Ok(self
            .command_port
            .update_unit(UpdateUnitCommit {
                tenant_id: command.tenant_id,
                unit_id: command.unit_id,
                name: command.name,
                unit_type: command.unit_type,
                status: command.status,
                expected_version: command.expected_version,
                audit,
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

/// Command for [`MoveOrganizationUnit`].
#[derive(Debug, Clone)]
pub struct MoveOrganizationUnitCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// New parent (`None` = tenant root).
    pub new_parent_id: Option<Uuid>,
    /// Optimistic version.
    pub expected_version: i64,
    /// Requesting actor.
    pub actor_user_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Audit reason.
    pub reason: Option<String>,
}

/// Reparent a unit, rejecting cycles and cross-tenant placement.
pub struct MoveOrganizationUnit {
    command_port: Arc<dyn OrganizationCommandPort>,
    query_port: Arc<dyn OrganizationQueryPort>,
}

impl MoveOrganizationUnit {
    #[must_use]
    pub fn new(
        command_port: Arc<dyn OrganizationCommandPort>,
        query_port: Arc<dyn OrganizationQueryPort>,
    ) -> Self {
        Self {
            command_port,
            query_port,
        }
    }

    pub async fn execute(
        &self,
        command: MoveOrganizationUnitCommand,
    ) -> Result<UnitCommitOutcome, OrganizationApplicationError> {
        if command.tenant_id.is_nil() || command.unit_id.is_nil() || command.actor_user_id.is_nil()
        {
            return Err(OrganizationApplicationError::Validation(
                "tenant, unit, and actor ids must not be nil".to_string(),
            ));
        }
        if command.new_parent_id == Some(command.unit_id) {
            return Err(OrganizationApplicationError::InvalidParent);
        }
        if command.expected_version < 1 {
            return Err(OrganizationApplicationError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(OrganizationApplicationError::Validation)?;
        let audit = audit_context(command.actor_user_id, command.reason)?;

        let units = self.query_port.list_units(command.tenant_id).await?;
        if !units.iter().any(|unit| unit.unit_id() == command.unit_id) {
            return Err(OrganizationApplicationError::NotFound);
        }
        if let Some(parent_id) = command.new_parent_id {
            let parent = units
                .iter()
                .find(|unit| unit.unit_id() == parent_id)
                .ok_or(OrganizationApplicationError::InvalidParent)?;
            if !parent.is_active() {
                return Err(OrganizationApplicationError::UnitDisabled);
            }
        }
        let tree: Vec<(Uuid, Option<Uuid>)> = units
            .iter()
            .map(|unit| (unit.unit_id(), unit.parent_id()))
            .collect();
        validate_tree_placement(&tree, command.unit_id, command.new_parent_id).map_err(
            |error| match error {
                crate::domain::OrganizationDomainError::InvalidIdentity => {
                    OrganizationApplicationError::Cycle
                }
                other => OrganizationApplicationError::Validation(other.to_string()),
            },
        )?;

        Ok(self
            .command_port
            .move_unit(MoveUnitCommit {
                tenant_id: command.tenant_id,
                unit_id: command.unit_id,
                new_parent_id: command.new_parent_id,
                expected_version: command.expected_version,
                audit,
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeOrganizationPorts;
    use std::sync::Arc;

    fn tenant() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn actor() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    #[tokio::test]
    async fn tree_rules_reject_cross_tenant_self_and_cycle_placement() {
        let ports = FakeOrganizationPorts::default();
        let create =
            CreateOrganizationUnit::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
        let root = create
            .execute(CreateOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: None,
                parent_id: None,
                unit_type: OrganizationUnitType::Company,
                name: "Acme".to_string(),
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        let child = create
            .execute(CreateOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: None,
                parent_id: Some(root.unit.unit_id()),
                unit_type: OrganizationUnitType::Department,
                name: "Legal".to_string(),
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());

        // Cross-tenant parent is unknown in this tenant.
        let cross = create
            .execute(CreateOrganizationUnitCommand {
                tenant_id: Uuid::from_bytes([9; 16]),
                parent_id: Some(root.unit.unit_id()),
                unit_type: OrganizationUnitType::Team,
                name: "Aliens".to_string(),
                unit_id: None,
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await;
        assert_eq!(cross, Err(OrganizationApplicationError::InvalidParent));

        // Cycle: move root under its own descendant.
        let move_case =
            MoveOrganizationUnit::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
        let cycle = move_case
            .execute(MoveOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: root.unit.unit_id(),
                new_parent_id: Some(child.unit.unit_id()),
                expected_version: 1,
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await;
        assert_eq!(cycle, Err(OrganizationApplicationError::Cycle));

        // Self-parenting is rejected pre-store.
        let self_parent = move_case
            .execute(MoveOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: root.unit.unit_id(),
                new_parent_id: Some(root.unit.unit_id()),
                expected_version: 1,
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await;
        assert_eq!(
            self_parent,
            Err(OrganizationApplicationError::InvalidParent)
        );
    }

    #[tokio::test]
    async fn disable_unit_then_new_members_are_rejected() {
        let ports = FakeOrganizationPorts::default();
        let create =
            CreateOrganizationUnit::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
        let root = create
            .execute(CreateOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: None,
                parent_id: None,
                unit_type: OrganizationUnitType::Company,
                name: "Acme".to_string(),
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await
            .unwrap_or_else(|_| unreachable!());

        let update = UpdateOrganizationUnit::new(Arc::clone(&ports.command));
        let disabled = update
            .execute(UpdateOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: root.unit.unit_id(),
                name: None,
                unit_type: None,
                status: Some(OrganizationUnitStatus::Disabled),
                expected_version: 1,
                actor_user_id: actor(),
                idempotency_key: None,
                reason: Some("restructuring".to_string()),
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(disabled.unit.status(), OrganizationUnitStatus::Disabled);

        let under_disabled = create
            .execute(CreateOrganizationUnitCommand {
                tenant_id: tenant(),
                unit_id: None,
                parent_id: Some(root.unit.unit_id()),
                unit_type: OrganizationUnitType::Team,
                name: "Nope".to_string(),
                actor_user_id: actor(),
                idempotency_key: None,
                reason: None,
            })
            .await;
        assert_eq!(
            under_disabled,
            Err(OrganizationApplicationError::UnitDisabled)
        );
    }
}
