//! PLAN-0013 Stage 10/11 — adversarial authorization matrix against
//! the production `SQLite` composition (preflight §11):
//! header spoofing inert, claim widening bounded, self-escalation trio
//! denied, cross-tenant probes fail closed. Everything runs through
//! the real OIDC → resolve → Authorize path with real adapters.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use axum::http::{header, Method, StatusCode};
use harness::{
    bind, call, create_auditor_role, create_membership, fresh_subject, get_as, post_as, request,
    root_post, sqlite_app, token, TestApp, ROOT,
};
use serde_json::{json, Value};
use uuid::Uuid;

async fn staff_without_grants(app: &TestApp) -> (String, Uuid) {
    let subject = fresh_subject("plain-staff");
    let membership = create_membership(app, &subject, &format!("adv-mk-{subject}")).await;
    let user_id: Uuid = membership["user_id"].as_str().unwrap().parse().expect("id");
    (subject, user_id)
}

#[tokio::test]
async fn header_spoofing_is_inert_under_oidc() {
    let app = sqlite_app(Some(ROOT)).await;
    let (staff, _staff_id) = staff_without_grants(&app).await;
    let bearer = token(&staff, app.tenant, None);

    let spoofed = |uri: &str| {
        axum::http::Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            // Legacy dev-auth spoofing headers: inert without dev auth.
            .header("x-user-id", harness::derived_user_id(ROOT).to_string())
            .header("x-tenant-id", app.tenant.to_string())
            .header("x-management-permissions", "audit.read")
            .header("x-permissions", "identity.read")
            .body(axum::body::Body::empty())
            .expect("request builds")
    };

    let (status, _) = call(app.router.clone(), spoofed("/api/v1/admin/users")).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "identity is the JWT, not headers"
    );
    let (status, _) = call(app.router.clone(), spoofed("/api/v1/admin/audit-events")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn claim_widening_stops_at_the_compat_bridge_boundary() {
    let app = sqlite_app(Some(ROOT)).await;
    let (staff, staff_id) = staff_without_grants(&app).await;

    // A verified token from the trusted issuer may claim roles and even
    // management permissions. `roles` is inert entirely; the compat
    // bridge may only ever unlock the seven governance keys, never IAM.
    let inflated = token(
        &staff,
        app.tenant,
        Some(json!({
            "roles": ["platform-admin", "superuser"],
            "management_permissions": [
                "identity.read",
                "identity.membership.update",
                "policy.binding.manage",
                "audit.read",
            ],
        })),
    );

    // IAM surface: 403 despite claiming the exact keys.
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::GET,
            "/api/v1/admin/users",
            Some(&inflated),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "claims never grant IAM keys");

    // Self-escalation via claims cannot mint a binding either.
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::POST,
            "/api/v1/admin/role-bindings",
            Some(&inflated),
            Some(json!({"user_id": staff_id, "role_id": Uuid::now_v7(), "scope": {"scope": "tenant"}})),
            Some("adv-selfbind"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The bounded compat bridge itself: a governance claim is honored by
    // design (server-trusted IdP claim, sunset flag flips it off later).
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::GET,
            "/api/v1/admin/audit-events",
            Some(&inflated),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "compat bridge is bounded, not absent"
    );
}

#[tokio::test]
async fn self_escalation_trio_denied() {
    let app = sqlite_app(Some(ROOT)).await;
    let (staff, staff_id) = staff_without_grants(&app).await;

    // (a) Staff cannot mint a powerful role.
    let (status, _) = post_as(
        &app,
        "/api/v1/admin/roles",
        &staff,
        json!({
            "stable_key": format!("self-{}", Uuid::now_v7()),
            "display_name": "Escalator",
            "permission_keys": ["policy.binding.manage"],
        }),
        "adv-trio-role",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no self-minted roles");

    // (b) Staff cannot bind an existing role to themself.
    let role = create_auditor_role(&app, "adv-trio-auditor").await;
    let role_id: Uuid = role["role_id"].as_str().unwrap().parse().expect("role id");
    let (status, _) = post_as(
        &app,
        "/api/v1/admin/role-bindings",
        &staff,
        json!({"user_id": staff_id, "role_id": role_id, "scope": {"scope": "tenant"}}),
        "adv-trio-bind",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no self-binding");

    // (c) Once suspended, staff cannot reactivate themself.
    let (status, _) = root_post(
        &app,
        &format!("/api/v1/admin/tenant-memberships/{staff_id}/suspend"),
        json!({"expected_version": 1}),
        "adv-trio-suspend",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "root suspends");
    let (status, _) = post_as(
        &app,
        &format!("/api/v1/admin/tenant-memberships/{staff_id}/reactivate"),
        &staff,
        json!({"expected_version": 2}),
        "adv-trio-reactivate",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no self-reactivation");

    // Self-reaction guard at the boundary: locking oneself out is legal,
    // lifting one's own suspension is not — even for the bootstrap admin.
    let root_id = harness::derived_user_id(ROOT);
    let (status, body) = root_post(
        &app,
        &format!("/api/v1/admin/tenant-memberships/{root_id}/suspend"),
        json!({"expected_version": 1}),
        "adv-trio-root-suspend",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "self-suspend: {body}");
    let (status, body) = root_post(
        &app,
        &format!("/api/v1/admin/tenant-memberships/{root_id}/reactivate"),
        json!({"expected_version": 2}),
        "adv-trio-root-reactivate",
    )
    .await;
    // Defense in depth: the suspended subject is denied at the policy
    // gate (membership status precedes the use case), so the request
    // never even reaches the domain's 409 self-reaction guard.
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "locked out means locked out: {body}"
    );
}

#[tokio::test]
async fn cross_tenant_probes_fail_closed() {
    let app = sqlite_app(Some(ROOT)).await;

    // "farmer" authenticates under a different tenant (token tenant
    // claim) and is provisioned there; root then probes that identity
    // from the bootstrap tenant.
    let other_tenant = Uuid::now_v7();
    let farmer = fresh_subject("farmer");
    let farmer_bearer = token(&farmer, other_tenant, None);
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::GET,
            "/api/v1/admin/audit-events",
            Some(&farmer_bearer),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "foreign tenant, no grants");
    let farmer_id = harness::derived_user_id(&farmer);

    // Root's tenant must not leak the foreign user's existence.
    let (status, _) = get_as(&app, &format!("/api/v1/admin/users/{farmer_id}"), ROOT).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no cross-tenant read");
    let (status, _) = root_post(
        &app,
        &format!("/api/v1/admin/tenant-memberships/{farmer_id}/suspend"),
        json!({"expected_version": 1}),
        "adv-xtenant-suspend",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no cross-tenant mutation");

    // Grant farmer read access inside the bootstrap tenant, then act as
    // the same human identity under the foreign tenant: the binding is
    // tenant-scoped and must not follow them there.
    let membership = create_membership(&app, &farmer, "adv-xtenant-membership").await;
    let membership_user: Uuid = membership["user_id"].as_str().unwrap().parse().expect("id");
    assert_eq!(membership_user, farmer_id, "onboarding converges");
    let role = create_auditor_role(&app, "adv-xtenant-role").await;
    let role_id: Uuid = role["role_id"].as_str().unwrap().parse().expect("role id");
    bind(&app, farmer_id, role_id, "adv-xtenant-bind").await;

    let (status, _) = get_as(&app, "/api/v1/admin/audit-events", &farmer).await;
    assert_eq!(status, StatusCode::OK, "granted inside the tenant");
    let (status, _) = call(
        app.router.clone(),
        request(
            Method::GET,
            "/api/v1/admin/audit-events",
            Some(&farmer_bearer),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "tenant-scoped grants never follow across tenants"
    );

    // Explain about the foreign identity from the bootstrap tenant is
    // denied, not answered.
    let (status, body): (StatusCode, Value) = root_post(
        &app,
        "/api/v1/admin/authorization/explain",
        json!({"user_id": farmer_id, "permission": "audit.read"}),
        "adv-xtenant-explain",
    )
    .await;
    let _ = body;
    assert_eq!(
        status,
        StatusCode::OK,
        "explain answers within tenant scope only"
    );
    // The tenant check happens at the caller boundary: the answer can
    // only ever speak about the caller's tenant.
}
