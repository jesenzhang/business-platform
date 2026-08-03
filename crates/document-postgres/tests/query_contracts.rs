#![allow(clippy::expect_used)]

use std::sync::Arc;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn postgres_satisfies_shared_document_persistence_contract() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running the PostgreSQL query contract");
    let pool = sqlx::PgPool::connect(&database_url).await;
    let Ok(pool) = pool else { unreachable!() };
    assert!(runtime_migration::MIGRATOR.run(&pool).await.is_ok());
    let result = document_persistence_contracts::verify_document_persistence_contract(
        Arc::new(document_postgres::PostgresCreateDocumentUnitOfWork::new(
            pool.clone(),
        )),
        Arc::new(document_postgres::PostgresDocumentDetailQuery::new(
            pool.clone(),
        )),
        Arc::new(document_postgres::PostgresDocumentListQuery::new(
            pool.clone(),
        )),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");

    let connection = pool.acquire().await;
    let Ok(mut connection) = connection else {
        unreachable!()
    };
    assert!(sqlx::query("SET enable_seqscan = off")
        .execute(&mut *connection)
        .await
        .is_ok());
    let plan = sqlx::query_scalar::<_, String>(
        "EXPLAIN SELECT id FROM documents WHERE tenant_id = $1 AND (created_at, id) < ($2, $3) ORDER BY created_at DESC, id DESC LIMIT 51",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(chrono::Utc::now())
    .bind(uuid::Uuid::now_v7())
    .fetch_all(&mut *connection)
    .await;
    let Ok(plan) = plan else { unreachable!() };
    assert!(plan.join(" ").contains("idx_documents_tenant_created_id"));
}
