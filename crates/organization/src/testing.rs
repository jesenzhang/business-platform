//! In-memory fake organization ports for application and API tests.
//!
//! Emulates the database guarantees the real adapters must provide (unique
//! keys, in-transaction parent/cycle re-validation, optimistic versions,
//! idempotent convergence, audit-on-commit) so fake-tested expectations stay
//! honest adapter expectations.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use uuid::Uuid;

use crate::application::MAX_UNITS_PER_TENANT;
use crate::domain::{
    validate_tree_placement, OrganizationMembership, OrganizationMembershipType, OrganizationUnit,
};
use crate::ports::{
    AddMemberCommit, CreateUnitCommit, MemberCommitOutcome, MoveUnitCommit, MutationContext,
    OrganizationCommandPort, OrganizationQueryPort, OrganizationStoreError, RemoveMemberCommit,
    TenantMembershipReader, UnitCommitOutcome, UnitMemberRecord, UpdateUnitCommit,
};

/// A captured fake audit record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeAuditRecord {
    /// Stable audit action, e.g. `organization.unit.created`.
    pub action: String,
    /// Actor identity as recorded.
    pub actor_id: String,
    /// Affected resource id (stringified).
    pub resource_id: String,
}

#[derive(Debug, Clone)]
enum StoredOutcome {
    Unit(OrganizationUnit),
    Member(OrganizationMembership),
}

type MembershipKey = (Uuid, Uuid, Uuid, OrganizationMembershipType);

#[derive(Default)]
struct State {
    units: BTreeMap<Uuid, OrganizationUnit>,
    last_seeded: Option<Uuid>,
    members: HashMap<MembershipKey, OrganizationMembership>,
    active_members: BTreeSet<(Uuid, Uuid)>,
    idempotency: HashMap<(String, String), (String, StoredOutcome)>,
    audits: Vec<FakeAuditRecord>,
    poisoned: bool,
}

/// Shared fake stores for all organization ports.
#[derive(Clone)]
pub struct FakeOrganizationPorts {
    state: Arc<Mutex<State>>,
    /// Command port handle.
    pub command: Arc<dyn OrganizationCommandPort>,
    /// Query port handle.
    pub query: Arc<dyn OrganizationQueryPort>,
    /// Tenant membership reader handle.
    pub reader: Arc<dyn TenantMembershipReader>,
}

impl Default for FakeOrganizationPorts {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeOrganizationPorts {
    #[must_use]
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        Self {
            command: Arc::new(FakeCommand {
                state: Arc::clone(&state),
            }),
            query: Arc::new(FakeQuery {
                state: Arc::clone(&state),
            }),
            reader: Arc::new(FakeReader {
                state: Arc::clone(&state),
            }),
            state,
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, OrganizationStoreError> {
        self.state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)
    }

    /// Seed a unit directly (test setup).
    pub fn seed_unit(&self, unit: OrganizationUnit) {
        if let Ok(mut state) = self.lock() {
            state.last_seeded = Some(unit.unit_id());
            state.units.insert(unit.unit_id(), unit);
        }
    }

    /// Unit id of the most recently seeded unit.
    #[must_use]
    pub fn seeded_unit_id(&self) -> Uuid {
        self.lock()
            .ok()
            .and_then(|state| state.last_seeded)
            .unwrap_or_else(Uuid::nil)
    }

    /// Mark a (tenant, user) pair as an active tenant member.
    pub fn set_active_member(&self, tenant_id: Uuid, user_id: Uuid) {
        if let Ok(mut state) = self.lock() {
            state.active_members.insert((tenant_id, user_id));
        }
    }

    /// Remove active tenant membership.
    pub fn clear_active_member(&self, tenant_id: Uuid, user_id: Uuid) {
        if let Ok(mut state) = self.lock() {
            state.active_members.remove(&(tenant_id, user_id));
        }
    }

    /// All audit records captured so far, in commit order.
    #[must_use]
    pub fn audit_records(&self) -> Vec<FakeAuditRecord> {
        self.lock()
            .map(|state| state.audits.clone())
            .unwrap_or_default()
    }

    /// Simulate a store failure.
    pub fn poison(&self) {
        if let Ok(mut state) = self.lock() {
            state.poisoned = true;
        }
    }
}

fn check_poisoned(state: &State) -> Result<(), OrganizationStoreError> {
    if state.poisoned {
        return Err(OrganizationStoreError::Unavailable);
    }
    Ok(())
}

fn record_audit(state: &mut State, action: &str, audit: &MutationContext, resource_id: String) {
    state.audits.push(FakeAuditRecord {
        action: action.to_string(),
        actor_id: audit.actor_id.clone(),
        resource_id,
    });
}

fn tree_snapshot(state: &State, tenant_id: Uuid) -> Vec<(Uuid, Option<Uuid>)> {
    state
        .units
        .values()
        .filter(|unit| unit.tenant_id() == tenant_id)
        .map(|unit| (unit.unit_id(), unit.parent_id()))
        .collect()
}

/// Parent must exist, be in the same tenant, and be active (the store
/// re-validates this inside the write transaction).
fn validate_parent_locked(
    state: &State,
    tenant_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<(), OrganizationStoreError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    match state.units.get(&parent_id) {
        Some(parent) if parent.tenant_id() == tenant_id && parent.is_active() => Ok(()),
        Some(_) | None => Err(OrganizationStoreError::InvalidParent),
    }
}

fn placement_error(parent_id: Option<Uuid>, placed_unit: Uuid) -> OrganizationStoreError {
    if parent_id == Some(placed_unit) {
        OrganizationStoreError::InvalidParent
    } else {
        OrganizationStoreError::Cycle
    }
}

fn idempotent_replay(
    state: &State,
    op: &str,
    key: Option<&String>,
    fingerprint: &str,
) -> Result<Option<StoredOutcome>, OrganizationStoreError> {
    let Some(key) = key else {
        return Ok(None);
    };
    match state.idempotency.get(&(op.to_string(), key.clone())) {
        Some((stored, outcome)) => {
            if stored == fingerprint {
                Ok(Some(outcome.clone()))
            } else {
                Err(OrganizationStoreError::IdempotencyConflict)
            }
        }
        None => Ok(None),
    }
}

fn store_idempotent(
    state: &mut State,
    op: &str,
    key: Option<&String>,
    fingerprint: String,
    outcome: StoredOutcome,
) {
    if let Some(key) = key {
        state
            .idempotency
            .insert((op.to_string(), key.clone()), (fingerprint, outcome));
    }
}

struct FakeCommand {
    state: Arc<Mutex<State>>,
}

impl FakeCommand {
    fn create_unit_inner(
        &self,
        command: &CreateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("create_unit|{}", command.tenant_id);
        // Payload fingerprint: placement fields plus the caller-chosen unit
        // id when present; a server-generated id never participates (retry
        // convergence for auto ids).
        let fingerprint = format!(
            "{}|{}|{}|{}",
            command
                .parent_id
                .map_or_else(|| "root".to_string(), |id| id.to_string()),
            command.unit_type.as_str(),
            command.name,
            command
                .unit_id
                .map_or_else(|| "auto".to_string(), |id| id.to_string()),
        );
        if let Some(StoredOutcome::Unit(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(UnitCommitOutcome {
                unit: existing,
                replayed: true,
            });
        }
        let unit_id = command.unit_id.unwrap_or_else(Uuid::now_v7);
        if state.units.contains_key(&unit_id) {
            return Err(OrganizationStoreError::AlreadyExists);
        }
        if state
            .units
            .values()
            .filter(|unit| unit.tenant_id() == command.tenant_id)
            .count()
            >= MAX_UNITS_PER_TENANT
        {
            return Err(OrganizationStoreError::TooManyResources);
        }
        // In-transaction placement re-validation.
        validate_parent_locked(&state, command.tenant_id, command.parent_id)?;
        let snapshot = tree_snapshot(&state, command.tenant_id);
        validate_tree_placement(&snapshot, unit_id, command.parent_id)
            .map_err(|_| placement_error(command.parent_id, unit_id))?;
        let unit = OrganizationUnit::create(
            unit_id,
            command.tenant_id,
            command.parent_id,
            command.unit_type,
            &command.name,
            command.now,
        )
        .map_err(|_| OrganizationStoreError::Failed)?;
        state.units.insert(unit.unit_id(), unit.clone());
        record_audit(
            &mut state,
            "organization.unit.created",
            &command.audit,
            unit.unit_id().to_string(),
        );
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Unit(unit.clone()),
        );
        Ok(UnitCommitOutcome {
            unit,
            replayed: false,
        })
    }

    fn update_unit_inner(
        &self,
        command: &UpdateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("update_unit|{}", command.tenant_id);
        // Payload fingerprint: semantic fields plus the request's
        // `expected_version` (versioned mutation bodies include it).
        let fingerprint = format!(
            "{}|{}|{}|{}|{}",
            command.unit_id,
            command.name.clone().unwrap_or_default(),
            command
                .unit_type
                .map_or(String::new(), |kind| kind.as_str().to_string()),
            command
                .status
                .map_or(String::new(), |status| status.as_str().to_string()),
            command.expected_version
        );
        if let Some(StoredOutcome::Unit(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(UnitCommitOutcome {
                unit: existing,
                replayed: true,
            });
        }
        let mut unit = state
            .units
            .get(&command.unit_id)
            .filter(|unit| unit.tenant_id() == command.tenant_id)
            .cloned()
            .ok_or(OrganizationStoreError::NotFound)?;
        if unit.version().value() != command.expected_version {
            return Err(OrganizationStoreError::VersionConflict);
        }
        // Semantic diff: supplied fields that actually differ. Identical
        // values converge (no version bump, no audit) — identity's rule.
        let changed = command
            .name
            .as_deref()
            .is_some_and(|name| unit.name() != name.trim())
            || command
                .unit_type
                .is_some_and(|kind| unit.unit_type() != kind)
            || command.status.is_some_and(|status| unit.status() != status);
        if changed {
            unit.update(
                command.name.as_deref(),
                command.unit_type,
                command.status,
                command.now,
            )
            .map_err(|_| OrganizationStoreError::Failed)?;
            state.units.insert(unit.unit_id(), unit.clone());
            record_audit(
                &mut state,
                "organization.unit.updated",
                &command.audit,
                unit.unit_id().to_string(),
            );
        }
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Unit(unit.clone()),
        );
        Ok(UnitCommitOutcome {
            unit,
            replayed: !changed,
        })
    }

    fn move_unit_inner(
        &self,
        command: &MoveUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("move_unit|{}", command.tenant_id);
        let fingerprint = format!(
            "{}|{}|{}",
            command.unit_id,
            command
                .new_parent_id
                .map_or_else(|| "root".to_string(), |id| id.to_string()),
            command.expected_version
        );
        if let Some(StoredOutcome::Unit(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(UnitCommitOutcome {
                unit: existing,
                replayed: true,
            });
        }
        let mut unit = state
            .units
            .get(&command.unit_id)
            .filter(|unit| unit.tenant_id() == command.tenant_id)
            .cloned()
            .ok_or(OrganizationStoreError::NotFound)?;
        if command.new_parent_id == Some(command.unit_id) {
            return Err(OrganizationStoreError::InvalidParent);
        }
        validate_parent_locked(&state, command.tenant_id, command.new_parent_id)?;
        let snapshot = tree_snapshot(&state, command.tenant_id);
        validate_tree_placement(&snapshot, command.unit_id, command.new_parent_id)
            .map_err(|_| placement_error(command.new_parent_id, command.unit_id))?;
        if unit.version().value() != command.expected_version {
            return Err(OrganizationStoreError::VersionConflict);
        }
        if unit.parent_id() == command.new_parent_id {
            return Ok(UnitCommitOutcome {
                unit,
                replayed: true,
            });
        }
        unit.reparent(command.new_parent_id, command.now)
            .map_err(|_| OrganizationStoreError::Failed)?;
        state.units.insert(unit.unit_id(), unit.clone());
        record_audit(
            &mut state,
            "organization.unit.moved",
            &command.audit,
            unit.unit_id().to_string(),
        );
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Unit(unit.clone()),
        );
        Ok(UnitCommitOutcome {
            unit,
            replayed: false,
        })
    }

    fn add_member_inner(
        &self,
        command: &AddMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("add_member|{}", command.tenant_id);
        let fingerprint = format!(
            "{}|{}|{}",
            command.unit_id,
            command.user_id,
            command.membership_type.as_str()
        );
        if let Some(StoredOutcome::Member(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(MemberCommitOutcome {
                membership: existing,
                replayed: true,
            });
        }
        let unit = state
            .units
            .get(&command.unit_id)
            .filter(|unit| unit.tenant_id() == command.tenant_id)
            .ok_or(OrganizationStoreError::NotFound)?;
        if !unit.is_active() {
            return Err(OrganizationStoreError::UnitDisabled);
        }
        let key = (
            command.tenant_id,
            command.user_id,
            command.unit_id,
            command.membership_type,
        );
        let membership = match state.members.get(&key).cloned() {
            Some(mut existing) if !existing.is_active() => {
                existing
                    .reactivate(command.now)
                    .map_err(|_| OrganizationStoreError::Failed)?;
                record_audit(
                    &mut state,
                    "organization.member.reactivated",
                    &command.audit,
                    existing.membership_id().to_string(),
                );
                existing
            }
            Some(_) => return Err(OrganizationStoreError::AlreadyExists),
            None => {
                let joined = OrganizationMembership::join(
                    Uuid::now_v7(),
                    command.tenant_id,
                    command.user_id,
                    command.unit_id,
                    command.membership_type,
                    command.now,
                )
                .map_err(|_| OrganizationStoreError::Failed)?;
                record_audit(
                    &mut state,
                    "organization.member.added",
                    &command.audit,
                    joined.membership_id().to_string(),
                );
                joined
            }
        };
        state.members.insert(key, membership.clone());
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Member(membership.clone()),
        );
        Ok(MemberCommitOutcome {
            membership,
            replayed: false,
        })
    }

    fn remove_member_inner(
        &self,
        command: &RemoveMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("remove_member|{}", command.tenant_id);
        let fingerprint = format!(
            "{}|{}|{}|{}",
            command.unit_id,
            command.user_id,
            command.membership_type.as_str(),
            command.expected_version
        );
        if let Some(StoredOutcome::Member(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(MemberCommitOutcome {
                membership: existing,
                replayed: true,
            });
        }
        let key = (
            command.tenant_id,
            command.user_id,
            command.unit_id,
            command.membership_type,
        );
        let mut membership = state
            .members
            .get(&key)
            .cloned()
            .ok_or(OrganizationStoreError::NotFound)?;
        if membership.version().value() != command.expected_version {
            return Err(OrganizationStoreError::VersionConflict);
        }
        if !membership.is_active() {
            return Ok(MemberCommitOutcome {
                membership,
                replayed: true,
            });
        }
        membership
            .deactivate(command.now)
            .map_err(|_| OrganizationStoreError::Failed)?;
        state.members.insert(key, membership.clone());
        record_audit(
            &mut state,
            "organization.member.removed",
            &command.audit,
            membership.membership_id().to_string(),
        );
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Member(membership.clone()),
        );
        Ok(MemberCommitOutcome {
            membership,
            replayed: false,
        })
    }
}

#[async_trait]
impl OrganizationCommandPort for FakeCommand {
    async fn create_unit(
        &self,
        command: CreateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        self.create_unit_inner(&command)
    }

    async fn update_unit(
        &self,
        command: UpdateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        self.update_unit_inner(&command)
    }

    async fn move_unit(
        &self,
        command: MoveUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        self.move_unit_inner(&command)
    }

    async fn add_member(
        &self,
        command: AddMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError> {
        self.add_member_inner(&command)
    }

    async fn remove_member(
        &self,
        command: RemoveMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError> {
        self.remove_member_inner(&command)
    }
}

struct FakeQuery {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl OrganizationQueryPort for FakeQuery {
    async fn get_unit(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Option<OrganizationUnit>, OrganizationStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state
            .units
            .get(&unit_id)
            .filter(|unit| unit.tenant_id() == tenant_id)
            .cloned())
    }

    async fn list_units(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<OrganizationUnit>, OrganizationStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut units: Vec<OrganizationUnit> = state
            .units
            .values()
            .filter(|unit| unit.tenant_id() == tenant_id)
            .cloned()
            .collect();
        if units.len() > MAX_UNITS_PER_TENANT {
            return Err(OrganizationStoreError::TooManyResources);
        }
        // Deterministic order matching the adapter's created_at ordering.
        units.sort_by_key(|unit| (unit.created_at(), unit.unit_id()));
        Ok(units)
    }

    async fn list_unit_members(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Vec<UnitMemberRecord>, OrganizationStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut memberships: Vec<&OrganizationMembership> = state
            .members
            .values()
            .filter(|m| m.tenant_id() == tenant_id && m.unit_id() == unit_id && m.is_active())
            .collect();
        memberships.sort_by_key(|membership| membership.joined_at());
        Ok(memberships
            .into_iter()
            .map(|membership| UnitMemberRecord {
                membership: membership.clone(),
            })
            .collect())
    }

    async fn list_user_memberships(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut memberships: Vec<OrganizationMembership> = state
            .members
            .values()
            .filter(|m| m.tenant_id() == tenant_id && m.user_id() == user_id && m.is_active())
            .cloned()
            .collect();
        memberships.sort_by_key(OrganizationMembership::joined_at);
        Ok(memberships)
    }
}

struct FakeReader {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl TenantMembershipReader for FakeReader {
    async fn is_active_member(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, OrganizationStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state.active_members.contains(&(tenant_id, user_id)))
    }
}
