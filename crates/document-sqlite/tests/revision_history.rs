use document::domain::{DocumentMetadata, DocumentRepository};
use document::ports::{CreateDocumentUnitOfWork, PersistNewDocument};
use document_sqlite::{SqliteCreateDocumentUnitOfWork, MIGRATOR};
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

#[tokio::test]
async fn saving_a_new_revision_keeps_r1_and_updates_current_revision_atomically() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await;
    let Ok(pool) = pool else { unreachable!() };
    assert!(MIGRATOR.run(&pool).await.is_ok());

    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();
    let document = DocumentMetadata::create(
        tenant_id,
        "contract.pdf".to_string(),
        "application/pdf".to_string(),
        "uploads/contract.pdf".to_string(),
        created_by,
        Some(10),
    );
    let Ok(mut document) = document else {
        unreachable!()
    };
    let created = SqliteCreateDocumentUnitOfWork::new(pool.clone())
        .execute(PersistNewDocument {
            document: document.clone(),
            idempotency_key: "revision-history-create".to_string(),
            request_fingerprint: "revision-history-fingerprint".to_string(),
            fingerprint_version: 1,
            initial_revision_sha256: None,
        })
        .await;
    assert!(created.is_ok());

    let expected_version = document.aggregate_version();
    let r1 = document
        .initial_revision()
        .unwrap_or_else(|_| unreachable!());
    let r1_id = r1.id().to_string();
    let r2 = document
        .replace_content_revision(
            "replacement.pdf".to_string(),
            Some("content replacement".to_string()),
        )
        .unwrap_or_else(|_| unreachable!());
    let store = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    assert!(store
        .save(&document, Some(&r2), expected_version)
        .await
        .is_ok());

    let loaded = store
        .load(tenant_id, document.id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    assert_eq!(loaded.current_revision_id(), r2.id());
    assert_eq!(loaded.content_revision().value(), 2);

    let revisions = sqlx::query_as::<_, (i64, String, Option<String>)>(
        "SELECT revision_no, id, parent_revision_id FROM document_revisions WHERE tenant_id = ?1 AND document_id = ?2 ORDER BY revision_no",
    )
    .bind(tenant_id.to_string())
    .bind(document.id().to_string())
    .fetch_all(&pool)
    .await;
    let Ok(revisions) = revisions else {
        unreachable!()
    };
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].0, 1);
    assert_eq!(revisions[0].1, r1_id);
    assert_eq!(revisions[1].0, 2);
    assert_eq!(revisions[1].1, r2.id().to_string());
    assert_eq!(revisions[1].2.as_deref(), Some(r1_id.as_str()));
}
