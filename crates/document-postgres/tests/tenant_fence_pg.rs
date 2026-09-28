#![allow(clippy::expect_used, clippy::panic)]
//! PLAN-0014 Wave 4 gap D-W4-01 - `PostgreSQL` mirror of the tenant fence on the
//! idempotent-create replay lookup.
//!
//! The foreign key on `document_idempotency.document_id` points at
//! `documents(id)` only, so `PostgreSQL` accepts a row that binds one tenant's
//! idempotency key to another tenant's document. The replay lookup used to join
//! on `id` alone and returned that document to the requesting tenant.

use document::domain::DocumentMetadata;
use document::ports::{ApplicationPortError, CreateDocumentUnitOfWork, PersistNewDocument};
use document_postgres::PostgresCreateDocumentUnitOfWork;
use uuid::Uuid;

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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn replay_never_returns_a_foreign_tenants_document() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running the PostgreSQL tenant fence");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("PostgreSQL must be reachable");
    assert!(
        runtime_migration::MIGRATOR.run(&pool).await.is_ok(),
        "runtime migrations must apply"
    );
    let adapter = PostgresCreateDocumentUnitOfWork::new(pool.clone());

    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let document_a = Uuid::now_v7();
    let document_b = Uuid::now_v7();

    adapter
        .execute(command(tenant_a, document_a, "shared-key", "fingerprint-a"))
        .await
        .expect("tenant A create must succeed");
    adapter
        .execute(command(tenant_b, document_b, "shared-key", "fingerprint-b"))
        .await
        .expect("tenant B create must succeed");

    // Corruption: the durable row keeps tenant B's fingerprint but names tenant
    // A document. The schema allows this because the foreign key is on id only.
    let rebound = sqlx::query(
        "UPDATE document_idempotency SET document_id = $3 WHERE tenant_id = $1 AND idempotency_key = $2",
    )
    .bind(tenant_b)
    .bind("shared-key")
    .bind(document_a)
    .execute(&pool)
    .await
    .expect("the mis-bound idempotency row must be plantable");
    assert_eq!(rebound.rows_affected(), 1);

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
}
