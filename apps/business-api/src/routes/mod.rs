pub mod health;

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    let api_v1 = Router::new();
    // Future: .nest("/contracts", contract::api::router())
    // Future: .nest("/customers", customer::api::router())

    let request_timeout = Duration::from_secs(state.config.server.request_timeout_secs);

    Router::new()
        .route("/health", axum::routing::get(health::liveness))
        .route("/health/ready", axum::routing::get(health::readiness))
        .nest("/api/v1", api_v1)
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
