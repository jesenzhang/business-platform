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
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

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
    use super::LogFormat;

    #[test]
    fn log_format_parse_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(LogFormat::parse("json"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse(" JSON "), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse("text"), Some(LogFormat::Text));
        assert_eq!(LogFormat::parse("xml"), None);
        assert_eq!(LogFormat::parse(""), None);
    }
}
