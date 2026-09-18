//! PLAN-0013 Stage 10 — full-chain E2E against the production
//! composition on local `SQLite` (runs everywhere; no server required).
//!
//! Chain exercised: OIDC ES256 token → middleware provisioning →
//! server-side bootstrap administrator → management API → role/binding
//! grant → governance 200 → membership suspension 403 → reactivation
//! 200 → binding revocation 403. Tokens stay cryptographically valid
//! throughout; only server state flips decisions (revocation before
//! expiry), and the cold start has no "first user becomes admin" path.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use axum::http::{Method, StatusCode};
use harness::{call, fresh_subject, request, run_grant_lifecycle, sqlite_app, token, ROOT};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn bootstrap_admin_grant_suspend_reactivate_revoke_chain() {
    let app = sqlite_app(Some(ROOT)).await;
    run_grant_lifecycle(&app, "sqlite").await;
}

#[tokio::test]
async fn no_bootstrap_means_no_admin_and_no_http_bootstrap_surface() {
    let app = sqlite_app(None).await;
    let nobody = fresh_subject("nobody");

    // Cold start without the bootstrap section: every subject can
    // authenticate but nobody can administer (fail closed, no auto-grant).
    let (status, _) = harness::get_as(&app, "/api/v1/admin/users", &nobody).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // There is no HTTP path to bootstrap: unknown admin routes are 404
    // and the membership request schema rejects extra fields.
    let bearer = token(&nobody, app.tenant, None);
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::POST,
            "/api/v1/admin/bootstrap",
            Some(&bearer),
            Some(json!({"tenant_id": app.tenant.to_string()})),
            Some("no-bootstrap"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(
        app.router.clone(),
        request(
            Method::POST,
            "/api/v1/admin/tenant-memberships",
            Some(&bearer),
            Some(json!({"user_id": Uuid::now_v7().to_string(), "bootstrap": true})),
            Some("no-bootstrap-2"),
        ),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::CREATED,
        "no bootstrap=true escape hatch"
    );
}

#[tokio::test]
async fn anonymous_and_wrong_audience_tokens_are_rejected() {
    let app = sqlite_app(Some(ROOT)).await;
    let (status, _) = call(
        app.router.clone(),
        request(Method::GET, "/api/v1/admin/audit-events", None, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no token, no entry");

    // A cryptographically valid token for another audience is rejected
    // by the validator before any authorization runs.
    let foreign = harness::token(ROOT, app.tenant, Some(json!({"aud": "some-other-app"})));
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::GET,
            "/api/v1/admin/audit-events",
            Some(&foreign),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "audience is bound");
}
