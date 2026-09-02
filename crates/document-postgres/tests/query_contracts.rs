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

    // The harness runs each fixture under a fresh random tenant, so it cannot
    // clean its own rows from inside the adapter-agnostic contract. This test
    // is the composition root that knows both the harness and the database, so
    // it snapshots the outbox before the run and deletes exactly the rows the
    // harness added afterwards. The contract suite runs targets sequentially
    // against a dedicated database (the CI contract-test job), so the harness
    // is the only writer inside this window. Documents themselves are retained
    // per the PLAN-0008 immutability precedent in business-api
    // documents_postgres tests.
    let before: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT event_id FROM outbox_events")
        .fetch_all(&pool)
        .await
        .expect("snapshot outbox");
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

    let cleanup = sqlx::query("DELETE FROM outbox_events WHERE NOT (event_id = ANY($1))")
        .bind(&before)
        .execute(&pool)
        .await;
    assert!(
        cleanup
            .as_ref()
            .is_ok_and(|report| report.rows_affected() >= 1),
        "harness outbox cleanup: {cleanup:?}"
    );

    let connection = pool.acquire().await;
    let Ok(mut connection) = connection else {
        unreachable!()
    };
    // Earlier targets insert and delete documents on this shared table, which
    // can leave the planner statistics stale enough to flip this index
    // assertion. Refresh them so the assertion measures the schema, not the
    // accumulated history of whichever targets ran before it.
    assert!(sqlx::query("ANALYZE documents")
        .execute(&mut *connection)
        .await
        .is_ok());
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
