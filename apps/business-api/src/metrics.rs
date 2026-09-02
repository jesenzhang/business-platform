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

/// Map an HTTP method onto the fixed, bounded metric label set
/// (PLAN-0012 T4.2 hardening). `http::Method` accepts arbitrary extension
/// methods, so the raw string must never reach a Prometheus label; anything
/// outside the known set collapses to `OTHER`.
#[must_use]
pub fn normalize_method(method: &axum::http::Method) -> &'static str {
    use axum::http::Method;
    if method == Method::GET {
        "GET"
    } else if method == Method::POST {
        "POST"
    } else if method == Method::PUT {
        "PUT"
    } else if method == Method::PATCH {
        "PATCH"
    } else if method == Method::DELETE {
        "DELETE"
    } else if method == Method::OPTIONS {
        "OPTIONS"
    } else if method == Method::HEAD {
        "HEAD"
    } else {
        "OTHER"
    }
}

/// Record one completed request with bounded labels (method, status).
pub async fn track_requests(request: Request, next: Next) -> axum::response::Response {
    let method = normalize_method(request.method());
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    metrics::counter!(
        "http_requests_total",
        "method" => method,
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

#[cfg(test)]
mod tests {
    use super::normalize_method;
    use axum::http::Method;

    #[test]
    fn known_methods_map_to_their_names() {
        for (method, expected) in [
            (Method::GET, "GET"),
            (Method::POST, "POST"),
            (Method::PUT, "PUT"),
            (Method::PATCH, "PATCH"),
            (Method::DELETE, "DELETE"),
            (Method::OPTIONS, "OPTIONS"),
            (Method::HEAD, "HEAD"),
        ] {
            assert_eq!(normalize_method(&method), expected);
        }
    }

    #[test]
    fn extension_methods_collapse_to_other() {
        // A client-chosen verb must never become its own label value.
        let murder = Method::from_bytes(b"MURDER").unwrap_or_else(|_| unreachable!());
        assert_eq!(normalize_method(&murder), "OTHER");
        let trace = Method::from_bytes(b"TRACE").unwrap_or_else(|_| unreachable!());
        assert_eq!(normalize_method(&trace), "OTHER");
    }
}

/// Record one authentication rejection with a bounded reason class.
pub fn record_auth_failure(reason: &str) {
    metrics::counter!("auth_failures_total", "reason" => reason.to_owned()).increment(1);
}
