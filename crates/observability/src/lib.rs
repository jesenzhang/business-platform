pub mod metrics;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::layer::Layer as _;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Log output format for the fmt layer.
///
/// `text` is the human-readable development default; `json` emits one JSON
/// object per event so preproduction collectors can parse logs without
/// custom codecs (PLAN-0012 T4.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

impl LogFormat {
    /// Parse the `observability.log_format` configuration value (fail-closed:
    /// unknown values are rejected by callers via [`Self::parse`]).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// Guard that shuts down the OpenTelemetry tracer provider on drop.
///
/// Hold this value for the lifetime of the application to ensure spans
/// are flushed and exported before the process exits.
pub struct TracingGuard {
    provider: Option<TracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(ref provider) = self.provider {
            if let Err(err) = provider.shutdown() {
                eprintln!("failed to shutdown tracer provider: {err}");
            }
        }
    }
}

/// Provider and transport crates log the full request URI, bucket names, object
/// keys and credential identifiers at DEBUG/TRACE. Those are storage references
/// and credential material, so they must not enter the log or trace pipeline
/// (AGENTS §10, PLAN-0003/PLAN-0004 storage reference rules).
///
/// Wave 4 gap OBS-W4-02: the default filter pins these targets to `warn` even
/// when the service itself runs at `debug`. Measured evidence: at `debug` the
/// AWS SDK emits `endpoint_params { bucket, key }`,
/// `signing request request=SignableRequest { uri: …/<bucket>/<key> }` and the
/// static credential's `access_key_id` (PLAN-0014 Wave 4 evidence
/// `23-object-storage-log-hygiene.log`).
///
/// Setting `RUST_LOG` replaces the whole filter, so an operator can still turn
/// these targets up deliberately.
const SUPPRESSED_TARGETS: [&str; 8] = [
    "aws_sdk_s3",
    "aws_smithy_runtime",
    "aws_sigv4",
    "aws_runtime",
    "hyper",
    "h2",
    "rustls",
    "reqwest",
];

/// Build the default [`EnvFilter`] directives for a service log level.
#[must_use]
pub fn default_filter_directives(log_level: &str) -> String {
    let mut directives = String::from(log_level.trim());
    for target in SUPPRESSED_TARGETS {
        directives.push(',');
        directives.push_str(target);
        directives.push_str("=warn");
    }
    directives
}

/// Initialize the global tracing subscriber.
///
/// When `config.otlp_endpoint` is `Some`, an OpenTelemetry OTLP exporter is
/// configured alongside the standard fmt layer. Otherwise only the fmt layer
/// with an env-filter (derived from `log_level`) is installed. `format`
/// selects human-readable text or single-line JSON output.
pub fn init_tracing(
    service_name: &str,
    log_level: &str,
    log_format: LogFormat,
    otlp_endpoint: Option<&str>,
) -> anyhow::Result<TracingGuard> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter_directives(log_level)));

    let fmt_layer = tracing_subscriber::fmt::layer().with_target(true);
    let fmt_layer = match log_format {
        LogFormat::Text => fmt_layer.boxed(),
        LogFormat::Json => fmt_layer.json().boxed(),
    };

    if let Some(endpoint) = otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let provider = TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(opentelemetry_sdk::Resource::new(vec![
                opentelemetry::KeyValue::new("service.name", service_name.to_string()),
            ]))
            .build();

        let tracer = provider.tracer(service_name.to_string());

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init();

        Ok(TracingGuard {
            provider: Some(provider),
        })
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        Ok(TracingGuard { provider: None })
    }
}

#[cfg(test)]
mod tests {
    use super::{default_filter_directives, LogFormat};
    use std::sync::{Arc, Mutex};
    use tracing::Event;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::EnvFilter;

    #[derive(Clone, Default)]
    struct RecordingLayer {
        seen: Arc<Mutex<Vec<&'static str>>>,
    }

    impl RecordingLayer {
        fn record(&self, target: &'static str) {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(target);
        }
    }

    impl<S: tracing::Subscriber> Layer<S> for RecordingLayer {
        fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
            self.record(event.metadata().target());
        }
    }

    #[test]
    fn log_format_parse_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(LogFormat::parse("json"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse(" JSON "), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse("text"), Some(LogFormat::Text));
        assert_eq!(LogFormat::parse("xml"), None);
        assert_eq!(LogFormat::parse(""), None);
    }

    /// Gap OBS-W4-02: provider crates log bucket names, object keys, full
    /// request URIs and credential identifiers at DEBUG. A service running at
    /// `debug` must not carry those into logs or traces.
    #[test]
    fn provider_debug_events_are_pinned_while_platform_debug_survives() {
        let recorder = RecordingLayer::default();
        let seen = Arc::clone(&recorder.seen);
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new(default_filter_directives("debug")))
            .with(recorder);

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "aws_sigv4::http_request::sign", "provider debug");
            tracing::debug!(target: "aws_sdk_s3::endpoint_auth", "provider debug");
            tracing::debug!(target: "hyper::client::connect::http", "provider debug");
            tracing::warn!(target: "aws_sdk_s3::client::orchestrator", "provider warn");
            tracing::debug!(target: "object_storage::client", "platform debug");
        });

        let seen = seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for suppressed in [
            "aws_sigv4::http_request::sign",
            "aws_sdk_s3::endpoint_auth",
            "hyper::client::connect::http",
        ] {
            assert!(
                !seen.contains(&suppressed),
                "provider target {suppressed} must be pinned out of the pipeline at log level debug, saw {seen:?}"
            );
        }
        assert!(
            seen.contains(&"aws_sdk_s3::client::orchestrator"),
            "provider WARN events must still reach operators, saw {seen:?}"
        );
        assert!(
            seen.contains(&"object_storage::client"),
            "the service's own DEBUG events must stay enabled, saw {seen:?}"
        );
    }

    #[test]
    fn default_directives_keep_the_requested_service_level() {
        let directives = default_filter_directives("info");
        assert!(
            directives.starts_with("info,"),
            "the service log level must stay the leading directive: {directives}"
        );
        assert!(
            directives.contains("aws_sigv4=warn"),
            "credential-bearing provider targets must be pinned: {directives}"
        );
        // The directive string must parse as a real EnvFilter.
        let _ = EnvFilter::new(directives);
    }
}
