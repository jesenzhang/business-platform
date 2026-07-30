use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::event::DomainEvent;

/// A single row in the `outbox_events` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OutboxRecord {
    pub event_id: Uuid,
    pub event_type: String,
    pub tenant_id: String,
    pub aggregate_id: String,
    pub aggregate_type: String,
    pub payload: serde_json::Value,
    pub schema_version: String,
    pub trace_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub published: bool,
}

impl OutboxRecord {
    /// Convert the stored record back into a [`DomainEvent`].
    pub fn into_domain_event(self) -> DomainEvent {
        DomainEvent {
            event_id: self.event_id,
            event_type: self.event_type,
            tenant_id: self.tenant_id,
            aggregate_id: self.aggregate_id,
            aggregate_type: self.aggregate_type,
            payload: self.payload,
            schema_version: self.schema_version,
            trace_id: self.trace_id,
            occurred_at: self.occurred_at,
        }
    }
}

/// Outbox 存储 - 在同一事务中写入事件，保证业务写入与事件发布的原子性。
pub struct OutboxStore {
    pool: PgPool,
}

impl OutboxStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 在给定事务中追加事件到 outbox 表。
    ///
    /// 调用方应在同一事务中完成业务写入和事件追加，确保原子性。
    pub async fn append_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: &DomainEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO outbox_events
                (event_id, event_type, tenant_id, aggregate_id, aggregate_type,
                 payload, schema_version, trace_id, occurred_at, published)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, false)
            "#,
        )
        .bind(event.event_id)
        .bind(&event.event_type)
        .bind(&event.tenant_id)
        .bind(&event.aggregate_id)
        .bind(&event.aggregate_type)
        .bind(&event.payload)
        .bind(&event.schema_version)
        .bind(&event.trace_id)
        .bind(event.occurred_at)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// 获取未发布的事件（供 worker 轮询）。
    pub async fn fetch_unpublished(&self, limit: i64) -> Result<Vec<OutboxRecord>, sqlx::Error> {
        sqlx::query_as::<_, OutboxRecord>(
            r#"
            SELECT event_id, event_type, tenant_id, aggregate_id, aggregate_type,
                   payload, schema_version, trace_id, occurred_at, published
            FROM outbox_events
            WHERE published = false
            ORDER BY occurred_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// 标记事件为已发布。
    pub async fn mark_published(&self, event_ids: &[Uuid]) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE outbox_events
            SET published = true
            WHERE event_id = ANY($1)
            "#,
        )
        .bind(event_ids)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
