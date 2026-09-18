//! `PostgreSQL` adapter for the minimal Organization context (production
//! authority). Implements the organization ports as audit-carrying units of
//! work per the binding adapter contract in `organization::ports`:
//! tenant-scoped statements, in-transaction parent/cycle re-validation,
//! conditional-update optimistic versioning, per-`(operation, tenant)`
//! idempotency with request-only fingerprints, add-member reactivation
//! convergence, and in-transaction unified audit chaining.
//!
//! Tables live in the runtime migration catalog
//! (`migrations/019_identity_authorization_foundation.sql`); cross-context
//! user references are bare `UUID`s by design (no cross-context foreign
//! keys).

use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use organization::application::MAX_UNITS_PER_TENANT;
use organization::domain::{
    validate_tree_placement, OrganizationMembership, OrganizationMembershipStatus,
    OrganizationMembershipType, OrganizationUnit, OrganizationUnitStatus, OrganizationUnitType,
    RehydrateOrganizationMembership, RehydrateOrganizationUnit,
};
use organization::ports::{
    AddMemberCommit, CreateUnitCommit, MemberCommitOutcome, MoveUnitCommit, MutationActorKind,
    MutationContext, OrganizationCommandPort, OrganizationQueryPort, OrganizationStoreError,
    RemoveMemberCommit, UnitCommitOutcome, UnitMemberRecord, UpdateUnitCommit,
};

/// `PostgreSQL` organization store. One adapter serves both organization
/// ports.
#[derive(Clone)]
pub struct PostgresOrganizationStore {
    pool: PgPool,
}

impl PostgresOrganizationStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    #[must_use]
    pub fn command_port(self: &Arc<Self>) -> Arc<dyn OrganizationCommandPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }

    #[must_use]
    pub fn query_port(self: &Arc<Self>) -> Arc<dyn OrganizationQueryPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }
}

// ---------------------------------------------------------------------------
// error / mapping helpers
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)] // `map_err` hands the error over by value.
fn map_sqlx_error(error: sqlx::Error) -> OrganizationStoreError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            OrganizationStoreError::Unavailable
        }
        _ => OrganizationStoreError::Failed,
    }
}

fn map_audit_error(_error: audit::AuditError) -> OrganizationStoreError {
    OrganizationStoreError::Failed
}

/// Canonical request fingerprint: SHA-256 over `|`-joined semantic request
/// fields. Never covers server timestamps, actors, or reasons.
fn fingerprint(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(parts.join("|").as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn parse_unit_type(raw: &str) -> Result<OrganizationUnitType, OrganizationStoreError> {
    match raw {
        "company" => Ok(OrganizationUnitType::Company),
        "department" => Ok(OrganizationUnitType::Department),
        "team" => Ok(OrganizationUnitType::Team),
        _ => Err(OrganizationStoreError::Failed),
    }
}

fn parse_unit_status(raw: &str) -> Result<OrganizationUnitStatus, OrganizationStoreError> {
    match raw {
        "active" => Ok(OrganizationUnitStatus::Active),
        "disabled" => Ok(OrganizationUnitStatus::Disabled),
        _ => Err(OrganizationStoreError::Failed),
    }
}

fn parse_membership_type(raw: &str) -> Result<OrganizationMembershipType, OrganizationStoreError> {
    match raw {
        "member" => Ok(OrganizationMembershipType::Member),
        "leader" => Ok(OrganizationMembershipType::Leader),
        _ => Err(OrganizationStoreError::Failed),
    }
}

fn parse_membership_status(
    raw: &str,
) -> Result<OrganizationMembershipStatus, OrganizationStoreError> {
    match raw {
        "active" => Ok(OrganizationMembershipStatus::Active),
        "inactive" => Ok(OrganizationMembershipStatus::Inactive),
        _ => Err(OrganizationStoreError::Failed),
    }
}

fn actor_of(audit: &MutationContext) -> Result<AuditActor, OrganizationStoreError> {
    let actor_id = Uuid::parse_str(&audit.actor_id).map_err(|_| OrganizationStoreError::Failed)?;
    let actor_type = match audit.actor_kind {
        MutationActorKind::User => AuditActorType::User,
        // The bootstrap service and migration/rehearsal jobs are service
        // principals, never end users.
        MutationActorKind::Bootstrap | MutationActorKind::Migration => AuditActorType::Service,
    };
    Ok(AuditActor {
        actor_type,
        actor_id,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_audit(
    tenant_id: Uuid,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    audit: &MutationContext,
    occurred_at: DateTime<Utc>,
    details: serde_json::Value,
    changed_fields: Vec<String>,
) -> Result<AuditEvent, OrganizationStoreError> {
    AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        actor_of(audit)?,
        AuditAction::new(action).map_err(|_| OrganizationStoreError::Failed)?,
        AuditResource::new(resource_type, resource_id)
            .map_err(|_| OrganizationStoreError::Failed)?,
        audit.operation_id,
        None,
        None,
        audit.trace_id.clone(),
        audit.reason.clone(),
        AuditResult::Succeeded,
        None,
        None,
        None,
        changed_fields,
        details,
        "audit.v1",
        occurred_at,
    )
    .map_err(|_| OrganizationStoreError::Failed)
}

// ---------------------------------------------------------------------------
// rows
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct UnitRow {
    unit_id: Uuid,
    tenant_id: Uuid,
    parent_id: Option<Uuid>,
    unit_type: String,
    name: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i64,
}

impl UnitRow {
    fn into_unit(self) -> Result<OrganizationUnit, OrganizationStoreError> {
        OrganizationUnit::rehydrate(RehydrateOrganizationUnit {
            unit_id: self.unit_id,
            tenant_id: self.tenant_id,
            parent_id: self.parent_id,
            unit_type: parse_unit_type(&self.unit_type)?,
            name: self.name,
            status: parse_unit_status(&self.status)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            version: self.version,
        })
        .map_err(|_| OrganizationStoreError::Failed)
    }
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    membership_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    unit_id: Uuid,
    membership_type: String,
    status: String,
    joined_at: DateTime<Utc>,
    deactivated_at: Option<DateTime<Utc>>,
    version: i64,
}

impl MemberRow {
    fn into_membership(self) -> Result<OrganizationMembership, OrganizationStoreError> {
        OrganizationMembership::rehydrate(RehydrateOrganizationMembership {
            membership_id: self.membership_id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            unit_id: self.unit_id,
            membership_type: parse_membership_type(&self.membership_type)?,
            status: parse_membership_status(&self.status)?,
            joined_at: self.joined_at,
            deactivated_at: self.deactivated_at,
            version: self.version,
        })
        .map_err(|_| OrganizationStoreError::Failed)
    }
}

const UNIT_COLUMNS: &str =
    "unit_id, tenant_id, parent_id, unit_type, name, status, created_at, updated_at, version";
const MEMBER_COLUMNS: &str = "membership_id, tenant_id, user_id, unit_id, membership_type, status, joined_at, deactivated_at, version";

async fn fetch_unit(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    unit_id: Uuid,
) -> Result<Option<OrganizationUnit>, OrganizationStoreError> {
    let sql = format!(
        "SELECT {UNIT_COLUMNS} FROM organization_units WHERE tenant_id = $1 AND unit_id = $2"
    );
    let row = sqlx::query_as::<_, UnitRow>(&sql)
        .bind(tenant_id)
        .bind(unit_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(UnitRow::into_unit).transpose()
}

/// Global unit-id probe (the primary key spans tenants): a chosen id that
/// exists anywhere is `AlreadyExists`, exactly like the fake's map check.
async fn unit_id_exists(
    conn: &mut sqlx::PgConnection,
    unit_id: Uuid,
) -> Result<bool, OrganizationStoreError> {
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM organization_units WHERE unit_id = $1")
            .bind(unit_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
    Ok(exists > 0)
}

async fn fetch_unit_by_id(
    conn: &mut sqlx::PgConnection,
    unit_id: Uuid,
) -> Result<OrganizationUnit, OrganizationStoreError> {
    let sql = format!("SELECT {UNIT_COLUMNS} FROM organization_units WHERE unit_id = $1");
    let row = sqlx::query_as::<_, UnitRow>(&sql)
        .bind(unit_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?
        .ok_or(OrganizationStoreError::Failed)?;
    row.into_unit()
}

async fn fetch_membership_by_key(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    unit_id: Uuid,
    user_id: Uuid,
    membership_type: OrganizationMembershipType,
) -> Result<Option<OrganizationMembership>, OrganizationStoreError> {
    let sql = format!(
        "SELECT {MEMBER_COLUMNS} FROM organization_members WHERE tenant_id = $1 AND unit_id = $2 AND user_id = $3 AND membership_type = $4"
    );
    let row = sqlx::query_as::<_, MemberRow>(&sql)
        .bind(tenant_id)
        .bind(unit_id)
        .bind(user_id)
        .bind(membership_type.as_str())
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(MemberRow::into_membership).transpose()
}

async fn fetch_membership_by_id(
    conn: &mut sqlx::PgConnection,
    membership_id: Uuid,
) -> Result<OrganizationMembership, OrganizationStoreError> {
    let sql = format!("SELECT {MEMBER_COLUMNS} FROM organization_members WHERE membership_id = $1");
    let row = sqlx::query_as::<_, MemberRow>(&sql)
        .bind(membership_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?
        .ok_or(OrganizationStoreError::Failed)?;
    row.into_membership()
}

/// The tenant's complete `(unit_id, parent_id)` snapshot for in-transaction
/// tree validation (the same snapshot the authoritative fakes build).
async fn tree_snapshot(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
) -> Result<Vec<(Uuid, Option<Uuid>)>, OrganizationStoreError> {
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
        "SELECT unit_id, parent_id FROM organization_units WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// placement validation (binding adapter contract rule 2)
// ---------------------------------------------------------------------------

/// Parent must exist, be in the same tenant, and be active. For `create`
/// the new unit id cannot pre-exist here (the `AlreadyExists` probe ran
/// first), so self-parenthood naturally surfaces as a missing parent.
async fn validate_parent(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<(), OrganizationStoreError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    match fetch_unit(conn, tenant_id, parent_id).await? {
        Some(parent) if parent.is_active() => Ok(()),
        Some(_) | None => Err(OrganizationStoreError::InvalidParent),
    }
}

/// Maps a [`validate_tree_placement`] rejection to the fake's error
/// variant: self-parenthood is `InvalidParent`, everything else (cycle or
/// depth) is `Cycle`.
fn placement_error(parent_id: Option<Uuid>, placed_unit: Uuid) -> OrganizationStoreError {
    if parent_id == Some(placed_unit) {
        OrganizationStoreError::InvalidParent
    } else {
        OrganizationStoreError::Cycle
    }
}

// ---------------------------------------------------------------------------
// idempotency helpers (per (operation, tenant) keys)
// ---------------------------------------------------------------------------

/// Advisory lock serializing idempotent retries of one exact request key.
async fn lock_idempotency(
    conn: &mut sqlx::PgConnection,
    operation: &str,
    tenant_id: Uuid,
    key: &str,
) -> Result<(), OrganizationStoreError> {
    let guard = format!("org:{operation}|{tenant_id}|{key}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(guard)
        .execute(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct IdempotencyRow {
    request_fingerprint: String,
    result_kind: String,
    result_id: Uuid,
}

async fn check_idempotency(
    conn: &mut sqlx::PgConnection,
    operation: &str,
    expected_kind: &str,
    tenant_id: Uuid,
    key: Option<&String>,
    expected_fingerprint: &str,
) -> Result<Option<IdempotencyRow>, OrganizationStoreError> {
    let Some(key) = key else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, IdempotencyRow>(
        "SELECT request_fingerprint, result_kind, result_id FROM organization_idempotency WHERE tenant_id = $1 AND operation = $2 AND idempotency_key = $3",
    )
    .bind(tenant_id)
    .bind(operation)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    match row {
        Some(row) if row.request_fingerprint == expected_fingerprint => {
            if row.result_kind != expected_kind {
                // Corrupted or foreign slot: fail closed rather than replay
                // a row whose result cannot be hydrated for this operation.
                return Err(OrganizationStoreError::Failed);
            }
            Ok(Some(row))
        }
        Some(_) => Err(OrganizationStoreError::IdempotencyConflict),
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_idempotency(
    conn: &mut sqlx::PgConnection,
    operation: &str,
    tenant_id: Uuid,
    key: Option<&String>,
    fingerprint_hex: &str,
    result_kind: &str,
    result_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), OrganizationStoreError> {
    let Some(key) = key else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO organization_idempotency (tenant_id, operation, idempotency_key, request_fingerprint, result_kind, result_id, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(tenant_id)
    .bind(operation)
    .bind(key)
    .bind(fingerprint_hex)
    .bind(result_kind)
    .bind(result_id)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ports
// ---------------------------------------------------------------------------

#[async_trait]
impl OrganizationCommandPort for PostgresOrganizationStore {
    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn create_unit(
        &self,
        command: CreateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        let operation = format!("create_unit|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        // Payload fingerprint: placement fields plus the caller-chosen unit
        // id when present; a server-generated id never participates (retry
        // convergence for auto ids).
        let fingerprint_hex = fingerprint(&[
            &command
                .parent_id
                .map_or_else(|| "root".to_string(), |id| id.to_string()),
            command.unit_type.as_str(),
            &command.name,
            &command
                .unit_id
                .map_or_else(|| "auto".to_string(), |id| id.to_string()),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "unit",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let unit = fetch_unit_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UnitCommitOutcome {
                unit,
                replayed: true,
            });
        }
        let unit_id = command.unit_id.unwrap_or_else(Uuid::now_v7);
        if unit_id_exists(&mut transaction, unit_id).await? {
            return Err(OrganizationStoreError::AlreadyExists);
        }
        let unit_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM organization_units WHERE tenant_id = $1",
        )
        .bind(command.tenant_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        if usize::try_from(unit_count).unwrap_or(usize::MAX) >= MAX_UNITS_PER_TENANT {
            return Err(OrganizationStoreError::TooManyResources);
        }
        // In-transaction placement re-validation (contract rule 2): parent
        // first, then the full tenant tree walk.
        validate_parent(&mut transaction, command.tenant_id, command.parent_id).await?;
        let snapshot = tree_snapshot(&mut transaction, command.tenant_id).await?;
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
        sqlx::query(
            "INSERT INTO organization_units (unit_id, tenant_id, parent_id, unit_type, name, status, created_at, updated_at, version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1)",
        )
        .bind(unit.unit_id())
        .bind(unit.tenant_id())
        .bind(unit.parent_id())
        .bind(unit.unit_type().as_str())
        .bind(unit.name())
        .bind(unit.status().as_str())
        .bind(unit.created_at())
        .bind(unit.updated_at())
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        let event = build_audit(
            command.tenant_id,
            "organization.unit.created",
            "organization_unit",
            &unit.unit_id().to_string(),
            &command.audit,
            command.now,
            serde_json::json!({
                "unit_type": unit.unit_type().as_str(),
                "status": unit.status().as_str(),
                "parent_id": unit.parent_id().map(|id| id.to_string()),
            }),
            vec![],
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &event)
            .await
            .map_err(map_audit_error)?;
        record_idempotency(
            &mut transaction,
            &operation,
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
            "unit",
            unit.unit_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(UnitCommitOutcome {
            unit,
            replayed: false,
        })
    }

    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn update_unit(
        &self,
        command: UpdateUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        let operation = format!("update_unit|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        // Payload fingerprint: semantic fields plus the request's
        // `expected_version` (versioned mutation bodies include it).
        let fingerprint_hex = fingerprint(&[
            &command.unit_id.to_string(),
            &command.name.clone().unwrap_or_default(),
            &command
                .unit_type
                .map_or(String::new(), |kind| kind.as_str().to_string()),
            &command
                .status
                .map_or(String::new(), |status| status.as_str().to_string()),
            &command.expected_version.to_string(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "unit",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let unit = fetch_unit_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UnitCommitOutcome {
                unit,
                replayed: true,
            });
        }
        let stored = fetch_unit(&mut transaction, command.tenant_id, command.unit_id)
            .await?
            .ok_or(OrganizationStoreError::NotFound)?;
        if stored.version().value() != command.expected_version {
            return Err(OrganizationStoreError::VersionConflict);
        }
        // Semantic diff: supplied fields that actually differ. Identical
        // values converge (no version bump, no audit) — identity's rule.
        let name_changed = command
            .name
            .as_deref()
            .is_some_and(|name| stored.name() != name.trim());
        let type_changed = command
            .unit_type
            .is_some_and(|kind| stored.unit_type() != kind);
        let status_changed = command
            .status
            .is_some_and(|status| stored.status() != status);
        let changed = name_changed || type_changed || status_changed;
        if !changed {
            // Same-value convergence: no bump, no audit; still replayable
            // (the fakes record the slot on this path).
            record_idempotency(
                &mut transaction,
                &operation,
                command.tenant_id,
                command.idempotency_key.as_ref(),
                &fingerprint_hex,
                "unit",
                stored.unit_id(),
                command.now,
            )
            .await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UnitCommitOutcome {
                unit: stored,
                replayed: true,
            });
        }
        let mut updated = stored;
        updated
            .update(
                command.name.as_deref(),
                command.unit_type,
                command.status,
                command.now,
            )
            .map_err(|_| OrganizationStoreError::Failed)?;
        let result = sqlx::query(
            "UPDATE organization_units SET unit_type = $1, name = $2, status = $3, updated_at = $4, version = $5 WHERE tenant_id = $6 AND unit_id = $7 AND version = $8",
        )
        .bind(updated.unit_type().as_str())
        .bind(updated.name())
        .bind(updated.status().as_str())
        .bind(updated.updated_at())
        .bind(updated.version().value())
        .bind(command.tenant_id)
        .bind(command.unit_id)
        .bind(command.expected_version)
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(OrganizationStoreError::VersionConflict);
        }
        let mut changed_fields = Vec::new();
        if name_changed {
            changed_fields.push("name".to_string());
        }
        if type_changed {
            changed_fields.push("unit_type".to_string());
        }
        if status_changed {
            changed_fields.push("status".to_string());
        }
        let event = build_audit(
            command.tenant_id,
            "organization.unit.updated",
            "organization_unit",
            &command.unit_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({
                "unit_type": updated.unit_type().as_str(),
                "status": updated.status().as_str(),
            }),
            changed_fields,
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &event)
            .await
            .map_err(map_audit_error)?;
        record_idempotency(
            &mut transaction,
            &operation,
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
            "unit",
            updated.unit_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(UnitCommitOutcome {
            unit: updated,
            replayed: false,
        })
    }

    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn move_unit(
        &self,
        command: MoveUnitCommit,
    ) -> Result<UnitCommitOutcome, OrganizationStoreError> {
        let operation = format!("move_unit|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        let fingerprint_hex = fingerprint(&[
            &command.unit_id.to_string(),
            &command
                .new_parent_id
                .map_or_else(|| "root".to_string(), |id| id.to_string()),
            &command.expected_version.to_string(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "unit",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let unit = fetch_unit_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UnitCommitOutcome {
                unit,
                replayed: true,
            });
        }
        let stored = fetch_unit(&mut transaction, command.tenant_id, command.unit_id)
            .await?
            .ok_or(OrganizationStoreError::NotFound)?;
        if command.new_parent_id == Some(command.unit_id) {
            return Err(OrganizationStoreError::InvalidParent);
        }
        // In-transaction placement re-validation (contract rule 2): parent
        // first, then the full tenant tree walk, then the version guard —
        // the exact order the authoritative fakes enforce (a cycle is
        // rejected even when the version is stale).
        validate_parent(&mut transaction, command.tenant_id, command.new_parent_id).await?;
        let snapshot = tree_snapshot(&mut transaction, command.tenant_id).await?;
        validate_tree_placement(&snapshot, command.unit_id, command.new_parent_id)
            .map_err(|_| placement_error(command.new_parent_id, command.unit_id))?;
        if stored.version().value() != command.expected_version {
            return Err(OrganizationStoreError::VersionConflict);
        }
        if stored.parent_id() == command.new_parent_id {
            // Already placed: converges as a replay with no bump, no
            // audit, and — matching the fakes — no idempotency slot.
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UnitCommitOutcome {
                unit: stored,
                replayed: true,
            });
        }
        let mut updated = stored;
        updated
            .reparent(command.new_parent_id, command.now)
            .map_err(|_| OrganizationStoreError::Failed)?;
        let result = sqlx::query(
            "UPDATE organization_units SET parent_id = $1, updated_at = $2, version = $3 WHERE tenant_id = $4 AND unit_id = $5 AND version = $6",
        )
        .bind(command.new_parent_id)
        .bind(updated.updated_at())
        .bind(updated.version().value())
        .bind(command.tenant_id)
        .bind(command.unit_id)
        .bind(command.expected_version)
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(OrganizationStoreError::VersionConflict);
        }
        let event = build_audit(
            command.tenant_id,
            "organization.unit.moved",
            "organization_unit",
            &command.unit_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({
                "parent_id": command.new_parent_id.map(|id| id.to_string()),
            }),
            vec!["parent_id".to_string()],
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &event)
            .await
            .map_err(map_audit_error)?;
        record_idempotency(
            &mut transaction,
            &operation,
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
            "unit",
            updated.unit_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(UnitCommitOutcome {
            unit: updated,
            replayed: false,
        })
    }

    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn add_member(
        &self,
        command: AddMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError> {
        let operation = format!("add_member|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        let fingerprint_hex = fingerprint(&[
            &command.unit_id.to_string(),
            &command.user_id.to_string(),
            command.membership_type.as_str(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "member",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let membership = fetch_membership_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(MemberCommitOutcome {
                membership,
                replayed: true,
            });
        }
        let unit = fetch_unit(&mut transaction, command.tenant_id, command.unit_id)
            .await?
            .ok_or(OrganizationStoreError::NotFound)?;
        if !unit.is_active() {
            return Err(OrganizationStoreError::UnitDisabled);
        }
        let existing = fetch_membership_by_key(
            &mut transaction,
            command.tenant_id,
            command.unit_id,
            command.user_id,
            command.membership_type,
        )
        .await?;
        let (membership, action) = match existing {
            Some(stored) if stored.is_active() => {
                return Err(OrganizationStoreError::AlreadyExists);
            }
            Some(mut stored) => {
                // Reactivation convergence (contract rule 5): the inactive
                // row is re-armed in place (version bump, audit, replayed
                // false).
                let stored_version = stored.version().value();
                stored
                    .reactivate(command.now)
                    .map_err(|_| OrganizationStoreError::Failed)?;
                let result = sqlx::query(
                    "UPDATE organization_members SET status = $1, deactivated_at = NULL, version = $2 WHERE tenant_id = $3 AND unit_id = $4 AND user_id = $5 AND membership_type = $6 AND version = $7",
                )
                .bind(stored.status().as_str())
                .bind(stored.version().value())
                .bind(command.tenant_id)
                .bind(command.unit_id)
                .bind(command.user_id)
                .bind(command.membership_type.as_str())
                .bind(stored_version)
                .execute(&mut *transaction)
                .await
                .map_err(map_sqlx_error)?;
                if result.rows_affected() != 1 {
                    return Err(OrganizationStoreError::VersionConflict);
                }
                (stored, "organization.member.reactivated")
            }
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
                // The `(tenant, unit, user, type)` unique key is the
                // membership identity; a conflict means a concurrent add
                // won the race — match-and-continue into `AlreadyExists`.
                let inserted = sqlx::query(
                    "INSERT INTO organization_members (membership_id, tenant_id, user_id, unit_id, membership_type, status, joined_at, deactivated_at, version) VALUES ($1,$2,$3,$4,$5,$6,$7,NULL,1) ON CONFLICT (tenant_id, unit_id, user_id, membership_type) DO NOTHING",
                )
                .bind(joined.membership_id())
                .bind(joined.tenant_id())
                .bind(joined.user_id())
                .bind(joined.unit_id())
                .bind(joined.membership_type().as_str())
                .bind(joined.status().as_str())
                .bind(joined.joined_at())
                .execute(&mut *transaction)
                .await;
                let raced = match inserted {
                    Ok(result) => result.rows_affected() == 0,
                    Err(sqlx::Error::Database(db_error))
                        if db_error.code().as_deref() == Some("23505") =>
                    {
                        true
                    }
                    Err(error) => return Err(map_sqlx_error(error)),
                };
                if raced {
                    return Err(OrganizationStoreError::AlreadyExists);
                }
                (joined, "organization.member.added")
            }
        };
        let event = build_audit(
            command.tenant_id,
            action,
            "organization_membership",
            &membership.membership_id().to_string(),
            &command.audit,
            command.now,
            serde_json::json!({
                "membership_type": membership.membership_type().as_str(),
                "status": membership.status().as_str(),
                "user_id": membership.user_id().to_string(),
                "unit_id": membership.unit_id().to_string(),
            }),
            vec!["status".to_string()],
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &event)
            .await
            .map_err(map_audit_error)?;
        record_idempotency(
            &mut transaction,
            &operation,
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
            "member",
            membership.membership_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(MemberCommitOutcome {
            membership,
            replayed: false,
        })
    }

    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn remove_member(
        &self,
        command: RemoveMemberCommit,
    ) -> Result<MemberCommitOutcome, OrganizationStoreError> {
        let operation = format!("remove_member|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        let fingerprint_hex = fingerprint(&[
            &command.unit_id.to_string(),
            &command.user_id.to_string(),
            command.membership_type.as_str(),
            &command.expected_version.to_string(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "member",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let membership = fetch_membership_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(MemberCommitOutcome {
                membership,
                replayed: true,
            });
        }
        let stored = fetch_membership_by_key(
            &mut transaction,
            command.tenant_id,
            command.unit_id,
            command.user_id,
            command.membership_type,
        )
        .await?
        .ok_or(OrganizationStoreError::NotFound)?;
        if stored.version().value() != command.expected_version {
            return Err(OrganizationStoreError::VersionConflict);
        }
        if !stored.is_active() {
            // Already inactive: converges as a replay with no bump, no
            // audit, and — matching the fakes — no idempotency slot.
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(MemberCommitOutcome {
                membership: stored,
                replayed: true,
            });
        }
        let mut updated = stored;
        updated
            .deactivate(command.now)
            .map_err(|_| OrganizationStoreError::Failed)?;
        let result = sqlx::query(
            "UPDATE organization_members SET status = $1, deactivated_at = $2, version = $3 WHERE tenant_id = $4 AND unit_id = $5 AND user_id = $6 AND membership_type = $7 AND version = $8",
        )
        .bind(updated.status().as_str())
        .bind(
            updated
                .deactivated_at()
                .ok_or(OrganizationStoreError::Failed)?,
        )
        .bind(updated.version().value())
        .bind(command.tenant_id)
        .bind(command.unit_id)
        .bind(command.user_id)
        .bind(command.membership_type.as_str())
        .bind(command.expected_version)
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(OrganizationStoreError::VersionConflict);
        }
        let event = build_audit(
            command.tenant_id,
            "organization.member.removed",
            "organization_membership",
            &updated.membership_id().to_string(),
            &command.audit,
            command.now,
            serde_json::json!({
                "membership_type": updated.membership_type().as_str(),
                "status": updated.status().as_str(),
            }),
            vec!["status".to_string()],
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &event)
            .await
            .map_err(map_audit_error)?;
        record_idempotency(
            &mut transaction,
            &operation,
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
            "member",
            updated.membership_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(MemberCommitOutcome {
            membership: updated,
            replayed: false,
        })
    }
}

#[async_trait]
impl OrganizationQueryPort for PostgresOrganizationStore {
    async fn get_unit(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Option<OrganizationUnit>, OrganizationStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        fetch_unit(&mut connection, tenant_id, unit_id).await
    }

    async fn list_units(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<OrganizationUnit>, OrganizationStoreError> {
        let unit_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM organization_units WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        // Store-side cap mirroring the authoritative fakes: a listing that
        // exceeds the tenant cap fails closed (writes stop at the cap, so
        // this only trips on externally seeded catalogs).
        if usize::try_from(unit_count).unwrap_or(usize::MAX) > MAX_UNITS_PER_TENANT {
            return Err(OrganizationStoreError::TooManyResources);
        }
        // Deterministic order matching the fake's `(created_at, unit_id)`.
        let sql = format!(
            "SELECT {UNIT_COLUMNS} FROM organization_units WHERE tenant_id = $1 ORDER BY created_at, unit_id"
        );
        let rows = sqlx::query_as::<_, UnitRow>(&sql)
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(UnitRow::into_unit).collect()
    }

    async fn list_unit_members(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Vec<UnitMemberRecord>, OrganizationStoreError> {
        // Active rows only, join-ordered (inactive history stays hidden).
        let sql = format!(
            "SELECT {MEMBER_COLUMNS} FROM organization_members WHERE tenant_id = $1 AND unit_id = $2 AND status = 'active' ORDER BY joined_at, membership_id"
        );
        let rows = sqlx::query_as::<_, MemberRow>(&sql)
            .bind(tenant_id)
            .bind(unit_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                row.into_membership()
                    .map(|membership| UnitMemberRecord { membership })
            })
            .collect()
    }

    async fn list_user_memberships(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationStoreError> {
        let sql = format!(
            "SELECT {MEMBER_COLUMNS} FROM organization_members WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' ORDER BY joined_at, membership_id"
        );
        let rows = sqlx::query_as::<_, MemberRow>(&sql)
            .bind(tenant_id)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(MemberRow::into_membership).collect()
    }
}
