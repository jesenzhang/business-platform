#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 - tenant fence on the idempotent-create replay lookup.
//!
//! `document_idempotency` is keyed by `(tenant_id, idempotency_key)`, but its
//! `document_id` column is a plain foreign key onto `documents(id)` and the
//! replay lookup joined `documents` on `id` alone. A row that names another
//! tenant document (bad migration, manual repair, point-in-time restore) was
//! therefore replayed to the requesting tenant verbatim.
//!
//! Wave 4 gap D-W4-01: the replay lookup must be tenant bound, and a rehydrated
//! document must be re-checked against the requesting tenant before it is
//! returned. This suite is the failing-first reproduction.

use document::domain::DocumentMetadata;
use document::ports::{ApplicationPortError, CreateDocumentUnitOfWork, PersistNewDocument};
use document_sqlite::{SqliteCreateDocumentUnitOfWork, MIGRATOR};
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

#[allow(clippy::expect_used)]
async fn setup() -> sqlx::SqlitePool {
    let path = std::env::temp_dir().join(format!("bp-wave4-tenant-fence-{}.db", Uuid::now_v7()));
    let url = format!("sqlite://{}", path.to_string_lossy().replace('\\', "/"));
    assert!(sqlx::Sqlite::create_database(&url).await.is_ok());
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("SQLite pool must connect");
    assert!(
        MIGRATOR.run(&pool).await.is_ok(),
        "SQLite migrations must apply"
    );
    pool
}

fn command(
    tenant_id: Uuid,
    document_id: Uuid,
    idempotency_key: &str,
    request_fingerprint: &str,
) -> PersistNewDocument {
    let document = DocumentMetadata::create_with_id(
        document_id,
        tenant_id,
        "wave4-tenant-fence.pdf".to_string(),
        "application/pdf".to_string(),
        "uploads/wave4-tenant-fence.pdf".to_string(),
        Uuid::now_v7(),
        Some(11),
    );
    let Ok(document) = document else {
        unreachable!("wave 4 tenant fence fixture must be valid");
    };
    PersistNewDocument {
        document,
        idempotency_key: idempotency_key.to_string(),
        request_fingerprint: request_fingerprint.to_string(),
        fingerprint_version: 1,
        initial_revision_sha256: None,
    }
}

#[allow(clippy::expect_used)]
async fn stored_fingerprint(pool: &sqlx::SqlitePool, tenant_id: Uuid, key: &str) -> String {
    let row: (String,) = sqlx::query_as(
        "SELECT request_fingerprint FROM document_idempotency WHERE tenant_id = ?1 AND idempotency_key = ?2",
    )
    .bind(tenant_id.to_string())
    .bind(key)
    .fetch_one(pool)
    .await
    .expect("idempotency row must exist after a create");
    row.0
}

#[allow(clippy::expect_used)]
async fn bind_idempotency_row_to(
    pool: &sqlx::SqlitePool,
    tenant_id: Uuid,
    key: &str,
    document_id: Uuid,
) {
    let updated = sqlx::query("UPDATE document_idempotency SET document_id = ?3 WHERE tenant_id = ?1 AND idempotency_key = ?2")
        .bind(tenant_id.to_string())
        .bind(key)
        .bind(document_id.to_string())
        .execute(pool)
        .await
        .expect("the mis-bound idempotency row must be plantable")
        .rows_affected();
    assert_eq!(updated, 1, "exactly one idempotency row must be re-bound");
}

/// A replay must never hand back a document owned by another tenant, even when
/// the durable idempotency row has been mis-bound to that document.
#[tokio::test]
async fn replay_never_returns_a_foreign_tenants_document() {
    let pool = setup().await;
    let adapter = SqliteCreateDocumentUnitOfWork::new(pool.clone());

    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let document_a = Uuid::now_v7();
    let document_b = Uuid::now_v7();

    let created_a = adapter
        .execute(command(tenant_a, document_a, "shared-key", "fingerprint-a"))
        .await;
    assert!(
        created_a.is_ok(),
        "tenant A create must succeed: {created_a:?}"
    );

    let created_b = adapter
        .execute(command(tenant_b, document_b, "shared-key", "fingerprint-b"))
        .await;
    assert!(
        created_b.is_ok(),
        "tenant B create must succeed: {created_b:?}"
    );

    // Corruption: tenant B keeps its own fingerprint but the durable row now
    // points at tenant A document, so a fingerprint comparison alone cannot
    // catch it.
    bind_idempotency_row_to(&pool, tenant_b, "shared-key", document_a).await;

    let replay = adapter
        .execute(command(tenant_b, document_b, "shared-key", "fingerprint-b"))
        .await;
    match replay {
        Ok(result) => {
            assert_eq!(
                result.document.tenant_id(),
                tenant_b,
                "replay crossed the tenant boundary and returned tenant A document"
            );
            assert_eq!(result.document.id(), document_b);
        }
        Err(error) => {
            assert!(
                matches!(error, ApplicationPortError::Failed),
                "a mis-bound idempotency row must fail closed, got {error:?}"
            );
        }
    }

    drop(pool);
}

/// The fence must not break the legitimate case: the same tenant replaying the
/// same key still receives the original document.
#[tokio::test]
async fn same_tenant_replay_still_returns_the_original_document() {
    let pool = setup().await;
    let adapter = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let tenant = Uuid::now_v7();
    let document_id = Uuid::now_v7();

    let created = adapter
        .execute(command(tenant, document_id, "replay-key", "same"))
        .await;
    assert!(created.is_ok(), "create must succeed: {created:?}");
    assert!(!created.expect("create result").replayed);

    let stored = stored_fingerprint(&pool, tenant, "replay-key").await;
    let replay = adapter
        .execute(command(tenant, document_id, "replay-key", &stored))
        .await;
    let Ok(replay) = replay else {
        unreachable!("same-tenant replay must succeed: {replay:?}");
    };
    assert!(replay.replayed);
    assert_eq!(replay.document.id(), document_id);
    assert_eq!(replay.document.tenant_id(), tenant);

    drop(pool);
}
