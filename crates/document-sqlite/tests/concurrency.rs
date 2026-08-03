use std::sync::Arc;

use document::domain::DocumentMetadata;
use document::ports::{CreateDocumentUnitOfWork, PersistNewDocument};
use document_sqlite::{SqliteCreateDocumentUnitOfWork, MIGRATOR};
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::Barrier;
use uuid::Uuid;

async fn setup() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await;
    let Ok(pool) = pool else {
        unreachable!("temporary SQLite pool must connect");
    };
    let migration = MIGRATOR.run(&pool).await;
    assert!(
        migration.is_ok(),
        "SQLite migrations must apply: {migration:?}"
    );
    pool
}

fn command(
    tenant_id: Uuid,
    filename: &str,
    idempotency_key: &str,
    fingerprint: &str,
) -> PersistNewDocument {
    let document = DocumentMetadata::create(
        tenant_id,
        filename.to_string(),
        "application/pdf".to_string(),
        filename.to_string(),
        Uuid::now_v7(),
        Some(1),
    );
    let Ok(document) = document else {
        unreachable!("concurrency fixture must be valid");
    };
    PersistNewDocument {
        document,
        idempotency_key: idempotency_key.to_string(),
        request_fingerprint: fingerprint.to_string(),
        fingerprint_version: 1,
    }
}

#[tokio::test]
async fn concurrent_same_key_is_one_create_and_nine_replays() {
    let pool = setup().await;
    let adapter = Arc::new(SqliteCreateDocumentUnitOfWork::new(pool.clone()));
    let fixture = command(
        Uuid::now_v7(),
        "concurrent.pdf",
        "same-key",
        "same-fingerprint",
    );
    let barrier = Arc::new(Barrier::new(10));
    let mut handles = Vec::new();
    for _ in 0..10 {
        let adapter = Arc::clone(&adapter);
        let barrier = Arc::clone(&barrier);
        let fixture = fixture.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            adapter.execute(fixture).await
        }));
    }

    let mut created = 0;
    let mut replayed = 0;
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "task must not panic");
        let Ok(result) = result else { unreachable!() };
        assert!(result.is_ok(), "same-key request must not fail: {result:?}");
        let Ok(result) = result else { unreachable!() };
        if result.replayed {
            replayed += 1;
        } else {
            created += 1;
        }
    }
    assert_eq!(created, 1);
    assert_eq!(replayed, 9);

    for table in [
        "documents",
        "audit_events",
        "outbox_events",
        "document_idempotency",
    ] {
        let query = format!("SELECT COUNT(*) FROM {table}");
        let count = sqlx::query_scalar::<_, i64>(&query).fetch_one(&pool).await;
        assert_eq!(count.ok(), Some(1), "exactly one {table} row expected");
    }

    drop(adapter);
    let restarted = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let replay = restarted.execute(fixture).await;
    assert!(replay.is_ok(), "restart replay must succeed: {replay:?}");
    let Ok(replay) = replay else { unreachable!() };
    assert!(replay.replayed);
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await;
    assert_eq!(count.ok(), Some(1));

    drop(pool);
}

#[tokio::test]
async fn concurrent_different_fingerprints_have_one_conflict_and_one_side_effect() {
    let pool = setup().await;
    let adapter = Arc::new(SqliteCreateDocumentUnitOfWork::new(pool.clone()));
    let tenant = Uuid::now_v7();
    let first = command(tenant, "first.pdf", "conflicting-key", "fingerprint-a");
    let second = command(tenant, "second.pdf", "conflicting-key", "fingerprint-b");
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for fixture in [first, second] {
        let adapter = Arc::clone(&adapter);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            adapter.execute(fixture).await
        }));
    }

    let mut successes = 0;
    let mut conflicts = 0;
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "task must not panic");
        let Ok(result) = result else { unreachable!() };
        match result {
            Ok(_) => successes += 1,
            Err(document::ports::ApplicationPortError::IdempotencyConflict) => conflicts += 1,
            Err(error) => unreachable!("unexpected concurrent result: {error}"),
        }
    }
    assert_eq!(successes, 1);
    assert_eq!(conflicts, 1);
    for table in [
        "documents",
        "audit_events",
        "outbox_events",
        "document_idempotency",
    ] {
        let query = format!("SELECT COUNT(*) FROM {table}");
        let count = sqlx::query_scalar::<_, i64>(&query).fetch_one(&pool).await;
        assert_eq!(count.ok(), Some(1), "exactly one {table} row expected");
    }

    drop(pool);
}
