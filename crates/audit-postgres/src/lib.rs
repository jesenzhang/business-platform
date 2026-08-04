//! `PostgreSQL` adapter for the unified Audit context.

use async_trait::async_trait;
use audit::{
    hash_record, AuditAppendPort, AuditChainScope, AuditChainVerification, AuditError, AuditEvent,
    AuditPage, AuditQuery, AuditQueryRequest,
};
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresAuditStore {
    pool: PgPool,
}

impl PostgresAuditStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl AuditAppendPort for PostgresAuditStore {
    async fn append(&self, event: &AuditEvent) -> Result<(), AuditError> {
        append_postgres(&self.pool, event).await
    }
}

#[async_trait]
impl AuditQuery for PostgresAuditStore {
    async fn list(&self, query: AuditQueryRequest) -> Result<AuditPage, AuditError> {
        if query.tenant_id.is_nil() {
            return Err(AuditError::Validation(
                audit::AuditValidationError::NilTenant,
            ));
        }
        let limit = i64::from(query.limit.clamp(1, 200));
        let mut builder = QueryBuilder::<Postgres>::new("SELECT id, tenant_id, action, resource_type, resource_id, details, trace_id, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash FROM audit_events WHERE tenant_id = ");
        builder.push_bind(query.tenant_id);
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
            builder.push(" AND operation_id = ").push_bind(operation_id);
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
            builder.push(" AND occurred_at >= ").push_bind(after);
        }
        if let Some(before) = query.occurred_before {
            builder.push(" AND occurred_at <= ").push_bind(before);
        }
        if let Some(cursor) = query.cursor {
            if cursor.stream_sequence > 0 {
                builder
                    .push(" AND stream_sequence < ")
                    .push_bind(cursor.stream_sequence);
            } else {
                builder
                    .push(" AND (occurred_at, id) < (")
                    .push_bind(cursor.occurred_at)
                    .push(", ")
                    .push_bind(cursor.id)
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
            "SELECT id, tenant_id, action, resource_type, resource_id, details, trace_id, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash FROM audit_events WHERE tenant_id=$1 AND id=$2",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| AuditError::Persistence)?;
        row.map(AuditRow::into_event).transpose()
    }

    async fn verify_chain(
        &self,
        scope: AuditChainScope,
    ) -> Result<AuditChainVerification, AuditError> {
        let rows = sqlx::query_as::<_, ChainRow>(
            "SELECT id, tenant_id, action, resource_type, resource_id, details, trace_id, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash FROM audit_events WHERE tenant_id = $1 ORDER BY stream_sequence ASC, id ASC",
        )
        .bind(scope.tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AuditError::Persistence)?;
        let mut previous = None;
        let mut previous_sequence = None;
        let mut checked = 0_u64;
        for row in rows {
            let event = row.into_event()?;
            if event.chain_version() == 0 {
                if previous_sequence.is_some() {
                    return Ok(AuditChainVerification {
                        checked,
                        valid: false,
                        first_broken_id: Some(event.id()),
                    });
                }
                continue;
            }
            let hash = hash_record(&event)?;
            if previous_sequence.is_none() && event.previous_hash().is_some()
                || previous_sequence.is_some_and(|sequence| event.stream_sequence() != sequence + 1)
                || event.previous_hash() != previous.as_deref()
                || event.record_hash() != Some(hash.as_str())
            {
                return Ok(AuditChainVerification {
                    checked,
                    valid: false,
                    first_broken_id: Some(event.id()),
                });
            }
            previous = event.record_hash().map(str::to_string);
            previous_sequence = Some(event.stream_sequence());
            if scope.from.is_none_or(|from| event.occurred_at() >= from)
                && scope.to.is_none_or(|to| event.occurred_at() <= to)
            {
                checked = checked.saturating_add(1);
            }
        }
        Ok(AuditChainVerification {
            checked,
            valid: true,
            first_broken_id: None,
        })
    }
}

/// Append through a caller-owned transaction when a business adapter needs
/// atomic business + Audit + Outbox semantics.
pub async fn append_postgres(pool: &PgPool, event: &AuditEvent) -> Result<(), AuditError> {
    let mut transaction = pool.begin().await.map_err(|_| AuditError::Persistence)?;
    append_postgres_in_transaction(&mut transaction, event).await?;
    transaction
        .commit()
        .await
        .map_err(|_| AuditError::Persistence)
}

/// Append using a caller-owned transaction.  Business adapters use this seam
/// so aggregate state, outbox and audit evidence commit or roll back together.
pub async fn append_postgres_in_transaction(
    connection: &mut PgConnection,
    event: &AuditEvent,
) -> Result<(), AuditError> {
    // Serialize only this tenant's append stream; unrelated tenants remain
    // concurrent and no global application lock is introduced.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(event.tenant_id().to_string())
        .execute(&mut *connection)
        .await
        .map_err(|_| AuditError::Persistence)?;
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT record_hash FROM audit_events WHERE tenant_id = $1 AND chain_version = 1 ORDER BY stream_sequence DESC LIMIT 1",
    )
    .bind(event.tenant_id())
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| AuditError::Persistence)?
    .flatten();
    let sequence = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(stream_sequence), 0) + 1 FROM audit_events WHERE tenant_id = $1",
    )
    .bind(event.tenant_id())
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| AuditError::Persistence)?;
    let event = event
        .clone()
        .with_chain_metadata(sequence, chrono::Utc::now(), 1, previous)?;
    sqlx::query("INSERT INTO audit_events (id, tenant_id, action, resource_type, resource_id, details, trace_id, created_at, occurred_at, recorded_at, stream_sequence, chain_version, operation_id, actor_type, actor_id, correlation_id, causation_id, reason, result, failure_code, before_hash, after_hash, changed_fields, schema_version, previous_hash, record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
        .bind(event.id())
        .bind(event.tenant_id())
        .bind(event.action().as_str())
        .bind(&event.resource().resource_type)
        .bind(&event.resource().resource_id)
        .bind(event.details())
        .bind(event.trace_id())
        .bind(event.occurred_at())
        .bind(event.recorded_at())
        .bind(event.stream_sequence())
        .bind(event.chain_version())
        .bind(event.operation_id())
        .bind(format!("{:?}", event.actor().actor_type).to_lowercase())
        .bind(event.actor().actor_id)
        .bind(event.correlation_id())
        .bind(event.causation_id())
        .bind(event.reason())
        .bind(format!("{:?}", event.result()).to_lowercase())
        .bind(event.failure_code())
        .bind(event.before_hash())
        .bind(event.after_hash())
        .bind(serde_json::to_value(event.changed_fields()).map_err(|_| AuditError::Persistence)?)
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
    id: Uuid,
    tenant_id: Uuid,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    details: Option<serde_json::Value>,
    trace_id: Option<String>,
    occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    recorded_at: Option<chrono::DateTime<chrono::Utc>>,
    stream_sequence: Option<i64>,
    chain_version: Option<i16>,
    operation_id: Option<Uuid>,
    actor_type: String,
    actor_id: Option<Uuid>,
    correlation_id: Option<Uuid>,
    causation_id: Option<Uuid>,
    reason: Option<String>,
    result: String,
    failure_code: Option<String>,
    before_hash: Option<String>,
    after_hash: Option<String>,
    changed_fields: serde_json::Value,
    schema_version: String,
    previous_hash: Option<String>,
    record_hash: Option<String>,
}
type ChainRow = AuditRow;

impl AuditRow {
    fn into_event(self) -> Result<AuditEvent, AuditError> {
        let chain_version = self.chain_version.ok_or(AuditError::Persistence)?;
        let actor_type = match self.actor_type.as_str() {
            "user" => audit::AuditActorType::User,
            "service" => audit::AuditActorType::Service,
            "worker" => audit::AuditActorType::Worker,
            "repairjob" | "repair_job" => audit::AuditActorType::RepairJob,
            _ => return Err(AuditError::Persistence),
        };
        // Rows written before the unified actor columns existed are mapped to
        // their immutable event id rather than fabricating a nil actor.
        let actor_id = match self.actor_id {
            Some(actor_id) => actor_id,
            None if chain_version == 0 => self.id,
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
            _ => return Err(AuditError::Persistence),
        };
        let occurred_at = self.occurred_at.ok_or(AuditError::Persistence)?;
        let recorded_at = match self.recorded_at {
            Some(recorded_at) => recorded_at,
            None if chain_version == 0 => occurred_at,
            None => return Err(AuditError::Persistence),
        };
        let changed_fields =
            serde_json::from_value(self.changed_fields).map_err(|_| AuditError::Persistence)?;
        let details = match self.details {
            Some(details) => details,
            None if chain_version == 0 => serde_json::Value::Null,
            None => return Err(AuditError::Persistence),
        };
        let event = AuditEvent::rehydrate(
            self.id,
            self.tenant_id,
            audit::AuditActor {
                actor_type,
                actor_id,
            },
            action,
            resource,
            match self.operation_id {
                Some(operation_id) => operation_id,
                None if chain_version == 0 => self.id,
                None => return Err(AuditError::Persistence),
            },
            self.correlation_id,
            self.causation_id,
            self.trace_id,
            self.reason,
            result,
            self.failure_code,
            self.before_hash,
            self.after_hash,
            changed_fields,
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
