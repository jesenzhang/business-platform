use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::TracerProvider;
use shared_kernel::config::ObservabilityConfig;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

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
/// with an env-filter (derived from `config.log_level`) is installed.
pub fn init_tracing(config: &ObservabilityConfig) -> anyhow::Result<TracingGuard> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let fmt_layer = tracing_subscriber::fmt::layer();

    match &config.otlp_endpoint {
        Some(endpoint) => {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .build()?;

            let provider = TracerProvider::builder()
                .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new(
                        "service.name",
                        config.service_name.clone(),
                    ),
                ]))
                .build();

            let tracer = provider.tracer(config.service_name.clone());

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(tracing_opentelemetry::layer().with_tracer(tracer))
                .init();

            Ok(TracingGuard {
                provider: Some(provider),
            })
        }
        None => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .init();

            Ok(TracingGuard { provider: None })
        }
    }
}
