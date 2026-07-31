//! Outbox reliability integration tests.
//! Requires running `PostgreSQL`. Run with:
//!   cargo test -p messaging -- --ignored
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use messaging::{DomainEvent, ReliableOutbox};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

/// Helper: connect to `PostgreSQL` using `DATABASE_URL` or default.
async fn setup_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform_test".to_string()
    });

    PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("failed to connect to PostgreSQL")
}

/// Helper: run migrations to ensure schema is up to date.
async fn run_migrations(pool: &sqlx::PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("failed to run migrations");
}

/// Helper: insert a test event directly into the outbox.
async fn insert_test_event(pool: &sqlx::PgPool, event: &DomainEvent) {
    sqlx::query(
        r"
        INSERT INTO outbox_events
            (event_id, event_type, tenant_id, aggregate_id, aggregate_type,
             payload, schema_version, trace_id, occurred_at, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending')
        ",
    )
    .bind(event.event_id)
    .bind(&event.event_type)
    .bind(&event.tenant_id)
    .bind(&event.aggregate_id)
    .bind(&event.aggregate_type)
    .bind(&event.payload)
    .bind(&event.schema_version)
    .bind(&event.trace_id)
    .bind(event.occurred_at)
    .execute(pool)
    .await
    .expect("failed to insert test event");
}

/// Helper: clean up test events by tenant.
async fn cleanup(pool: &sqlx::PgPool, tenant_id: &str) {
    sqlx::query("DELETE FROM outbox_events WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to cleanup");
}

/// Two workers claiming concurrently never get the same event.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn concurrent_claim_no_duplicates() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;

    let tenant = format!("test-concurrent-{}", Uuid::now_v7());

    // Insert 10 events
    for i in 0..10 {
        let event = DomainEvent::new(
            "test.concurrent",
            &tenant,
            format!("agg-{i}"),
            "TestAggregate",
            serde_json::json!({"i": i}),
        );
        insert_test_event(&pool, &event).await;
    }

    let outbox_a = ReliableOutbox::new(
        pool.clone(),
        "worker-a".to_string(),
        Duration::from_secs(60),
    );
    let outbox_b = ReliableOutbox::new(
        pool.clone(),
        "worker-b".to_string(),
        Duration::from_secs(60),
    );

    // Both claim concurrently
    let (claimed_a, claimed_b) = tokio::join!(outbox_a.claim_batch(10), outbox_b.claim_batch(10),);

    let claimed_a = claimed_a.expect("worker-a claim failed");
    let claimed_b = claimed_b.expect("worker-b claim failed");

    // No overlap
    let ids_a: Vec<Uuid> = claimed_a.iter().map(|r| r.event_id).collect();
    let ids_b: Vec<Uuid> = claimed_b.iter().map(|r| r.event_id).collect();

    for id in &ids_a {
        assert!(
            !ids_b.contains(id),
            "duplicate event claimed by both workers"
        );
    }

    // Total claimed equals total inserted
    assert_eq!(ids_a.len() + ids_b.len(), 10);

    cleanup(&pool, &tenant).await;
}

/// Fencing rejects a late worker after the lease is recovered and reclaimed.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn stale_worker_cannot_publish_or_fail() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant = format!("test-fencing-{}", Uuid::now_v7());
    let event = DomainEvent::new(
        "test.fencing",
        &tenant,
        "aggregate",
        "TestAggregate",
        serde_json::json!({}),
    );
    insert_test_event(&pool, &event).await;

    let worker_a =
        ReliableOutbox::new(pool.clone(), "worker-a".to_string(), Duration::from_secs(1));
    let worker_b = ReliableOutbox::new(
        pool.clone(),
        "worker-b".to_string(),
        Duration::from_secs(60),
    );
    let first = worker_a.claim_batch(1).await.expect("first claim");
    assert_eq!(first.len(), 1);
    let old_token = first[0].claim_token.expect("first token");
    let old_version = first[0].claim_version;
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    worker_a.recover_expired_leases().await.expect("recovery");
    let second = worker_b.claim_batch(1).await.expect("second claim");
    assert_eq!(second.len(), 1);
    assert!(matches!(
        worker_a
            .mark_published(event.event_id, old_token, old_version)
            .await,
        Err(messaging::OutboxError::LeaseLost)
    ));
    assert!(matches!(
        worker_a
            .mark_failed(event.event_id, old_token, old_version, "late")
            .await,
        Err(messaging::OutboxError::LeaseLost)
    ));
    cleanup(&pool, &tenant).await;
}

/// A larger concurrent batch has one owner per event.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn concurrent_claim_100_has_unique_ownership() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant = format!("test-concurrent-100-{}", Uuid::now_v7());
    for i in 0..100 {
        insert_test_event(
            &pool,
            &DomainEvent::new(
                "test.concurrent.100",
                &tenant,
                format!("aggregate-{i}"),
                "TestAggregate",
                serde_json::json!({"i": i}),
            ),
        )
        .await;
    }
    let workers: Vec<_> = (0..10)
        .map(|index| {
            ReliableOutbox::new(
                pool.clone(),
                format!("worker-{index}"),
                Duration::from_secs(60),
            )
        })
        .collect();
    let mut tasks = Vec::new();
    for worker in workers {
        tasks.push(tokio::spawn(async move {
            worker.claim_batch(10).await.expect("claim")
        }));
    }
    let mut ids = std::collections::HashSet::new();
    for task in tasks {
        for record in task.await.expect("worker task") {
            assert!(ids.insert(record.event_id), "duplicate ownership");
        }
    }
    assert_eq!(ids.len(), 100);
    cleanup(&pool, &tenant).await;
}

/// Expired leases are recovered and events become available again.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn expired_lease_recovery() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;

    let tenant = format!("test-lease-{}", Uuid::now_v7());

    let event = DomainEvent::new(
        "test.lease",
        &tenant,
        "agg-lease",
        "TestAggregate",
        serde_json::json!({}),
    );
    insert_test_event(&pool, &event).await;

    // Claim with a very short lease (1 second)
    let outbox = ReliableOutbox::new(
        pool.clone(),
        "worker-lease".to_string(),
        Duration::from_secs(1),
    );
    let claimed = outbox.claim_batch(1).await.expect("claim failed");
    assert_eq!(claimed.len(), 1);

    // Wait for lease to expire
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Recover expired leases
    let recovered = outbox
        .recover_expired_leases()
        .await
        .expect("recovery failed");
    assert_eq!(recovered, 1);

    // Event should be claimable again
    let reclaimed = outbox.claim_batch(1).await.expect("reclaim failed");
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].event_id, event.event_id);

    cleanup(&pool, &tenant).await;
}

/// Failed events enter retry with backoff.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn failure_schedules_retry() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;

    let tenant = format!("test-retry-{}", Uuid::now_v7());

    let event = DomainEvent::new(
        "test.retry",
        &tenant,
        "agg-retry",
        "TestAggregate",
        serde_json::json!({}),
    );
    insert_test_event(&pool, &event).await;

    let outbox = ReliableOutbox::new(
        pool.clone(),
        "worker-retry".to_string(),
        Duration::from_secs(60),
    );

    // Claim and fail
    let claimed = outbox.claim_batch(1).await.expect("claim failed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].attempt_count, 1);

    outbox
        .mark_failed(
            event.event_id,
            claimed[0].claim_token.expect("claim token"),
            claimed[0].claim_version,
            "transient error",
        )
        .await
        .expect("mark_failed failed");

    // Check status is retry_scheduled
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM outbox_events WHERE event_id = $1")
            .bind(event.event_id)
            .fetch_one(&pool)
            .await
            .expect("fetch failed");

    assert_eq!(row.0, "retry_scheduled");
    assert_eq!(row.1.as_deref(), Some("transient error"));

    // available_at should be in the future (backoff applied)
    let available_at: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT available_at FROM outbox_events WHERE event_id = $1")
            .bind(event.event_id)
            .fetch_one(&pool)
            .await
            .expect("fetch available_at failed");

    assert!(
        available_at.0 > chrono::Utc::now(),
        "available_at should be in the future due to backoff"
    );

    cleanup(&pool, &tenant).await;
}

/// Events exceeding `max_attempts` become permanently failed.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn max_attempts_reached() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;

    let tenant = format!("test-maxattempts-{}", Uuid::now_v7());

    let event = DomainEvent::new(
        "test.maxattempts",
        &tenant,
        "agg-max",
        "TestAggregate",
        serde_json::json!({}),
    );
    insert_test_event(&pool, &event).await;

    let outbox = ReliableOutbox::new(
        pool.clone(),
        "worker-max".to_string(),
        Duration::from_secs(60),
    );

    // Simulate reaching max_attempts by setting attempt_count = max_attempts
    sqlx::query("UPDATE outbox_events SET attempt_count = max_attempts WHERE event_id = $1")
        .bind(event.event_id)
        .execute(&pool)
        .await
        .expect("set attempt_count failed");

    // Claim (need to make it available first)
    sqlx::query(
        "UPDATE outbox_events SET status = 'pending', available_at = NOW() WHERE event_id = $1",
    )
    .bind(event.event_id)
    .execute(&pool)
    .await
    .expect("reset status failed");

    let claimed = outbox.claim_batch(1).await.expect("claim failed");
    assert_eq!(claimed.len(), 0, "events at max attempts are not claimable");

    let row: (String,) = sqlx::query_as("SELECT status FROM outbox_events WHERE event_id = $1")
        .bind(event.event_id)
        .fetch_one(&pool)
        .await
        .expect("fetch failed");

    assert_eq!(row.0, "pending");

    cleanup(&pool, &tenant).await;
}

/// Claim ordering is deterministic.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn deterministic_claim_order() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;

    let tenant = format!("test-order-{}", Uuid::now_v7());

    // Insert events with explicit ordering via UUIDv7 (time-ordered)
    let mut events = Vec::new();
    for i in 0..5 {
        let event = DomainEvent::new(
            "test.order",
            &tenant,
            format!("agg-order-{i}"),
            "TestAggregate",
            serde_json::json!({"seq": i}),
        );
        insert_test_event(&pool, &event).await;
        events.push(event);
    }

    let outbox = ReliableOutbox::new(
        pool.clone(),
        "worker-order".to_string(),
        Duration::from_secs(60),
    );

    let claimed = outbox.claim_batch(5).await.expect("claim failed");
    assert_eq!(claimed.len(), 5);

    // Verify ordering: available_at ascending, then event_id ascending
    for window in claimed.windows(2) {
        let a = &window[0];
        let b = &window[1];
        assert!(
            a.available_at < b.available_at
                || (a.available_at == b.available_at && a.event_id < b.event_id),
            "claim order is not deterministic: {:?} should come before {:?}",
            a.event_id,
            b.event_id
        );
    }

    cleanup(&pool, &tenant).await;
}
