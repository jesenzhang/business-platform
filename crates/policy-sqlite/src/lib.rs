//! `SQLite` adapter for the Policy bounded context (local, single-process
//! profile). Implements the policy command/query ports as audit-carrying
//! units of work per the binding adapter contract in `policy::ports`:
//! unique-key rejection of duplicate role ids/keys, store-immutable system
//! roles, tenant-or-system role visibility, transactional set replace of
//! role permissions, identical-active binding convergence, optimistic
//! versioning through conditional updates, per-`(operation, tenant)`
//! idempotency with request-only fingerprints, store-side caps, and
//! in-transaction unified audit chaining.
//!
//! Mirrors `policy-postgres` semantics exactly in the `SQLite` dialect:
//! ids and timestamps live in TEXT columns (RFC 3339), writers serialize
//! through `BEGIN IMMEDIATE`, and `NULL`-safe column comparisons use `IS`.

use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

use policy::application::{
    MAX_BINDINGS_PER_TENANT, MAX_BINDINGS_PER_USER, MAX_ROLES_PER_TENANT, MAX_ROLE_PERMISSIONS,
};
use policy::domain::{
    BindingStatus, PermissionDefinition, PermissionKey, RehydrateRoleBinding,
    RehydrateRoleDefinition, ResourceScope, RoleBinding, RoleDefinition, RoleStatus,
    ValidityWindow,
};
use policy::ports::{
    BindRoleCommit, BindingCommitOutcome, CreateRoleCommit, MutationActorKind, MutationContext,
    PermissionsCommitOutcome, PolicyCommandPort, PolicyQueryPort, PolicyStoreError,
    RevokeBindingCommit, RoleCommitOutcome, SetRolePermissionsCommit, UpdateRoleCommit,
};

/// Apply the policy schema without colliding with the other `SQLite`
/// catalogs. `SQLx`'s built-in migrator uses the global `_sqlx_migrations`
/// table, which would collide between this crate and the document catalogs
/// sharing one local database, so policy keeps its own ledger table.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS policy_migrations (version INTEGER PRIMARY KEY, checksum BLOB NOT NULL, applied_at TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;
    let applied =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM policy_migrations")
            .fetch_one(pool)
            .await?;
    if applied < 1 {
        let mut transaction = pool.begin().await?;
        sqlx::raw_sql(include_str!("../migrations/001_policy_foundation.sql"))
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO policy_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(1_i64)
        .bind(include_bytes!("../migrations/001_policy_foundation.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    Ok(())
}

/// `SQLite` policy store. One adapter serves both policy ports. Intended
/// for the local, single-process profile: every mutation opens a
/// `BEGIN IMMEDIATE` transaction, which serializes writers process-wide.
#[derive(Clone)]
pub struct SqlitePolicyStore {
    pool: SqlitePool,
}

impl SqlitePolicyStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[must_use]
    pub fn command_port(self: &Arc<Self>) -> Arc<dyn PolicyCommandPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }

    #[must_use]
    pub fn query_port(self: &Arc<Self>) -> Arc<dyn PolicyQueryPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }
}

// ---------------------------------------------------------------------------
// error / mapping helpers
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)] // `map_err` hands the error over by value.
fn map_sqlx_error(error: sqlx::Error) -> PolicyStoreError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            PolicyStoreError::Unavailable
        }
        _ => PolicyStoreError::Failed,
    }
}

fn map_audit_error(_error: audit::AuditError) -> PolicyStoreError {
    PolicyStoreError::Failed
}

/// Canonical request fingerprint: SHA-256 over `|`-joined semantic request
/// fields. Never covers server timestamps, actors, reasons, or
/// server-generated ids (an auto id enters the fingerprint as `auto`).
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

/// Parse a TEXT-stored UUID; corrupted rows fail closed as `Failed`.
fn parse_uuid(raw: &str) -> Result<Uuid, PolicyStoreError> {
    Uuid::parse_str(raw).map_err(|_| PolicyStoreError::Failed)
}

fn parse_opt_uuid(raw: Option<&str>) -> Result<Option<Uuid>, PolicyStoreError> {
    raw.map(parse_uuid).transpose()
}

/// Parse a TEXT-stored RFC 3339 timestamp; corrupted rows fail closed.
fn parse_time(raw: &str) -> Result<DateTime<Utc>, PolicyStoreError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| PolicyStoreError::Failed)
}

fn parse_opt_time(raw: Option<&str>) -> Result<Option<DateTime<Utc>>, PolicyStoreError> {
    raw.map(parse_time).transpose()
}

fn parse_role_status(raw: &str) -> Result<RoleStatus, PolicyStoreError> {
    match raw {
        "active" => Ok(RoleStatus::Active),
        "disabled" => Ok(RoleStatus::Disabled),
        _ => Err(PolicyStoreError::Failed),
    }
}

fn parse_binding_status(raw: &str) -> Result<BindingStatus, PolicyStoreError> {
    match raw {
        "active" => Ok(BindingStatus::Active),
        "revoked" => Ok(BindingStatus::Revoked),
        _ => Err(PolicyStoreError::Failed),
    }
}

fn actor_of(audit: &MutationContext) -> Result<AuditActor, PolicyStoreError> {
    let actor_id = parse_uuid(&audit.actor_id)?;
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
) -> Result<AuditEvent, PolicyStoreError> {
    AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        actor_of(audit)?,
        AuditAction::new(action).map_err(|_| PolicyStoreError::Failed)?,
        AuditResource::new(resource_type, resource_id).map_err(|_| PolicyStoreError::Failed)?,
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
    .map_err(|_| PolicyStoreError::Failed)
}

// ---------------------------------------------------------------------------
// manual `SQLite` transaction control
//
// Mutations rely on `BEGIN IMMEDIATE` for single-process writer
// serialization instead of the `PostgreSQL` advisory locks the
// `policy-postgres` adapter takes; `SQLite` is only supported as a local,
// single-process deployment, so no key- or row-level lock primitive is
// needed here.
// ---------------------------------------------------------------------------

async fn begin_immediate(connection: &mut sqlx::SqliteConnection) -> Result<(), PolicyStoreError> {
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(map_sqlx_error)
}

async fn commit_tx(connection: &mut sqlx::SqliteConnection) -> Result<(), PolicyStoreError> {
    sqlx::query("COMMIT")
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(map_sqlx_error)
}

async fn rollback_tx(connection: &mut sqlx::SqliteConnection) -> Result<(), PolicyStoreError> {
    sqlx::query("ROLLBACK")
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(map_sqlx_error)
}

// ---------------------------------------------------------------------------
// scope persistence
// ---------------------------------------------------------------------------

/// Fingerprint segment for one scope, shaped exactly like the fake store's
/// scope encoding so replay semantics match across backends.
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

/// The five `role_bindings` scope columns written for one scope
/// (`scope_kind`, `scope_org_unit_id`, `scope_include_subtree`,
/// `scope_resource_kind`, `scope_resource_id`).
struct ScopeColumns {
    kind: &'static str,
    org_unit_id: Option<String>,
    include_subtree: i64,
    resource_kind: Option<String>,
    resource_id: Option<String>,
}

fn scope_columns(scope: &ResourceScope) -> ScopeColumns {
    match scope {
        ResourceScope::Tenant => ScopeColumns {
            kind: "tenant",
            org_unit_id: None,
            include_subtree: 0,
            resource_kind: None,
            resource_id: None,
        },
        ResourceScope::OrganizationUnit {
            org_unit_id,
            include_subtree,
        } => ScopeColumns {
            kind: "organization_unit",
            org_unit_id: Some(org_unit_id.to_string()),
            include_subtree: i64::from(*include_subtree),
            resource_kind: None,
            resource_id: None,
        },
        ResourceScope::ResourceType { kind } => ScopeColumns {
            kind: "resource_type",
            org_unit_id: None,
            include_subtree: 0,
            resource_kind: Some(kind.clone()),
            resource_id: None,
        },
        ResourceScope::Resource { kind, resource_id } => ScopeColumns {
            kind: "resource",
            org_unit_id: None,
            include_subtree: 0,
            resource_kind: Some(kind.clone()),
            resource_id: Some(resource_id.to_string()),
        },
    }
}

// ---------------------------------------------------------------------------
// rows
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct RoleRow {
    role_id: String,
    tenant_id: Option<String>,
    stable_key: String,
    display_name: String,
    status: String,
    system: i64,
    created_at: String,
    updated_at: String,
    version: i64,
}

impl RoleRow {
    fn into_role(self) -> Result<RoleDefinition, PolicyStoreError> {
        RoleDefinition::rehydrate(RehydrateRoleDefinition {
            role_id: parse_uuid(&self.role_id)?,
            tenant_id: parse_opt_uuid(self.tenant_id.as_deref())?,
            stable_key: self.stable_key,
            display_name: self.display_name,
            status: parse_role_status(&self.status)?,
            system: self.system != 0,
            created_at: parse_time(&self.created_at)?,
            updated_at: parse_time(&self.updated_at)?,
            version: self.version,
        })
        .map_err(|_| PolicyStoreError::Failed)
    }
}

#[derive(sqlx::FromRow)]
struct BindingRow {
    binding_id: String,
    tenant_id: String,
    user_id: String,
    role_id: String,
    scope_kind: String,
    scope_org_unit_id: Option<String>,
    scope_include_subtree: i64,
    scope_resource_kind: Option<String>,
    scope_resource_id: Option<String>,
    status: String,
    effective_at: String,
    expires_at: Option<String>,
    created_at: String,
    updated_at: String,
    version: i64,
}

impl BindingRow {
    fn into_binding(self) -> Result<RoleBinding, PolicyStoreError> {
        // Reconstruct the scope only through the domain constructors so the
        // persisted shape is re-validated on the way in; a row that does
        // not fit the bounded enum is corruption and fails closed.
        let scope = match self.scope_kind.as_str() {
            "tenant" => Ok(ResourceScope::Tenant),
            "organization_unit" => ResourceScope::organization_unit(
                parse_uuid(
                    self.scope_org_unit_id
                        .as_deref()
                        .ok_or(PolicyStoreError::Failed)?,
                )?,
                self.scope_include_subtree != 0,
            ),
            "resource_type" => ResourceScope::resource_type(
                self.scope_resource_kind.ok_or(PolicyStoreError::Failed)?,
            ),
            "resource" => ResourceScope::resource(
                self.scope_resource_kind.ok_or(PolicyStoreError::Failed)?,
                parse_uuid(
                    self.scope_resource_id
                        .as_deref()
                        .ok_or(PolicyStoreError::Failed)?,
                )?,
            ),
            _ => Err(policy::domain::PolicyDomainError::InvalidResourceKind(
                self.scope_kind.clone(),
            )),
        }
        .map_err(|_| PolicyStoreError::Failed)?;
        RoleBinding::rehydrate(RehydrateRoleBinding {
            binding_id: parse_uuid(&self.binding_id)?,
            tenant_id: parse_uuid(&self.tenant_id)?,
            user_id: parse_uuid(&self.user_id)?,
            role_id: parse_uuid(&self.role_id)?,
            scope,
            status: parse_binding_status(&self.status)?,
            effective_at: parse_time(&self.effective_at)?,
            expires_at: parse_opt_time(self.expires_at.as_deref())?,
            created_at: parse_time(&self.created_at)?,
            updated_at: parse_time(&self.updated_at)?,
            version: self.version,
        })
        .map_err(|_| PolicyStoreError::Failed)
    }
}

#[derive(sqlx::FromRow)]
struct PermissionRow {
    stable_key: String,
    description: String,
    reserved: i64,
    active: i64,
}

impl PermissionRow {
    fn into_definition(self) -> Result<PermissionDefinition, PolicyStoreError> {
        let key = PermissionKey::parse(&self.stable_key).map_err(|_| PolicyStoreError::Failed)?;
        Ok(PermissionDefinition::restored(
            key,
            self.description,
            self.reserved != 0,
            self.active != 0,
        ))
    }
}

const ROLE_COLUMNS: &str =
    "role_id, tenant_id, stable_key, display_name, status, system, created_at, updated_at, version";
const BINDING_COLUMNS: &str = "binding_id, tenant_id, user_id, role_id, scope_kind, scope_org_unit_id, scope_include_subtree, scope_resource_kind, scope_resource_id, status, effective_at, expires_at, created_at, updated_at, version";

async fn fetch_visible_role(
    conn: &mut sqlx::SqliteConnection,
    tenant_id: Uuid,
    role_id: Uuid,
) -> Result<Option<RoleDefinition>, PolicyStoreError> {
    // Role visibility: the tenant's own rows or migration-seeded system
    // rows (`tenant_id IS NULL`), nothing else.
    let sql = format!(
        "SELECT {ROLE_COLUMNS} FROM roles WHERE role_id = ?1 AND (tenant_id = ?2 OR tenant_id IS NULL)"
    );
    let row = sqlx::query_as::<_, RoleRow>(&sql)
        .bind(role_id.to_string())
        .bind(tenant_id.to_string())
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(RoleRow::into_role).transpose()
}

async fn fetch_role_in_tenant(
    conn: &mut sqlx::SqliteConnection,
    tenant_id: Uuid,
    role_id: Uuid,
) -> Result<RoleDefinition, PolicyStoreError> {
    fetch_visible_role(conn, tenant_id, role_id)
        .await?
        .ok_or(PolicyStoreError::Failed)
}

async fn fetch_binding(
    conn: &mut sqlx::SqliteConnection,
    tenant_id: Uuid,
    binding_id: Uuid,
) -> Result<Option<RoleBinding>, PolicyStoreError> {
    let sql = format!(
        "SELECT {BINDING_COLUMNS} FROM role_bindings WHERE binding_id = ?1 AND tenant_id = ?2"
    );
    let row = sqlx::query_as::<_, BindingRow>(&sql)
        .bind(binding_id.to_string())
        .bind(tenant_id.to_string())
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(BindingRow::into_binding).transpose()
}

async fn fetch_permission_keys(
    conn: &mut sqlx::SqliteConnection,
    role_id: Uuid,
) -> Result<Vec<String>, PolicyStoreError> {
    let keys = sqlx::query_scalar::<_, String>(
        "SELECT permission_key FROM role_permissions WHERE role_id = ?1 ORDER BY permission_key",
    )
    .bind(role_id.to_string())
    .fetch_all(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    Ok(keys)
}

// ---------------------------------------------------------------------------
// idempotency helpers (per (operation, tenant) keys)
// ---------------------------------------------------------------------------

// The `PostgreSQL` adapter wraps idempotent retries in an advisory lock;
// here the surrounding `BEGIN IMMEDIATE` already serializes writers in the
// single-process profile, so no extra lock primitive is needed.

#[derive(sqlx::FromRow)]
struct IdempotencyRow {
    request_fingerprint: String,
    result_kind: String,
    result_id: String,
}

async fn check_idempotency(
    conn: &mut sqlx::SqliteConnection,
    operation: &str,
    expected_kind: &str,
    tenant_id: Uuid,
    key: Option<&String>,
    expected_fingerprint: &str,
) -> Result<Option<IdempotencyRow>, PolicyStoreError> {
    let Some(key) = key else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, IdempotencyRow>(
        "SELECT request_fingerprint, result_kind, result_id FROM policy_idempotency WHERE tenant_id = ?1 AND operation = ?2 AND idempotency_key = ?3",
    )
    .bind(tenant_id.to_string())
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
                return Err(PolicyStoreError::Failed);
            }
            Ok(Some(row))
        }
        Some(_) => Err(PolicyStoreError::IdempotencyConflict),
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_idempotency(
    conn: &mut sqlx::SqliteConnection,
    operation: &str,
    tenant_id: Uuid,
    key: Option<&String>,
    fingerprint_hex: &str,
    result_kind: &str,
    result_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), PolicyStoreError> {
    let Some(key) = key else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO policy_idempotency (tenant_id, operation, idempotency_key, request_fingerprint, result_kind, result_id, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
    )
    .bind(tenant_id.to_string())
    .bind(operation)
    .bind(key)
    .bind(fingerprint_hex)
    .bind(result_kind)
    .bind(result_id.to_string())
    .bind(now.to_rfc3339())
    .execute(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// command transaction bodies (run inside a caller-owned BEGIN IMMEDIATE)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)] // one atomic multi-statement write per contract rule; kept linear on purpose.
async fn create_role_tx(
    conn: &mut sqlx::SqliteConnection,
    command: &CreateRoleCommit,
) -> Result<RoleCommitOutcome, PolicyStoreError> {
    let operation = format!("create_role|{}", command.tenant_id);
    let fingerprint_hex = fingerprint(&[
        &command
            .role_id
            .map_or_else(|| "auto".to_string(), |id| id.to_string()),
        &command.stable_key,
        &command.display_name,
    ]);
    if let Some(existing) = check_idempotency(
        conn,
        &operation,
        "role",
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
    )
    .await?
    {
        let role =
            fetch_role_in_tenant(conn, command.tenant_id, parse_uuid(&existing.result_id)?).await?;
        return Ok(RoleCommitOutcome {
            role,
            replayed: true,
        });
    }
    // Caller-chosen ids are globally unique (system role ids included).
    if let Some(role_id) = command.role_id {
        let id_taken = sqlx::query_scalar::<_, i64>("SELECT 1 FROM roles WHERE role_id = ?1")
            .bind(role_id.to_string())
            .fetch_optional(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
        if id_taken.is_some() {
            return Err(PolicyStoreError::AlreadyExists);
        }
    }
    // Stable keys are unique per tenant (the partial unique index mirrors
    // this pre-check under races).
    let key_taken = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM roles WHERE tenant_id = ?1 AND stable_key = ?2",
    )
    .bind(command.tenant_id.to_string())
    .bind(&command.stable_key)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    if key_taken.is_some() {
        return Err(PolicyStoreError::AlreadyExists);
    }
    // Authoritative tenant role cap: tenant-owned rows only (system roles
    // are global and never consume a tenant's budget).
    let tenant_roles =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM roles WHERE tenant_id = ?1")
            .bind(command.tenant_id.to_string())
            .fetch_one(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
    if usize::try_from(tenant_roles).unwrap_or(usize::MAX) >= MAX_ROLES_PER_TENANT {
        return Err(PolicyStoreError::TooManyResources);
    }
    let role_id = command.role_id.unwrap_or_else(Uuid::now_v7);
    let role = RoleDefinition::create_tenant_role(
        role_id,
        command.tenant_id,
        &command.stable_key,
        &command.display_name,
        command.now,
    )
    .map_err(|_| PolicyStoreError::Failed)?;
    let insert = sqlx::query(
        "INSERT INTO roles (role_id, tenant_id, stable_key, display_name, status, system, created_at, updated_at, version) VALUES (?1,?2,?3,?4,?5,0,?6,?6,1)",
    )
    .bind(role.role_id().to_string())
    .bind(command.tenant_id.to_string())
    .bind(role.stable_key().to_string())
    .bind(role.display_name().to_string())
    .bind(role.status().as_str())
    .bind(command.now.to_rfc3339())
    .execute(&mut *conn)
    .await;
    if let Err(error) = insert {
        // A racing create took the id or the stable key first.
        if matches!(&error, sqlx::Error::Database(db_error) if db_error.is_unique_violation()) {
            return Err(PolicyStoreError::AlreadyExists);
        }
        return Err(map_sqlx_error(error));
    }
    let event = build_audit(
        command.tenant_id,
        "policy.role.created",
        "role",
        &role_id.to_string(),
        &command.audit,
        command.now,
        serde_json::json!({
            "status": role.status().as_str(),
            "stable_key": role.stable_key(),
        }),
        vec!["status".to_string()],
    )?;
    audit_sqlite::append_sqlite_in_transaction(&mut *conn, &event)
        .await
        .map_err(map_audit_error)?;
    record_idempotency(
        conn,
        &operation,
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
        "role",
        role_id,
        command.now,
    )
    .await?;
    // Return the canonical stored row so idempotent replays compare equal.
    let role = fetch_role_in_tenant(conn, command.tenant_id, role_id).await?;
    Ok(RoleCommitOutcome {
        role,
        replayed: false,
    })
}

#[allow(clippy::too_many_lines)] // one atomic multi-statement write per contract rule; kept linear on purpose.
async fn update_role_tx(
    conn: &mut sqlx::SqliteConnection,
    command: &UpdateRoleCommit,
) -> Result<RoleCommitOutcome, PolicyStoreError> {
    let operation = format!("update_role|{}", command.tenant_id);
    let fingerprint_hex = fingerprint(&[
        &command.role_id.to_string(),
        command.display_name.as_deref().unwrap_or_default(),
        command.status.map_or("", |status| status.as_str()),
        &command.expected_version.to_string(),
    ]);
    if let Some(existing) = check_idempotency(
        conn,
        &operation,
        "role",
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
    )
    .await?
    {
        let role =
            fetch_role_in_tenant(conn, command.tenant_id, parse_uuid(&existing.result_id)?).await?;
        return Ok(RoleCommitOutcome {
            role,
            replayed: true,
        });
    }
    let stored = fetch_visible_role(conn, command.tenant_id, command.role_id)
        .await?
        .ok_or(PolicyStoreError::NotFound)?;
    if stored.is_system() {
        return Err(PolicyStoreError::RoleImmutable);
    }
    if stored.version().value() != command.expected_version {
        return Err(PolicyStoreError::VersionConflict);
    }
    let name_changed = command
        .display_name
        .as_deref()
        .is_some_and(|name| stored.display_name() != name.trim());
    let status_changed = command
        .status
        .is_some_and(|status| stored.status() != status);
    let changed = name_changed || status_changed;
    if changed {
        let mut updated = stored;
        updated
            .update(command.display_name.as_deref(), command.status, command.now)
            .map_err(|_| PolicyStoreError::Failed)?;
        let result = sqlx::query(
            "UPDATE roles SET display_name = ?1, status = ?2, updated_at = ?3, version = ?4 WHERE role_id = ?5 AND tenant_id = ?6 AND version = ?7",
        )
        .bind(updated.display_name().to_string())
        .bind(updated.status().as_str())
        .bind(command.now.to_rfc3339())
        .bind(updated.version().value())
        .bind(command.role_id.to_string())
        .bind(command.tenant_id.to_string())
        .bind(command.expected_version)
        .execute(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(PolicyStoreError::VersionConflict);
        }
        let mut changed_fields = Vec::new();
        if name_changed {
            changed_fields.push("display_name".to_string());
        }
        if status_changed {
            changed_fields.push("status".to_string());
        }
        let event = build_audit(
            command.tenant_id,
            "policy.role.updated",
            "role",
            &command.role_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({ "status": updated.status().as_str() }),
            changed_fields,
        )?;
        audit_sqlite::append_sqlite_in_transaction(&mut *conn, &event)
            .await
            .map_err(map_audit_error)?;
    }
    // Same-value convergence: no bump, no audit; still replayable.
    record_idempotency(
        conn,
        &operation,
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
        "role",
        command.role_id,
        command.now,
    )
    .await?;
    let role = fetch_role_in_tenant(conn, command.tenant_id, command.role_id).await?;
    Ok(RoleCommitOutcome {
        role,
        replayed: !changed,
    })
}

#[allow(clippy::too_many_lines)] // one atomic multi-statement write per contract rule; kept linear on purpose.
async fn set_role_permissions_tx(
    conn: &mut sqlx::SqliteConnection,
    command: &SetRolePermissionsCommit,
) -> Result<PermissionsCommitOutcome, PolicyStoreError> {
    let operation = format!("set_role_permissions|{}", command.tenant_id);
    let mut sorted = command.permission_keys.clone();
    sorted.sort();
    sorted.dedup();
    let fingerprint_hex = fingerprint(&[
        &command.role_id.to_string(),
        &sorted.join(","),
        &command.expected_version.to_string(),
    ]);
    if let Some(existing) = check_idempotency(
        conn,
        &operation,
        "role",
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
    )
    .await?
    {
        let role_id = parse_uuid(&existing.result_id)?;
        let role = fetch_role_in_tenant(conn, command.tenant_id, role_id).await?;
        let permission_keys = fetch_permission_keys(conn, role_id).await?;
        return Ok(PermissionsCommitOutcome {
            role,
            permission_keys,
            replayed: true,
        });
    }
    let stored = fetch_visible_role(conn, command.tenant_id, command.role_id)
        .await?
        .ok_or(PolicyStoreError::NotFound)?;
    if stored.is_system() {
        return Err(PolicyStoreError::RoleImmutable);
    }
    if stored.version().value() != command.expected_version {
        return Err(PolicyStoreError::VersionConflict);
    }
    // The cap covers the requested key count (mirrors the authoritative
    // fake), before dedup.
    if command.permission_keys.len() > MAX_ROLE_PERMISSIONS {
        return Err(PolicyStoreError::TooManyPermissions);
    }
    // Every key must exist in the catalog (retired rows stay catalogued
    // and grantable; only missing keys are UnknownPermission).
    if !sorted.is_empty() {
        let placeholders: Vec<String> = (1..=sorted.len())
            .map(|index| format!("?{index}"))
            .collect();
        let sql = format!(
            "SELECT COUNT(*) FROM permission_definitions WHERE stable_key IN ({})",
            placeholders.join(",")
        );
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for key in &sorted {
            query = query.bind(key.clone());
        }
        let known = query.fetch_one(&mut *conn).await.map_err(map_sqlx_error)?;
        if usize::try_from(known).unwrap_or(usize::MAX) < sorted.len() {
            return Err(PolicyStoreError::UnknownPermission);
        }
    }
    let current = fetch_permission_keys(conn, command.role_id).await?;
    let same_set = current == sorted;
    if !same_set {
        let mut updated = stored;
        updated
            .record_permissions_replaced(command.now)
            .map_err(|_| PolicyStoreError::Failed)?;
        let result = sqlx::query(
            "UPDATE roles SET updated_at = ?1, version = ?2 WHERE role_id = ?3 AND tenant_id = ?4 AND version = ?5",
        )
        .bind(command.now.to_rfc3339())
        .bind(updated.version().value())
        .bind(command.role_id.to_string())
        .bind(command.tenant_id.to_string())
        .bind(command.expected_version)
        .execute(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(PolicyStoreError::VersionConflict);
        }
        // Transactional set replace of the role's permission rows.
        sqlx::query("DELETE FROM role_permissions WHERE role_id = ?1")
            .bind(command.role_id.to_string())
            .execute(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
        for key in &sorted {
            sqlx::query("INSERT INTO role_permissions (role_id, permission_key) VALUES (?1,?2)")
                .bind(command.role_id.to_string())
                .bind(key.clone())
                .execute(&mut *conn)
                .await
                .map_err(map_sqlx_error)?;
        }
        let event = build_audit(
            command.tenant_id,
            "policy.role.permissions_updated",
            "role",
            &command.role_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({ "permission_count": sorted.len() }),
            vec!["permissions".to_string()],
        )?;
        audit_sqlite::append_sqlite_in_transaction(&mut *conn, &event)
            .await
            .map_err(map_audit_error)?;
    }
    // Same-set convergence: no bump, no audit; still replayable.
    record_idempotency(
        conn,
        &operation,
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
        "role",
        command.role_id,
        command.now,
    )
    .await?;
    let role = fetch_role_in_tenant(conn, command.tenant_id, command.role_id).await?;
    Ok(PermissionsCommitOutcome {
        role,
        permission_keys: sorted,
        replayed: same_set,
    })
}

#[allow(clippy::too_many_lines)] // one atomic multi-statement write per contract rule; kept linear on purpose.
async fn bind_role_tx(
    conn: &mut sqlx::SqliteConnection,
    command: &BindRoleCommit,
) -> Result<BindingCommitOutcome, PolicyStoreError> {
    let operation = format!("bind_role|{}", command.tenant_id);
    let fingerprint_hex = fingerprint(&[
        &command.user_id.to_string(),
        &command.role_id.to_string(),
        &scope_fingerprint(&command.scope),
        &command.effective_at.to_rfc3339(),
        &command
            .expires_at
            .map_or_else(|| "open".to_string(), |at| at.to_rfc3339()),
        &command
            .binding_id
            .map_or_else(|| "auto".to_string(), |id| id.to_string()),
    ]);
    if let Some(existing) = check_idempotency(
        conn,
        &operation,
        "binding",
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
    )
    .await?
    {
        let binding = fetch_binding(conn, command.tenant_id, parse_uuid(&existing.result_id)?)
            .await?
            .ok_or(PolicyStoreError::Failed)?;
        return Ok(BindingCommitOutcome {
            binding,
            replayed: true,
        });
    }
    // Role visibility inside the write transaction: own or system roles
    // only, never a foreign tenant's role.
    if fetch_visible_role(conn, command.tenant_id, command.role_id)
        .await?
        .is_none()
    {
        return Err(PolicyStoreError::NotFound);
    }
    // Identical active binding convergence (no duplicate spam). `IS`
    // compares the nullable scope/window columns `NULL`-safely.
    let scope = scope_columns(&command.scope);
    let existing = sqlx::query_as::<_, BindingRow>(&format!(
        "SELECT {BINDING_COLUMNS} FROM role_bindings WHERE tenant_id = ?1 AND user_id = ?2 AND role_id = ?3 AND scope_kind = ?4 AND scope_org_unit_id IS ?5 AND scope_include_subtree = ?6 AND scope_resource_kind IS ?7 AND scope_resource_id IS ?8 AND status = 'active' AND effective_at = ?9 AND expires_at IS ?10",
    ))
    .bind(command.tenant_id.to_string())
    .bind(command.user_id.to_string())
    .bind(command.role_id.to_string())
    .bind(scope.kind)
    .bind(scope.org_unit_id.clone())
    .bind(scope.include_subtree)
    .bind(scope.resource_kind.clone())
    .bind(scope.resource_id.clone())
    .bind(command.effective_at.to_rfc3339())
    .bind(command.expires_at.map(|at| at.to_rfc3339()))
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    if let Some(existing) = existing {
        return Ok(BindingCommitOutcome {
            binding: existing.into_binding()?,
            replayed: true,
        });
    }
    // Authoritative caps (evaluated after convergence so idempotent
    // replays never trip them): total binding rows in the tenant (all
    // statuses — rows are retained), then per-user active rows.
    let tenant_total =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM role_bindings WHERE tenant_id = ?1")
            .bind(command.tenant_id.to_string())
            .fetch_one(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
    if usize::try_from(tenant_total).unwrap_or(usize::MAX) >= MAX_BINDINGS_PER_TENANT {
        return Err(PolicyStoreError::TooManyResources);
    }
    let user_active = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM role_bindings WHERE tenant_id = ?1 AND user_id = ?2 AND status = 'active'",
    )
    .bind(command.tenant_id.to_string())
    .bind(command.user_id.to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    if usize::try_from(user_active).unwrap_or(usize::MAX) >= MAX_BINDINGS_PER_USER {
        return Err(PolicyStoreError::TooManyResources);
    }
    let binding_id = command.binding_id.unwrap_or_else(Uuid::now_v7);
    let id_taken =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM role_bindings WHERE binding_id = ?1")
            .bind(binding_id.to_string())
            .fetch_optional(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
    if id_taken.is_some() {
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
    let insert = sqlx::query(
        "INSERT INTO role_bindings (binding_id, tenant_id, user_id, role_id, scope_kind, scope_org_unit_id, scope_include_subtree, scope_resource_kind, scope_resource_id, status, effective_at, expires_at, created_at, updated_at, version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,?11,?12,?12,1)",
    )
    .bind(binding.binding_id().to_string())
    .bind(binding.tenant_id().to_string())
    .bind(binding.user_id().to_string())
    .bind(binding.role_id().to_string())
    .bind(scope.kind)
    .bind(scope.org_unit_id)
    .bind(scope.include_subtree)
    .bind(scope.resource_kind)
    .bind(scope.resource_id)
    .bind(binding.effective_at().to_rfc3339())
    .bind(binding.expires_at().map(|at| at.to_rfc3339()))
    .bind(command.now.to_rfc3339())
    .execute(&mut *conn)
    .await;
    if let Err(error) = insert {
        // A racing bind took the id first.
        if matches!(&error, sqlx::Error::Database(db_error) if db_error.is_unique_violation()) {
            return Err(PolicyStoreError::AlreadyExists);
        }
        return Err(map_sqlx_error(error));
    }
    let event = build_audit(
        command.tenant_id,
        "policy.binding.created",
        "role_binding",
        &binding_id.to_string(),
        &command.audit,
        command.now,
        serde_json::json!({
            "status": binding.status().as_str(),
            "scope": binding.scope().kind_label(),
        }),
        vec!["status".to_string()],
    )?;
    audit_sqlite::append_sqlite_in_transaction(&mut *conn, &event)
        .await
        .map_err(map_audit_error)?;
    record_idempotency(
        conn,
        &operation,
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
        "binding",
        binding_id,
        command.now,
    )
    .await?;
    // Return the canonical stored row so idempotent replays compare equal.
    let binding = fetch_binding(conn, command.tenant_id, binding_id)
        .await?
        .ok_or(PolicyStoreError::Failed)?;
    Ok(BindingCommitOutcome {
        binding,
        replayed: false,
    })
}

#[allow(clippy::too_many_lines)] // one atomic multi-statement write per contract rule; kept linear on purpose.
async fn revoke_binding_tx(
    conn: &mut sqlx::SqliteConnection,
    command: &RevokeBindingCommit,
) -> Result<BindingCommitOutcome, PolicyStoreError> {
    let operation = format!("revoke_binding|{}", command.tenant_id);
    let fingerprint_hex = fingerprint(&[
        &command.binding_id.to_string(),
        &command.expected_version.to_string(),
    ]);
    if let Some(existing) = check_idempotency(
        conn,
        &operation,
        "binding",
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
    )
    .await?
    {
        let binding = fetch_binding(conn, command.tenant_id, parse_uuid(&existing.result_id)?)
            .await?
            .ok_or(PolicyStoreError::Failed)?;
        return Ok(BindingCommitOutcome {
            binding,
            replayed: true,
        });
    }
    let stored = fetch_binding(conn, command.tenant_id, command.binding_id)
        .await?
        .ok_or(PolicyStoreError::NotFound)?;
    if stored.version().value() != command.expected_version {
        return Err(PolicyStoreError::VersionConflict);
    }
    if !stored.is_active() {
        // Already-revoked converges with no bump and no audit (mirrors the
        // authoritative fake, which stores no idempotency slot on this
        // path either).
        return Ok(BindingCommitOutcome {
            binding: stored,
            replayed: true,
        });
    }
    let mut updated = stored;
    updated
        .revoke(command.now)
        .map_err(|_| PolicyStoreError::Failed)?;
    let result = sqlx::query(
        "UPDATE role_bindings SET status = 'revoked', updated_at = ?1, version = ?2 WHERE binding_id = ?3 AND tenant_id = ?4 AND version = ?5",
    )
    .bind(command.now.to_rfc3339())
    .bind(updated.version().value())
    .bind(command.binding_id.to_string())
    .bind(command.tenant_id.to_string())
    .bind(command.expected_version)
    .execute(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    if result.rows_affected() != 1 {
        return Err(PolicyStoreError::VersionConflict);
    }
    let event = build_audit(
        command.tenant_id,
        "policy.binding.revoked",
        "role_binding",
        &command.binding_id.to_string(),
        &command.audit,
        command.now,
        serde_json::json!({ "status": updated.status().as_str() }),
        vec!["status".to_string()],
    )?;
    audit_sqlite::append_sqlite_in_transaction(&mut *conn, &event)
        .await
        .map_err(map_audit_error)?;
    record_idempotency(
        conn,
        &operation,
        command.tenant_id,
        command.idempotency_key.as_ref(),
        &fingerprint_hex,
        "binding",
        command.binding_id,
        command.now,
    )
    .await?;
    // Return the canonical stored row so idempotent replays compare equal.
    let binding = fetch_binding(conn, command.tenant_id, command.binding_id)
        .await?
        .ok_or(PolicyStoreError::Failed)?;
    Ok(BindingCommitOutcome {
        binding,
        replayed: false,
    })
}

// ---------------------------------------------------------------------------
// ports
// ---------------------------------------------------------------------------

#[async_trait]
impl PolicyCommandPort for SqlitePolicyStore {
    async fn create_role(
        &self,
        command: CreateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        // `BEGIN IMMEDIATE` provides the single-process writer serialization
        // that the `PostgreSQL` adapter achieves with advisory locks.
        begin_immediate(&mut connection).await?;
        let result = create_role_tx(&mut connection, &command).await;
        match result {
            Ok(outcome) => {
                commit_tx(&mut connection).await?;
                Ok(outcome)
            }
            Err(error) => {
                rollback_tx(&mut connection).await?;
                Err(error)
            }
        }
    }

    async fn update_role(
        &self,
        command: UpdateRoleCommit,
    ) -> Result<RoleCommitOutcome, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        // `BEGIN IMMEDIATE` provides the single-process writer serialization
        // that the `PostgreSQL` adapter achieves with advisory locks.
        begin_immediate(&mut connection).await?;
        let result = update_role_tx(&mut connection, &command).await;
        match result {
            Ok(outcome) => {
                commit_tx(&mut connection).await?;
                Ok(outcome)
            }
            Err(error) => {
                rollback_tx(&mut connection).await?;
                Err(error)
            }
        }
    }

    async fn set_role_permissions(
        &self,
        command: SetRolePermissionsCommit,
    ) -> Result<PermissionsCommitOutcome, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        // `BEGIN IMMEDIATE` provides the single-process writer serialization
        // that the `PostgreSQL` adapter achieves with advisory locks.
        begin_immediate(&mut connection).await?;
        let result = set_role_permissions_tx(&mut connection, &command).await;
        match result {
            Ok(outcome) => {
                commit_tx(&mut connection).await?;
                Ok(outcome)
            }
            Err(error) => {
                rollback_tx(&mut connection).await?;
                Err(error)
            }
        }
    }

    async fn bind_role(
        &self,
        command: BindRoleCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        // `BEGIN IMMEDIATE` provides the single-process writer serialization
        // that the `PostgreSQL` adapter achieves with advisory locks.
        begin_immediate(&mut connection).await?;
        let result = bind_role_tx(&mut connection, &command).await;
        match result {
            Ok(outcome) => {
                commit_tx(&mut connection).await?;
                Ok(outcome)
            }
            Err(error) => {
                rollback_tx(&mut connection).await?;
                Err(error)
            }
        }
    }

    async fn revoke_binding(
        &self,
        command: RevokeBindingCommit,
    ) -> Result<BindingCommitOutcome, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        // `BEGIN IMMEDIATE` provides the single-process writer serialization
        // that the `PostgreSQL` adapter achieves with advisory locks.
        begin_immediate(&mut connection).await?;
        let result = revoke_binding_tx(&mut connection, &command).await;
        match result {
            Ok(outcome) => {
                commit_tx(&mut connection).await?;
                Ok(outcome)
            }
            Err(error) => {
                rollback_tx(&mut connection).await?;
                Err(error)
            }
        }
    }
}

#[async_trait]
impl PolicyQueryPort for SqlitePolicyStore {
    async fn get_permission(
        &self,
        key: &PermissionKey,
    ) -> Result<Option<PermissionDefinition>, PolicyStoreError> {
        // Retired (`active = false`) rows are returned; the evaluator
        // treats them as unknown.
        let row = sqlx::query_as::<_, PermissionRow>(
            "SELECT stable_key, description, reserved, active FROM permission_definitions WHERE stable_key = ?1",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        row.map(PermissionRow::into_definition).transpose()
    }

    async fn list_permissions(&self) -> Result<Vec<PermissionDefinition>, PolicyStoreError> {
        let rows = sqlx::query_as::<_, PermissionRow>(
            "SELECT stable_key, description, reserved, active FROM permission_definitions ORDER BY stable_key",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(PermissionRow::into_definition)
            .collect()
    }

    async fn get_role(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<RoleDefinition>, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        fetch_visible_role(&mut connection, tenant_id, role_id).await
    }

    async fn list_roles(&self, tenant_id: Uuid) -> Result<Vec<RoleDefinition>, PolicyStoreError> {
        let sql = format!(
            "SELECT {ROLE_COLUMNS} FROM roles WHERE tenant_id = ?1 OR tenant_id IS NULL ORDER BY stable_key"
        );
        let rows = sqlx::query_as::<_, RoleRow>(&sql)
            .bind(tenant_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(RoleRow::into_role).collect()
    }

    async fn get_role_permissions(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Vec<String>, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        if fetch_visible_role(&mut connection, tenant_id, role_id)
            .await?
            .is_none()
        {
            return Err(PolicyStoreError::NotFound);
        }
        fetch_permission_keys(&mut connection, role_id).await
    }

    async fn list_bindings_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError> {
        // Every status: the evaluator distinguishes "revoked" from
        // "never bound". Ordered by binding id for deterministic deny
        // selection.
        let sql = format!(
            "SELECT {BINDING_COLUMNS} FROM role_bindings WHERE tenant_id = ?1 AND user_id = ?2 ORDER BY binding_id"
        );
        let rows = sqlx::query_as::<_, BindingRow>(&sql)
            .bind(tenant_id.to_string())
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(BindingRow::into_binding).collect()
    }

    async fn list_active_bindings_for_role(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError> {
        let sql = format!(
            "SELECT {BINDING_COLUMNS} FROM role_bindings WHERE tenant_id = ?1 AND role_id = ?2 AND status = 'active' ORDER BY binding_id"
        );
        let rows = sqlx::query_as::<_, BindingRow>(&sql)
            .bind(tenant_id.to_string())
            .bind(role_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(BindingRow::into_binding).collect()
    }

    async fn list_bindings(
        &self,
        tenant_id: Uuid,
        user_filter: Option<Uuid>,
    ) -> Result<Vec<RoleBinding>, PolicyStoreError> {
        // Bounded listing: more rows than the tenant cap is a store-state
        // violation, never an unbounded response.
        let count_sql = if user_filter.is_some() {
            "SELECT COUNT(*) FROM role_bindings WHERE tenant_id = ?1 AND user_id = ?2"
        } else {
            "SELECT COUNT(*) FROM role_bindings WHERE tenant_id = ?1"
        };
        let mut count_query = sqlx::query_scalar::<_, i64>(count_sql).bind(tenant_id.to_string());
        if let Some(user_id) = user_filter {
            count_query = count_query.bind(user_id.to_string());
        }
        let count = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        if usize::try_from(count).unwrap_or(usize::MAX) > MAX_BINDINGS_PER_TENANT {
            return Err(PolicyStoreError::TooManyResources);
        }
        let sql = if user_filter.is_some() {
            format!(
                "SELECT {BINDING_COLUMNS} FROM role_bindings WHERE tenant_id = ?1 AND user_id = ?2 ORDER BY binding_id"
            )
        } else {
            format!(
                "SELECT {BINDING_COLUMNS} FROM role_bindings WHERE tenant_id = ?1 ORDER BY binding_id"
            )
        };
        let mut query = sqlx::query_as::<_, BindingRow>(&sql).bind(tenant_id.to_string());
        if let Some(user_id) = user_filter {
            query = query.bind(user_id.to_string());
        }
        let rows = query.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;
        rows.into_iter().map(BindingRow::into_binding).collect()
    }

    async fn get_binding(
        &self,
        tenant_id: Uuid,
        binding_id: Uuid,
    ) -> Result<Option<RoleBinding>, PolicyStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        fetch_binding(&mut connection, tenant_id, binding_id).await
    }
}
