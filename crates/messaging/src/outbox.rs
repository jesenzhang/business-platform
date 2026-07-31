use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::postgres::PgQueryResult;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

use crate::event::DomainEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxStatus {
    Pending,
    Processing,
    Published,
    RetryScheduled,
    Failed,
}

impl OutboxStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Published => "published",
            Self::RetryScheduled => "retry_scheduled",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for OutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
pub enum OutboxError {
    #[error("outbox lease was lost or claim is stale")]
    LeaseLost,
    #[error("outbox database operation failed")]
    Database(#[source] sqlx::Error),
}

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
    pub status: String,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub claim_token: Option<Uuid>,
    pub claim_version: i64,
    pub lease_until: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl OutboxRecord {
    #[must_use]
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

pub struct ReliableOutbox {
    pool: PgPool,
    worker_id: String,
    lease_duration: Duration,
}

impl ReliableOutbox {
    #[must_use]
    pub fn new(pool: PgPool, worker_id: String, lease_duration: Duration) -> Self {
        Self {
            pool,
            worker_id,
            lease_duration,
        }
    }

    pub async fn append_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        event: &DomainEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO outbox_events
                (event_id, event_type, tenant_id, aggregate_id, aggregate_type,
                 payload, schema_version, trace_id, occurred_at, status, published)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', FALSE)
            ",
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

    pub async fn claim_batch(&self, batch_size: i64) -> Result<Vec<OutboxRecord>, OutboxError> {
        let lease_secs = i64::try_from(self.lease_duration.as_secs()).unwrap_or(i64::MAX);
        let mut tx = self.pool.begin().await.map_err(OutboxError::Database)?;
        reconcile_exhausted_in_tx(&mut tx)
            .await
            .map_err(OutboxError::Database)?;
        let records = sqlx::query_as::<_, OutboxRecord>(
            r"
            WITH claimed AS (
                SELECT event_id
                FROM outbox_events
                WHERE status IN ('pending', 'retry_scheduled')
                  AND available_at <= NOW()
                  AND attempt_count < max_attempts
                ORDER BY available_at, event_id
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE outbox_events e
            SET status = 'processing',
                claimed_by = $2,
                claim_token = uuid_generate_v4(),
                claim_version = e.claim_version + 1,
                lease_until = NOW() + make_interval(secs => $3),
                attempt_count = e.attempt_count + 1,
                published = FALSE
            FROM claimed
            WHERE e.event_id = claimed.event_id
            RETURNING e.event_id, e.event_type, e.tenant_id, e.aggregate_id,
                      e.aggregate_type, e.payload, e.schema_version, e.trace_id,
                      e.occurred_at, e.status, e.attempt_count, e.max_attempts,
                      e.available_at, e.claimed_by, e.claim_token, e.claim_version,
                      e.lease_until, e.published_at, e.last_error
            ",
        )
        .bind(batch_size)
        .bind(&self.worker_id)
        .bind(lease_secs)
        .fetch_all(&mut *tx)
        .await
        .map_err(OutboxError::Database)?;
        tx.commit().await.map_err(OutboxError::Database)?;
        Ok(records)
    }

    pub async fn mark_published(
        &self,
        event_id: Uuid,
        claim_token: Uuid,
        claim_version: i64,
    ) -> Result<(), OutboxError> {
        let result = sqlx::query(
            r"
            UPDATE outbox_events
            SET status = 'published',
                published = TRUE,
                published_at = NOW(),
                claimed_by = NULL,
                claim_token = NULL,
                lease_until = NULL
            WHERE event_id = $1
              AND status = 'processing'
              AND claimed_by = $2
              AND claim_token = $3
              AND claim_version = $4
              AND lease_until > NOW()
            ",
        )
        .bind(event_id)
        .bind(&self.worker_id)
        .bind(claim_token)
        .bind(claim_version)
        .execute(&self.pool)
        .await
        .map_err(OutboxError::Database)?;
        ensure_lease_owned(&result)
    }

    pub async fn mark_failed(
        &self,
        event_id: Uuid,
        claim_token: Uuid,
        claim_version: i64,
        error: &str,
    ) -> Result<(), OutboxError> {
        let result = sqlx::query(
            r"
            UPDATE outbox_events
            SET status = CASE
                    WHEN attempt_count < max_attempts THEN 'retry_scheduled'
                    ELSE 'failed'
                END,
                published = FALSE,
                available_at = CASE
                    WHEN attempt_count < max_attempts
                    THEN NOW() + make_interval(secs => LEAST(power(2, attempt_count)::bigint, 300))
                    ELSE available_at
                END,
                last_error = $5,
                claimed_by = NULL,
                claim_token = NULL,
                lease_until = NULL
            WHERE event_id = $1
              AND status = 'processing'
              AND claimed_by = $2
              AND claim_token = $3
              AND claim_version = $4
              AND lease_until > NOW()
            ",
        )
        .bind(event_id)
        .bind(&self.worker_id)
        .bind(claim_token)
        .bind(claim_version)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(OutboxError::Database)?;
        ensure_lease_owned(&result)
    }

    pub async fn recover_expired_leases(&self) -> Result<u64, OutboxError> {
        let result: PgQueryResult = sqlx::query(
            r"
            UPDATE outbox_events
            SET status = CASE
                    WHEN attempt_count < max_attempts THEN 'retry_scheduled'
                    ELSE 'failed'
                END,
                published = FALSE,
                claimed_by = NULL,
                claim_token = NULL,
                lease_until = NULL,
                available_at = CASE
                    WHEN attempt_count < max_attempts THEN NOW()
                    ELSE available_at
                END,
                last_error = COALESCE(last_error, 'lease expired and was recovered')
            WHERE status = 'processing' AND lease_until < NOW()
            ",
        )
        .execute(&self.pool)
        .await
        .map_err(OutboxError::Database)?;
        Ok(result.rows_affected())
    }

    /// Move non-terminal events that have exhausted their retry budget to a
    /// stable failed state. Repeated calls only update eligible rows once.
    pub async fn reconcile_exhausted_events(&self) -> Result<u64, OutboxError> {
        let mut tx = self.pool.begin().await.map_err(OutboxError::Database)?;
        let affected = reconcile_exhausted_in_tx(&mut tx)
            .await
            .map_err(OutboxError::Database)?;
        tx.commit().await.map_err(OutboxError::Database)?;
        Ok(affected)
    }
}

async fn reconcile_exhausted_in_tx(tx: &mut Transaction<'_, Postgres>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r"
        UPDATE outbox_events
        SET status = 'failed',
            published = FALSE,
            claimed_by = NULL,
            claim_token = NULL,
            lease_until = NULL,
            last_error = COALESCE(last_error, 'maximum attempts reached')
        WHERE status IN ('pending', 'retry_scheduled')
          AND attempt_count >= max_attempts
        ",
    )
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected())
}

fn ensure_lease_owned(result: &PgQueryResult) -> Result<(), OutboxError> {
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(OutboxError::LeaseLost)
    }
}

#[must_use]
pub fn backoff_duration(attempt_count: i32) -> Duration {
    let exp = u32::try_from(attempt_count).unwrap_or(0);
    Duration::from_secs(2u64.saturating_pow(exp).min(300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_caps_at_five_minutes() {
        assert_eq!(backoff_duration(10), Duration::from_secs(300));
    }

    #[test]
    fn status_display_is_stable() {
        assert_eq!(OutboxStatus::RetryScheduled.to_string(), "retry_scheduled");
    }
}
