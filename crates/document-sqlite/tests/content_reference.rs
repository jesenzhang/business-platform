#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 - content reference shape and revision immutability.
//!
//! Migration `016_document_revisions.sql` claims that every new write uses a
//! revision-scoped storage key, and the `SQLite` migration installs triggers that
//! make `document_revisions` append-only. Neither claim had a test. Wave 4 pins
//! both against the real adapter, because the claim decides whether an evidence
//! or artifact version can be bound to a content revision at all.

use document::domain::DocumentMetadata;
use document::ports::{CreateDocumentUnitOfWork, PersistNewDocument};
use document_sqlite::{SqliteCreateDocumentUnitOfWork, MIGRATOR};
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

#[allow(clippy::expect_used)]
async fn setup() -> sqlx::SqlitePool {
    let path = std::env::temp_dir().join(format!("bp-wave4-content-ref-{}.db", Uuid::now_v7()));
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
    revision_id: Uuid,
    sha256: Option<&str>,
) -> PersistNewDocument {
    let document = DocumentMetadata::create_with_revision_id(
        document_id,
        tenant_id,
        "wave4-reference.pdf".to_string(),
        "application/pdf".to_string(),
        // The caller supplies a logical path; the aggregate owns the physical
        // reference and must replace it with the revision-scoped key.
        "uploads/wave4-reference.pdf".to_string(),
        Uuid::now_v7(),
        Some(17),
        revision_id,
    );
    let Ok(document) = document else {
        unreachable!("wave 4 content reference fixture must be valid");
    };
    PersistNewDocument {
        document,
        idempotency_key: "content-reference-key".to_string(),
        request_fingerprint: "c".repeat(64),
        fingerprint_version: 1,
        initial_revision_sha256: sha256.map(ToOwned::to_owned),
    }
}

#[allow(clippy::expect_used)]
async fn text(pool: &sqlx::SqlitePool, sql: &str) -> String {
    let row: (String,) = sqlx::query_as(sql)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("query must succeed: {error}\n{sql}"));
    row.0
}

/// A new document must persist the revision-scoped key in both the aggregate row
/// and the immutable revision row, labelled as an initial upload.
#[tokio::test]
async fn new_write_persists_the_revision_scoped_content_reference() {
    let pool = setup().await;
    let adapter = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let tenant_id = Uuid::now_v7();
    let document_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let sha256 = "a".repeat(64);

    adapter
        .execute(command(tenant_id, document_id, revision_id, Some(&sha256)))
        .await
        .expect("create must succeed");

    let expected =
        format!("tenants/{tenant_id}/documents/{document_id}/revisions/{revision_id}/source");
    let stored_key = text(
        &pool,
        &format!("SELECT object_key FROM documents WHERE id = '{document_id}'"),
    )
    .await;
    assert_eq!(
        stored_key, expected,
        "stored object key must be revision scoped"
    );

    let (source_ref, reason, revision_no, stored_sha): (String, Option<String>, i64, Option<String>) =
        sqlx::query_as(
            "SELECT source_object_ref, change_reason, revision_no, sha256 FROM document_revisions WHERE document_id = ?1",
        )
        .bind(document_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("the initial revision row must exist");
    assert_eq!(source_ref, expected, "revision reference must match");
    assert_eq!(revision_no, 1);
    assert_eq!(
        reason.as_deref(),
        Some("initial upload"),
        "a new upload must not be labelled as a legacy backfill"
    );
    assert_eq!(stored_sha.as_deref(), Some(sha256.as_str()));

    drop(pool);
}

/// `document_revisions` is append-only: content revision rows that evidence and
/// artifacts are bound to must not be mutable or deletable.
#[tokio::test]
async fn revision_rows_are_append_only() {
    let pool = setup().await;
    let adapter = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let tenant_id = Uuid::now_v7();
    let document_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();

    adapter
        .execute(command(tenant_id, document_id, revision_id, None))
        .await
        .expect("create must succeed");

    let update = sqlx::query("UPDATE document_revisions SET sha256 = ?1 WHERE id = ?2")
        .bind("b".repeat(64))
        .bind(revision_id.to_string())
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "updating a content revision must be rejected: {update:?}"
    );

    let delete = sqlx::query("DELETE FROM document_revisions WHERE id = ?1")
        .bind(revision_id.to_string())
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "deleting a content revision must be rejected: {delete:?}"
    );

    let remaining: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM document_revisions WHERE document_id = ?1",
    )
    .bind(document_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count must succeed")
    .0;
    assert_eq!(
        remaining, 1,
        "the revision row must survive the rejected writes"
    );

    drop(pool);
}
