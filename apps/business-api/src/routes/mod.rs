pub mod admin;
pub mod documents;
pub mod health;
mod operations;
pub mod processing;
mod public_dto;
mod upload;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderValue, StatusCode};
use axum::middleware;
use axum::routing::get;
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::auth::{auth_middleware, AuthMiddlewareConfig};
use crate::config::ServerConfig;
use crate::state::AppState;

/// 构建 HTTP 路由。
///
/// 路由分为两组：
/// - 公开路由（`/health/live`、`/health/ready`）：无需认证，供探针使用。
/// - 受保护路由（`/api/v1/**`）：经过认证中间件，fail-closed。
///
/// 全局中间件按请求处理顺序（外→内）为：
/// Request ID → Trace → CORS → Body Limit → Timeout → \[Auth(仅受保护路由)\] → Handler。
/// 由于 Axum 的 `.layer()` 越靠后越靠近 handler，下面按相反顺序声明。
pub fn create_router(
    state: Arc<AppState>,
    auth_config: AuthMiddlewareConfig,
    server_config: &ServerConfig,
) -> Router {
    let request_timeout = Duration::from_secs(server_config.request_timeout_secs);
    let body_limit = server_config.body_limit_bytes;
    let cors = build_cors_layer(&server_config.cors_origins);

    let protected_routes = Router::new()
        .nest("/api/v1/documents", documents::router())
        .route("/api/v1/processing-jobs", get(processing::list_jobs))
        .route("/api/v1/operations/overview", get(operations::overview))
        .route("/api/v1/processing-jobs/{job_id}", get(processing::get_job))
        .route(
            "/api/v1/processing-jobs/{job_id}/cancel",
            axum::routing::post(processing::cancel_job),
        )
        .route(
            "/api/v1/processing-jobs/{job_id}/candidate",
            get(processing::get_candidate),
        )
        .route(
            "/api/v1/processing-jobs/{job_id}/review",
            axum::routing::post(processing::review_candidate),
        )
        .route(
            "/api/v1/admin/integrity/scans",
            axum::routing::post(admin::create_scan).get(admin::list_scans),
        )
        .route("/api/v1/admin/integrity/scans/{id}", get(admin::get_scan))
        .route(
            "/api/v1/admin/integrity/findings",
            get(admin::list_findings),
        )
        .route(
            "/api/v1/admin/integrity/findings/{id}",
            get(admin::get_finding),
        )
        .route(
            "/api/v1/admin/repairs/dry-run",
            axum::routing::post(admin::dry_run_repair),
        )
        .route(
            "/api/v1/admin/repairs",
            axum::routing::post(admin::create_repair),
        )
        .route("/api/v1/admin/repairs/{id}", get(admin::get_repair))
        .route(
            "/api/v1/admin/repairs/{id}/approve",
            axum::routing::post(admin::approve_repair),
        )
        .route(
            "/api/v1/admin/repairs/{id}/execute",
            axum::routing::post(admin::execute_repair),
        )
        .route(
            "/api/v1/admin/repairs/{id}/cancel",
            axum::routing::post(admin::cancel_repair),
        )
        .route(
            "/api/v1/admin/repairs/{id}/resume",
            axum::routing::post(admin::resume_repair),
        )
        .route("/api/v1/admin/audit-events", get(admin::list_audit_events))
        .route(
            "/api/v1/admin/audit-events/{id}",
            get(admin::get_audit_event),
        )
        .route(
            "/api/v1/admin/audit/verify-chain",
            axum::routing::post(admin::verify_audit_chain),
        )
        .layer(middleware::from_fn_with_state(auth_config, auth_middleware));

    let public_routes = Router::new()
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        .layer(RequestBodyLimitLayer::new(body_limit))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

/// 根据配置构建 CORS 层。
///
/// - 空列表：不允许任何跨域来源（限制性默认）。
/// - 含 `"*"`：允许任意来源（仅开发环境）。
/// - 其它：仅允许显式列出的来源；非法来源被忽略。
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    if origins.is_empty() {
        return CorsLayer::new();
    }

    if origins.iter().any(|origin| origin == "*") {
        return CorsLayer::new()
            .allow_origin(AllowOrigin::any())
            .allow_methods(AllowMethods::any())
            .allow_headers(AllowHeaders::any());
    }

    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods(AllowMethods::any())
        .allow_headers(AllowHeaders::any())
}
