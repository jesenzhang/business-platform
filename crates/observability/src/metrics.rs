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

/// Fixed bucket boundaries (seconds) for every `*_seconds` duration metric.
///
/// `metrics-exporter-prometheus` renders unconfigured histograms as
/// client-side summaries whose quantiles are approximate and unreliable; the
/// versioned dashboard queries `histogram_quantile(..._bucket)` instead, so
/// duration metrics must be exposed as proper Prometheus histograms. The set
/// spans sub-millisecond API work to multi-minute AI task durations.
const DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0,
];

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
            .set_buckets_for_metric(
                metrics_exporter_prometheus::Matcher::Suffix("_seconds".to_string()),
                DURATION_BUCKETS,
            )
            .expect("duration bucket configuration must be valid")
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

    #[test]
    fn duration_histograms_render_as_bucketed_prometheus_histograms() {
        // The versioned dashboard resolves p50/p95 via
        // `histogram_quantile(..._bucket)`; an unconfigured exporter would
        // render these as summary quantiles instead and the panels go blank.
        install_metrics();
        metrics::histogram!("observability_selftest_duration_seconds").record(0.3);
        let body = render();
        assert!(
            body.contains("observability_selftest_duration_seconds_bucket{le=\"0.5\"} 1"),
            "duration metrics must expose explicit histogram buckets"
        );
        assert!(
            !body.contains("observability_selftest_duration_seconds{quantile="),
            "duration metrics must not degrade to client-side summary quantiles"
        );
    }
}
