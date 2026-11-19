//! Prometheus metrics endpoint and request instrumentation (PLAN-0012 T4.2).
//!
//! Design constraints (`OBSERVABILITY_ARCHITECTURE`):
//! - labels stay bounded: method + numeric status class, never raw paths,
//!   tenant ids, or user input;
//! - `/metrics` is a public scrape endpoint and carries no auth context;
//! - a scrape failure must not affect health endpoints or business routes.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::IntoResponse;
use metrics_exporter_prometheus::PrometheusHandle;

/// Install the global Prometheus recorder exactly once and keep the render
/// handle for the `/metrics` endpoint.
///
/// Installation is delegated to [`observability::metrics`] on purpose (PLAN-0014
/// Wave 4 gap OBS-W4-03): that is where the `*_seconds` duration buckets are
/// configured, and a process that installs its own recorder loses them, which
/// silently blanks the versioned dashboard's `histogram_quantile(..._bucket)`
/// panels. There is exactly one recorder per process either way, so going
/// through the shared installer is the only way to keep both properties.
///
/// # Panics
///
/// Panics if the recorder fails to install, which can only happen if the
/// process attempts a second installation; startup code and tests both go
/// through this function's `OnceLock`, so that indicates a wiring bug.
pub fn install_metrics() -> &'static PrometheusHandle {
    observability::metrics::install_metrics()
}

/// Render the current Prometheus text exposition.
#[must_use]
pub fn render() -> String {
    observability::metrics::render()
}

/// Public scrape endpoint rendering the Prometheus text exposition format.
#[allow(clippy::unused_async)]
pub async fn metrics_handler() -> impl IntoResponse {
    let body = render();
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

/// Record one authentication rejection with a bounded reason class.
pub fn record_auth_failure(reason: &str) {
    metrics::counter!("auth_failures_total", "reason" => reason.to_owned()).increment(1);
}

/// Record one Policy `Authorize` outcome (PLAN-0013 §10). Labels are drawn
/// exclusively from bounded vocabularies: `decision` is `allow` or `deny`
/// and `reason` is the closed `DecisionReason` enum's stable string. No
/// user/tenant/resource identifiers ever enter these labels.
pub fn record_authorization_decision(allowed: bool, reason: policy::domain::DecisionReason) {
    metrics::counter!(
        "authorization_decisions_total",
        "decision" => if allowed { "allow" } else { "deny" },
        "reason" => reason.as_str(),
    )
    .increment(1);
}

/// Record the wall-clock duration of one Policy `Authorize` evaluation.
pub fn record_authorization_duration(elapsed: std::time::Duration) {
    metrics::histogram!("authorization_duration_seconds").record(elapsed.as_secs_f64());
}

/// Record one startup bootstrap execution outcome (PLAN-0013 §10). The
/// outcome label is a bounded enum rendered by [`BootstrapMetricOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapMetricOutcome {
    Executed,
    NoOp,
    Failed,
    ConfigStale,
    PrincipalMismatch,
    Unavailable,
}

impl BootstrapMetricOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::NoOp => "noop",
            Self::Failed => "failed",
            Self::ConfigStale => "config_stale",
            Self::PrincipalMismatch => "principal_mismatch",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Record the outcome of one bootstrap administrator execution.
pub fn record_bootstrap_outcome(outcome: BootstrapMetricOutcome) {
    metrics::counter!("bootstrap_outcome_total", "outcome" => outcome.as_str()).increment(1);
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

    /// Gap OBS-W4-03: `/metrics` in this process must expose durations as
    /// bucketed histograms, because the versioned dashboard resolves
    /// `histogram_quantile(..._bucket)`. That only holds while the shared
    /// recorder from `observability::metrics` is the installed one.
    #[test]
    fn duration_metrics_render_as_bucketed_histograms() {
        super::install_metrics();
        super::record_authorization_duration(std::time::Duration::from_millis(300));
        let body = super::render();
        assert!(
            body.contains("authorization_duration_seconds_bucket{le=\"0.5\"} 1"),
            "business-api durations must expose explicit histogram buckets, got:\n{body}"
        );
        assert!(
            !body.contains("authorization_duration_seconds{quantile="),
            "business-api durations must not degrade to client-side summary quantiles"
        );
    }
}
