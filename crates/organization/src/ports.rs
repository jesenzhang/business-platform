//! Persistence and audit-carrying unit-of-work ports for the minimal
//! Organization context.
//!
//! # Adapter contract (binding for `organization-postgres` / `organization-sqlite`)
//!
//! 1. **Unique indexes.** `organization_units` PK + `tenant_id`;
//!    `organization_memberships` unique on
//!    `(tenant_id, user_id, org_unit_id, membership_type)`.
//! 2. **Tree safety in the write transaction.** `create`/`move` must
//!    re-validate inside the transaction: parent exists, parent is in the
//!    same tenant, parent chain is acyclic and within the depth cap
//!    (re-check after acquiring write locks — an application-side pre-check
//!    is only an optimization). Violations surface as
//!    [`OrganizationStoreError::InvalidParent`] /
//!    [`OrganizationStoreError::Cycle`].
//! 3. **Tenant scoping.** Every statement binds `tenant_id`; cross-tenant
//!    reads/writes are not expressible.
//! 4. **Optimistic versioning, atomic audit, idempotency** exactly as in
//!    the identity ports: conditional update on `version`; the aggregate
//!    change, audit record, and idempotency row commit together; key
//!    scoping is `(operation, tenant)`; fingerprints cover semantic fields
//!    only.
//! 5. **Add-member convergence.** If an inactive membership row exists for
//!    the same key, `add_member` reactivates it (version bump) and returns
//!    `replayed = false`; a fully identical idempotent replay returns
//!    `replayed = true`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    OrganizationMembership, OrganizationMembershipType, OrganizationUnit, OrganizationUnitStatus,
    OrganizationUnitType,
};

/// Stable failure categories exposed by organization ports.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationStoreError {
    /// The target aggregate does not exist.
    #[error("organization aggregate not found")]
    NotFound,
    /// The membership/unit key already exists.
    #[error("organization aggregate already exists")]
    AlreadyExists,
    /// Optimistic concurrency conflict.
    #[error("organization aggregate version conflict")]
    VersionConflict,
    /// Parent does not exist, is in another tenant, is not active, or would
    /// exceed the depth bound.
    #[error("invalid parent placement")]
    InvalidParent,
    /// The placement would create a cycle.
    #[error("organization tree cycle rejected")]
    Cycle,
    /// The organization unit is disabled.
    #[error("organization unit is disabled")]
    UnitDisabled,
    /// A listing exceeded the tenant resource cap.
    #[error("tenant organization resource cap exceeded")]
    TooManyResources,
    /// Idempotency key reuse with a different payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// The store is unavailable.
    #[error("organization persistence is unavailable")]
    Unavailable,
    /// Any other store failure.
    #[error("organization persistence failed")]
    Failed,
}

/// Audit metadata carried with each mutation (mirrors the identity port
/// vocabulary; adapters map it to the shared audit trail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationContext {
    /// Actor identity for the audit record.
    pub actor_id: String,
    /// Stable operation id shared by mutation and audit.
    pub operation_id: Uuid,
    /// Optional trace id.
    pub trace_id: Option<String>,
    /// Optional human reason.
    pub reason: Option<String>,
}

/// Atomic unit creation.
#[derive(Debug, Clone)]
pub struct CreateUnitCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Caller-chosen unit id (idempotent provisioning).
    pub unit_id: Uuid,
    /// Parent unit, if any.
    pub parent_id: Option<Uuid>,
    /// Unit kind.
    pub unit_type: OrganizationUnitType,
    /// Display name.
    pub name: String,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of an atomic unit write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitCommitOutcome {
    /// Resulting unit.
    pub unit: OrganizationUnit,
    /// True on idempotent convergence.
    pub replayed: bool,
}

/// Atomic unit update (rename / retyping / status).
#[derive(Debug, Clone)]
pub struct UpdateUnitCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// New name.
    pub name: Option<String>,
    /// New kind.
    pub unit_type: Option<OrganizationUnitType>,
    /// New status.
    pub status: Option<OrganizationUnitStatus>,
    /// Optimistic version.
    pub expected_version: i64,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Atomic reparenting.
#[derive(Debug, Clone)]
pub struct MoveUnitCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// New parent (`None` = tenant root).
    pub new_parent_id: Option<Uuid>,
    /// Optimistic version.
    pub expected_version: i64,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Atomic membership add.
#[derive(Debug, Clone)]
pub struct AddMemberCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target unit.
    pub unit_id: Uuid,
    /// Platform user to attach.
    pub user_id: Uuid,
    /// Kind of membership.
    pub membership_type: OrganizationMembershipType,
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of an atomic membership write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberCommitOutcome {
    /// Resulting membership.
    pub membership: OrganizationMembership,
    /// True on idempotent convergence.
    pub replayed: bool,
}

/// Atomic membership removal.
#[derive(Debug, Clone)]
pub struct RemoveMemberCommit {
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
    /// Audit metadata.
    pub audit: MutationContext,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Command-side organization mutations; each method commits aggregate change
/// + unified audit atomically.
#[async_trait]
pub trait OrganizationCommandPort: Send + Sync {
    /// Create a unit (parent re-validated in-transaction).
    async fn create_unit(
        &self,
        command: CreateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError>;

    /// Update unit metadata.
    async fn update_unit(
        &self,
        command: UpdateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError>;

    /// Move a unit to a new parent (cycle/depth re-validated
    /// in-transaction).
    async fn move_unit(
        &self,
        command: MoveUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError>;

    /// Attach a user to a unit (unit must be same-tenant and active;
    /// inactive rows converge by reactivation).
    async fn add_member(
        &self,
        command: AddMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError>;

    /// Deactivate a membership (removal). Already-inactive converges as
    /// `replayed = true`.
    async fn remove_member(
        &self,
        command: RemoveMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError>;
}

/// One membership row of a unit listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitMemberRecord {
    /// The membership.
    pub membership: OrganizationMembership,
}

/// Read-only organization queries. Everything is tenant-scoped.
#[async_trait]
pub trait OrganizationQueryPort: Send + Sync {
    /// Fetch one unit.
    async fn get_unit(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Option<OrganizationUnit>, OrganizationStoreError>;

    /// All units of the tenant (flat; the delivery layer renders trees).
    /// Fails with [`OrganizationStoreError::TooManyResources`] beyond the
    /// tenant cap.
    async fn list_units(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<OrganizationUnit>, OrganizationStoreError>;

    /// Members of one unit, ordered by join time.
    async fn list_unit_members(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Vec<UnitMemberRecord>, OrganizationStoreError>;

    /// Active memberships of a user within a tenant (policy scoping input).
    async fn list_user_memberships(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationStoreError>;
}

/// Consumer-side narrow port answering identity questions about the tenant,
/// implemented in the composition root over the Identity context (never by
/// reading identity tables).
#[async_trait]
pub trait TenantMembershipReader: Send + Sync {
    /// True when the user is an **active** member of the tenant.
    async fn is_active_member(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, OrganizationStoreError>;
}
