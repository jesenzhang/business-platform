//! HTTP security baseline tests (WP-04).
//!
//! These exercise the composed router directly via `tower::ServiceExt::oneshot`,
//! without binding a socket or requiring a live database (a lazy pool is used so
//! the readiness probe degrades gracefully instead of failing the test setup).

#![allow(clippy::expect_used)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use business_api::auth::AuthMiddlewareConfig;
use business_api::routes::create_router;
use business_api::state::AppState;
use shared_kernel::config::{
    AppEnv, AuthConfig, BucketConfig, DatabaseConfig, MessagingConfig, ObservabilityConfig,
    ServerConfig, StorageConfig,
};
use shared_kernel::{AppConfig, Secret};
use tower::ServiceExt;

const DEV_SECRET: &str = "test-dev-secret";

fn test_config(dev_auth_enabled: bool, cors_origins: Vec<String>) -> AppConfig {
    AppConfig {
        env: AppEnv::Development,
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            request_timeout_secs: 30,
            cors_origins,
            body_limit_bytes: 1024,
        },
        database: DatabaseConfig {
            url: "postgres://user:pass@localhost:5432/db".to_string(),
            max_connections: 1,
            min_connections: 0,
            acquire_timeout_secs: 1,
        },
        storage: StorageConfig {
            endpoint: "http://localhost:9000".to_string(),
            access_key: Secret::new("minioadmin".to_string()),
            secret_key: Secret::new("minioadmin".to_string()),
            region: "us-east-1".to_string(),
            buckets: BucketConfig::default(),
        },
        messaging: MessagingConfig {
            nats_url: "nats://localhost:4222".to_string(),
            enabled: false,
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
        },
    }
}

fn test_router(dev_auth_enabled: bool) -> axum::Router {
    let config = test_config(dev_auth_enabled, vec!["*".to_string()]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(0)
        .max_connections(1)
        .connect_lazy(&config.database.url)
        .expect("lazy pool must not connect eagerly");

    let state = Arc::new(AppState { pool, config });
    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled,
        dev_secret: Some(DEV_SECRET.to_string()),
    };
    create_router(state, auth_config)
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
            .header("x-tenant-id", "tenant-1")
            .header("x-user-id", "user-1");
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
async fn dev_auth_missing_tenant_header_is_rejected() {
    let router = test_router(true);
    let status = status_of(
        router,
        authorized_get("/api/v1/anything", DEV_SECRET, false),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn production_mode_is_fail_closed() {
    // With dev auth disabled, every protected request is rejected even with a
    // token that would otherwise be valid (OIDC not yet implemented).
    let router = test_router(false);
    let status = status_of(router, authorized_get("/api/v1/anything", DEV_SECRET, true)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
