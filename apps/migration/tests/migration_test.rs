//! Migration integration tests.
//!
//! The compile-time test runs without a database; the runtime tests require a
//! reachable `PostgreSQL` instance and are gated behind `#[ignore]`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use sqlx::migrate::MigrateDatabase;
use uuid::Uuid;

fn isolated_database_url() -> String {
    let base = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    let (server, _) = base
        .rsplit_once('/')
        .expect("DATABASE_URL database segment");
    format!("{server}/plan2_{}", Uuid::now_v7().simple())
}

async fn create_isolated_database() -> (String, sqlx::PgPool) {
    let url = isolated_database_url();
    sqlx::Postgres::create_database(&url)
        .await
        .expect("create isolated database");
    let pool = sqlx::PgPool::connect(&url)
        .await
        .expect("connect isolated database");
    (url, pool)
}

async fn create_legacy_document_schema(pool: &sqlx::PgPool) {
    sqlx::raw_sql(
        r"
        CREATE TABLE documents (
            id UUID PRIMARY KEY,
            tenant_id UUID NOT NULL,
            original_filename VARCHAR(500) NOT NULL,
            content_type VARCHAR(200) NOT NULL,
            object_key VARCHAR(1024) NOT NULL,
            status VARCHAR(50) NOT NULL DEFAULT 'active',
            version BIGINT NOT NULL DEFAULT 1,
            size_bytes BIGINT,
            created_by UUID NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE document_idempotency (
            tenant_id UUID NOT NULL,
            idempotency_key VARCHAR(255) NOT NULL,
            request_fingerprint CHAR(64) NOT NULL,
            document_id UUID NOT NULL REFERENCES documents(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (tenant_id, idempotency_key)
        );
        ",
    )
    .execute(pool)
    .await
    .expect("create legacy schema");
}

/// Verify migrations compile and can be listed without a database.
///
/// The path is relative to this crate's manifest directory (`apps/migration`).
#[test]
fn migrations_are_valid() {
    let count = runtime_migration::MIGRATOR.iter().count();
    assert!(count > 0, "expected at least one migration, got {count}");

    // Versions must be unique and strictly increasing in iteration order.
    let mut last: Option<i64> = None;
    for migration in runtime_migration::MIGRATOR.iter() {
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

#[test]
fn outbox_reconciliation_migration_is_forward_only_and_fenced() {
    let migration = include_str!("../../../migrations/004_outbox_state_reconciliation.sql");
    assert!(migration.contains("status = 'published'"));
    assert!(migration.contains("claim_token"));
    assert!(migration.contains("outbox_status_check"));
    assert!(migration.contains("published = (status = 'published')"));
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

    runtime_migration::MIGRATOR
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

    runtime_migration::MIGRATOR
        .run(&pool)
        .await
        .expect("first run failed");
    runtime_migration::MIGRATOR
        .run(&pool)
        .await
        .expect("second run failed (not idempotent)");
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn document_integrity_upgrade_preserves_legal_legacy_rows_and_enforces_constraints() {
    let (url, pool) = create_isolated_database().await;
    create_legacy_document_schema(&pool).await;
    let document_id = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, size_bytes, created_by) VALUES ($1, $2, 'legacy.pdf', 'application/pdf', 'legacy.pdf', 'active', 1, NULL, $3)",
    )
    .bind(document_id)
    .bind(tenant_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert legal legacy row");

    sqlx::raw_sql(include_str!(
        "../../../migrations/007_document_integrity_constraints.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply integrity migration");

    let row: (String, Option<i64>) =
        sqlx::query_as("SELECT original_filename, size_bytes FROM documents WHERE id = $1")
            .bind(document_id)
            .fetch_one(&pool)
            .await
            .expect("read preserved row");
    assert_eq!(row, ("legacy.pdf".to_string(), None));
    assert!(sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, size_bytes, created_by) VALUES ($1, $2, 'bad.pdf', 'application/pdf', 'bad.pdf', 'active', 1, -1, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .is_err());

    pool.close().await;
    sqlx::Postgres::drop_database(&url)
        .await
        .expect("drop isolated database");
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn document_integrity_upgrade_fails_closed_for_invalid_legacy_rows() {
    let (url, pool) = create_isolated_database().await;
    create_legacy_document_schema(&pool).await;
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, size_bytes, created_by) VALUES ($1, $2, 'bad.pdf', 'application/pdf', 'bad.pdf', 'active', 1, -1, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await
    .expect("insert invalid legacy row");

    let result = sqlx::raw_sql(include_str!(
        "../../../migrations/007_document_integrity_constraints.sql"
    ))
    .execute(&pool)
    .await;
    assert!(result.is_err());

    pool.close().await;
    sqlx::Postgres::drop_database(&url)
        .await
        .expect("drop isolated database");
}
