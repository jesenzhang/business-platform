//! PLAN-0013 Stage 10 — full-chain E2E against the production
//! composition on `PostgreSQL` (CI; requires `DATABASE_URL`).
//!
//! Runs the same lifecycle chain as the `SQLite` target —
//! OIDC ES256 → provisioning → server-side bootstrap admin → grant →
//! governance 200 → suspension 403 → reactivation → revocation — plus
//! the cold-start fail-closed check, through `composition::build_app`
//! with the `PostgreSQL` adapters and the embedded migration catalog.
//! Subjects and tenants are fresh per run, so repeated runs against a
//! shared database never collide.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use axum::http::StatusCode;
use harness::{fresh_subject, get_as, postgres_app, run_grant_lifecycle, ROOT};

#[tokio::test]
#[ignore = "requires running PostgreSQL (DATABASE_URL)"]
async fn bootstrap_admin_grant_suspend_reactivate_revoke_chain_postgres() {
    let app = postgres_app(Some(ROOT)).await;
    run_grant_lifecycle(&app, "pg").await;
}

#[tokio::test]
#[ignore = "requires running PostgreSQL (DATABASE_URL)"]
async fn no_bootstrap_leaves_the_api_adminless_postgres() {
    let app = postgres_app(None).await;
    let nobody = fresh_subject("nobody");
    let (status, _) = get_as(&app, "/api/v1/admin/users", &nobody).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cold start without bootstrap must not grant authority"
    );
}
