//! HTTP security baseline tests (WP-04).
//!
//! These exercise the composed router directly via `tower::ServiceExt::oneshot`,
//! without binding a socket or requiring a live database (a lazy pool is used so
//! the readiness probe degrades gracefully instead of failing the test setup).

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, StatusCode};
use business_api::auth::{AuthMiddlewareConfig, ManagementPermission};
use business_api::config::{
    AuthConfig, BusinessApiConfig, DatabaseBackend, DatabaseConfig, ObservabilityConfig,
    ServerConfig,
};
use business_api::routes::create_router;
use business_api::state::{
    AppState, DocumentServices, ReadinessProbe, ReadinessReport, ReadinessStatus,
};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use document::query::{
    DocumentDetailQuery, DocumentDetailView, DocumentListPage, DocumentListQuery,
    DocumentListRequest, QueryError,
};
use runtime_config::{RuntimeEnvironment, Secret, SecretUrl};
use tower::ServiceExt;

const DEV_SECRET: &str = "test-dev-secret";
const DEV_TENANT_ID: uuid::Uuid = uuid::Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001);
const DEV_USER_ID: uuid::Uuid = uuid::Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0002);

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
    ) -> Result<Option<DocumentDetailView>, QueryError> {
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

fn test_config(dev_auth_enabled: bool, cors_origins: Vec<String>) -> BusinessApiConfig {
    BusinessApiConfig {
        env: RuntimeEnvironment::Development,
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            request_timeout_secs: 30,
            cors_origins,
            body_limit_bytes: 1024,
        },
        database: DatabaseConfig {
            backend: DatabaseBackend::Postgres,
            url: SecretUrl::parse("postgres://user:pass@localhost:5432/db")
                .expect("test URL should parse"),
            max_connections: 1,
            min_connections: 0,
            acquire_timeout_secs: 1,
        },
        observability: ObservabilityConfig {
            service_name: "business-api-test".to_string(),
            otlp_endpoint: None,
            log_level: "off".to_string(),
        },
        auth: AuthConfig {
            issuer_url: String::new(),
            audience: None,
            dev_secret: Some(Secret::new(DEV_SECRET.to_string())),
            dev_auth_enabled,
            dev_permissions: BTreeSet::new(),
            dev_tenant_id: Some(DEV_TENANT_ID),
            dev_user_id: Some(DEV_USER_ID),
            dev_subject: Some("security-test-user".to_string()),
            dev_roles: BTreeSet::new(),
        },
    }
}

fn test_router_with_permissions(
    dev_auth_enabled: bool,
    dev_permissions: BTreeSet<ManagementPermission>,
) -> axum::Router {
    let config = test_config(dev_auth_enabled, vec!["*".to_string()]);
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
    });
    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled,
        dev_secret: Some(DEV_SECRET.to_string()),
        dev_permissions,
        dev_tenant_id: Some(DEV_TENANT_ID),
        dev_user_id: Some(DEV_USER_ID),
        dev_subject: Some("security-test-user".to_string()),
        dev_roles: BTreeSet::new(),
    };
    create_router(state, auth_config, &config.server)
}

fn test_router(dev_auth_enabled: bool) -> axum::Router {
    test_router_with_permissions(dev_auth_enabled, BTreeSet::new())
}

async fn status_of(router: axum::Router, request: Request<Body>) -> StatusCode {
    let response = router.oneshot(request).await.expect("router must respond");
    response.status()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request must build")
}

fn authorized_get(uri: &str, token: &str, tenant: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if tenant {
        builder = builder
            .header("x-tenant-id", "00000000-0000-0000-0000-000000000001")
            .header("x-user-id", "00000000-0000-0000-0000-000000000002");
    }
    builder.body(Body::empty()).expect("request must build")
}

#[tokio::test]
async fn unauthenticated_api_request_is_rejected() {
    let router = test_router(true);
    let status = status_of(router, get("/api/v1/anything")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn liveness_is_public() {
    let router = test_router(true);
    let status = status_of(router, get("/health/live")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn readiness_is_public() {
    // No live database: readiness must still be reachable without auth and
    // report unavailability rather than an auth error.
    let router = test_router(true);
    let status = status_of(router, get("/health/ready")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn dev_auth_with_valid_token_and_tenant_passes_auth() {
    let router = test_router(true);
    // No route is registered under /api/v1 yet, so passing auth yields 404
    // (not 401), proving the middleware accepted the credentials.
    let status = status_of(router, authorized_get("/api/v1/anything", DEV_SECRET, true)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dev_auth_with_wrong_token_is_rejected() {
    let router = test_router(true);
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", "wrong-token", true),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dev_auth_without_client_tenant_header_uses_trusted_principal() {
    let router = test_router(true);
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", DEV_SECRET, false),
    )
    .await;
    // The server-configured principal is authoritative; client tenant headers
    // are intentionally unnecessary and ignored.
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn production_mode_is_fail_closed() {
    // With dev auth disabled, every protected request is rejected even with a
    // token that would otherwise be valid (OIDC not yet implemented).
    let router = test_router(false);
    let status = status_of(router, authorized_get("/api/v1/anything", DEV_SECRET, true)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forged_permission_header_cannot_read_integrity_findings() {
    let router = test_router(true);
    let mut request = authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true);
    request.headers_mut().insert(
        "x-management-permissions",
        HeaderValue::from_static("integrity.read"),
    );
    let status = status_of(router, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn server_granted_integrity_read_reaches_governance_boundary() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::IntegrityRead);
    let router = test_router_with_permissions(true, permissions);
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true),
    )
    .await;
    // The test state intentionally has no GovernanceServices.  A non-403
    // response proves the trusted server grant, rather than the request
    // header, authorized the handler.
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn execute_permission_cannot_approve_repair() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::RepairExecute);
    let router = test_router_with_permissions(true, permissions);
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/admin/repairs/00000000-0000-0000-0000-000000000003/approve")
        .header("authorization", format!("Bearer {DEV_SECRET}"))
        .header("x-tenant-id", "00000000-0000-0000-0000-000000000001")
        .header("x-user-id", "00000000-0000-0000-0000-000000000002")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"note":"approve","expected_version":0}"#))
        .expect("request must build");
    let status = status_of(router, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
