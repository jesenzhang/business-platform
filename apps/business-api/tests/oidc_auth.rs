//! OIDC/JWT authentication contract tests (PLAN-0012 M3, T3.1/T3.2).
//!
//! The tests spin up a local JWKS endpoint serving an in-test ES256 (P-256)
//! key, build the real router with dev auth disabled, and exercise the full
//! fail-closed matrix: valid token, expired token, wrong audience, wrong
//! issuer, tampered signature, unknown `kid`, missing/nil tenant claim, JWKS
//! outage, `alg=none`, and the verified-claim → principal mapping.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use base64::Engine;
use business_api::auth::{AuthMiddlewareConfig, AuthenticationType, ManagementPermission};
use business_api::config::ServerConfig;
use business_api::oidc::OidcValidator;
use business_api::routes::create_router;
use business_api::state::{
    AppState, DocumentServices, ReadinessProbe, ReadinessReport, ReadinessStatus,
};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use document::query::{
    DocumentDetailQuery, DocumentListFilter, DocumentListPage, DocumentListQuery,
    DocumentListRequest, QueryError,
};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePrivateKey;
use serde_json::{json, Value};
use tower::ServiceExt;

/// Fixed test-only signing key. It never signs anything outside these tests
/// and there is deliberately no randomness so failures are reproducible.
const TEST_SIGNING_KEY_BYTES: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0x94, 0x37, 0x3a, 0xff, 0x8c, 0xa3,
];
const KID: &str = "test-key-1";
const TENANT_ID: &str = "11111111-2222-3333-4444-555555555555";
const USER_ID: &str = "66666666-7777-8888-9999-000000000001";
const SUB: &str = "oidc-user-1";
const AUDIENCE: &str = "business-api";
const ISSUER: &str = "https://identity.example.test/realms/test";

struct EmptyPorts;

#[async_trait]
impl CreateDocumentUnitOfWork for EmptyPorts {
    async fn execute(
        &self,
        _command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError> {
        Err(ApplicationPortError::Unavailable)
    }
}

#[async_trait]
impl DocumentDetailQuery for EmptyPorts {
    async fn execute(
        &self,
        _tenant_id: uuid::Uuid,
        _document_id: uuid::Uuid,
    ) -> Result<Option<document::query::DocumentDetailView>, QueryError> {
        Ok(None)
    }
}

#[async_trait]
impl DocumentListQuery for EmptyPorts {
    async fn execute(&self, _request: DocumentListRequest) -> Result<DocumentListPage, QueryError> {
        Ok(DocumentListPage {
            items: Vec::new(),
            next_cursor: None,
        })
    }

    async fn count(
        &self,
        _tenant_id: uuid::Uuid,
        _filter: DocumentListFilter,
    ) -> Result<u64, QueryError> {
        Ok(0)
    }
}

#[async_trait]
impl ReadinessProbe for EmptyPorts {
    async fn check(&self) -> ReadinessReport {
        ReadinessReport {
            status: ReadinessStatus::NotReady,
            database: "unavailable",
            migrations: "unknown",
        }
    }
}

struct JwksServer {
    jwks_url: String,
    /// Flip to false to simulate a JWKS outage.
    healthy: Arc<AtomicBool>,
}

impl JwksServer {
    async fn spawn() -> Self {
        let jwk = test_jwk();
        let healthy = Arc::new(AtomicBool::new(true));
        let healthy_for_handler = Arc::clone(&healthy);
        let jwks = axum::routing::get(move || {
            let healthy = Arc::clone(&healthy_for_handler);
            let jwk = jwk.clone();
            async move {
                if healthy.load(Ordering::Relaxed) {
                    axum::Json(json!({ "keys": [jwk] })).into_response()
                } else {
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
            }
        });
        let app: axum::Router = axum::Router::new().route("/jwks.json", jwks);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test JWKS listener must bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test JWKS server must serve");
        });
        Self {
            jwks_url: format!("http://{addr}/jwks.json"),
            healthy,
        }
    }

    fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::Relaxed);
    }
}

/// Deterministic ES256 P-256 JWK for the fixed test signing key.
fn test_jwk() -> Value {
    let signing = signing_key();
    let verifying = p256::ecdsa::VerifyingKey::from(&signing);
    let encoded = verifying.to_encoded_point(false);
    let p256::elliptic_curve::sec1::Coordinates::Uncompressed { x, y } = encoded.coordinates()
    else {
        unreachable!("uncompressed point");
    };
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    json!({
        "kty": "EC",
        "crv": "P-256",
        "kid": KID,
        "use": "sig",
        "alg": "ES256",
        "x": engine.encode(x),
        "y": engine.encode(y),
    })
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes((&TEST_SIGNING_KEY_BYTES).into()).expect("fixed test key")
}

fn encoding_key() -> EncodingKey {
    EncodingKey::from_ec_der(signing_key().to_pkcs8_der().expect("pkcs8").as_bytes())
}

/// Sign a token with the test key, allowing a `kid` override.
fn sign_token(claims: &Value, kid: Option<&str>) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = kid.map(str::to_owned);
    jsonwebtoken::encode(&header, &claims, &encoding_key()).expect("token must encode")
}

fn base_claims() -> Value {
    json!({
        "sub": SUB,
        "iss": ISSUER,
        "aud": AUDIENCE,
        "exp": chrono_offset_now(600),
        "iat": chrono_offset_now(-5),
        "tenant_id": TENANT_ID,
        "user_id": USER_ID,
        "roles": ["operator"],
        "management_permissions": ["audit.read", "repair.approve", "not.a.real.permission"],
    })
}

fn chrono_offset_now(seconds: i64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs()
        .cast_signed();
    u64::try_from(now + seconds).expect("timestamp")
}

fn oidc_router(validator: Arc<OidcValidator>) -> axum::Router {
    let ports = Arc::new(EmptyPorts);
    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(document::application::CreateDocumentMetadata::new(
                ports.clone(),
            )),
            detail: ports.clone(),
            list: ports.clone(),
        },
        processing: None,
        governance: None,
        readiness: ports,
        storage: None,
    });
    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled: false,
        dev_secret: None,
        dev_permissions: BTreeSet::new(),
        dev_tenant_id: None,
        dev_user_id: None,
        dev_subject: None,
        dev_roles: BTreeSet::new(),
        oidc: Some(validator),
    };
    let server = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        request_timeout_secs: 30,
        cors_origins: vec![],
        body_limit_bytes: 1024,
    };
    create_router(state, auth_config, &server)
}

fn validator_for(jwks_url: &str) -> Arc<OidcValidator> {
    Arc::new(OidcValidator::new(
        ISSUER.to_string(),
        Some(AUDIENCE.to_string()),
        Some(jwks_url.to_string()),
    ))
}

fn authorized_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request must build")
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request must build")
}

async fn status_of(router: axum::Router, request: Request<Body>) -> StatusCode {
    router
        .oneshot(request)
        .await
        .expect("router must respond")
        .status()
}

#[tokio::test]
async fn valid_oidc_token_passes_authentication() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    // No route is registered under /api/v1 in this test state, so passing
    // auth yields 404 (not 401), proving the middleware accepted the token.
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&base_claims(), Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let mut claims = base_claims();
    claims["exp"] = json!(chrono_offset_now(-120));
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&claims, Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_audience_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let mut claims = base_claims();
    claims["aud"] = json!("other-service");
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&claims, Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_issuer_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let mut claims = base_claims();
    claims["iss"] = json!("https://evil.example.test/realms/test");
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&claims, Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tampered_signature_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let token = sign_token(&base_claims(), Some(KID));
    let (header, rest) = token.split_once('.').expect("token shape");
    let (payload, signature) = rest.split_once('.').expect("token shape");
    let mut tampered = signature.to_string();
    let first = tampered.remove(0);
    tampered.insert(0, if first == 'A' { 'B' } else { 'A' });
    let forged = format!("{header}.{payload}.{tampered}");
    let status = status_of(router, authorized_get("/api/v1/anything", &forged)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_kid_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let status = status_of(
        router,
        authorized_get(
            "/api/v1/anything",
            &sign_token(&base_claims(), Some("rotated-away-key")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_tenant_claim_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let mut claims = base_claims();
    claims
        .as_object_mut()
        .expect("claims object")
        .remove("tenant_id");
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&claims, Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn nil_tenant_claim_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let mut claims = base_claims();
    claims["tenant_id"] = json!("00000000-0000-0000-0000-000000000000");
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&claims, Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn jwks_outage_fails_closed() {
    let server = JwksServer::spawn().await;
    server.set_healthy(false);
    let router = oidc_router(validator_for(&server.jwks_url));
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", &sign_token(&base_claims(), Some(KID))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_bearer_token_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    let status = status_of(router, get("/api/v1/anything")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn algorithm_none_is_rejected() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    // An unsigned token cannot be produced by jsonwebtoken; forge one
    // directly: header with "alg":"none" and an empty signature.
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = engine.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = engine.encode(base_claims().to_string());
    let forged = format!("{header}.{payload}.");
    let status = status_of(router, authorized_get("/api/v1/anything", &forged)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn metrics_endpoint_is_public_and_counts_requests() {
    let server = JwksServer::spawn().await;
    let router = oidc_router(validator_for(&server.jwks_url));
    business_api::metrics::install_metrics();
    // One rejected request goes through the metrics layer.
    let status = status_of(router.clone(), get("/api/v1/anything")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let response = router
        .oneshot(get("/metrics"))
        .await
        .expect("metrics must respond");
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1_048_576)
        .await
        .expect("metrics body");
    let text = String::from_utf8(body.to_vec()).expect("utf8 metrics body");
    assert!(text.contains("http_requests_total"), "got: {text}");
    assert!(text.contains("auth_failures_total"), "got: {text}");
}

#[tokio::test]
async fn claim_mapping_populates_tenant_user_and_permissions() {
    let server = JwksServer::spawn().await;
    let validator = validator_for(&server.jwks_url);
    let principal = validator
        .validate(&sign_token(&base_claims(), Some(KID)))
        .await
        .expect("valid token must produce a principal");
    assert_eq!(principal.tenant_id().to_string(), TENANT_ID);
    assert_eq!(principal.user_id().to_string(), USER_ID);
    assert_eq!(principal.subject(), SUB);
    assert!(principal.has_management_permission(ManagementPermission::AuditRead));
    assert!(principal.has_management_permission(ManagementPermission::RepairApprove));
    // Unknown permission strings are not granted.
    assert!(!principal.has_permission("not.a.real.permission"));
    assert!(principal.roles().contains("operator"));
    // The dev-token path must not be reported.
    assert_eq!(principal.authentication_type(), AuthenticationType::Oidc);
}

#[tokio::test]
async fn user_id_falls_back_to_sub_when_uuid() {
    let server = JwksServer::spawn().await;
    let validator = validator_for(&server.jwks_url);
    let mut claims = base_claims();
    claims["sub"] = json!(USER_ID);
    claims
        .as_object_mut()
        .expect("claims object")
        .remove("user_id");
    let principal = validator
        .validate(&sign_token(&claims, Some(KID)))
        .await
        .expect("valid token must produce a principal");
    assert_eq!(principal.user_id().to_string(), USER_ID);
}
