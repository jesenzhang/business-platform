//! Runs the shared identity behavior contract against the `PostgreSQL`
//! adapter (production authority).
//!
//! Ignored by default: it needs a reachable `PostgreSQL` (`DATABASE_URL`)
//! with the runtime migration catalog; CI runs it via `--include-ignored`
//! against a dedicated database. The suite salts its issuer, tenants, and
//! users per run, but the user-status idempotency fixture reuses fixed key
//! names in the global platform-scope slot, so repeat runs must target a
//! fresh (or identity-table-cleared) database.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use identity_authorization_contracts::{verify_identity_contract, IdentityContractPorts};
use identity_postgres::PostgresIdentityStore;

#[tokio::test]
#[ignore = "requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored)"]
async fn postgres_identity_contract_suite_passes() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running the PostgreSQL identity contract");
    let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
        unreachable!("PostgreSQL pool must connect")
    };
    assert!(runtime_migration::MIGRATOR.run(&pool).await.is_ok());

    let store = Arc::new(PostgresIdentityStore::new(pool));
    let ports = IdentityContractPorts {
        resolve: store.resolve_port(),
        command: store.command_port(),
        query: store.query_port(),
        ledger: store.ledger_port(),
    };
    let result = verify_identity_contract(&ports).await;
    assert!(
        result.is_ok(),
        "identity `PostgreSQL` contract failed: {result:?}"
    );
}
