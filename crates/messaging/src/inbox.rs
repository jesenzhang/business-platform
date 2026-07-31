use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Database-backed consumer idempotency marker.
pub struct InboxIdempotency;

impl InboxIdempotency {
    /// Inserts a marker and returns false when this consumer already handled
    /// the event. The caller performs its business side effect in the same
    /// transaction.
    pub async fn record_if_new(
        transaction: &mut Transaction<'_, Postgres>,
        consumer_name: &str,
        event_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r"
            INSERT INTO inbox_events (consumer_name, event_id)
            VALUES ($1, $2)
            ON CONFLICT (consumer_name, event_id) DO NOTHING
            ",
        )
        .bind(consumer_name)
        .bind(event_id)
        .execute(&mut **transaction)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
