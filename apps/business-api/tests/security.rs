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
    ServerConfig, StorageConfig,
};
use business_api::platform_authorization::IdentitySubjectStatusBridge;
use business_api::routes::create_router;
use business_api::state::{
    AccessServices, AppState, DocumentServices, ReadinessProbe, ReadinessReport, ReadinessStatus,
};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use document::query::{
    DocumentDetailQuery, DocumentDetailView, DocumentListFilter, DocumentListPage,
    DocumentListQuery, DocumentListRequest, QueryError,
};
use identity::application::{ResolveAuthenticatedUser, TenantAccessChecker};
use identity::domain::{MembershipSource, PlatformUser, TenantMembership};
use identity::testing::FakeIdentityStores;
use policy::application::Authorize;
use policy::domain::{ResourceScope, RoleBinding, RoleDefinition, ValidityWindow};
use policy::testing::FakePolicyPorts;
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
            log_format: "text".to_string(),
        },
        storage: StorageConfig::default(),
        auth: AuthConfig {
            issuer_url: String::new(),
            audience: None,
            jwks_url: None,
            dev_secret: Some(Secret::new(DEV_SECRET.to_string())),
            dev_auth_enabled,
            dev_permissions: BTreeSet::new(),
            dev_tenant_id: Some(DEV_TENANT_ID),
            dev_user_id: Some(DEV_USER_ID),
            dev_subject: Some("security-test-user".to_string()),
            dev_roles: BTreeSet::new(),
            management_permission_compat_enabled: true,
            bootstrap: business_api::config::BootstrapAdminConfig::default(),
        },
    }
}

/// A `policy.org` stand-in for governance routes: Tenant-scoped bindings
/// never consult the organization bridge, and an empty store fails closed.
fn access_kit(compat_enabled: bool) -> (AccessServices, Arc<FakeIdentityStores>, FakePolicyPorts) {
    let stores = Arc::new(FakeIdentityStores::new());
    // Seed the dev-auth principal as an Active platform user with an
    // Active membership of the dev tenant (the request-path resolver
    // adopts the claimed id on first contact).
    let now = chrono::Utc::now();
    stores.seed_user(PlatformUser::create(DEV_USER_ID, now).expect("test user fixture"));
    stores.seed_membership(
        TenantMembership::join(
            uuid::Uuid::now_v7(),
            DEV_TENANT_ID,
            DEV_USER_ID,
            MembershipSource::Admin,
            now,
        )
        .expect("test membership fixture"),
    );
    let policy = FakePolicyPorts::new();
    let subject = Arc::new(IdentitySubjectStatusBridge::new(Arc::new(
        TenantAccessChecker::new(Arc::clone(&stores.query)),
    )));
    (
        AccessServices {
            resolve: Arc::new(ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve))),
            authorize: Arc::new(Authorize::new(
                Arc::clone(&policy.query),
                subject,
                Arc::clone(&policy.org),
            )),
            compat_enabled,
            oidc_issuer: String::new(),
        },
        stores,
        policy,
    )
}

fn test_router_with_permissions(
    dev_auth_enabled: bool,
    dev_permissions: BTreeSet<ManagementPermission>,
) -> axum::Router {
    test_router_with_access(dev_auth_enabled, dev_permissions, true, |_, _| {})
}

/// Kit-aware builder: `configure` can suspend the membership, seed a role
/// binding, retire a catalog entry, etc. before the router is built.
fn test_router_with_access<F>(
    dev_auth_enabled: bool,
    dev_permissions: BTreeSet<ManagementPermission>,
    compat_enabled: bool,
    configure: F,
) -> axum::Router
where
    F: FnOnce(&FakeIdentityStores, &FakePolicyPorts),
{
    let config = test_config(dev_auth_enabled, vec!["*".to_string()]);
    let ports = Arc::new(EmptyPorts);
    let (access, stores, policy) = access_kit(compat_enabled);
    configure(&stores, &policy);
    let mut state = AppState {
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
        access: None,
    };
    state.access = Some(access);
    let state = Arc::new(state);
    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled,
        dev_secret: Some(DEV_SECRET.to_string()),
        dev_permissions,
        dev_tenant_id: Some(DEV_TENANT_ID),
        dev_user_id: Some(DEV_USER_ID),
        dev_subject: Some("security-test-user".to_string()),
        dev_roles: BTreeSet::new(),
        oidc: None,
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

// ---------------------------------------------------------------------------
// PLAN-0013 Stage 7: compat bridge parity, RoleBinding path, overrides.
// ---------------------------------------------------------------------------

/// BEFORE (fixed `ManagementPermission` check) vs AFTER (Policy `Authorize`
/// with the compat bridge): a server-trusted dev-config grant on a
/// governance route passes the guard while the platform membership is
/// Active, and lands on the governance boundary (502: `governance: None`
/// in this test state, exactly as before the migration).
#[tokio::test]
async fn compat_claim_grant_passes_guard_when_bridge_enabled() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::IntegrityRead);
    let router = test_router_with_access(true, permissions, true, |_, _| {});
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

/// With the bridge disabled the same claim grant decides nothing: the
/// evaluator falls through to `RoleBindings` and denies (403). This is the
/// flag's entire purpose and must stay behaviorally observable.
#[tokio::test]
async fn compat_flag_off_denies_claim_only_grant() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::IntegrityRead);
    let router = test_router_with_access(true, permissions, false, |_, _| {});
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// `RoleBinding` is the primary path and must work with the bridge OFF: an
/// active tenant-role binding for `integrity.read` reaches the governance
/// boundary (502) without any claim grant.
#[tokio::test]
async fn role_binding_grant_works_with_compat_flag_off() {
    let router = test_router_with_access(true, BTreeSet::new(), false, |stores, policy| {
        let _ = stores;
        let now = chrono::Utc::now();
        let role = RoleDefinition::create_tenant_role(
            uuid::Uuid::now_v7(),
            DEV_TENANT_ID,
            "integrity-readers",
            "Integrity readers",
            now,
        )
        .expect("test role fixture");
        let role_id = role.role_id();
        policy.seed_role(role, &["integrity.read"]);
        policy.seed_binding(
            RoleBinding::create(
                uuid::Uuid::now_v7(),
                DEV_TENANT_ID,
                DEV_USER_ID,
                role_id,
                ResourceScope::Tenant,
                ValidityWindow {
                    effective_at: now,
                    expires_at: None,
                },
                now,
            )
            .expect("test binding fixture"),
        );
    });
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

/// Suspended membership overrides even a compat grant (deny on the very
/// next request, unexpired token or not).
#[tokio::test]
async fn suspended_membership_overrides_claim_grant() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::IntegrityRead);
    let router = test_router_with_access(true, permissions, true, |stores, _| {
        stores.suspend_membership(DEV_TENANT_ID, DEV_USER_ID);
    });
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// A disabled platform user overrides even a compat grant.
#[tokio::test]
async fn disabled_user_overrides_claim_grant() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::IntegrityRead);
    let router = test_router_with_access(true, permissions, true, |stores, _| {
        let now = chrono::Utc::now();
        let mut user = PlatformUser::create(DEV_USER_ID, now).expect("test user fixture");
        user.disable(now).expect("disable fixture");
        stores.seed_user(user);
    });
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/integrity/findings", DEV_SECRET, true),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// A retired catalog permission denies even with a compat grant: the
/// catalog-existence check is locked before the bridge in the evaluator.
#[tokio::test]
async fn retired_permission_denies_claim_grant() {
    let mut permissions = BTreeSet::new();
    permissions.insert(ManagementPermission::AuditRead);
    let router = test_router_with_access(true, permissions, true, |_, policy| {
        policy.retire_permission("audit.read");
    });
    let status = status_of(
        router,
        authorized_get("/api/v1/admin/audit-events", DEV_SECRET, true),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
