//! Runs the shared organization behavior contract against the `PostgreSQL`
//! adapter (production authority).
//!
//! Ignored by default: it needs a reachable `PostgreSQL` (`DATABASE_URL`)
//! with the runtime migration catalog; CI runs it via `--include-ignored`
//! against a dedicated database. The suite salts its tenants and users with
//! fresh `UUIDv7`s per run and its idempotency keys live in tenant-scoped
//! slots, so repeat runs against the same database never collide.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use identity_authorization_contracts::{verify_organization_contract, OrganizationContractPorts};
use organization_postgres::PostgresOrganizationStore;

#[tokio::test]
#[ignore = "requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored)"]
async fn postgres_organization_contract_suite_passes() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running the PostgreSQL organization contract");
    let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
        unreachable!("PostgreSQL pool must connect")
    };
    assert!(runtime_migration::MIGRATOR.run(&pool).await.is_ok());

    let store = Arc::new(PostgresOrganizationStore::new(pool));
    let ports = OrganizationContractPorts {
        command: store.command_port(),
        query: store.query_port(),
    };
    let result = verify_organization_contract(&ports).await;
    assert!(
        result.is_ok(),
        "organization `PostgreSQL` contract failed: {result:?}"
    );
}
