use std::sync::Arc;

#[tokio::test]
async fn sqlite_satisfies_shared_document_persistence_contract() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await;
    let Ok(pool) = pool else { unreachable!() };
    assert!(document_sqlite::MIGRATOR.run(&pool).await.is_ok());

    let result = document_persistence_contracts::verify_document_persistence_contract(
        Arc::new(document_sqlite::SqliteCreateDocumentUnitOfWork::new(
            pool.clone(),
        )),
        Arc::new(document_sqlite::SqliteDocumentDetailQuery::new(
            pool.clone(),
        )),
        Arc::new(document_sqlite::SqliteDocumentListQuery::new(pool)),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn sqlite_rolls_back_the_document_when_atomic_side_effect_fails() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await;
    let Ok(pool) = pool else { unreachable!() };
    assert!(document_sqlite::MIGRATOR.run(&pool).await.is_ok());
    assert!(sqlx::query("DROP TABLE outbox_events")
        .execute(&pool)
        .await
        .is_ok());
    let tenant = uuid::Uuid::now_v7();
    let document = document::domain::DocumentMetadata::create(
        tenant,
        "rollback.pdf".to_string(),
        "application/pdf".to_string(),
        "uploads/rollback.pdf".to_string(),
        uuid::Uuid::now_v7(),
        Some(1),
    );
    let Ok(document) = document else {
        unreachable!()
    };
    let command = document::ports::PersistNewDocument {
        document: document.clone(),
        idempotency_key: "rollback".to_string(),
        request_fingerprint: "rollback".to_string(),
        fingerprint_version: 1,
        initial_revision_sha256: None,
    };
    let adapter = document_sqlite::SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let result = document::ports::CreateDocumentUnitOfWork::execute(&adapter, command).await;
    assert!(result.is_err());
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE id = ?1")
        .bind(document.id().to_string())
        .fetch_one(&pool)
        .await;
    assert_eq!(count.ok(), Some(0));
}
