//! Shared end-to-end harness for the PLAN-0013 identity/authorization
//! chains (Stages 10/11). It composes the API through the exact
//! production entry point (`business_api::composition::build_app`) —
//! real `SQLite` or `PostgreSQL` adapters, real use cases, real OIDC
//! validation — and signs ES256 tokens against an in-test JWKS endpoint.
//!
//! Deterministic identity model: every test subject has no `user_id`
//! claim of its own; the harness sends the platform's deterministic
//! `UUIDv5` derivation of `(issuer, subject)` as the claim, which is what
//! the identity context provisions on first authentication (the same
//! derivation the bootstrap and membership-by-subject paths use), so
//! tokens and provisioning always converge.

#![allow(dead_code)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::{header, HeaderValue, Method, Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use business_api::composition::build_app;
use business_api::config::{
    AuthConfig, BootstrapAdminConfig, BusinessApiConfig, DatabaseBackend, DatabaseConfig,
    ObservabilityConfig, ServerConfig, StorageBackend, StorageConfig,
};
use http_body_util::BodyExt;
use identity::application::EXTERNAL_IDENTITY_NAMESPACE;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePrivateKey;
use runtime_config::{RuntimeEnvironment, SecretUrl};
use serde_json::{json, Value};
use sqlx::migrate::MigrateDatabase;
use tower::ServiceExt;
use uuid::Uuid;

/// OIDC issuer used by every harness token. The bootstrap issuer equals
/// it, so the bootstrap principal is a normal OIDC subject.
pub const ISSUER: &str = "https://identity.example.test/realms/e2e";
pub const AUDIENCE: &str = "business-api";
const KID: &str = "e2e-key-1";

/// Fixed ES256 P-256 signing key (test-only, deterministic).
const TEST_SIGNING_KEY_BYTES: [u8; 32] = [
    0x21, 0x7c, 0x9a, 0x3f, 0x55, 0x8e, 0x14, 0xc6, 0x0d, 0x92, 0x7b, 0xa8, 0x46, 0xe1, 0x33, 0xf0,
    0x68, 0x15, 0x9d, 0xc2, 0x7a, 0x0e, 0x51, 0xb9, 0x36, 0x44, 0x8f, 0xad, 0x20, 0x6b, 0x75, 0x1c,
];

/// Platform user id the identity context derives for `(issuer, subject)`:
/// `UUIDv5` over the canonical `issuer \u{1f} subject` key. Tokens must claim
/// exactly this id or resolution fails closed with a principal conflict.
pub fn derived_user_id(subject: &str) -> Uuid {
    Uuid::new_v5(
        &EXTERNAL_IDENTITY_NAMESPACE,
        format!("{ISSUER}\u{1f}{subject}").as_bytes(),
    )
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes((&TEST_SIGNING_KEY_BYTES).into()).expect("fixed test key")
}

fn encoding_key() -> EncodingKey {
    EncodingKey::from_ec_der(signing_key().to_pkcs8_der().expect("pkcs8").as_bytes())
}

fn test_jwk() -> Value {
    let verifying = p256::ecdsa::VerifyingKey::from(&signing_key());
    let encoded = verifying.to_encoded_point(false);
    let p256::elliptic_curve::sec1::Coordinates::Uncompressed { x, y } = encoded.coordinates()
    else {
        unreachable!("uncompressed point");
    };
    json!({
        "kty": "EC", "crv": "P-256", "kid": KID, "use": "sig", "alg": "ES256",
        "x": URL_SAFE_NO_PAD.encode(x),
        "y": URL_SAFE_NO_PAD.encode(y),
    })
}

struct JwksServer {
    jwks_url: String,
}

impl JwksServer {
    async fn spawn() -> Self {
        let jwk = test_jwk();
        let jwks = axum::routing::get(move || {
            let jwk = jwk.clone();
            async move { axum::Json(json!({ "keys": [jwk] })) }
        });
        let app: axum::Router = axum::Router::new().route("/jwks.json", jwks);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test JWKS listener must bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("JWKS server");
        });
        Self {
            jwks_url: format!("http://{addr}/jwks.json"),
        }
    }
}

fn now_offset(seconds: i64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs()
        .checked_add_signed(seconds)
        .expect("clock offset")
}

/// Sign a token for `subject` in `tenant`. `extra` merges additional
/// claims (e.g. inert `roles` or `management_permissions`).
pub fn token(subject: &str, tenant: Uuid, extra: Option<Value>) -> String {
    let mut claims = json!({
        "sub": subject,
        "iss": ISSUER,
        "aud": AUDIENCE,
        "exp": now_offset(600),
        "iat": now_offset(-5),
        "tenant_id": tenant.to_string(),
        "user_id": derived_user_id(subject).to_string(),
    });
    if let Some(extra) = extra {
        if let (Some(base), Some(extra)) = (claims.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                base.insert(key.clone(), value.clone());
            }
        }
    }
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(KID.to_string());
    jsonwebtoken::encode(&header, &claims, &encoding_key()).expect("token must encode")
}

/// A fully composed application under test plus the live JWKS server.
pub struct TestApp {
    pub router: axum::Router,
    pub tenant: Uuid,
    _jwks: JwksServer,
    _temp: TempDir,
}

/// Minimal unique scratch directory (removed on drop best effort).
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("bp-plan0013-{label}-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn base_config(env: RuntimeEnvironment, database: DatabaseConfig) -> BusinessApiConfig {
    BusinessApiConfig {
        env,
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3999,
            request_timeout_secs: 30,
            cors_origins: Vec::new(),
            body_limit_bytes: 1024 * 1024,
        },
        database,
        storage: StorageConfig::default(),
        auth: AuthConfig {
            issuer_url: ISSUER.to_string(),
            audience: Some(AUDIENCE.to_string()),
            jwks_url: None,
            dev_secret: None,
            dev_auth_enabled: false,
            dev_permissions: BTreeSet::new(),
            dev_tenant_id: None,
            dev_user_id: None,
            dev_subject: None,
            dev_roles: BTreeSet::new(),
            management_permission_compat_enabled: true,
            bootstrap: BootstrapAdminConfig::default(),
        },
        observability: ObservabilityConfig::default(),
    }
}

fn with_bootstrap(config: &mut BusinessApiConfig, tenant: Uuid, admin_subject: &str) {
    config.auth.bootstrap = BootstrapAdminConfig {
        enabled: true,
        tenant_id: Some(tenant),
        issuer: Some(ISSUER.to_string()),
        subject: Some(admin_subject.to_string()),
        version: 1,
    };
}

/// `SQLite` E2E target: fresh file-backed database in a temp directory,
/// local object storage, OIDC-only authentication, optional bootstrap
/// administrator subject. Runs everywhere CI runs (no server needed).
pub async fn sqlite_app(bootstrap_admin_subject: Option<&str>) -> TestApp {
    let temp = TempDir::new("sqlite");
    let db_path = temp.path().join("api.db");
    let storage_path = temp.path().join("storage");
    let database_url = format!("sqlite://{}", db_path.to_string_lossy().replace('\\', "/"));

    // The API's SQLite branch expects `apps/migration` to have prepared
    // the database (document-scope migrations); the other catalogs are
    // applied inside `build_app`. Mirror the migration tool here.
    sqlx::Sqlite::create_database(&database_url)
        .await
        .expect("create database");
    let migration_pool = document_sqlite::connect(&database_url, 1)
        .await
        .expect("migration pool");
    document_sqlite::MIGRATOR
        .run(&migration_pool)
        .await
        .expect("document migrations");
    migration_pool.close().await;

    let jwks = JwksServer::spawn().await;
    let tenant = Uuid::now_v7();
    let mut config = base_config(
        RuntimeEnvironment::Development,
        DatabaseConfig {
            backend: DatabaseBackend::Sqlite,
            url: SecretUrl::parse(&database_url).expect("sqlite url"),
            max_connections: 4,
            min_connections: 1,
            acquire_timeout_secs: 10,
        },
    );
    config.storage = StorageConfig {
        backend: StorageBackend::Local,
        local_path: storage_path.to_string_lossy().replace('\\', "/"),
        ..StorageConfig::default()
    };
    config.auth.jwks_url = Some(jwks.jwks_url.clone());
    if let Some(subject) = bootstrap_admin_subject {
        with_bootstrap(&mut config, tenant, subject);
    }
    let router = build_app(&config).await.expect("SQLite composition");
    TestApp {
        router,
        tenant,
        _jwks: jwks,
        _temp: temp,
    }
}

/// `PostgreSQL` E2E target: same composition against `DATABASE_URL`,
/// applying the embedded migration catalog at startup (production path).
pub async fn postgres_app(bootstrap_admin_subject: Option<&str>) -> TestApp {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let storage = TempDir::new("pg-storage");
    let jwks = JwksServer::spawn().await;
    let tenant = Uuid::now_v7();
    let mut config = base_config(
        RuntimeEnvironment::Development,
        DatabaseConfig {
            backend: DatabaseBackend::Postgres,
            url: SecretUrl::parse(&url).expect("postgres url"),
            max_connections: 10,
            min_connections: 0,
            acquire_timeout_secs: 10,
        },
    );
    config.storage = StorageConfig {
        backend: StorageBackend::Local,
        local_path: storage.path().to_string_lossy().replace('\\', "/"),
        ..StorageConfig::default()
    };
    config.auth.jwks_url = Some(jwks.jwks_url.clone());
    if let Some(subject) = bootstrap_admin_subject {
        with_bootstrap(&mut config, tenant, subject);
    }
    let router = build_app(&config).await.expect("PostgreSQL composition");
    TestApp {
        router,
        tenant,
        _jwks: jwks,
        _temp: storage,
    }
}

// ---------------------------------------------------------------------------
// Request plumbing
// ---------------------------------------------------------------------------

fn bearer(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("Bearer {token}")).expect("valid header")
}

pub fn request(
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
    idempotency_key: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, bearer(token));
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("idempotency-key", key);
    }
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .expect("request must build")
}

/// Send a request and parse the JSON envelope body.
pub async fn call(router: axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("router must answer");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body must collect")
        .to_bytes();
    if bytes.is_empty() {
        return (status, Value::Null);
    }
    // Framework-level responses (e.g. axum's 404 fallback) may carry a
    // non-JSON body; the tests only assert on their status codes.
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, value)
}

pub fn data(body: &Value) -> &Value {
    &body["data"]
}

pub async fn get_as(app: &TestApp, uri: &str, subject: &str) -> (StatusCode, Value) {
    let token = token(subject, app.tenant, None);
    call(
        app.router.clone(),
        request(Method::GET, uri, Some(&token), None, None),
    )
    .await
}

pub async fn post_as(
    app: &TestApp,
    uri: &str,
    subject: &str,
    body: Value,
    idempotency_key: &str,
) -> (StatusCode, Value) {
    let token = token(subject, app.tenant, None);
    call(
        app.router.clone(),
        request(
            Method::POST,
            uri,
            Some(&token),
            Some(body),
            Some(idempotency_key),
        ),
    )
    .await
}

// ---------------------------------------------------------------------------
// The shared PLAN-0013 lifecycle chain (SQLite locally, PostgreSQL in CI).
// ---------------------------------------------------------------------------

/// Bootstrap administrator subject for chain runs.
pub const ROOT: &str = "root-admin";

/// Fresh unique subject per invocation keeps repeated runs against the
/// same database isolated (ledger, bindings, audit all key off tenant
/// and subject).
pub fn fresh_subject(label: &str) -> String {
    format!("{label}-{}", Uuid::now_v7())
}

pub async fn create_membership(app: &TestApp, subject: &str, key: &str) -> Value {
    let (status, body) = post_as(
        app,
        "/api/v1/admin/tenant-memberships",
        ROOT,
        json!({"issuer": ISSUER, "subject": subject}),
        key,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "membership create: {body}");
    data(&body).clone()
}

pub async fn create_auditor_role(app: &TestApp, key: &str) -> Value {
    let (status, body) = post_as(
        app,
        "/api/v1/admin/roles",
        ROOT,
        json!({
            "stable_key": format!("auditors-{}", Uuid::now_v7()),
            "display_name": "Auditors",
            "permission_keys": ["audit.read"],
        }),
        key,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "role create: {body}");
    data(&body).clone()
}

pub async fn bind(app: &TestApp, user_id: Uuid, role_id: Uuid, key: &str) -> Value {
    let (status, body) = post_as(
        app,
        "/api/v1/admin/role-bindings",
        ROOT,
        json!({
            "user_id": user_id,
            "role_id": role_id,
            "scope": { "scope": "tenant" },
        }),
        key,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "binding create: {body}");
    data(&body).clone()
}

pub async fn root_post(app: &TestApp, uri: &str, body: Value, key: &str) -> (StatusCode, Value) {
    post_as(app, uri, ROOT, body, key).await
}

/// The full lifecycle chain against a `TestApp` bootstrapped with
/// `ROOT`: bootstrap admin reach → no-first-user promotion → membership
/// provisioning by subject (with idempotent replay and key-conflict) →
/// deny without grants → role+binding grant → explain → suspend (valid
/// JWT denied) → stale version 409 → reactivate → revoke (immediate).
#[allow(clippy::too_many_lines)]
pub async fn run_grant_lifecycle(app: &TestApp, chain: &str) {
    // 1. Bootstrap ran at startup: the configured subject administers
    //    through its IAM binding, not through any compat claim.
    let (status, _) = get_as(app, "/api/v1/admin/audit-events", ROOT).await;
    assert_eq!(status, StatusCode::OK, "bootstrap admin must reach audit");

    // A different subject that merely authenticated has zero authority:
    // the cold start never promotes the first user.
    let stranger = fresh_subject("stranger");
    let (status, _) = get_as(app, "/api/v1/admin/audit-events", &stranger).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no first-user-is-admin");

    // 2. Membership via issuer+subject provisions and attaches staff.
    let staff_subject = fresh_subject("staff");
    let membership = create_membership(app, &staff_subject, &format!("{chain}-mk-create")).await;
    let staff_id: Uuid = membership["user_id"]
        .as_str()
        .unwrap()
        .parse()
        .expect("user id");
    assert_eq!(membership["status"], "active");
    assert_eq!(membership["version"], 1);

    // Replay of the same idempotency key converges to the same row.
    let (status, replay) = post_as(
        app,
        "/api/v1/admin/tenant-memberships",
        ROOT,
        json!({"issuer": ISSUER, "subject": &staff_subject}),
        &format!("{chain}-mk-create"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "replay is the 200 idempotent path");
    assert_eq!(data(&replay)["user_id"], membership["user_id"]);

    // Same key, different content fails closed.
    let (status, _) = post_as(
        app,
        "/api/v1/admin/tenant-memberships",
        ROOT,
        json!({"issuer": ISSUER, "subject": fresh_subject("other")}),
        &format!("{chain}-mk-create"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "key reuse with new content");

    // 3. Staff without grants is denied even though authenticated.
    let (status, _) = get_as(app, "/api/v1/admin/audit-events", &staff_subject).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 4. Role + tenant-scope binding grants governance access.
    let role = create_auditor_role(app, &format!("{chain}-mk-role")).await;
    let role_id: Uuid = role["role_id"].as_str().unwrap().parse().expect("role id");
    assert!(role["created_at"].is_string(), "role view keeps timestamps");
    let binding = bind(app, staff_id, role_id, &format!("{chain}-mk-bind")).await;
    assert!(
        binding["created_at"].is_string() && binding["updated_at"].is_string(),
        "binding view keeps audit timestamps: {binding}"
    );

    let (status, _) = get_as(app, "/api/v1/admin/audit-events", &staff_subject).await;
    assert_eq!(status, StatusCode::OK, "binding grants access");

    // Explain agrees with the live decision for the staff user.
    let (status, explained) = root_post(
        app,
        "/api/v1/admin/authorization/explain",
        json!({"user_id": staff_id, "permission": "audit.read"}),
        &format!("{chain}-mk-explain"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "explain: {explained}");
    assert_eq!(data(&explained)["allowed"], json!(true));

    // 5. Membership suspension flips the decision immediately: the JWT
    //    itself is unchanged and unexpired, only server state moved.
    let (status, body) = root_post(
        app,
        &format!("/api/v1/admin/tenant-memberships/{staff_id}/suspend"),
        json!({"expected_version": 1}),
        &format!("{chain}-mk-suspend"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "suspend: {body}");
    let (status, _) = get_as(app, "/api/v1/admin/audit-events", &staff_subject).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "suspension beats a valid JWT"
    );

    // 6. Stale versions fail closed; reactivation restores access.
    let (status, body) = root_post(
        app,
        &format!("/api/v1/admin/tenant-memberships/{staff_id}/suspend"),
        json!({"expected_version": 1}),
        &format!("{chain}-mk-suspend-stale"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "stale expected_version: {body}"
    );
    let (status, body) = root_post(
        app,
        &format!("/api/v1/admin/tenant-memberships/{staff_id}/reactivate"),
        json!({"expected_version": 2}),
        &format!("{chain}-mk-reactivate"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reactivate: {body}");
    let (status, _) = get_as(app, "/api/v1/admin/audit-events", &staff_subject).await;
    assert_eq!(status, StatusCode::OK, "reactivation restores access");

    // 7. Binding revocation is final for the next decision.
    let binding_id = binding["binding_id"].as_str().expect("binding id");
    let (status, body) = root_post(
        app,
        &format!("/api/v1/admin/role-bindings/{binding_id}/revoke"),
        json!({"expected_version": 1}),
        &format!("{chain}-mk-revoke"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "revoke: {body}");
    let (status, _) = get_as(app, "/api/v1/admin/audit-events", &staff_subject).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "revocation flips immediately"
    );
}
