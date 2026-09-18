//! `PostgreSQL` adapter for the Platform Identity context (production
//! authority). Implements the identity ports as audit-carrying units of
//! work per the binding adapter contract in `identity::ports`: unique-key
//! match-and-continue, fail-closed principal mismatch, conditional-update
//! optimistic versioning, per-`(operation, tenant)` idempotency with
//! request-only fingerprints, and in-transaction unified audit chaining.

use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use identity::domain::{
    ExternalIdentity, MembershipSource, MembershipStatus, PlatformUser, RehydratePlatformUser,
    RehydrateTenantMembership, TenantMembership, UserLifecycleStatus,
};
use identity::ports::{
    BootstrapLedgerEntry, BootstrapLedgerPort, BootstrapOutcome, ChangeMembershipStatusCommit,
    ChangeUserStatusCommit, CreateMembershipCommit, IdentityCommandPort, IdentityQueryPort,
    IdentityResolvePort, IdentityStoreError, KeysetPosition, MembershipCommitOutcome,
    MembershipRecord, MembershipTarget, MutationActorKind, MutationContext, ResolvePrincipalCommit,
    ResolvedPrincipal, TenantUserRecord, UserCommitOutcome, PLATFORM_AUDIT_TENANT,
};

/// `PostgreSQL` identity store. One adapter serves all four identity ports.
#[derive(Clone)]
pub struct PostgresIdentityStore {
    pool: PgPool,
}

impl PostgresIdentityStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    #[must_use]
    pub fn resolve_port(self: &Arc<Self>) -> Arc<dyn IdentityResolvePort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }

    #[must_use]
    pub fn command_port(self: &Arc<Self>) -> Arc<dyn IdentityCommandPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }

    #[must_use]
    pub fn query_port(self: &Arc<Self>) -> Arc<dyn IdentityQueryPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }

    #[must_use]
    pub fn ledger_port(self: &Arc<Self>) -> Arc<dyn BootstrapLedgerPort> {
        let store: Arc<Self> = Arc::clone(self);
        store
    }
}

// ---------------------------------------------------------------------------
// error / mapping helpers
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)] // `map_err` hands the error over by value.
fn map_sqlx_error(error: sqlx::Error) -> IdentityStoreError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            IdentityStoreError::Unavailable
        }
        _ => IdentityStoreError::Failed,
    }
}

fn map_audit_error(_error: audit::AuditError) -> IdentityStoreError {
    IdentityStoreError::Failed
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

fn parse_user_status(raw: &str) -> Result<UserLifecycleStatus, IdentityStoreError> {
    match raw {
        "active" => Ok(UserLifecycleStatus::Active),
        "disabled" => Ok(UserLifecycleStatus::Disabled),
        _ => Err(IdentityStoreError::Failed),
    }
}

fn parse_membership_status(raw: &str) -> Result<MembershipStatus, IdentityStoreError> {
    match raw {
        "active" => Ok(MembershipStatus::Active),
        "suspended" => Ok(MembershipStatus::Suspended),
        _ => Err(IdentityStoreError::Failed),
    }
}

fn parse_membership_source(raw: &str) -> Result<MembershipSource, IdentityStoreError> {
    match raw {
        "bootstrap" => Ok(MembershipSource::Bootstrap),
        "admin" => Ok(MembershipSource::Admin),
        "migration" => Ok(MembershipSource::Migration),
        _ => Err(IdentityStoreError::Failed),
    }
}

fn actor_of(audit: &MutationContext) -> Result<AuditActor, IdentityStoreError> {
    let actor_id = Uuid::parse_str(&audit.actor_id).map_err(|_| IdentityStoreError::Failed)?;
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
) -> Result<AuditEvent, IdentityStoreError> {
    AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        actor_of(audit)?,
        AuditAction::new(action).map_err(|_| IdentityStoreError::Failed)?,
        AuditResource::new(resource_type, resource_id).map_err(|_| IdentityStoreError::Failed)?,
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
    .map_err(|_| IdentityStoreError::Failed)
}

// ---------------------------------------------------------------------------
// rows
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct UserRow {
    user_id: Uuid,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i64,
}

impl UserRow {
    fn into_user(self) -> Result<PlatformUser, IdentityStoreError> {
        PlatformUser::rehydrate(RehydratePlatformUser {
            user_id: self.user_id,
            status: parse_user_status(&self.status)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            version: self.version,
        })
        .map_err(|_| IdentityStoreError::Failed)
    }
}

#[derive(sqlx::FromRow)]
struct MembershipRow {
    membership_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    status: String,
    joined_at: DateTime<Utc>,
    suspended_at: Option<DateTime<Utc>>,
    source: String,
    version: i64,
}

impl MembershipRow {
    fn into_membership(self) -> Result<TenantMembership, IdentityStoreError> {
        TenantMembership::rehydrate(RehydrateTenantMembership {
            membership_id: self.membership_id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            status: parse_membership_status(&self.status)?,
            joined_at: self.joined_at,
            suspended_at: self.suspended_at,
            source: parse_membership_source(&self.source)?,
            version: self.version,
        })
        .map_err(|_| IdentityStoreError::Failed)
    }
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    external_identity_id: Uuid,
    issuer: String,
    subject: String,
    user_id: Uuid,
    linked_at: DateTime<Utc>,
    version: i64,
}

impl LinkRow {
    fn into_link(self) -> Result<ExternalIdentity, IdentityStoreError> {
        ExternalIdentity::rehydrate(
            self.external_identity_id,
            self.issuer,
            self.subject,
            self.user_id,
            self.linked_at,
            self.version,
        )
        .map_err(|_| IdentityStoreError::Failed)
    }
}

const USER_COLUMNS: &str = "user_id, status, created_at, updated_at, version";
const MEMBERSHIP_COLUMNS: &str =
    "membership_id, tenant_id, user_id, status, joined_at, suspended_at, source, version";
const LINK_COLUMNS: &str = "external_identity_id, issuer, subject, user_id, linked_at, version";

async fn fetch_user(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
) -> Result<Option<PlatformUser>, IdentityStoreError> {
    let sql = format!("SELECT {USER_COLUMNS} FROM platform_users WHERE user_id = $1");
    let row = sqlx::query_as::<_, UserRow>(&sql)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(UserRow::into_user).transpose()
}

async fn fetch_membership(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<TenantMembership>, IdentityStoreError> {
    let sql = format!(
        "SELECT {MEMBERSHIP_COLUMNS} FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2"
    );
    let row = sqlx::query_as::<_, MembershipRow>(&sql)
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(MembershipRow::into_membership).transpose()
}

async fn fetch_membership_by_id(
    conn: &mut sqlx::PgConnection,
    membership_id: Uuid,
) -> Result<TenantMembership, IdentityStoreError> {
    let sql =
        format!("SELECT {MEMBERSHIP_COLUMNS} FROM tenant_memberships WHERE membership_id = $1");
    let row = sqlx::query_as::<_, MembershipRow>(&sql)
        .bind(membership_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?
        .ok_or(IdentityStoreError::Failed)?;
    row.into_membership()
}

async fn fetch_link(
    conn: &mut sqlx::PgConnection,
    issuer: &str,
    subject: &str,
) -> Result<Option<ExternalIdentity>, IdentityStoreError> {
    let sql = format!(
        "SELECT {LINK_COLUMNS} FROM external_identities WHERE issuer = $1 AND subject = $2"
    );
    let row = sqlx::query_as::<_, LinkRow>(&sql)
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(LinkRow::into_link).transpose()
}

async fn fetch_link_by_user(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
) -> Result<Option<ExternalIdentity>, IdentityStoreError> {
    let sql = format!("SELECT {LINK_COLUMNS} FROM external_identities WHERE user_id = $1");
    let row = sqlx::query_as::<_, LinkRow>(&sql)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_sqlx_error)?;
    row.map(LinkRow::into_link).transpose()
}

async fn external_subjects_for(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
) -> Result<Vec<(String, String)>, IdentityStoreError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT issuer, subject FROM external_identities WHERE user_id = $1
         ORDER BY issuer COLLATE \"C\", subject COLLATE \"C\"",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows)
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
) -> Result<(), IdentityStoreError> {
    let guard = format!("identity:{operation}|{tenant_id}|{key}");
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
) -> Result<Option<IdempotencyRow>, IdentityStoreError> {
    let Some(key) = key else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, IdempotencyRow>(
        "SELECT request_fingerprint, result_kind, result_id FROM identity_idempotency WHERE tenant_id = $1 AND operation = $2 AND idempotency_key = $3",
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
                return Err(IdentityStoreError::Failed);
            }
            Ok(Some(row))
        }
        Some(_) => Err(IdentityStoreError::IdempotencyConflict),
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
) -> Result<(), IdentityStoreError> {
    let Some(key) = key else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO identity_idempotency (tenant_id, operation, idempotency_key, request_fingerprint, result_kind, result_id, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
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
// provisioning (shared by resolve and external-target membership create)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
async fn provision_in_tx(
    conn: &mut sqlx::PgConnection,
    issuer: &str,
    subject: &str,
    claimed_user_id: Option<Uuid>,
    deterministic_user_id: Uuid,
    audit: &MutationContext,
    now: DateTime<Utc>,
) -> Result<ResolvedPrincipal, IdentityStoreError> {
    // Existing key ⇒ match (claim, if any, must agree).
    if let Some(existing) = fetch_link(conn, issuer, subject).await? {
        if claimed_user_id.is_some_and(|claimed| claimed != existing.user_id()) {
            return Err(IdentityStoreError::PrincipalMismatch);
        }
        let user = fetch_user(conn, existing.user_id())
            .await?
            .ok_or(IdentityStoreError::Failed)?;
        return Ok(ResolvedPrincipal {
            user,
            external_identity: existing,
            provisioned: false,
        });
    }

    let candidate = claimed_user_id.unwrap_or(deterministic_user_id);
    // Fail closed: a user already linked to a *different* external key can
    // never adopt another subject silently.
    if let Some(other) = fetch_link_by_user(conn, candidate).await? {
        if !other.matches(issuer, subject) {
            return Err(IdentityStoreError::PrincipalMismatch);
        }
        // Linked exactly to this key ⇒ match (a racing provision completed
        // between the two reads).
        let user = fetch_user(conn, candidate)
            .await?
            .ok_or(IdentityStoreError::Failed)?;
        return Ok(ResolvedPrincipal {
            user,
            external_identity: other,
            provisioned: false,
        });
    }

    // Create the user when absent (match-and-continue on the primary key).
    let inserted = sqlx::query(
        "INSERT INTO platform_users (user_id, status, created_at, updated_at, version) VALUES ($1,'active',$2,$2,1) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(candidate)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    let provisioned = inserted.rows_affected() == 1;
    let user = fetch_user(conn, candidate)
        .await?
        .ok_or(IdentityStoreError::Failed)?;

    // Link insert. `ON CONFLICT (user_id) DO NOTHING` handles the candidate
    // being claimed concurrently; a unique violation on (issuer, subject)
    // is the concurrent-provision race the port contract mandates resolving
    // by re-reading and continuing (match-and-continue).
    let link_id = Uuid::now_v7();
    let link_insert = sqlx::query(
        "INSERT INTO external_identities (external_identity_id, issuer, subject, user_id, linked_at, version) VALUES ($1,$2,$3,$4,$5,1) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(link_id)
    .bind(issuer)
    .bind(subject)
    .bind(candidate)
    .bind(now)
    .execute(&mut *conn)
    .await;
    let raced = match link_insert {
        Ok(result) if result.rows_affected() == 1 => false,
        Ok(_) => true,
        Err(sqlx::Error::Database(db_error)) if db_error.code().as_deref() == Some("23505") => true,
        Err(error) => return Err(map_sqlx_error(error)),
    };
    if raced {
        // Candidate/key were claimed concurrently: either this exact key
        // now exists (match-and-continue) or the candidate was linked to a
        // different key (fail closed).
        if let Some(race) = fetch_link(conn, issuer, subject).await? {
            let user = fetch_user(conn, race.user_id())
                .await?
                .ok_or(IdentityStoreError::Failed)?;
            return Ok(ResolvedPrincipal {
                user,
                external_identity: race,
                provisioned: false,
            });
        }
        return Err(IdentityStoreError::PrincipalMismatch);
    }

    let link = fetch_link(conn, issuer, subject)
        .await?
        .ok_or(IdentityStoreError::Failed)?;
    let (action, resource_type) = if provisioned {
        ("identity.user.provisioned", "platform_user")
    } else {
        ("identity.external_identity.linked", "external_identity")
    };
    let event = build_audit(
        PLATFORM_AUDIT_TENANT,
        action,
        resource_type,
        &user.user_id().to_string(),
        audit,
        now,
        serde_json::json!({ "lifecycle_status": user.status().as_str() }),
        vec![],
    )?;
    audit_postgres::append_postgres_in_transaction(&mut *conn, &event)
        .await
        .map_err(map_audit_error)?;
    Ok(ResolvedPrincipal {
        user,
        external_identity: link,
        provisioned,
    })
}

// ---------------------------------------------------------------------------
// ports
// ---------------------------------------------------------------------------

#[async_trait]
impl IdentityResolvePort for PostgresIdentityStore {
    async fn resolve_or_provision(
        &self,
        command: ResolvePrincipalCommit,
    ) -> Result<ResolvedPrincipal, IdentityStoreError> {
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        // Serialize concurrent first-contact provisioning for this exact
        // external key (the audit chain writer takes its own tenant lock).
        let guard = format!("identity:resolve|{}|{}", command.issuer, command.subject);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(guard)
            .execute(&mut *transaction)
            .await
            .map_err(map_sqlx_error)?;
        let resolved = provision_in_tx(
            &mut transaction,
            &command.issuer,
            &command.subject,
            command.claimed_user_id,
            command.deterministic_user_id,
            &command.audit,
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(resolved)
    }
}

#[async_trait]
impl IdentityCommandPort for PostgresIdentityStore {
    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn create_membership(
        &self,
        command: CreateMembershipCommit,
    ) -> Result<MembershipCommitOutcome, IdentityStoreError> {
        let operation = format!("create_membership|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        let target_user_id = match &command.target {
            MembershipTarget::UserId(user_id) => {
                if fetch_user(&mut transaction, *user_id).await?.is_none() {
                    return Err(IdentityStoreError::NotFound);
                }
                *user_id
            }
            MembershipTarget::ExternalSubject {
                issuer,
                subject,
                deterministic_user_id,
            } => provision_in_tx(
                &mut transaction,
                issuer,
                subject,
                None,
                *deterministic_user_id,
                &command.audit,
                command.now,
            )
            .await?
            .user
            .user_id(),
        };
        let fingerprint_hex = fingerprint(&[
            &command.tenant_id.to_string(),
            &target_user_id.to_string(),
            command.source.as_str(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "membership",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let membership = fetch_membership_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(MembershipCommitOutcome {
                membership,
                replayed: true,
            });
        }
        if fetch_membership(&mut transaction, command.tenant_id, target_user_id)
            .await?
            .is_some()
        {
            return Err(IdentityStoreError::AlreadyExists);
        }
        let membership = TenantMembership::join(
            Uuid::now_v7(),
            command.tenant_id,
            target_user_id,
            command.source,
            command.now,
        )
        .map_err(|_| IdentityStoreError::Failed)?;
        sqlx::query(
            "INSERT INTO tenant_memberships (membership_id, tenant_id, user_id, status, joined_at, suspended_at, source, version) VALUES ($1,$2,$3,$4,$5,NULL,$6,1)",
        )
        .bind(membership.membership_id())
        .bind(membership.tenant_id())
        .bind(membership.user_id())
        .bind(membership.status().as_str())
        .bind(membership.joined_at())
        .bind(membership.source().as_str())
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        let event = build_audit(
            command.tenant_id,
            "identity.membership.created",
            "tenant_membership",
            &target_user_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({
                "status": membership.status().as_str(),
                "source": membership.source().as_str(),
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
            "membership",
            membership.membership_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(MembershipCommitOutcome {
            membership,
            replayed: false,
        })
    }

    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn change_membership_status(
        &self,
        command: ChangeMembershipStatusCommit,
    ) -> Result<MembershipCommitOutcome, IdentityStoreError> {
        let operation = format!("change_membership_status|{}", command.tenant_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(&mut transaction, &operation, command.tenant_id, key).await?;
        }
        let fingerprint_hex = fingerprint(&[
            &command.tenant_id.to_string(),
            &command.user_id.to_string(),
            command.target_status.as_str(),
            &command.expected_version.to_string(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            &operation,
            "membership",
            command.tenant_id,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let membership = fetch_membership_by_id(&mut transaction, existing.result_id).await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(MembershipCommitOutcome {
                membership,
                replayed: true,
            });
        }
        let stored = fetch_membership(&mut transaction, command.tenant_id, command.user_id)
            .await?
            .ok_or(IdentityStoreError::NotFound)?;
        if stored.version().value() != command.expected_version {
            return Err(IdentityStoreError::VersionConflict);
        }
        if stored.status() == command.target_status {
            // Same-status convergence: no bump, no audit; still replayable.
            record_idempotency(
                &mut transaction,
                &operation,
                command.tenant_id,
                command.idempotency_key.as_ref(),
                &fingerprint_hex,
                "membership",
                stored.membership_id(),
                command.now,
            )
            .await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(MembershipCommitOutcome {
                membership: stored,
                replayed: true,
            });
        }
        let mut updated = stored;
        match command.target_status {
            MembershipStatus::Suspended => updated
                .suspend(command.now)
                .map_err(|_| IdentityStoreError::Failed)?,
            MembershipStatus::Active => updated
                .reactivate(command.now)
                .map_err(|_| IdentityStoreError::Failed)?,
        }
        // The identity schema (`019_identity_authorization_foundation.sql`)
        // has no `updated_at` column on `tenant_memberships`; the mutation
        // timestamp lands in the audit record instead.
        let result = sqlx::query(
            "UPDATE tenant_memberships SET status = $1, suspended_at = $2, version = $3 WHERE tenant_id = $4 AND membership_id = $5 AND version = $6",
        )
        .bind(updated.status().as_str())
        .bind(updated.suspended_at())
        .bind(updated.version().value())
        .bind(command.tenant_id)
        .bind(updated.membership_id())
        .bind(command.expected_version)
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(IdentityStoreError::VersionConflict);
        }
        let action = match command.target_status {
            MembershipStatus::Active => "identity.membership.reactivated",
            MembershipStatus::Suspended => "identity.membership.suspended",
        };
        let event = build_audit(
            command.tenant_id,
            action,
            "tenant_membership",
            &command.user_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({ "status": command.target_status.as_str() }),
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
            "membership",
            updated.membership_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(MembershipCommitOutcome {
            membership: updated,
            replayed: false,
        })
    }

    // One atomic multi-statement write per contract rule; kept linear on purpose.
    #[allow(clippy::too_many_lines)]
    async fn change_user_status(
        &self,
        command: ChangeUserStatusCommit,
    ) -> Result<UserCommitOutcome, IdentityStoreError> {
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        // Tenant-less operation: keyed in the platform-scope slot of the
        // tenant-keyed idempotency table.
        if let Some(key) = &command.idempotency_key {
            lock_idempotency(
                &mut transaction,
                "change_user_status",
                PLATFORM_AUDIT_TENANT,
                key,
            )
            .await?;
        }
        let fingerprint_hex = fingerprint(&[
            &command.user_id.to_string(),
            command.target_status.as_str(),
            &command.expected_version.to_string(),
        ]);
        if let Some(existing) = check_idempotency(
            &mut transaction,
            "change_user_status",
            "user",
            PLATFORM_AUDIT_TENANT,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
        )
        .await?
        {
            let user = fetch_user(&mut transaction, existing.result_id)
                .await?
                .ok_or(IdentityStoreError::Failed)?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UserCommitOutcome {
                user,
                replayed: true,
            });
        }
        let stored = fetch_user(&mut transaction, command.user_id)
            .await?
            .ok_or(IdentityStoreError::NotFound)?;
        if stored.version().value() != command.expected_version {
            return Err(IdentityStoreError::VersionConflict);
        }
        if stored.status() == command.target_status {
            record_idempotency(
                &mut transaction,
                "change_user_status",
                PLATFORM_AUDIT_TENANT,
                command.idempotency_key.as_ref(),
                &fingerprint_hex,
                "user",
                stored.user_id(),
                command.now,
            )
            .await?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(UserCommitOutcome {
                user: stored,
                replayed: true,
            });
        }
        let mut updated = stored;
        match command.target_status {
            UserLifecycleStatus::Disabled => updated
                .disable(command.now)
                .map_err(|_| IdentityStoreError::Failed)?,
            UserLifecycleStatus::Active => updated
                .enable(command.now)
                .map_err(|_| IdentityStoreError::Failed)?,
        }
        let result = sqlx::query(
            "UPDATE platform_users SET status = $1, updated_at = $2, version = $3 WHERE user_id = $4 AND version = $5",
        )
        .bind(updated.status().as_str())
        .bind(command.now)
        .bind(updated.version().value())
        .bind(command.user_id)
        .bind(command.expected_version)
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(IdentityStoreError::VersionConflict);
        }
        let action = match command.target_status {
            UserLifecycleStatus::Active => "identity.user.enabled",
            UserLifecycleStatus::Disabled => "identity.user.disabled",
        };
        let event = build_audit(
            PLATFORM_AUDIT_TENANT,
            action,
            "platform_user",
            &command.user_id.to_string(),
            &command.audit,
            command.now,
            serde_json::json!({ "lifecycle_status": command.target_status.as_str() }),
            vec!["lifecycle_status".to_string()],
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &event)
            .await
            .map_err(map_audit_error)?;
        record_idempotency(
            &mut transaction,
            "change_user_status",
            PLATFORM_AUDIT_TENANT,
            command.idempotency_key.as_ref(),
            &fingerprint_hex,
            "user",
            updated.user_id(),
            command.now,
        )
        .await?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(UserCommitOutcome {
            user: updated,
            replayed: false,
        })
    }
}

#[async_trait]
impl IdentityQueryPort for PostgresIdentityStore {
    async fn get_tenant_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantUserRecord>, IdentityStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        let Some(membership) = fetch_membership(&mut connection, tenant_id, user_id).await? else {
            return Ok(None);
        };
        let user = fetch_user(&mut connection, user_id)
            .await?
            .ok_or(IdentityStoreError::Failed)?;
        let external_subjects = external_subjects_for(&mut connection, user_id).await?;
        Ok(Some(TenantUserRecord {
            user,
            membership,
            external_subjects,
        }))
    }

    async fn list_tenant_users(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<KeysetPosition>,
    ) -> Result<(Vec<TenantUserRecord>, Option<KeysetPosition>), IdentityStoreError> {
        #[derive(sqlx::FromRow)]
        struct ListRow {
            membership_id: Uuid,
            tenant_id: Uuid,
            user_id: Uuid,
            status: String,
            joined_at: DateTime<Utc>,
            suspended_at: Option<DateTime<Utc>>,
            source: String,
            membership_version: i64,
            user_status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            user_version: i64,
        }
        let limit = i64::from(limit.clamp(1, 200));
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        let select = "SELECT m.membership_id, m.tenant_id, m.user_id, m.status, m.joined_at, m.suspended_at, m.source, m.version AS membership_version, u.status AS user_status, u.created_at, u.updated_at, u.version AS user_version FROM tenant_memberships m JOIN platform_users u ON u.user_id = m.user_id WHERE m.tenant_id = $1";
        let sql = if after.is_some() {
            format!(
                "{select} AND (m.joined_at, m.membership_id) < ($2, $3) ORDER BY m.joined_at DESC, m.membership_id DESC LIMIT $4"
            )
        } else {
            format!("{select} ORDER BY m.joined_at DESC, m.membership_id DESC LIMIT $2")
        };
        let mut query = sqlx::query_as::<_, ListRow>(&sql).bind(tenant_id);
        if let Some(after) = after {
            query = query.bind(after.timestamp).bind(after.row_id);
        }
        let rows = query.bind(limit + 1);
        let rows = rows
            .fetch_all(&mut *connection)
            .await
            .map_err(map_sqlx_error)?;
        let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
        let rows: Vec<ListRow> = rows
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .collect();
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let membership = MembershipRow {
                membership_id: row.membership_id,
                tenant_id: row.tenant_id,
                user_id: row.user_id,
                status: row.status,
                joined_at: row.joined_at,
                suspended_at: row.suspended_at,
                source: row.source,
                version: row.membership_version,
            }
            .into_membership()?;
            let user = UserRow {
                user_id: row.user_id,
                status: row.user_status,
                created_at: row.created_at,
                updated_at: row.updated_at,
                version: row.user_version,
            }
            .into_user()?;
            let external_subjects = external_subjects_for(&mut connection, user.user_id()).await?;
            records.push(TenantUserRecord {
                user,
                membership,
                external_subjects,
            });
        }
        let cursor = has_more
            .then(|| {
                records.last().map(|record| KeysetPosition {
                    timestamp: record.membership.joined_at(),
                    row_id: record.membership.membership_id(),
                })
            })
            .flatten();
        Ok((records, cursor))
    }

    async fn list_memberships(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<KeysetPosition>,
    ) -> Result<(Vec<MembershipRecord>, Option<KeysetPosition>), IdentityStoreError> {
        #[derive(sqlx::FromRow)]
        struct ListRow {
            membership_id: Uuid,
            tenant_id: Uuid,
            user_id: Uuid,
            status: String,
            joined_at: DateTime<Utc>,
            suspended_at: Option<DateTime<Utc>>,
            source: String,
            membership_version: i64,
            user_status: String,
        }
        let limit = i64::from(limit.clamp(1, 200));
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        let select = "SELECT m.membership_id, m.tenant_id, m.user_id, m.status, m.joined_at, m.suspended_at, m.source, m.version AS membership_version, u.status AS user_status FROM tenant_memberships m JOIN platform_users u ON u.user_id = m.user_id WHERE m.tenant_id = $1";
        let sql = if after.is_some() {
            format!(
                "{select} AND (m.joined_at, m.membership_id) < ($2, $3) ORDER BY m.joined_at DESC, m.membership_id DESC LIMIT $4"
            )
        } else {
            format!("{select} ORDER BY m.joined_at DESC, m.membership_id DESC LIMIT $2")
        };
        let mut query = sqlx::query_as::<_, ListRow>(&sql).bind(tenant_id);
        if let Some(after) = after {
            query = query.bind(after.timestamp).bind(after.row_id);
        }
        let rows = query.bind(limit + 1);
        let rows = rows
            .fetch_all(&mut *connection)
            .await
            .map_err(map_sqlx_error)?;
        let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
        let rows: Vec<ListRow> = rows
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .collect();
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let membership = MembershipRow {
                membership_id: row.membership_id,
                tenant_id: row.tenant_id,
                user_id: row.user_id,
                status: row.status,
                joined_at: row.joined_at,
                suspended_at: row.suspended_at,
                source: row.source,
                version: row.membership_version,
            }
            .into_membership()?;
            records.push(MembershipRecord {
                membership,
                user_status: parse_user_status(&row.user_status)?,
            });
        }
        let cursor = has_more
            .then(|| {
                records.last().map(|record| KeysetPosition {
                    timestamp: record.membership.joined_at(),
                    row_id: record.membership.membership_id(),
                })
            })
            .flatten();
        Ok((records, cursor))
    }

    async fn get_membership(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantMembership>, IdentityStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        fetch_membership(&mut connection, tenant_id, user_id).await
    }

    async fn find_user_by_external_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<PlatformUser>, IdentityStoreError> {
        let mut connection = self.pool.acquire().await.map_err(map_sqlx_error)?;
        let Some(link) = fetch_link(&mut connection, issuer, subject).await? else {
            return Ok(None);
        };
        fetch_user(&mut connection, link.user_id()).await
    }
}

#[async_trait]
impl BootstrapLedgerPort for PostgresIdentityStore {
    async fn latest_for(
        &self,
        tenant_id: Uuid,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<BootstrapLedgerEntry>, IdentityStoreError> {
        #[derive(sqlx::FromRow)]
        struct LedgerRow {
            tenant_id: Uuid,
            issuer: String,
            subject: String,
            role_stable_key: String,
            config_version: i64,
            config_digest: String,
            outcome: String,
            recorded_at: DateTime<Utc>,
        }
        let row = sqlx::query_as::<_, LedgerRow>(
            "SELECT tenant_id, issuer, subject, role_stable_key, config_version, config_digest, outcome, recorded_at FROM platform_bootstrap_executions WHERE tenant_id = $1 AND issuer = $2 AND subject = $3 ORDER BY recorded_at DESC, config_version DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let outcome = match row.outcome.as_str() {
            "executed" => BootstrapOutcome::Executed,
            "no_op" => BootstrapOutcome::NoOp,
            "failed" => BootstrapOutcome::Failed,
            _ => return Err(IdentityStoreError::Failed),
        };
        Ok(Some(BootstrapLedgerEntry {
            tenant_id: row.tenant_id,
            issuer: row.issuer,
            subject: row.subject,
            role_stable_key: row.role_stable_key,
            config_version: row.config_version,
            config_digest: row.config_digest,
            outcome,
            recorded_at: row.recorded_at,
        }))
    }

    async fn record(&self, entry: &BootstrapLedgerEntry) -> Result<bool, IdentityStoreError> {
        let outcome = match entry.outcome {
            BootstrapOutcome::Executed => "executed",
            BootstrapOutcome::NoOp => "no_op",
            BootstrapOutcome::Failed => "failed",
        };
        let result = sqlx::query(
            "INSERT INTO platform_bootstrap_executions (tenant_id, issuer, subject, role_stable_key, config_version, config_digest, outcome, recorded_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id, issuer, subject, config_digest) DO NOTHING",
        )
        .bind(entry.tenant_id)
        .bind(&entry.issuer)
        .bind(&entry.subject)
        .bind(&entry.role_stable_key)
        .bind(entry.config_version)
        .bind(&entry.config_digest)
        .bind(outcome)
        .bind(entry.recorded_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }
}
