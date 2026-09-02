//! Prometheus recorder installation and a minimal `/metrics` scrape endpoint
//! for processes without a business HTTP surface (`business-worker`,
//! `ai-worker`) — PLAN-0012 T4.2/T4.5.
//!
//! Label discipline (`OBSERVABILITY_ARCHITECTURE`): every label value must
//! come from a bounded, code-defined enumeration. Tenant ids, document ids,
//! correlation ids, storage paths, raw HTTP paths, and provider/model output
//! must never enter metric labels.

use std::sync::OnceLock;

use axum::response::IntoResponse as _;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder exactly once and keep the render
/// handle for the `/metrics` endpoint.
///
/// # Panics
///
/// Panics if the recorder fails to install, which can only happen on a second
/// installation attempt; startup code goes through this function's
/// `OnceLock`, so that indicates a wiring bug.
#[allow(clippy::expect_used)]
pub fn install_metrics() -> &'static PrometheusHandle {
    METRICS_HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("prometheus recorder must install exactly once")
    })
}

/// Render the current Prometheus text exposition.
#[must_use]
pub fn render() -> String {
    METRICS_HANDLE.get().map_or_else(
        || "# metrics not installed\n".to_string(),
        metrics_exporter_prometheus::PrometheusHandle::render,
    )
}

/// Serve `GET /metrics` on `addr` until the returned task is aborted.
///
/// Bind failures are returned so startup can fail closed; once serving,
/// transport errors are logged and never panic the worker.
pub async fn spawn_metrics_server(addr: &str) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| anyhow::anyhow!("failed to bind metrics endpoint on {addr}: {error}"))?;
    let app = axum::Router::new().route("/metrics", axum::routing::get(handler));
    Ok(tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(error = %error, "metrics endpoint stopped serving");
        }
    }))
}

async fn handler() -> axum::response::Response {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        render(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{install_metrics, render};

    #[test]
    fn install_is_idempotent_and_render_exposes_prometheus_text() {
        let first = std::ptr::from_ref(install_metrics());
        let second = std::ptr::from_ref(install_metrics());
        assert!(std::ptr::eq(first, second));
        metrics::counter!("observability_metrics_selftest_total").increment(1);
        let body = render();
        assert!(
            body.contains("observability_metrics_selftest_total"),
            "rendered exposition must contain recorded counters"
        );
    }
}
