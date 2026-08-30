//! Prometheus metrics endpoint and request instrumentation (PLAN-0012 T4.2).
//!
//! Design constraints (`OBSERVABILITY_ARCHITECTURE`):
//! - labels stay bounded: method + numeric status class, never raw paths,
//!   tenant ids, or user input;
//! - `/metrics` is a public scrape endpoint and carries no auth context;
//! - a scrape failure must not affect health endpoints or business routes.

use std::sync::OnceLock;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::IntoResponse;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder exactly once and keep the render
/// handle for the `/metrics` endpoint.
///
/// # Panics
///
/// Panics if the recorder fails to install, which can only happen if the
/// process attempts a second installation; startup code and tests both go
/// through this function's `OnceLock`, so that indicates a wiring bug.
#[allow(clippy::expect_used)]
pub fn install_metrics() -> &'static PrometheusHandle {
    METRICS_HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("prometheus recorder must install exactly once")
    })
}

/// Public scrape endpoint rendering the Prometheus text exposition format.
#[allow(clippy::unused_async)]
pub async fn metrics_handler() -> impl IntoResponse {
    let body = METRICS_HANDLE.get().map_or_else(
        || "# metrics not installed\n".to_string(),
        metrics_exporter_prometheus::PrometheusHandle::render,
    );
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

/// Record one completed request with bounded labels (method, status).
pub async fn track_requests(request: Request, next: Next) -> axum::response::Response {
    let method = request.method().as_str().to_owned();
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "status" => status.as_u16().to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
    )
    .record(started.elapsed().as_secs_f64());
    response
}

/// Record one authentication rejection with a bounded reason class.
pub fn record_auth_failure(reason: &str) {
    metrics::counter!("auth_failures_total", "reason" => reason.to_owned()).increment(1);
}
