//! Persistence and audit-carrying unit-of-work ports for Platform Identity.
//!
//! Command ports are transactional units of work: each commits its aggregate
//! change together with the unified audit record (adapters use the shared
//! in-transaction audit writer). Query ports are read-only and always
//! tenant-scoped where a tenant boundary exists.
//!
//! # Adapter contract (binding for `identity-postgres` / `identity-sqlite`)
//!
//! 1. **Unique indexes.** `external_identities(issuer, subject)` unique, and
//!    `external_identities(user_id)` unique (v1: one link per user — the
//!    resolve adoption path is only race-safe with this index). A unique
//!    violation on the *same* `(issuer, subject)` key during
//!    [`IdentityResolvePort::resolve_or_provision`] must re-read and continue
//!    (match-and-continue), never surface as an error.
//! 2. **Pre-normalized keys.** Ports receive already-trimmed, control-char
//!    free `issuer`/`subject` values: only the application use cases
//!    normalize, and every writer must go through them. Adapters may assume
//!    the caps from `crate::domain` constants.
//! 3. **Idempotency.** Keys are scoped per `(operation, tenant)` where the
//!    operation is tenant-bound (global otherwise, keyed by the operation
//!    name). The request fingerprint must cover all *semantic* fields
//!    (create: tenant + target user + source; status change: tenant + user +
//!    target status + expected version; user status: user + target status +
//!    expected version) and must exclude `now`, `actor`, and `reason`. Same
//!    key + same fingerprint converges onto the stored outcome; same key +
//!    different fingerprint fails with `IdempotencyConflict` without
//!    mutating.
//! 4. **Optimistic versioning.** Every mutation is a conditional update
//!    `WHERE tenant_id = $ AND id = $ AND version = expected`; zero rows
//!    affected ⇒ `VersionConflict`. Aggregate + audit + idempotency rows
//!    commit or roll back together.
//! 5. **Disabled users may be attached.** `create_membership` deliberately
//!    permits attaching a globally disabled user (the access checker denies
//!    at decision time); adapters must not add an implicit "active only"
//!    filter here.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    ExternalIdentity, MembershipSource, MembershipStatus, PlatformUser, TenantMembership,
    UserLifecycleStatus,
};

/// Stable failure categories exposed by identity ports.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStoreError {
    /// The target aggregate does not exist.
    #[error("identity aggregate not found")]
    NotFound,
    /// A create collided with an existing aggregate.
    #[error("identity aggregate already exists")]
    AlreadyExists,
    /// Optimistic concurrency conflict (`expected_version` did not match).
    #[error("identity aggregate version conflict")]
    VersionConflict,
    /// A fail-closed identity inconsistency was detected while resolving or
    /// provisioning (same key → different user, or user already linked to a
    /// different external subject).
    #[error("external identity mapping conflict")]
    PrincipalMismatch,
    /// An idempotency key was reused with a different request payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// The store is unavailable (connection/pool level failure).
    #[error("identity persistence is unavailable")]
    Unavailable,
    /// Any other store failure.
    #[error("identity persistence failed")]
    Failed,
}

/// Who performed a mutation and how it should be audited. The composition
/// root / delivery layer fills this from the authenticated request context
/// (never from client-supplied identity headers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationContext {
    /// Actor identity written to the unified audit trail.
    pub actor_id: String,
    /// Actor classification for the audit trail.
    pub actor_kind: MutationActorKind,
    /// Stable operation id shared by aggregate change and audit record.
    pub operation_id: Uuid,
    /// Trace id for correlation (optional outside HTTP flows).
    pub trace_id: Option<String>,
    /// Human-readable reason for the mutation (optional).
    pub reason: Option<String>,
}

/// Actor classification used by identity mutations (mapped to the shared
/// audit vocabulary by adapters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationActorKind {
    /// An authenticated platform user (management plane call).
    User,
    /// The bootstrap service at startup.
    Bootstrap,
    /// A migration or rehearsal job.
    Migration,
}

/// Resolve-or-provision commit: atomically match `(issuer, subject)` to a
/// platform user, provisioning one (plus the external identity link and an
/// audit record) when it is unknown. A unique violation on the same key is
/// match-and-continue; a same-key/different-user inconsistency must surface
/// as [`IdentityStoreError::PrincipalMismatch`] (fail closed).
#[derive(Debug, Clone)]
pub struct ResolvePrincipalCommit {
    /// Verified token issuer (server-side value, never a client header).
    pub issuer: String,
    /// Verified token subject.
    pub subject: String,
    /// The trusted `user_id` claim when present.
    pub claimed_user_id: Option<Uuid>,
    /// Deterministic user id derived by the application when no claim exists
    /// (`UUIDv5` over the external key).
    pub deterministic_user_id: Uuid,
    /// Audit context for the provisioning record.
    pub audit: MutationContext,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of [`ResolvePrincipalCommit`] handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPrincipal {
    /// The platform user the external key maps to.
    pub user: PlatformUser,
    /// The durable external identity link (existing or just provisioned).
    pub external_identity: ExternalIdentity,
    /// True when the user (and link) were created by this call.
    pub provisioned: bool,
}

/// Resolve-or-provision port used by the authorization middleware path.
#[async_trait]
pub trait IdentityResolvePort: Send + Sync {
    /// Atomically resolve `(issuer, subject)` to an existing platform user or
    /// provision a new one. The provisioning audit record is written in the
    /// same transaction as the new rows.
    async fn resolve_or_provision(
        &self,
        command: ResolvePrincipalCommit,
    ) -> Result<ResolvedPrincipal, IdentityStoreError>;
}

/// Membership create target: an existing user id, or an external subject to
/// provision on demand (admin onboarding flow).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembershipTarget {
    /// Reference an existing platform user.
    UserId(Uuid),
    /// Provision-or-match by external issuer/subject, then attach.
    ExternalSubject {
        /// Verified issuer.
        issuer: String,
        /// Verified subject.
        subject: String,
        /// Deterministic user id the application derives from the external
        /// key (same derivation as [`crate::application::ResolveAuthenticatedUser`]).
        deterministic_user_id: Uuid,
    },
}

/// Atomic membership creation (membership + audit + idempotency).
#[derive(Debug, Clone)]
pub struct CreateMembershipCommit {
    /// Tenant the user joins.
    pub tenant_id: Uuid,
    /// Who is being attached.
    pub target: MembershipTarget,
    /// Origin of the record.
    pub source: MembershipSource,
    /// Audit context.
    pub audit: MutationContext,
    /// Caller idempotency key; same key + same payload converges (replay),
    /// same key + different payload fails with `IdempotencyConflict`.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of an atomic membership write. `replayed` marks an idempotent
/// convergence onto an existing record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipCommitOutcome {
    /// The resulting membership (existing on replay).
    pub membership: TenantMembership,
    /// True when this call converged onto a previous identical request.
    pub replayed: bool,
}

/// Atomic membership status change (membership + audit + idempotency),
/// guarded by optimistic versioning.
#[derive(Debug, Clone)]
pub struct ChangeMembershipStatusCommit {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target user.
    pub user_id: Uuid,
    /// Desired status.
    pub target_status: MembershipStatus,
    /// Version the caller observed; mismatch fails with `VersionConflict`.
    pub expected_version: i64,
    /// Audit context.
    pub audit: MutationContext,
    /// Caller idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Atomic user lifecycle change (user + audit + idempotency), versioned.
#[derive(Debug, Clone)]
pub struct ChangeUserStatusCommit {
    /// Target user.
    pub user_id: Uuid,
    /// Desired lifecycle status.
    pub target_status: UserLifecycleStatus,
    /// Version the caller observed.
    pub expected_version: i64,
    /// Audit context.
    pub audit: MutationContext,
    /// Caller idempotency key.
    pub idempotency_key: Option<String>,
    /// Mutation timestamp.
    pub now: DateTime<Utc>,
}

/// Outcome of an atomic user status change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserCommitOutcome {
    /// The resulting user.
    pub user: PlatformUser,
    /// True when this call converged onto a previous identical request.
    pub replayed: bool,
}

/// Command-side membership and user mutations. All methods write the unified
/// audit record inside the same transaction as the aggregate change.
#[async_trait]
pub trait IdentityCommandPort: Send + Sync {
    /// Create a tenant membership (optionally provisioning the user by
    /// external subject). Fails with `AlreadyExists` when the user is already
    /// a member of the tenant.
    async fn create_membership(
        &self,
        command: CreateMembershipCommit,
    ) -> Result<MembershipCommitOutcome, IdentityStoreError>;

    /// Suspend/reactivate a tenant membership. If the membership already has
    /// the target status the call converges (`replayed = true`, no version
    /// bump) so duplicate management requests are harmless.
    async fn change_membership_status(
        &self,
        command: ChangeMembershipStatusCommit,
    ) -> Result<MembershipCommitOutcome, IdentityStoreError>;

    /// Disable/enable a platform user globally.
    async fn change_user_status(
        &self,
        command: ChangeUserStatusCommit,
    ) -> Result<UserCommitOutcome, IdentityStoreError>;
}

/// One user row in a tenant listing (user + its membership summary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantUserRecord {
    /// The platform user.
    pub user: PlatformUser,
    /// The membership inside the listed tenant.
    pub membership: TenantMembership,
    /// Linked external subjects (issuer + subject pairs) for display.
    pub external_subjects: Vec<(String, String)>,
}

/// Keyset position for identity listings: strictly after this
/// `(joined_at_or_created_at, row_id)` pair, ordered descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysetPosition {
    /// Timestamp component of the last returned row.
    pub timestamp: DateTime<Utc>,
    /// Row id component of the last returned row.
    pub row_id: Uuid,
}

/// One membership row in a membership listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipRecord {
    /// The membership.
    pub membership: TenantMembership,
    /// Display-only user lifecycle status.
    pub user_status: UserLifecycleStatus,
}

/// Read-only identity queries. Every method is tenant-scoped except the
/// explicit external-subject lookup used by bootstrap/onboarding flows.
#[async_trait]
pub trait IdentityQueryPort: Send + Sync {
    /// Fetch one user together with its membership inside `tenant_id`.
    async fn get_tenant_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantUserRecord>, IdentityStoreError>;

    /// Keyset-paginated list of users that hold a membership in `tenant_id`,
    /// ordered by `joined_at DESC, membership_id DESC`.
    async fn list_tenant_users(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<KeysetPosition>,
    ) -> Result<(Vec<TenantUserRecord>, Option<KeysetPosition>), IdentityStoreError>;

    /// Keyset-paginated listing of memberships in `tenant_id`, ordered by
    /// `joined_at DESC, membership_id DESC`.
    async fn list_memberships(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<KeysetPosition>,
    ) -> Result<(Vec<MembershipRecord>, Option<KeysetPosition>), IdentityStoreError>;

    /// Fetch one membership by tenant boundary and user.
    async fn get_membership(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantMembership>, IdentityStoreError>;

    /// Global lookup of a user by its external identity link (used by the
    /// bootstrap and onboarding flows only).
    async fn find_user_by_external_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<PlatformUser>, IdentityStoreError>;
}

/// The bootstrap ledger contract (identity-owned). Records each bootstrap
/// execution by config digest so re-runs are no-ops even after the binding
/// was revoked; a deliberate config version bump is required to re-bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapLedgerEntry {
    /// Tenant the bootstrap targeted.
    pub tenant_id: Uuid,
    /// External issuer that was bound.
    pub issuer: String,
    /// External subject that was bound.
    pub subject: String,
    /// Role stable key that was bound (owned by Policy).
    pub role_stable_key: String,
    /// Configuration version chosen by the operator.
    pub config_version: i64,
    /// SHA-256 hex digest of the canonical bootstrap configuration.
    pub config_digest: String,
    /// Outcome recorded at execution time.
    pub outcome: BootstrapOutcome,
    /// When the ledger row was written.
    pub recorded_at: DateTime<Utc>,
}

/// Result classification recorded by a bootstrap execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapOutcome {
    /// User/membership/binding were created.
    Executed,
    /// An identical earlier execution exists; nothing was changed.
    NoOp,
    /// The execution failed (audited; the deployment must react).
    Failed,
}

/// Bootstrap ledger port.
#[async_trait]
pub trait BootstrapLedgerPort: Send + Sync {
    /// Fetch the latest ledger entry for a (tenant, issuer, subject) tuple.
    async fn latest_for(
        &self,
        tenant_id: Uuid,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<BootstrapLedgerEntry>, IdentityStoreError>;

    /// Append a ledger entry with a uniqueness rule on
    /// (tenant, issuer, subject, `config_digest)`: an existing identical digest
    /// converges (no duplicate row, `Ok(false)`), otherwise `Ok(true)`.
    async fn record(&self, entry: &BootstrapLedgerEntry) -> Result<bool, IdentityStoreError>;
}
