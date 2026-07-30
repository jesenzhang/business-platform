use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::postgres::PgQueryResult;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::event::DomainEvent;

/// Outbox event status representing the delivery state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxStatus {
    Pending,
    Processing,
    Published,
    RetryScheduled,
    Failed,
}

impl OutboxStatus {
    /// Return the database string representation.
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

/// A claimable outbox record with full delivery state.
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
    pub lease_until: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl OutboxRecord {
    /// Convert the stored record back into a [`DomainEvent`].
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

/// Reliable outbox with claim-based multi-worker delivery.
///
/// Uses `FOR UPDATE SKIP LOCKED` for safe concurrent claiming and
/// lease-based ownership with expiration recovery.
pub struct ReliableOutbox {
    pool: PgPool,
    worker_id: String,
    lease_duration: Duration,
}

impl ReliableOutbox {
    /// Create a new reliable outbox worker.
    ///
    /// - `pool`: database connection pool
    /// - `worker_id`: unique identifier for this worker instance
    /// - `lease_duration`: how long a claimed event is held before recovery
    #[must_use]
    pub fn new(pool: PgPool, worker_id: String, lease_duration: Duration) -> Self {
        Self {
            pool,
            worker_id,
            lease_duration,
        }
    }

    /// Append an event within an existing transaction.
    ///
    /// The caller should perform business writes and event append in the same
    /// transaction to guarantee atomicity.
    pub async fn append_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        event: &DomainEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO outbox_events
                (event_id, event_type, tenant_id, aggregate_id, aggregate_type,
                 payload, schema_version, trace_id, occurred_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending')
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

    /// Atomically claim a batch of events using `FOR UPDATE SKIP LOCKED`.
    ///
    /// Returns claimed events with lease set. Events are ordered
    /// deterministically by `(available_at, event_id)`.
    pub async fn claim_batch(&self, batch_size: i64) -> Result<Vec<OutboxRecord>, sqlx::Error> {
        let lease_secs = i64::try_from(self.lease_duration.as_secs()).unwrap_or(i64::MAX);

        let mut tx = self.pool.begin().await?;

        let records = sqlx::query_as::<_, OutboxRecord>(
            r"
            WITH claimed AS (
                SELECT event_id
                FROM outbox_events
                WHERE status IN ('pending', 'retry_scheduled')
                  AND available_at <= NOW()
                ORDER BY available_at, event_id
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE outbox_events e
            SET status = 'processing',
                claimed_by = $2,
                lease_until = NOW() + make_interval(secs => $3),
                attempt_count = e.attempt_count + 1
            FROM claimed
            WHERE e.event_id = claimed.event_id
            RETURNING e.event_id, e.event_type, e.tenant_id, e.aggregate_id,
                      e.aggregate_type, e.payload, e.schema_version, e.trace_id,
                      e.occurred_at, e.status, e.attempt_count, e.max_attempts,
                      e.available_at, e.claimed_by, e.lease_until, e.published_at,
                      e.last_error
            ",
        )
        .bind(batch_size)
        .bind(&self.worker_id)
        .bind(lease_secs)
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(records)
    }

    /// Mark an event as successfully published.
    pub async fn mark_published(&self, event_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE outbox_events
            SET status = 'published',
                published_at = NOW(),
                claimed_by = NULL,
                lease_until = NULL
            WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark an event as failed with error.
    ///
    /// Schedules retry with exponential backoff if attempts remain;
    /// otherwise marks the event as permanently failed.
    pub async fn mark_failed(&self, event_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE outbox_events
            SET status = CASE
                    WHEN attempt_count < max_attempts THEN 'retry_scheduled'
                    ELSE 'failed'
                END,
                available_at = CASE
                    WHEN attempt_count < max_attempts
                    THEN NOW() + make_interval(secs => LEAST(power(2, attempt_count)::bigint, 300))
                    ELSE available_at
                END,
                last_error = $2,
                claimed_by = NULL,
                lease_until = NULL
            WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Recover events whose lease has expired (worker crashed).
    ///
    /// Returns the number of recovered events.
    pub async fn recover_expired_leases(&self) -> Result<u64, sqlx::Error> {
        let result: PgQueryResult = sqlx::query(
            r"
            UPDATE outbox_events
            SET status = 'retry_scheduled',
                claimed_by = NULL,
                lease_until = NULL,
                available_at = NOW()
            WHERE status = 'processing' AND lease_until < NOW()
            ",
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// Exponential backoff: 2^attempt seconds, capped at 5 minutes.
#[must_use]
pub fn backoff_duration(attempt_count: i32) -> Duration {
    let exp = u32::try_from(attempt_count).unwrap_or(0);
    let secs = 2u64.saturating_pow(exp).min(300);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_attempt_zero() {
        // 2^0 = 1 second
        assert_eq!(backoff_duration(0), Duration::from_secs(1));
    }

    #[test]
    fn backoff_attempt_one() {
        // 2^1 = 2 seconds
        assert_eq!(backoff_duration(1), Duration::from_secs(2));
    }

    #[test]
    fn backoff_attempt_three() {
        // 2^3 = 8 seconds
        assert_eq!(backoff_duration(3), Duration::from_secs(8));
    }

    #[test]
    fn backoff_caps_at_five_minutes() {
        // 2^10 = 1024 > 300, so capped at 300
        assert_eq!(backoff_duration(10), Duration::from_secs(300));
    }

    #[test]
    fn backoff_large_attempt_caps() {
        // Very large attempt still caps at 300
        assert_eq!(backoff_duration(30), Duration::from_secs(300));
    }

    #[test]
    fn outbox_status_display() {
        assert_eq!(OutboxStatus::Pending.as_str(), "pending");
        assert_eq!(OutboxStatus::Processing.as_str(), "processing");
        assert_eq!(OutboxStatus::Published.as_str(), "published");
        assert_eq!(OutboxStatus::RetryScheduled.as_str(), "retry_scheduled");
        assert_eq!(OutboxStatus::Failed.as_str(), "failed");
    }
}
