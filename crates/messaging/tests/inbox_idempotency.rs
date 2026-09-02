//! `PostgreSQL` contracts for transactional Inbox idempotency.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use messaging::InboxIdempotency;
use tokio::sync::OnceCell;
use uuid::Uuid;

async fn setup_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("failed to connect to PostgreSQL")
}

/// `CREATE TABLE IF NOT EXISTS` is not concurrency safe in PostgreSQL: two
/// parallel sessions can pass the existence check together and one fails on
/// the duplicate. Tests in this binary run on parallel threads, so the schema
/// setup happens exactly once per test process.
static PREPARED: OnceCell<()> = OnceCell::const_new();

async fn prepare(pool: &sqlx::PgPool) {
    PREPARED
        .get_or_init(|| async {
            runtime_migration::MIGRATOR
                .run(pool)
                .await
                .expect("failed to run migrations");
            sqlx::query(
                r"
                CREATE TABLE IF NOT EXISTS inbox_contract_effects (
                    consumer_name VARCHAR(200) NOT NULL,
                    event_id UUID NOT NULL,
                    effect_count INTEGER NOT NULL,
                    PRIMARY KEY (consumer_name, event_id)
                )
                ",
            )
            .execute(pool)
            .await
            .expect("failed to create contract projection");
        })
        .await;
}

async fn consume(pool: &sqlx::PgPool, consumer: &str, event_id: Uuid) -> bool {
    let mut tx = pool.begin().await.expect("begin transaction");
    let is_new = InboxIdempotency::record_if_new(&mut tx, consumer, event_id)
        .await
        .expect("record inbox marker");
    if is_new {
        sqlx::query(
            "INSERT INTO inbox_contract_effects (consumer_name, event_id, effect_count) VALUES ($1, $2, 1)",
        )
        .bind(consumer)
        .bind(event_id)
        .execute(&mut *tx)
        .await
        .expect("write consumer side effect");
    }
    tx.commit().await.expect("commit transaction");
    is_new
}

async fn effect_count(pool: &sqlx::PgPool, consumer: &str, event_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(effect_count), 0)::BIGINT FROM inbox_contract_effects WHERE consumer_name = $1 AND event_id = $2",
    )
    .bind(consumer)
    .bind(event_id)
    .fetch_one(pool)
    .await
    .expect("read side effect count")
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn sequential_duplicate_commits_one_side_effect() {
    let pool = setup_pool().await;
    prepare(&pool).await;
    let consumer = format!("sequential-{}", Uuid::now_v7());
    let event_id = Uuid::now_v7();

    assert!(consume(&pool, &consumer, event_id).await);
    assert!(!consume(&pool, &consumer, event_id).await);
    assert_eq!(effect_count(&pool, &consumer, event_id).await, 1);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn concurrent_duplicate_commits_one_side_effect() {
    let pool = setup_pool().await;
    prepare(&pool).await;
    let consumer = format!("concurrent-{}", Uuid::now_v7());
    let event_id = Uuid::now_v7();

    let (first, second) = tokio::join!(
        consume(&pool, &consumer, event_id),
        consume(&pool, &consumer, event_id)
    );
    assert_ne!(first, second);
    assert_eq!(effect_count(&pool, &consumer, event_id).await, 1);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn rollback_releases_marker_and_side_effect() {
    let pool = setup_pool().await;
    prepare(&pool).await;
    let consumer = format!("rollback-{}", Uuid::now_v7());
    let event_id = Uuid::now_v7();

    let mut tx = pool.begin().await.expect("begin transaction");
    assert!(
        InboxIdempotency::record_if_new(&mut tx, &consumer, event_id)
            .await
            .expect("record marker")
    );
    sqlx::query(
        "INSERT INTO inbox_contract_effects (consumer_name, event_id, effect_count) VALUES ($1, $2, 1)",
    )
    .bind(&consumer)
    .bind(event_id)
    .execute(&mut *tx)
    .await
    .expect("write side effect");
    tx.rollback().await.expect("rollback transaction");

    assert!(consume(&pool, &consumer, event_id).await);
    assert_eq!(effect_count(&pool, &consumer, event_id).await, 1);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn consumers_have_independent_idempotency_scopes() {
    let pool = setup_pool().await;
    prepare(&pool).await;
    let event_id = Uuid::now_v7();
    let first = format!("consumer-a-{}", Uuid::now_v7());
    let second = format!("consumer-b-{}", Uuid::now_v7());

    assert!(consume(&pool, &first, event_id).await);
    assert!(consume(&pool, &second, event_id).await);
    assert_eq!(effect_count(&pool, &first, event_id).await, 1);
    assert_eq!(effect_count(&pool, &second, event_id).await, 1);
}
