use chrono::{Duration, Utc};
use document_processing::ports::{
    ProcessingJobClaimPort, ProcessingJobCommandPort, ProcessingJobQuery,
};
use document_processing::{ProcessingJob, ProcessingJobStatus};
use document_processing_sqlite::{SqliteProcessingStore, MIGRATOR};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Executor;
use uuid::Uuid;

async fn setup() -> (sqlx::SqlitePool, Uuid, Uuid) {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await;
    let Ok(pool) = pool else {
        unreachable!("SQLite pool must connect")
    };
    pool.execute("PRAGMA foreign_keys = ON").await.ok();
    pool.execute("CREATE TABLE documents (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, original_filename TEXT NOT NULL, content_type TEXT NOT NULL, object_key TEXT NOT NULL, status TEXT NOT NULL, version INTEGER NOT NULL, content_revision INTEGER NOT NULL, size_bytes INTEGER, created_by TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)")
        .await
        .ok();
    pool.execute("CREATE TABLE outbox_events (event_id TEXT PRIMARY KEY, event_type TEXT NOT NULL, tenant_id TEXT NOT NULL, aggregate_id TEXT NOT NULL, aggregate_type TEXT NOT NULL, payload TEXT NOT NULL, schema_version TEXT NOT NULL, occurred_at TEXT NOT NULL, published INTEGER NOT NULL DEFAULT 0)")
        .await
        .ok();
    assert!(MIGRATOR.run(&pool).await.is_ok());
    let tenant = Uuid::now_v7();
    let document = Uuid::now_v7();
    sqlx::query("INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, created_by, created_at, updated_at) VALUES (?1, ?2, 'source.txt', 'text/plain', 'tenants/source', 'active', 1, 1, ?3, ?4, ?4)")
        .bind(document.to_string())
        .bind(tenant.to_string())
        .bind(Uuid::now_v7().to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .ok();
    (pool, tenant, document)
}

#[tokio::test]
async fn create_replay_claim_and_tenant_scoped_query_are_durable() {
    let (pool, tenant, document) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let now = Utc::now();
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        "request-1".to_string(),
        Uuid::now_v7(),
        3,
        now - Duration::seconds(1),
    )
    .unwrap_or_else(|_| unreachable!());
    let first = store.create(&job).await;
    assert!(first.is_ok());
    let replay = store.create(&job).await;
    assert!(replay.is_ok());
    let claimed = store.claim_next("worker-a", Utc::now(), 30).await;
    assert!(claimed.is_ok());
    let claimed = claimed.ok().flatten().unwrap_or_else(|| unreachable!());
    assert_eq!(claimed.job.status(), ProcessingJobStatus::Running);
    assert!(store
        .heartbeat(
            claimed.job.id(),
            "worker-a",
            &claimed.lease_token,
            claimed.fence_version,
            Utc::now(),
            30,
        )
        .await
        .is_ok());
    assert!(store
        .release(
            claimed.job.id(),
            "worker-a",
            &claimed.lease_token,
            claimed.fence_version,
            Utc::now(),
        )
        .await
        .is_ok());
    let detail = store.detail(tenant, job.id()).await;
    assert!(detail.ok().flatten().is_some());
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1")
            .bind(job.id().to_string())
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| unreachable!());
    assert_eq!(outbox_count, 1);
}
