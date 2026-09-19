//! In-memory fake policy ports for application and API tests.
//!
//! Emulates the database guarantees the real adapters must provide (unique
//! keys, system-role immutability, tenant-or-system role visibility,
//! transactional set replace, optimistic versions, idempotent convergence,
//! audit-on-commit). The catalog and the two system roles are seeded from
//! [`crate::catalog`], exactly like the Stage-5 migration seeds — drift
//! between code and migration fixtures is structurally impossible.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::application::{
    MAX_BINDINGS_PER_TENANT, MAX_BINDINGS_PER_USER, MAX_ROLES_PER_TENANT, MAX_ROLE_PERMISSIONS,
};
use crate::catalog;
use crate::domain::{
    PermissionDefinition, PermissionKey, RehydrateRoleDefinition, ResourceScope, RoleBinding,
    RoleDefinition, RoleStatus, ValidityWindow,
};
use crate::ports::{
    BindRoleCommit, BindingCommitOutcome, CreateRoleCommit, MutationContext, OrganizationScopePort,
    PermissionsCommitOutcome, PolicyCommandPort, PolicyQueryPort, PolicyStoreError,
    RevokeBindingCommit, RoleCommitOutcome, SetRolePermissionsCommit, SubjectStatus,
    SubjectStatusPort, UpdateRoleCommit,
};

/// A captured fake audit record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeAuditRecord {
    /// Stable audit action, e.g. `policy.role.created`.
    pub action: String,
    /// Actor identity as recorded.
    pub actor_id: String,
    /// Affected resource id (stringified).
    pub resource_id: String,
}

#[derive(Debug, Clone)]
enum StoredOutcome {
    Role(RoleDefinition),
    Binding(RoleBinding),
}

/// Fault-injection bits. `FAULT_POISON` kills every port call; the rest
/// are one-shot per-method failures so tests can pin the exact
/// mid-evaluation store-failure paths (global poison short-circuits at
/// the first port call and would mask everything behind it).
const FAULT_POISON: u32 = 0b0_0001;
const FAULT_LIST_BINDINGS: u32 = 0b0_0010;
const FAULT_GET_ROLE: u32 = 0b0_0100;
const FAULT_GET_ROLE_PERMISSIONS: u32 = 0b0_1000;
const FAULT_SUBTREE_CONTAINS: u32 = 0b1_0000;

#[derive(Default)]
struct State {
    permissions: BTreeMap<String, PermissionDefinition>,
    roles: BTreeMap<Uuid, RoleDefinition>,
    role_permissions: HashMap<Uuid, BTreeSet<String>>,
    bindings: BTreeMap<Uuid, RoleBinding>,
    subjects: HashMap<(Uuid, Uuid), SubjectStatus>,
    units: HashMap<Uuid, FakeUnit>,
    idempotency: HashMap<(String, String), (String, StoredOutcome)>,
    audits: Vec<FakeAuditRecord>,
    faults: u32,
}

fn take_fault(state: &mut State, bit: u32) -> Result<(), PolicyStoreError> {
    let hit = state.faults & bit != 0;
    state.faults &= !bit;
    if hit {
        return Err(PolicyStoreError::Unavailable);
    }
    Ok(())
}

struct FakeUnit {
    tenant_id: Uuid,
    parent_id: Option<Uuid>,
    active: bool,
}

/// Shared fake stores for all policy and bridge ports.
#[derive(Clone)]
pub struct FakePolicyPorts {
    state: Arc<Mutex<State>>,
    /// Command port handle.
    pub command: Arc<dyn PolicyCommandPort>,
    /// Query port handle.
    pub query: Arc<dyn PolicyQueryPort>,
    /// Subject status bridge handle.
    pub subject: Arc<dyn SubjectStatusPort>,
    /// Organization scope bridge handle.
    pub org: Arc<dyn OrganizationScopePort>,
}

impl Default for FakePolicyPorts {
    fn default() -> Self {
        Self::new()
    }
}

impl FakePolicyPorts {
    #[must_use]
    pub fn new() -> Self {
        let mut state = State::default();
        seed_catalog(&mut state);
        let state = Arc::new(Mutex::new(state));
        Self {
            command: Arc::new(FakeCommand {
                state: Arc::clone(&state),
            }),
            query: Arc::new(FakeQuery {
                state: Arc::clone(&state),
            }),
            subject: Arc::new(FakeSubject {
                state: Arc::clone(&state),
            }),
            org: Arc::new(FakeOrg {
                state: Arc::clone(&state),
            }),
            state,
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, PolicyStoreError> {
        self.state.lock().map_err(|_| PolicyStoreError::Failed)
    }

    /// Seed the catalog + system roles from the shared `catalog` module
    /// (the same source the Stage-5 migration consumes).
    pub fn seed_catalog(&self) {
        if let Ok(mut state) = self.lock() {
            seed_catalog(&mut state);
        }
    }

    /// Set the subject status the identity bridge will report.
    pub fn set_subject(&self, tenant_id: Uuid, user_id: Uuid, status: SubjectStatus) {
        if let Ok(mut state) = self.lock() {
            state.subjects.insert((tenant_id, user_id), status);
        }
    }

    /// Register an organization unit for scope resolution.
    pub fn set_unit(&self, tenant_id: Uuid, unit_id: Uuid, parent_id: Option<Uuid>, active: bool) {
        if let Ok(mut state) = self.lock() {
            state.units.insert(
                unit_id,
                FakeUnit {
                    tenant_id,
                    parent_id,
                    active,
                },
            );
        }
    }

    /// Seed a role aggregate directly (system roles in tests arrive only
    /// through seeding, mirroring migration-only creation).
    pub fn seed_role(&self, role: RoleDefinition, permission_keys: &[&str]) {
        if let Ok(mut state) = self.lock() {
            let role_id = role.role_id();
            state.roles.insert(role_id, role);
            state.role_permissions.insert(
                role_id,
                permission_keys
                    .iter()
                    .map(|key| (*key).to_string())
                    .collect(),
            );
        }
    }

    /// Seed a binding directly.
    pub fn seed_binding(&self, binding: RoleBinding) {
        if let Ok(mut state) = self.lock() {
            state.bindings.insert(binding.binding_id(), binding);
        }
    }

    /// All audit records captured so far, in commit order.
    #[must_use]
    pub fn audit_records(&self) -> Vec<FakeAuditRecord> {
        self.lock()
            .map(|state| state.audits.clone())
            .unwrap_or_default()
    }

    /// Simulate a store failure on every port.
    pub fn poison(&self) {
        if let Ok(mut state) = self.lock() {
            state.faults |= FAULT_POISON;
        }
    }

    /// Fail the next `list_bindings_for_user` call exactly once.
    pub fn fail_next_list_bindings(&self) {
        if let Ok(mut state) = self.lock() {
            state.faults |= FAULT_LIST_BINDINGS;
        }
    }

    /// Fail the next `get_role` call exactly once.
    pub fn fail_next_get_role(&self) {
        if let Ok(mut state) = self.lock() {
            state.faults |= FAULT_GET_ROLE;
        }
    }

    /// Fail the next `get_role_permissions` call exactly once.
    pub fn fail_next_get_role_permissions(&self) {
        if let Ok(mut state) = self.lock() {
            state.faults |= FAULT_GET_ROLE_PERMISSIONS;
        }
    }

    /// Fail the next `subtree_contains` call exactly once.
    pub fn fail_next_subtree_contains(&self) {
        if let Ok(mut state) = self.lock() {
            state.faults |= FAULT_SUBTREE_CONTAINS;
        }
    }

    /// Retire a catalog permission (`active = false`): it evaluates as
    /// unknown from then on, without losing audit-resolvable rows.
    pub fn retire_permission(&self, key: &str) {
        if let Ok(mut state) = self.lock() {
            if let Some(definition) = state.permissions.get(key) {
                let retired = PermissionDefinition::restored(
                    definition.key().clone(),
                    definition.description().to_string(),
                    definition.is_reserved(),
                    false,
                );
                state.permissions.insert(key.to_string(), retired);
            }
        }
    }
}

fn seed_catalog(state: &mut State) {
    for (key, description, reserved) in catalog::catalog() {
        let Ok(parsed) = PermissionKey::parse(key) else {
            continue;
        };
        if let Ok(definition) = PermissionDefinition::new(parsed, description, reserved) {
            state.permissions.insert(key.to_string(), definition);
        }
    }
    for stable_key in [
        catalog::SYSTEM_BOOTSTRAP_ADMIN_KEY,
        catalog::SYSTEM_PLATFORM_ADMIN_KEY,
    ] {
        let already = state
            .roles
            .values()
            .any(|role| role.is_system() && role.stable_key() == stable_key);
        if already {
            continue;
        }
        let role_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("policy-system-role:{stable_key}").as_bytes(),
        );
        if let Ok(role) = RoleDefinition::rehydrate(RehydrateRoleDefinition {
            role_id,
            tenant_id: None,
            stable_key: stable_key.to_string(),
            display_name: if stable_key.ends_with("bootstrap-admin") {
                "Bootstrap Admin".to_string()
            } else {
                "Platform Admin".to_string()
            },
            status: RoleStatus::Active,
            system: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        }) {
            state.roles.insert(role_id, role);
            state
                .role_permissions
                .insert(role_id, catalog::system_role_permissions());
        }
    }
}

fn check_poisoned(state: &State) -> Result<(), PolicyStoreError> {
    if state.faults & FAULT_POISON != 0 {
        return Err(PolicyStoreError::Unavailable);
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

/// Role visibility rule: tenant-owned or system.
fn visible_role(state: &State, tenant_id: Uuid, role_id: Uuid) -> Option<RoleDefinition> {
    state
        .roles
        .get(&role_id)
        .filter(|role| role.tenant_id() == Some(tenant_id) || role.is_system())
        .cloned()
}

fn idempotent_replay(
    state: &State,
    op: &str,
    key: Option<&String>,
    fingerprint: &str,
) -> Result<Option<StoredOutcome>, PolicyStoreError> {
    let Some(key) = key else {
        return Ok(None);
    };
    match state.idempotency.get(&(op.to_string(), key.clone())) {
        Some((stored, outcome)) => {
            if stored == fingerprint {
                Ok(Some(outcome.clone()))
            } else {
                Err(PolicyStoreError::IdempotencyConflict)
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

fn scope_fingerprint(scope: &ResourceScope) -> String {
    match scope {
        ResourceScope::Tenant => "tenant".to_string(),
        ResourceScope::OrganizationUnit {
            org_unit_id,
            include_subtree,
        } => format!("org:{org_unit_id}:{include_subtree}"),
        ResourceScope::ResourceType { kind } => format!("kind:{kind}"),
        ResourceScope::Resource { kind, resource_id } => format!("res:{kind}:{resource_id}"),
    }
}

struct FakeCommand {
    state: Arc<Mutex<State>>,
}

impl FakeCommand {
    fn create_role_inner(
        &self,
        command: &CreateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("create_role|{}", command.tenant_id);
        let fingerprint = format!(
            "{}|{}|{}",
            command
                .role_id
                .map_or_else(|| "auto".to_string(), |id| id.to_string()),
            command.stable_key,
            command.display_name
        );
        if let Some(StoredOutcome::Role(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(RoleCommitOutcome {
                role: existing,
                replayed: true,
            });
        }
        let role_id = command.role_id.unwrap_or_else(Uuid::now_v7);
        if state.roles.contains_key(&role_id) {
            return Err(PolicyStoreError::AlreadyExists);
        }
        let duplicate_key = state.roles.values().any(|role| {
            role.tenant_id() == Some(command.tenant_id) && role.stable_key() == command.stable_key
        });
        if duplicate_key {
            return Err(PolicyStoreError::AlreadyExists);
        }
        // Authoritative tenant role cap: tenant-owned rows only (system
        // roles are global and never consume a tenant's budget).
        if state
            .roles
            .values()
            .filter(|role| role.tenant_id() == Some(command.tenant_id))
            .count()
            >= MAX_ROLES_PER_TENANT
        {
            return Err(PolicyStoreError::TooManyResources);
        }
        let role = RoleDefinition::create_tenant_role(
            role_id,
            command.tenant_id,
            &command.stable_key,
            &command.display_name,
            command.now,
        )
        .map_err(|_| PolicyStoreError::Failed)?;
        state.roles.insert(role_id, role.clone());
        state.role_permissions.insert(role_id, BTreeSet::new());
        record_audit(
            &mut state,
            "policy.role.created",
            &command.audit,
            role_id.to_string(),
        );
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Role(role.clone()),
        );
        Ok(RoleCommitOutcome {
            role,
            replayed: false,
        })
    }

    fn update_role_inner(
        &self,
        command: &UpdateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("update_role|{}", command.tenant_id);
        let fingerprint = format!(
            "{}|{}|{}|{}",
            command.role_id,
            command.display_name.clone().unwrap_or_default(),
            command
                .status
                .map_or(String::new(), |status| status.as_str().to_string()),
            command.expected_version
        );
        if let Some(StoredOutcome::Role(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(RoleCommitOutcome {
                role: existing,
                replayed: true,
            });
        }
        let mut role = visible_role(&state, command.tenant_id, command.role_id)
            .ok_or(PolicyStoreError::NotFound)?;
        if role.is_system() {
            return Err(PolicyStoreError::RoleImmutable);
        }
        if role.version().value() != command.expected_version {
            return Err(PolicyStoreError::VersionConflict);
        }
        let changed = command
            .display_name
            .as_deref()
            .is_some_and(|name| role.display_name() != name.trim())
            || command.status.is_some_and(|status| role.status() != status);
        if changed {
            role.update(command.display_name.as_deref(), command.status, command.now)
                .map_err(|_| PolicyStoreError::Failed)?;
            state.roles.insert(role.role_id(), role.clone());
            record_audit(
                &mut state,
                "policy.role.updated",
                &command.audit,
                role.role_id().to_string(),
            );
        }
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Role(role.clone()),
        );
        Ok(RoleCommitOutcome {
            role,
            replayed: !changed,
        })
    }

    fn set_permissions_inner(
        &self,
        command: &SetRolePermissionsCommit,
    ) -> Result<PermissionsCommitOutcome, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("set_role_permissions|{}", command.tenant_id);
        let mut sorted = command.permission_keys.clone();
        sorted.sort();
        let fingerprint = format!(
            "{}|{}|{}",
            command.role_id,
            sorted.join(","),
            command.expected_version
        );
        if let Some(StoredOutcome::Role(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            let keys = state
                .role_permissions
                .get(&command.role_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            return Ok(PermissionsCommitOutcome {
                role: existing,
                permission_keys: keys,
                replayed: true,
            });
        }
        let mut role = visible_role(&state, command.tenant_id, command.role_id)
            .ok_or(PolicyStoreError::NotFound)?;
        if role.is_system() {
            return Err(PolicyStoreError::RoleImmutable);
        }
        if role.version().value() != command.expected_version {
            return Err(PolicyStoreError::VersionConflict);
        }
        if command.permission_keys.len() > MAX_ROLE_PERMISSIONS {
            return Err(PolicyStoreError::TooManyPermissions);
        }
        for key in &command.permission_keys {
            if !state.permissions.contains_key(key) {
                return Err(PolicyStoreError::UnknownPermission);
            }
        }
        let new_set: BTreeSet<String> = command.permission_keys.iter().cloned().collect();
        let same_set = state
            .role_permissions
            .get(&command.role_id)
            .is_some_and(|current| current == &new_set);
        if !same_set {
            role.record_permissions_replaced(command.now)
                .map_err(|_| PolicyStoreError::Failed)?;
            state.roles.insert(role.role_id(), role.clone());
            state
                .role_permissions
                .insert(command.role_id, new_set.clone());
            record_audit(
                &mut state,
                "policy.role.permissions_updated",
                &command.audit,
                role.role_id().to_string(),
            );
        }
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Role(role.clone()),
        );
        Ok(PermissionsCommitOutcome {
            role,
            permission_keys: new_set.into_iter().collect(),
            replayed: same_set,
        })
    }

    fn bind_role_inner(
        &self,
        command: &BindRoleCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("bind_role|{}", command.tenant_id);
        let fingerprint = format!(
            "{}|{}|{}|{}|{}|{}",
            command.user_id,
            command.role_id,
            scope_fingerprint(&command.scope),
            command.effective_at.to_rfc3339(),
            command
                .expires_at
                .map_or_else(|| "open".to_string(), |at| at.to_rfc3339()),
            command
                .binding_id
                .map_or_else(|| "auto".to_string(), |id| id.to_string()),
        );
        if let Some(StoredOutcome::Binding(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(BindingCommitOutcome {
                binding: existing,
                replayed: true,
            });
        }
        // Role visibility inside the write transaction.
        if visible_role(&state, command.tenant_id, command.role_id).is_none() {
            return Err(PolicyStoreError::NotFound);
        }
        // Identical active binding convergence (no duplicate spam).
        if let Some(existing) = state.bindings.values().find(|binding| {
            binding.tenant_id() == command.tenant_id
                && binding.user_id() == command.user_id
                && binding.role_id() == command.role_id
                && binding.scope() == &command.scope
                && binding.effective_at() == command.effective_at
                && binding.expires_at() == command.expires_at
                && binding.is_active()
        }) {
            return Ok(BindingCommitOutcome {
                binding: existing.clone(),
                replayed: true,
            });
        }
        // Authoritative caps (evaluated after convergence so idempotent
        // replays never trip them): per-user active bindings, and total
        // binding rows in the tenant (all statuses — rows are retained).
        if state
            .bindings
            .values()
            .filter(|binding| binding.tenant_id() == command.tenant_id)
            .count()
            >= MAX_BINDINGS_PER_TENANT
        {
            return Err(PolicyStoreError::TooManyResources);
        }
        if state
            .bindings
            .values()
            .filter(|binding| {
                binding.tenant_id() == command.tenant_id
                    && binding.user_id() == command.user_id
                    && binding.is_active()
            })
            .count()
            >= MAX_BINDINGS_PER_USER
        {
            return Err(PolicyStoreError::TooManyResources);
        }
        let binding_id = command.binding_id.unwrap_or_else(Uuid::now_v7);
        if state.bindings.contains_key(&binding_id) {
            return Err(PolicyStoreError::AlreadyExists);
        }
        let binding = RoleBinding::create(
            binding_id,
            command.tenant_id,
            command.user_id,
            command.role_id,
            command.scope.clone(),
            ValidityWindow {
                effective_at: command.effective_at,
                expires_at: command.expires_at,
            },
            command.now,
        )
        .map_err(|_| PolicyStoreError::Failed)?;
        state.bindings.insert(binding_id, binding.clone());
        record_audit(
            &mut state,
            "policy.binding.created",
            &command.audit,
            binding_id.to_string(),
        );
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Binding(binding.clone()),
        );
        Ok(BindingCommitOutcome {
            binding,
            replayed: false,
        })
    }

    fn revoke_binding_inner(
        &self,
        command: &RevokeBindingCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let op = format!("revoke_binding|{}", command.tenant_id);
        let fingerprint = format!("{}|{}", command.binding_id, command.expected_version);
        if let Some(StoredOutcome::Binding(existing)) =
            idempotent_replay(&state, &op, command.idempotency_key.as_ref(), &fingerprint)?
        {
            return Ok(BindingCommitOutcome {
                binding: existing,
                replayed: true,
            });
        }
        let mut binding = state
            .bindings
            .get(&command.binding_id)
            .filter(|binding| binding.tenant_id() == command.tenant_id)
            .cloned()
            .ok_or(PolicyStoreError::NotFound)?;
        if binding.version().value() != command.expected_version {
            return Err(PolicyStoreError::VersionConflict);
        }
        if !binding.is_active() {
            return Ok(BindingCommitOutcome {
                binding,
                replayed: true,
            });
        }
        binding
            .revoke(command.now)
            .map_err(|_| PolicyStoreError::Failed)?;
        state.bindings.insert(binding.binding_id(), binding.clone());
        record_audit(
            &mut state,
            "policy.binding.revoked",
            &command.audit,
            binding.binding_id().to_string(),
        );
        store_idempotent(
            &mut state,
            &op,
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Binding(binding.clone()),
        );
        Ok(BindingCommitOutcome {
            binding,
            replayed: false,
        })
    }
}

#[async_trait]
impl PolicyCommandPort for FakeCommand {
    async fn create_role(
        &self,
        command: CreateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError> {
        self.create_role_inner(&command)
    }

    async fn update_role(
        &self,
        command: UpdateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError> {
        self.update_role_inner(&command)
    }

    async fn set_role_permissions(
        &self,
        command: SetRolePermissionsCommit,
    ) -> Result<PermissionsCommitOutcome, PolicyStoreError> {
        self.set_permissions_inner(&command)
    }

    async fn bind_role(
        &self,
        command: BindRoleCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError> {
        self.bind_role_inner(&command)
    }

    async fn revoke_binding(
        &self,
        command: RevokeBindingCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError> {
        self.revoke_binding_inner(&command)
    }
}

struct FakeQuery {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl PolicyQueryPort for FakeQuery {
    async fn get_permission(
        &self,
        key: &PermissionKey,
    ) -> Result<Option<PermissionDefinition>, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state.permissions.get(key.as_str()).cloned())
    }

    async fn list_permissions(&self) -> Result<Vec<PermissionDefinition>, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state.permissions.values().cloned().collect())
    }

    async fn get_role(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<RoleDefinition>, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        take_fault(&mut state, FAULT_GET_ROLE)?;
        Ok(visible_role(&state, tenant_id, role_id))
    }

    async fn list_roles(&self, tenant_id: Uuid) -> Result<Vec<RoleDefinition>, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut roles: Vec<RoleDefinition> = state
            .roles
            .values()
            .filter(|role| role.is_system() || role.tenant_id() == Some(tenant_id))
            .cloned()
            .collect();
        roles.sort_by(|a, b| a.stable_key().cmp(b.stable_key()));
        Ok(roles)
    }

    async fn get_role_permissions(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Vec<String>, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        take_fault(&mut state, FAULT_GET_ROLE_PERMISSIONS)?;
        if visible_role(&state, tenant_id, role_id).is_none() {
            return Err(PolicyStoreError::NotFound);
        }
        Ok(state
            .role_permissions
            .get(&role_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect())
    }

    async fn list_bindings_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        take_fault(&mut state, FAULT_LIST_BINDINGS)?;
        let mut bindings: Vec<RoleBinding> = state
            .bindings
            .values()
            .filter(|binding| binding.tenant_id() == tenant_id && binding.user_id() == user_id)
            .cloned()
            .collect();
        bindings.sort_by_key(RoleBinding::binding_id);
        Ok(bindings)
    }

    async fn list_active_bindings_for_role(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut bindings: Vec<RoleBinding> = state
            .bindings
            .values()
            .filter(|binding| {
                binding.tenant_id() == tenant_id
                    && binding.role_id() == role_id
                    && binding.is_active()
            })
            .cloned()
            .collect();
        bindings.sort_by_key(RoleBinding::binding_id);
        Ok(bindings)
    }

    async fn list_bindings(
        &self,
        tenant_id: Uuid,
        user_filter: Option<Uuid>,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut bindings: Vec<RoleBinding> = state
            .bindings
            .values()
            .filter(|binding| {
                binding.tenant_id() == tenant_id
                    && user_filter.is_none_or(|user| binding.user_id() == user)
            })
            .cloned()
            .collect();
        // Bounded listing: more rows than the tenant cap is a store-state
        // violation, never an unbounded response.
        if bindings.len() > MAX_BINDINGS_PER_TENANT {
            return Err(PolicyStoreError::TooManyResources);
        }
        bindings.sort_by_key(RoleBinding::binding_id);
        Ok(bindings)
    }

    async fn get_binding(
        &self,
        tenant_id: Uuid,
        binding_id: Uuid,
    ) -> Result<Option<RoleBinding>, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state
            .bindings
            .get(&binding_id)
            .filter(|binding| binding.tenant_id() == tenant_id)
            .cloned())
    }
}

struct FakeSubject {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl SubjectStatusPort for FakeSubject {
    async fn subject_status(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<SubjectStatus, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state
            .subjects
            .get(&(tenant_id, user_id))
            .copied()
            .unwrap_or(SubjectStatus::MissingUser))
    }
}

struct FakeOrg {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl OrganizationScopePort for FakeOrg {
    async fn unit_is_active(
        &self,
        tenant_id: Uuid,
        org_unit_id: Uuid,
    ) -> Result<bool, PolicyStoreError> {
        let state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state
            .units
            .get(&org_unit_id)
            .is_some_and(|unit| unit.tenant_id == tenant_id && unit.active))
    }

    async fn subtree_contains(
        &self,
        tenant_id: Uuid,
        ancestor: Uuid,
        candidate: Uuid,
    ) -> Result<bool, PolicyStoreError> {
        let mut state = self.state.lock().map_err(|_| PolicyStoreError::Failed)?;
        check_poisoned(&state)?;
        take_fault(&mut state, FAULT_SUBTREE_CONTAINS)?;
        let Some(ancestor_unit) = state.units.get(&ancestor) else {
            return Ok(false);
        };
        if ancestor_unit.tenant_id != tenant_id {
            return Ok(false);
        }
        // Bounded ancestor walk from the candidate upward.
        let mut current = candidate;
        for _ in 0..=64 {
            let Some(unit) = state.units.get(&current) else {
                return Ok(false);
            };
            if unit.tenant_id != tenant_id {
                return Ok(false);
            }
            if current == ancestor {
                return Ok(true);
            }
            let Some(parent) = unit.parent_id else {
                return Ok(false);
            };
            current = parent;
        }
        Ok(false)
    }
}
