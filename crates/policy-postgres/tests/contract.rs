//! Runs the shared policy behavior contract against the `PostgreSQL`
//! adapter (production authority).
//!
//! Ignored by default: it needs a reachable `PostgreSQL` (`DATABASE_URL`)
//! with the runtime migration catalog (`019_identity_authorization_
//! foundation.sql` creates the policy tables and seeds); CI runs it via
//! `--include-ignored` against a dedicated database. The suite salts its
//! tenants, roles, and users per run and its idempotency keys live in
//! tenant-scoped slots, so repeat runs are safe on the same database.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use identity_authorization_contracts::{verify_policy_contract, PolicyContractPorts};
use policy_postgres::PostgresPolicyStore;

#[tokio::test]
#[ignore = "requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored)"]
async fn postgres_policy_contract_suite_passes() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running the PostgreSQL policy contract");
    let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
        unreachable!("PostgreSQL pool must connect")
    };
    assert!(runtime_migration::MIGRATOR.run(&pool).await.is_ok());

    let store = Arc::new(PostgresPolicyStore::new(pool));
    let ports = PolicyContractPorts {
        command: store.command_port(),
        query: store.query_port(),
    };
    let result = verify_policy_contract(&ports).await;
    assert!(
        result.is_ok(),
        "policy `PostgreSQL` contract failed: {result:?}"
    );
}
