//! `SQLite` adapter for local, single-process audit evidence.

use async_trait::async_trait;
use audit::{
    audit_chain_genesis, hash_record, AuditAppendPort, AuditChainScope, AuditChainVerification,
    AuditError, AuditEvent, AuditPage, AuditQuery, AuditQueryRequest,
};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};
use uuid::Uuid;

#[derive(Clone)]
pub struct SqliteAuditStore {
    pool: SqlitePool,
}

impl SqliteAuditStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditAppendPort for SqliteAuditStore {
    async fn append(&self, event: &AuditEvent) -> Result<(), AuditError> {
        append_sqlite(&self.pool, event).await
    }
}

#[async_trait]
impl AuditQuery for SqliteAuditStore {
    async fn list(&self, query: AuditQueryRequest) -> Result<AuditPage, AuditError> {
        if query.tenant_id.is_nil() {
            return Err(AuditError::Validation(
                audit::AuditValidationError::NilTenant,
            ));
        }
        let limit = i64::from(query.limit.clamp(1, 200));
        let mut builder = QueryBuilder::<Sqlite>::new("SELECT id, tenant_id, action, resource_type, resource_id, details, trace_id, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash FROM audit_events WHERE tenant_id = ");
        builder.push_bind(query.tenant_id.to_string());
        if let Some(actor) = query.actor {
            builder
                .push(" AND actor_type = ")
                .push_bind(format!("{actor:?}").to_lowercase());
        }
        if let Some(action) = query.action {
            builder.push(" AND action = ").push_bind(action);
        }
        if let Some(resource_type) = query.resource_type {
            builder
                .push(" AND resource_type = ")
                .push_bind(resource_type);
        }
        if let Some(resource_id) = query.resource_id {
            builder.push(" AND resource_id = ").push_bind(resource_id);
        }
        if let Some(operation_id) = query.operation_id {
            builder
                .push(" AND operation_id = ")
                .push_bind(operation_id.to_string());
        }
        if let Some(trace_id) = query.trace_id {
            builder.push(" AND trace_id = ").push_bind(trace_id);
        }
        if let Some(result) = query.result {
            builder
                .push(" AND result = ")
                .push_bind(format!("{result:?}").to_lowercase());
        }
        if let Some(after) = query.occurred_after {
            builder
                .push(" AND occurred_at >= ")
                .push_bind(after.to_rfc3339());
        }
        if let Some(before) = query.occurred_before {
            builder
                .push(" AND occurred_at <= ")
                .push_bind(before.to_rfc3339());
        }
        if let Some(cursor) = query.cursor {
            if cursor.stream_sequence > 0 {
                builder
                    .push(" AND stream_sequence < ")
                    .push_bind(cursor.stream_sequence);
            } else {
                builder
                    .push(" AND (occurred_at, id) < (")
                    .push_bind(cursor.occurred_at.to_rfc3339())
                    .push(", ")
                    .push_bind(cursor.id.to_string())
                    .push(")");
            }
        }
        builder
            .push(" ORDER BY stream_sequence DESC, id DESC LIMIT ")
            .push_bind(limit);
        let rows = builder
            .build_query_as::<AuditRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(|_| AuditError::Persistence)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(row.into_event()?);
        }
        let next_cursor = items.last().map(|item| audit::AuditCursor {
            version: 1,
            stream_sequence: item.stream_sequence(),
            occurred_at: item.occurred_at(),
            id: item.id(),
        });
        Ok(AuditPage { items, next_cursor })
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<AuditEvent>, AuditError> {
        if tenant_id.is_nil() || id.is_nil() {
            return Err(AuditError::InvalidCursor);
        }
        let row = sqlx::query_as::<_, AuditRow>(
            "SELECT id, tenant_id, action, resource_type, resource_id, details, trace_id, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash FROM audit_events WHERE tenant_id=?1 AND id=?2",
        )
        .bind(tenant_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| AuditError::Persistence)?;
        row.map(AuditRow::into_event).transpose()
    }

    async fn verify_chain(
        &self,
        scope: AuditChainScope,
    ) -> Result<AuditChainVerification, AuditError> {
        let rows = sqlx::query_as::<_, AuditRow>(
            "SELECT id, tenant_id, action, resource_type, resource_id, details, trace_id, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash FROM audit_events WHERE tenant_id = ?1 ORDER BY stream_sequence ASC, id ASC",
        )
        .bind(scope.tenant_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AuditError::Persistence)?;
        let genesis = audit_chain_genesis(scope.tenant_id);
        let mut previous = genesis;
        let mut previous_sequence = None;
        let mut checked = 0_u64;
        let mut legacy_count = 0_u64;
        let mut verified_count = 0_u64;
        for row in rows {
            let event = row.into_event()?;
            if event.chain_version() == 0 {
                if previous_sequence.is_some() {
                    return Ok(AuditChainVerification {
                        checked,
                        legacy_count,
                        verified_count,
                        valid: false,
                        first_broken_id: Some(event.id()),
                        chain_version: 1,
                    });
                }
                legacy_count = legacy_count.saturating_add(1);
                continue;
            }
            let hash = hash_record(&event)?;
            if previous_sequence.is_some_and(|sequence| event.stream_sequence() != sequence + 1)
                || event.previous_hash() != Some(previous.as_str())
                || event.record_hash() != Some(hash.as_str())
            {
                return Ok(AuditChainVerification {
                    checked,
                    legacy_count,
                    verified_count,
                    valid: false,
                    first_broken_id: Some(event.id()),
                    chain_version: 1,
                });
            }
            previous = event
                .record_hash()
                .ok_or(AuditError::Persistence)?
                .to_string();
            previous_sequence = Some(event.stream_sequence());
            verified_count = verified_count.saturating_add(1);
            if scope.from.is_none_or(|from| event.occurred_at() >= from)
                && scope.to.is_none_or(|to| event.occurred_at() <= to)
            {
                checked = checked.saturating_add(1);
            }
        }
        Ok(AuditChainVerification {
            checked,
            legacy_count,
            verified_count,
            valid: true,
            first_broken_id: None,
            chain_version: i16::from(previous_sequence.is_some()),
        })
    }
}

pub async fn append_sqlite(pool: &SqlitePool, event: &AuditEvent) -> Result<(), AuditError> {
    let mut connection = pool.acquire().await.map_err(|_| AuditError::Persistence)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(|_| AuditError::Persistence)?;
    match append_sqlite_in_transaction(&mut connection, event).await {
        Ok(()) => sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map(|_| ())
            .map_err(|_| AuditError::Persistence),
        Err(error) => {
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(|_| AuditError::Persistence)?;
            Err(error)
        }
    }
}

/// Append using a caller-owned transaction for atomic business + audit writes.
pub async fn append_sqlite_in_transaction(
    connection: &mut SqliteConnection,
    event: &AuditEvent,
) -> Result<(), AuditError> {
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT record_hash FROM audit_events WHERE tenant_id = ?1 AND chain_version = 1 ORDER BY stream_sequence DESC LIMIT 1",
    )
    .bind(event.tenant_id().to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| AuditError::Persistence)?
    .flatten()
    .unwrap_or_else(|| audit_chain_genesis(event.tenant_id()));
    let sequence = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(stream_sequence), 0) + 1 FROM audit_events WHERE tenant_id = ?1",
    )
    .bind(event.tenant_id().to_string())
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| AuditError::Persistence)?;
    let event =
        event
            .clone()
            .with_chain_metadata(sequence, chrono::Utc::now(), 1, Some(previous))?;
    sqlx::query("INSERT INTO audit_events (id, tenant_id, action, resource_type, resource_id, details, trace_id, created_at, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
        .bind(event.id().to_string())
        .bind(event.tenant_id().to_string())
        .bind(event.action().as_str())
        .bind(&event.resource().resource_type)
        .bind(&event.resource().resource_id)
        .bind(event.details().to_string())
        .bind(event.trace_id())
        .bind(event.occurred_at().to_rfc3339())
        .bind(event.recorded_at().to_rfc3339())
        .bind(event.stream_sequence())
        .bind(event.chain_version())
        .bind(event.operation_id().to_string())
        .bind(format!("{:?}", event.actor().actor_type).to_lowercase())
        .bind(event.actor().actor_id.to_string())
        .bind(event.correlation_id().map(|id| id.to_string()))
        .bind(event.causation_id().map(|id| id.to_string()))
        .bind(event.reason())
        .bind(format!("{:?}", event.result()).to_lowercase())
        .bind(event.failure_code())
        .bind(event.before_hash())
        .bind(event.after_hash())
        .bind(serde_json::to_string(event.changed_fields()).map_err(|_| AuditError::Persistence)?)
        .bind(event.schema_version())
        .bind(event.previous_hash())
        .bind(event.record_hash())
        .execute(&mut *connection)
        .await
        .map_err(|_| AuditError::Persistence)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: String,
    tenant_id: String,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    details: Option<String>,
    trace_id: Option<String>,
    occurred_at: Option<String>,
    recorded_at: Option<String>,
    stream_sequence: Option<i64>,
    chain_version: Option<i16>,
    operation_id: Option<String>,
    actor_type: String,
    actor_id: Option<String>,
    correlation_id: Option<String>,
    causation_id: Option<String>,
    reason: Option<String>,
    result: String,
    failure_code: Option<String>,
    before_hash: Option<String>,
    after_hash: Option<String>,
    changed_fields: String,
    schema_version: String,
    previous_hash: Option<String>,
    record_hash: Option<String>,
}

impl AuditRow {
    fn into_event(self) -> Result<AuditEvent, AuditError> {
        let id = Uuid::parse_str(&self.id).map_err(|_| AuditError::Persistence)?;
        let tenant_id = Uuid::parse_str(&self.tenant_id).map_err(|_| AuditError::Persistence)?;
        let chain_version = self.chain_version.ok_or(AuditError::Persistence)?;
        let actor_id = match self.actor_id.as_deref() {
            Some(value) => Uuid::parse_str(value).map_err(|_| AuditError::Persistence)?,
            None if chain_version == 0 => id,
            None => return Err(AuditError::Persistence),
        };
        let operation_id = match self.operation_id.as_deref() {
            Some(value) => Uuid::parse_str(value).map_err(|_| AuditError::Persistence)?,
            None if chain_version == 0 => id,
            None => return Err(AuditError::Persistence),
        };
        let action = audit::AuditAction::new(self.action).map_err(AuditError::Validation)?;
        let resource = audit::AuditResource::new(
            self.resource_type,
            self.resource_id.ok_or(AuditError::Persistence)?,
        )
        .map_err(AuditError::Validation)?;
        let result = match self.result.as_str() {
            "failed" => audit::AuditResult::Failed,
            "denied" => audit::AuditResult::Denied,
            "cancelled" => audit::AuditResult::Cancelled,
            "succeeded" => audit::AuditResult::Succeeded,
            _ => return Err(AuditError::InvalidStoredEnum),
        };
        let occurred_at = self
            .occurred_at
            .as_deref()
            .ok_or(AuditError::Persistence)
            .and_then(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .map(|value| value.with_timezone(&chrono::Utc))
                    .map_err(|_| AuditError::Persistence)
            })?;
        let recorded_at = match self.recorded_at.as_deref() {
            Some(value) => chrono::DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&chrono::Utc))
                .map_err(|_| AuditError::Persistence)?,
            None if chain_version == 0 => occurred_at,
            None => return Err(AuditError::Persistence),
        };
        let details = match self.details.as_deref() {
            Some(value) => serde_json::from_str(value).map_err(|_| AuditError::Persistence)?,
            None if chain_version == 0 => serde_json::Value::Null,
            None => return Err(AuditError::Persistence),
        };
        let event = AuditEvent::rehydrate(
            id,
            tenant_id,
            audit::AuditActor {
                actor_type: match self.actor_type.as_str() {
                    "user" => audit::AuditActorType::User,
                    "service" => audit::AuditActorType::Service,
                    "worker" => audit::AuditActorType::Worker,
                    "repairjob" | "repair_job" => audit::AuditActorType::RepairJob,
                    "system" => audit::AuditActorType::System,
                    _ => return Err(AuditError::InvalidStoredEnum),
                },
                actor_id,
            },
            action,
            resource,
            operation_id,
            self.correlation_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| AuditError::Persistence)?,
            self.causation_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| AuditError::Persistence)?,
            self.trace_id,
            self.reason,
            result,
            self.failure_code,
            self.before_hash,
            self.after_hash,
            serde_json::from_str(&self.changed_fields).map_err(|_| AuditError::Persistence)?,
            audit::sanitize_details_for_read(details),
            self.schema_version,
            occurred_at,
            recorded_at,
            self.stream_sequence.ok_or(AuditError::Persistence)?,
            chain_version,
            self.previous_hash,
            self.record_hash,
        )?;
        Ok(event)
    }
}
