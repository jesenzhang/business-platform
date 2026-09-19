//! Runs the shared identity behavior contract against the `SQLite` adapter.
//!
//! The adapter writes unified audit rows, so the test database needs the
//! runtime-audit `audit_events` table. In production that table is created
//! by the chained document-processing `SQLite` catalog (see `apps/migration`
//! chain order); test suites stay context-pure by creating the table inline
//! with raw SQL — the same self-contained pattern the `document-processing-
//! sqlite` suites use for their prerequisites. The DDL mirrors that catalog
//! (`003_runtime_audit_integrity_repair.sql` plus the three chain columns
//! added by `004_runtime_governance_revision1.sql`).

use identity_authorization_contracts::{verify_identity_contract, IdentityContractPorts};
use identity_sqlite::{run_migrations, SqliteIdentityStore};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Executor;
use std::sync::Arc;

const AUDIT_EVENTS_DDL: &str = r"
CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT,
    trace_id TEXT,
    created_at TEXT NOT NULL,
    operation_id TEXT,
    actor_type TEXT NOT NULL DEFAULT 'user',
    actor_id TEXT,
    correlation_id TEXT,
    causation_id TEXT,
    reason TEXT,
    result TEXT NOT NULL DEFAULT 'succeeded',
    failure_code TEXT,
    before_hash TEXT,
    after_hash TEXT,
    changed_fields TEXT NOT NULL DEFAULT '[]',
    schema_version TEXT NOT NULL DEFAULT 'audit.v1',
    previous_hash TEXT,
    record_hash TEXT,
    occurred_at TEXT,
    stream_sequence INTEGER,
    recorded_at TEXT,
    chain_version INTEGER
);
";

async fn setup() -> sqlx::SqlitePool {
    // `max_connections(1)` keeps one connection, hence one shared
    // `sqlite::memory:` database across the pool.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap_or_else(|_| unreachable!("SQLite pool must connect"));
    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .unwrap_or_else(|_| unreachable!("pragma must apply"));
    pool.execute(AUDIT_EVENTS_DDL)
        .await
        .unwrap_or_else(|_| unreachable!("audit_events table must be created"));
    run_migrations(&pool)
        .await
        .unwrap_or_else(|_| unreachable!("identity catalog must apply"));
    pool
}

#[tokio::test]
async fn sqlite_identity_contract_suite_passes() {
    let pool = setup().await;
    let store = Arc::new(SqliteIdentityStore::new(pool));
    let ports = IdentityContractPorts {
        resolve: store.resolve_port(),
        command: store.command_port(),
        query: store.query_port(),
        ledger: store.ledger_port(),
    };
    let result = verify_identity_contract(&ports).await;
    assert!(
        result.is_ok(),
        "identity `SQLite` contract failed: {:?}",
        result.as_ref().err()
    );
}
