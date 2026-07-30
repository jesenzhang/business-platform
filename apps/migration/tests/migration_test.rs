//! Migration integration tests.
//!
//! The compile-time test runs without a database; the runtime tests require a
//! reachable `PostgreSQL` instance and are gated behind `#[ignore]`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

/// Verify migrations compile and can be listed without a database.
///
/// The path is relative to this crate's manifest directory (`apps/migration`).
#[test]
fn migrations_are_valid() {
    let migrator = sqlx::migrate!("../../migrations");
    let count = migrator.iter().count();
    assert!(count > 0, "expected at least one migration, got {count}");

    // Versions must be unique and strictly increasing in iteration order.
    let mut last: Option<i64> = None;
    for migration in migrator.iter() {
        if let Some(prev) = last {
            assert!(
                migration.version > prev,
                "migration versions must be strictly increasing: {prev} then {}",
                migration.version
            );
        }
        last = Some(migration.version);
    }
}

/// Apply migrations to a fresh database. Requires `DATABASE_URL`.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn apply_migrations_from_empty() {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/enterprise_platform".to_string()
    });

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("failed to connect to database");

    let migrator = sqlx::migrate!("../../migrations");
    migrator
        .run(&pool)
        .await
        .expect("failed to apply migrations");

    // The migration bookkeeping table must exist after running.
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .expect("failed to read _sqlx_migrations");
    assert!(row.0 > 0, "expected applied migrations to be recorded");
}

/// Verify idempotency: running twice does not fail.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn migrations_are_idempotent() {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/enterprise_platform".to_string()
    });

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("failed to connect to database");

    let migrator = sqlx::migrate!("../../migrations");
    migrator.run(&pool).await.expect("first run failed");
    migrator
        .run(&pool)
        .await
        .expect("second run failed (not idempotent)");
}
