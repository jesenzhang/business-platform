//! Persistence and audit-carrying unit-of-work ports for Policy, plus the
//! two consumer-side bridge ports (identity subject status, organization
//! scope resolution) that the composition root implements over the owning
//! contexts' query surfaces.
//!
//! # Adapter contract (binding for `policy-postgres` / `policy-sqlite`)
//!
//! 1. **Unique indexes.** `role_definitions`: partial unique on
//!    `stable_key WHERE tenant_id IS NULL` (system) and on
//!    `(tenant_id, stable_key)` (tenant); `role_permissions` unique on
//!    `(role_id, permission_key)`; `role_bindings` indexed
//!    `(tenant_id, user_id)` (decisions and escalation guards list by it).
//! 2. **System roles are store-immutable.** No create/update/set-permissions
//!    statement may touch rows with `tenant_id IS NULL`; attempts surface as
//!    [`PolicyStoreError::RoleImmutable`]. Only the migration seed inserts
//!    them (with their `role_permissions` rows).
//! 3. **Tenant scoping.** Role visibility is `tenant_id = $ OR tenant_id IS
//!    NULL`: a binding may reference the tenant's own roles or system roles
//!    and nothing else; every read/write statement binds `tenant_id`.
//! 4. **`SetRolePermissions` is a transactional set replace** of the role's
//!    `role_permissions` rows plus the role row's version bump plus audit —
//!    one commit; each key must exist in `permission_definitions` or the
//!    commit fails with [`PolicyStoreError::UnknownPermission`].
//! 5. **Binding create convergence.** An already-active identical binding
//!    `(tenant_id, user_id, role_id, scope, effective_at, expires_at)`
//!    converges as `replayed = true` with no audit (no duplicate spam);
//!    otherwise a new row is inserted. Multiple distinct bindings per
//!    user/role are legal by design.
//! 6. **Optimistic versioning, atomic audit, idempotency** exactly as in the
//!    identity/organization ports: conditional update on `version`; the
//!    aggregate change, audit record, and idempotency row commit together;
//!    key scoping is `(operation, tenant)`; fingerprints cover the request
//!    payload only (semantic fields + `expected_version` where present;
//!    never server-generated ids, timestamps, actors, reasons, trace ids).
//! 7. **Bindings list every status.** `list_bindings_for_user` returns
//!    revoked rows too — the evaluator distinguishes "revoked" from "never
//!    bound"; rows come ordered by `binding_id` so deny-reason selection is
//!    deterministic.
//! 8. **Caps are store-side and race-free.** Inside the commit
//!    transaction: `create_role` refuses past `MAX_ROLES_PER_TENANT`
//!    tenant-owned rows (system roles never count); `bind_role` refuses
//!    past `MAX_BINDINGS_PER_USER` active rows for the user and
//!    `MAX_BINDINGS_PER_TENANT` total rows (all statuses); `list_bindings`
//!    surfaces `TooManyResources` rather than an unbounded result.
//!    Application-layer cap checks are advisory UX only. Catalog rows
//!    carry `active`; a retired (`active = false`) row is returned by
//!    `get_permission` but evaluates as unknown — adapters seed active.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    PermissionDefinition, PermissionKey, ResourceScope, RoleBinding, RoleDefinition, RoleStatus,
};

/// Stable failure categories exposed by policy ports.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PolicyStoreError {
    /// The target aggregate does not exist (or is not visible in tenant).
    #[error("policy aggregate not found")]
    NotFound,
    /// The role/permission key already exists.
    #[error("policy aggregate already exists")]
    AlreadyExists,
    /// Optimistic concurrency conflict.
    #[error("policy aggregate version conflict")]
    VersionConflict,
    /// A mutation targeted a system role (immutable by contract).
    #[error("system roles are immutable")]
    RoleImmutable,
    /// `SetRolePermissions` referenced a key missing from the catalog.
    #[error("permission key is not in the catalog")]
    UnknownPermission,
    /// A role exceeds the bounded permission count.
    #[error("role permission bound exceeded")]
    TooManyPermissions,
    /// A listing exceeded the tenant binding cap.
    #[error("tenant policy resource cap exceeded")]
    TooManyResources,
    /// Idempotency key reuse with a different payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// The store is unavailable.
    #[error("policy persistence is unavailable")]
    Unavailable,
    /// Any other store failure.
    #[error("policy persistence failed")]
    Failed,
}

/// Audit metadata carried with each mutation (same vocabulary as the
/// identity/organization ports).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationContext {
    /// Actor identity for the audit record.
    pub actor_id: String,
    /// Actor classification for the audit trail (`actor_type`).
    pub actor_kind: MutationActorKind,
    /// Stable operation id shared by mutation and audit.
    pub operation_id: Uuid,
    /// Optional trace id.
    pub trace_id: Option<String>,
    /// Optional human reason.
    pub reason: Option<String>,
}

/// Actor classification used by policy mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationActorKind {
    /// An authenticated platform user (management plane call).
    User,
    /// The bootstrap service at startup.
    Bootstrap,
    /// A migration or rehearsal job.
    Migration,
}

/// Atomic role creation (tenant roles only — system roles are seeded).
#[derive(Debug, Clone)]
pub struct CreateRoleCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Caller-chosen role id (`None` lets the store generate one).
    pub role_id: Option<Uuid>,
    /// Stable key (unique per tenant).
    pub stable_key: String,
    /// Display name.
    pub display_name: String,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Atomic role metadata update.
#[derive(Debug, Clone)]
pub struct UpdateRoleCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target role.
    pub role_id: Uuid,
    /// New display name.
    pub display_name: Option<String>,
    /// New status.
    pub status: Option<RoleStatus>,
    /// Optimistic version of the role row.
    pub expected_version: i64,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Atomic transactional set replace of a role's permissions.
#[derive(Debug, Clone)]
pub struct SetRolePermissionsCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target role (must be a tenant role).
    pub role_id: Uuid,
    /// The complete new permission key set (validated keys, catalog members).
    pub permission_keys: Vec<String>,
    /// Optimistic version of the role row.
    pub expected_version: i64,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of a role mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleCommitOutcome {
    /// Resulting role.
    pub role: RoleDefinition,
    /// True on idempotent or same-value convergence.
    pub replayed: bool,
}

/// Outcome of a permission-set mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionsCommitOutcome {
    /// Resulting role.
    pub role: RoleDefinition,
    /// The stored permission set.
    pub permission_keys: Vec<String>,
    /// True on idempotent convergence.
    pub replayed: bool,
}

/// Atomic role binding.
#[derive(Debug, Clone)]
pub struct BindRoleCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Caller-chosen binding id (`None` lets the store generate one).
    pub binding_id: Option<Uuid>,
    /// Bound platform user.
    pub user_id: Uuid,
    /// Bound role (tenant-owned or system).
    pub role_id: Uuid,
    /// Granted scope.
    pub scope: ResourceScope,
    /// Validity window start.
    pub effective_at: DateTime<Utc>,
    /// Validity window end (exclusive when set).
    pub expires_at: Option<DateTime<Utc>>,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Atomic binding revocation.
#[derive(Debug, Clone)]
pub struct RevokeBindingCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target binding.
    pub binding_id: Uuid,
    /// Optimistic version.
    pub expected_version: i64,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of a binding mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingCommitOutcome {
    /// Resulting binding.
    pub binding: RoleBinding,
    /// True on idempotent or identical-active-binding convergence.
    pub replayed: bool,
}

/// Command-side policy mutations; each commits aggregate change + unified
/// audit atomically.
#[async_trait]
pub trait PolicyCommandPort: Send + Sync {
    /// Create a tenant role.
    async fn create_role(
        &self,
        command: CreateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError>;

    /// Update role metadata/status (system roles rejected).
    async fn update_role(
        &self,
        command: UpdateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError>;

    /// Transactional set replace of role permissions.
    async fn set_role_permissions(
        &self,
        command: SetRolePermissionsCommit,
    ) -> Result<PermissionsCommitOutcome, PolicyStoreError>;

    /// Bind a role to a user (identical active binding converges).
    async fn bind_role(
        &self,
        command: BindRoleCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError>;

    /// Revoke a binding. Already-revoked converges as `replayed = true`
    /// while `expected_version` matches the current row.
    async fn revoke_binding(
        &self,
        command: RevokeBindingCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError>;
}

/// Read-only policy queries. Everything is tenant-scoped (role visibility
/// extends to system roles).
#[async_trait]
pub trait PolicyQueryPort: Send + Sync {
    /// The permission catalog entry for a key (decision fail-closed gate).
    async fn get_permission(
        &self,
        key: &PermissionKey,
    ) -> Result<Option<PermissionDefinition>, PolicyStoreError>;

    /// The full catalog (bounded: this is seed content, not user data).
    async fn list_permissions(&self) -> Result<Vec<PermissionDefinition>, PolicyStoreError>;

    /// One role visible to the tenant (own or system).
    async fn get_role(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<RoleDefinition>, PolicyStoreError>;

    /// Roles visible to the tenant (system + own, ordered by stable key).
    async fn list_roles(&self, tenant_id: Uuid) -> Result<Vec<RoleDefinition>, PolicyStoreError>;

    /// Permission keys of a visible role.
    async fn get_role_permissions(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Vec<String>, PolicyStoreError>;

    /// **Every** binding of a user in a tenant (all statuses), ordered by
    /// binding id (deterministic deny selection).
    async fn list_bindings_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError>;

    /// Active bindings of a role in a tenant (self-escalation guard input).
    async fn list_active_bindings_for_role(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError>;

    /// All bindings of a tenant, optionally filtered by user (management
    /// listing surface; bounded by the tenant resource cap).
    async fn list_bindings(
        &self,
        tenant_id: Uuid,
        user_filter: Option<Uuid>,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError>;

    /// One binding by id (tenant-scoped).
    async fn get_binding(
        &self,
        tenant_id: Uuid,
        binding_id: Uuid,
    ) -> Result<Option<RoleBinding>, PolicyStoreError>;
}

/// Subject facts the evaluator must re-read on every decision (composition
/// root implements this over the Identity context's query ports — never by
/// reading identity tables). Suspension/disable takes effect on the next
/// request even with an unexpired JWT.
#[async_trait]
pub trait SubjectStatusPort: Send + Sync {
    /// Current user/membership status for `(tenant, user)`.
    async fn subject_status(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<SubjectStatus, PolicyStoreError>;
}

/// Closed subject-status vocabulary (mirrors identity's access reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectStatus {
    /// No platform user for the resolved principal.
    MissingUser,
    /// User exists but is disabled.
    UserDisabled,
    /// Active user, no membership in this tenant.
    NoMembership,
    /// Active user, suspended membership.
    MembershipSuspended,
    /// Active user, active membership.
    Active,
}

impl SubjectStatus {
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Trusted organization facts for org-unit scopes (composition root
/// implements this over the Organization context's query ports). Subtree
/// resolution is the *only* scope expansion and comes from stored tree
/// data, never request parameters.
#[async_trait]
pub trait OrganizationScopePort: Send + Sync {
    /// True when the unit exists in the tenant and is active.
    async fn unit_is_active(
        &self,
        tenant_id: Uuid,
        org_unit_id: Uuid,
    ) -> Result<bool, PolicyStoreError>;

    /// True when `candidate` is `ancestor` itself or a descendant of it in
    /// the tenant's tree (bounded traversal, fail-closed on depth).
    async fn subtree_contains(
        &self,
        tenant_id: Uuid,
        ancestor: Uuid,
        candidate: Uuid,
    ) -> Result<bool, PolicyStoreError>;
}
