pub mod metrics;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::TracerProvider;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::Interest;
use tracing::Metadata;
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::Layer;
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
/// Gap OBS-W4-02. Measured evidence: at `debug` the AWS SDK emits
/// `endpoint_params { bucket, key }`,
/// `signing request request=SignableRequest { uri: …/<bucket>/<key> }` and the
/// static credential's `access_key_id` (PLAN-0014 Wave 4 evidence
/// `23-object-storage-log-hygiene.log`, re-run in Round 2 as
/// `2026-09-28-wave4-round2-gates/10-red-provider-log-safety-before-fix.log`).
///
/// A match is this target **or any target below it** (`aws_sigv4` also covers
/// `aws_sigv4::http_request::sign`), because `EnvFilter` directives prove that
/// the deeper targets are the ones carrying the material.
pub const PROVIDER_SUPPRESSED_TARGETS: &[&str] = &[
    "aws_sdk_s3",
    "aws_smithy_runtime",
    // The transport connector itself logs at DEBUG (measured in the Wave 4
    // Round 2 SDK log run: `aws_smithy_http_client::client: new connector
    // created in 564.4µs`, and on other versions the resolved TLS/endpoint
    // configuration), so it belongs to the provider path.
    "aws_smithy_http_client",
    "aws_sigv4",
    "aws_runtime",
    "hyper",
    "h2",
    "rustls",
    "reqwest",
];

/// True when `target` is one of [`PROVIDER_SUPPRESSED_TARGETS`] or a child of
/// one. The `::` boundary matters: a prefix match would also silence an
/// application target that merely starts with the same characters.
#[must_use]
pub fn is_provider_suppressed_target(target: &str) -> bool {
    PROVIDER_SUPPRESSED_TARGETS.iter().any(|pinned| {
        target == *pinned
            || target
                .strip_prefix(*pinned)
                .is_some_and(|rest| rest.starts_with("::"))
    })
}

/// True for the two levels a provider leaks storage references and credential
/// material at. INFO, WARN and ERROR stay reachable: they are the operator's
/// only provider-side signal, and this boundary does not claim to redact their
/// payload (that gap stays open as OBS-W4-07).
///
/// The two levels are named explicitly. `Level`'s `Ord` ranks by severity
/// (`ERROR < WARN < INFO < DEBUG`), so `level <= Level::DEBUG` would read as
/// "DEBUG or *more severe*" and silently swallow WARN and ERROR — the
/// `only_debug_and_trace_are_within_the_boundary` test pins this down.
#[must_use]
pub fn is_provider_suppressed_level(level: &tracing::Level) -> bool {
    *level == tracing::Level::TRACE || *level == tracing::Level::DEBUG
}

/// True when a record must never reach a sink, whatever the operator asked for.
#[must_use]
pub fn is_provider_suppressed(metadata: &Metadata<'_>) -> bool {
    is_provider_suppressed_target(metadata.target())
        && is_provider_suppressed_level(metadata.level())
}

/// The provider safety boundary: a global filtering layer that closes provider
/// and transport DEBUG/TRACE records for good, independent of the configured
/// log level and independent of `RUST_LOG`.
///
/// Why a layer and not filter directives (gap OBS-W4-02, Wave 4 Round 1
/// bypass):
///
/// * Directives are a *level* setting. `RUST_LOG` replaces the whole filter, so
///   appending `<target>=warn` to the default string does nothing once an
///   operator sets `RUST_LOG=debug` — and an operator setting `RUST_LOG` is
///   normal, not exceptional.
/// * Even inside one `EnvFilter`, the most specific matching directive wins, so
///   `aws_sigv4::http_request::sign=trace` outranks a `aws_sigv4=warn` entry
///   appended to it. A parent-target directive cannot outrank its own children.
/// * Filtering only the *sinks* is not enough either: `fmt` renders the fields
///   of the enclosing span on every nested record, so a provider span that
///   exists at all keeps leaking `bucket`/`key`/`access_key_id` through the
///   prefix of unrelated application lines (measured in the Round 2 red log).
///
/// This layer closes the callsite instead. `register_callsite` answers
/// [`Interest::never`], `enabled` answers `false` for spans and
/// [`Layer::event_enabled`] answers `false` for events (`event_enabled` is an
/// independent hook whose default is `true`, so overriding `enabled` alone
/// filters spans but not events), and `Layered` combines layers conjunctively
/// and short-circuits on `never`
/// (`tracing-subscriber-0.3.23/src/layer/layered.rs:99-120,146-154,435-476`),
/// so no `EnvFilter` — however permissive, however specific — can re-open a
/// callsite this layer has closed. The record is therefore never constructed:
/// it reaches neither the fmt sink nor the OpenTelemetry span pipeline, it is
/// never registered in the span registry, and its fields never appear as the
/// span context of another record.
///
/// [`Interest::never`]: tracing::subscriber::Interest::never
/// [`Layer::event_enabled`]: tracing_subscriber::layer::Layer::event_enabled
#[derive(Clone, Copy, Debug, Default)]
pub struct ProviderSafetyFilter;

impl<S: Subscriber> Layer<S> for ProviderSafetyFilter {
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if is_provider_suppressed(metadata) {
            Interest::never()
        } else {
            Interest::sometimes()
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>, _context: Context<'_, S>) -> bool {
        !is_provider_suppressed(metadata)
    }

    /// `Layer::event_enabled` is a separate hook and its default is `true`:
    /// overriding `enabled` alone filters spans but not events, so both must
    /// answer for the boundary to hold.
    fn event_enabled(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) -> bool {
        !is_provider_suppressed(event.metadata())
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        // The boundary is per target, not per level: it must not lower the
        // level hint of anything else in the stack.
        None
    }
}

/// Build the default [`EnvFilter`] directives for a service log level.
///
/// The provider entries are an optimisation only: they keep provider
/// DEBUG/TRACE records from being constructed at all when no `RUST_LOG` is set.
/// The safety property does not depend on this string — it is enforced by
/// [`ProviderSafetyFilter`], which no `RUST_LOG` value can weaken.
#[must_use]
pub fn default_filter_directives(log_level: &str) -> String {
    let mut directives = String::from(log_level.trim());
    for target in PROVIDER_SUPPRESSED_TARGETS {
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
///
/// [`ProviderSafetyFilter`] is installed next to the env-filter, never behind
/// it. Because it closes callsites rather than filtering sinks, the provider
/// records it rejects are never constructed at all, so they reach the fmt
/// output, the OTLP span pipeline and the span context of other records
/// equally little — which is the whole point of making the boundary
/// independent of `log_level` and of `RUST_LOG`.
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
            .with(ProviderSafetyFilter)
            .with(env_filter)
            .with(fmt_layer)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init();

        Ok(TracingGuard {
            provider: Some(provider),
        })
    } else {
        tracing_subscriber::registry()
            .with(ProviderSafetyFilter)
            .with(env_filter)
            .with(fmt_layer)
            .init();

        Ok(TracingGuard { provider: None })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_filter_directives, is_provider_suppressed_level, is_provider_suppressed_target,
        LogFormat, ProviderSafetyFilter,
    };
    use std::fmt as std_fmt;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id};
    use tracing::Event;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::EnvFilter;

    /// Synthetic sentinel: it must never reach the recorder through a provider
    /// record or through the fields of a provider span.
    const SENTINEL: &str = "synth-credential-plan0014";
    const SENTINEL_KEY: &str = "tenants/synth/documents/synth-rev/source";

    #[derive(Default)]
    struct FieldCollector {
        rendered: Vec<String>,
    }

    impl Visit for FieldCollector {
        fn record_debug(&mut self, field: &Field, value: &dyn std_fmt::Debug) {
            self.rendered.push(format!("{}={value:?}", field.name()));
        }
    }

    /// Records spans as well as events: a boundary that only drops events still
    /// leaks, because sinks render the fields of enclosing spans.
    #[derive(Clone, Default)]
    struct RecordingLayer {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingLayer {
        fn record(&self, entry: String) {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(entry);
        }
    }

    impl<S: tracing::Subscriber> Layer<S> for RecordingLayer {
        fn on_new_span(&self, attributes: &Attributes<'_>, _id: &Id, _context: Context<'_, S>) {
            let mut visitor = FieldCollector::default();
            attributes.record(&mut visitor);
            let metadata = attributes.metadata();
            self.record(format!(
                "span {}::{} {}",
                metadata.target(),
                metadata.name(),
                visitor.rendered.join(" ")
            ));
        }

        fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
            let mut visitor = FieldCollector::default();
            event.record(&mut visitor);
            let metadata = event.metadata();
            self.record(format!(
                "event {}::{} {}",
                metadata.target(),
                metadata.name(),
                visitor.rendered.join(" ")
            ));
        }
    }

    impl RecordingLayer {
        fn snapshot(&self) -> Vec<String> {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    /// Emit the record shape a provider produces, plus what the service itself
    /// must keep.
    ///
    /// A macro, not a shared function: `tracing` caches callsite interest for
    /// the whole process, so tests that install *different* subscribers must
    /// not share callsites — the first test to touch a callsite would decide
    /// the interest every later test sees. Expanding per test gives each stack
    /// its own callsites, so an assertion cannot depend on test order.
    macro_rules! emit_provider_and_application_records {
        () => {{
            tracing::debug!(
                target: "aws_sigv4::http_request::sign",
                "provider debug"
            );
            tracing::trace!(target: "aws_sigv4", "provider trace");
            tracing::debug!(target: "aws_sdk_s3::endpoint_auth", "provider debug");
            tracing::debug!(
                target: "hyper::client::connect::http",
                "provider debug"
            );
            tracing::debug!(
                target: "aws_smithy_runtime::client::http::hyper_014",
                access_key_id = SENTINEL,
                "provider debug with a credential"
            );
            {
                let provider_span = tracing::span!(
                    target: "aws_sigv4::http_request::sign",
                    tracing::Level::DEBUG,
                    "signing",
                    bucket = SENTINEL,
                    key = SENTINEL_KEY,
                );
                let _entered = provider_span.entered();
                tracing::debug!(
                    target: "aws_smithy_runtime",
                    "provider debug inside provider span"
                );
            }
            tracing::warn!(
                target: "aws_sdk_s3::client::orchestrator",
                "provider warn"
            );
            tracing::error!(target: "reqwest::connect", "provider error");
            tracing::debug!(target: "object_storage::client", "platform debug");
            tracing::trace!(target: "object_storage::client", "platform trace");
        }};
    }

    fn assert_provider_material_absent(seen: &[String]) {
        for entry in seen {
            assert!(
                !entry.contains(SENTINEL) && !entry.contains(SENTINEL_KEY),
                "provider material reached the pipeline: {entry}"
            );
            assert!(
                !entry.contains("provider debug") && !entry.contains("provider trace"),
                "a provider DEBUG/TRACE record reached the pipeline: {entry}"
            );
        }
    }

    /// What the pipeline must still carry: the service's own diagnostics at the
    /// levels its own filter enables, and the provider's INFO and up.
    fn assert_diagnostics_kept(seen: &[String], trace_enabled: bool) {
        assert!(
            seen.iter()
                .any(|entry| entry.contains("object_storage::client")
                    && entry.contains("platform debug")),
            "the service's own DEBUG events must stay enabled, saw {seen:?}"
        );
        assert_eq!(
            seen.iter()
                .any(|entry| entry.contains("object_storage::client")
                    && entry.contains("platform trace")),
            trace_enabled,
            "the service's own TRACE events must follow its own filter level, saw {seen:?}"
        );
        assert!(
            seen.iter()
                .any(|entry| entry.contains("aws_sdk_s3::client::orchestrator")
                    && entry.contains("provider warn")),
            "provider WARN events must still reach operators, saw {seen:?}"
        );
        assert!(
            seen.iter()
                .any(|entry| entry.contains("reqwest::connect")
                    && entry.contains("provider error")),
            "provider ERROR events must still reach operators, saw {seen:?}"
        );
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
        let subscriber = tracing_subscriber::registry()
            .with(ProviderSafetyFilter)
            .with(EnvFilter::new(default_filter_directives("debug")))
            .with(recorder.clone());

        tracing::subscriber::with_default(subscriber, || {
            emit_provider_and_application_records!();
        });

        let seen = recorder.snapshot();
        assert_provider_material_absent(&seen);
        assert_diagnostics_kept(&seen, false);
    }

    /// Run the provider/application record set through a subscriber stack and
    /// assert both halves of the contract. Generic so that several distinct
    /// `Layered` stacks can be checked without boxing them.
    fn assert_boundary_holds<S>(recorder: &RecordingLayer, subscriber: S, trace_enabled: bool)
    where
        S: tracing::Subscriber + Send + Sync + 'static,
    {
        tracing::subscriber::with_default(subscriber, || {
            emit_provider_and_application_records!();
        });
        let seen = recorder.snapshot();
        assert_provider_material_absent(&seen);
        assert_diagnostics_kept(&seen, trace_enabled);
    }

    /// The Round 1 bypass: `RUST_LOG=trace` replaces the whole filter, so the
    /// appended `<target>=warn` directives are simply gone. The boundary must
    /// hold against the most permissive filter the operator can ask for.
    #[test]
    fn boundary_holds_against_the_most_permissive_env_filter() {
        let recorder = RecordingLayer::default();
        let stack = tracing_subscriber::registry().with(recorder.clone());
        assert_boundary_holds(
            &recorder,
            stack
                .with(ProviderSafetyFilter)
                .with(EnvFilter::new("trace")),
            true,
        );
    }

    /// The other Round 1 bypass: `EnvFilter` resolves the most specific
    /// matching directive, so a subtarget directive outranks a parent-target
    /// `=warn` entry inside the same filter string.
    #[test]
    fn boundary_holds_against_more_specific_subtarget_directives() {
        let recorder = RecordingLayer::default();
        let stack = tracing_subscriber::registry().with(recorder.clone());
        let filter = EnvFilter::new(
            "debug,aws_sigv4::http_request::sign=trace,\
             aws_smithy_runtime::client::http=trace,object_storage=trace",
        );
        assert_boundary_holds(
            &recorder,
            stack.with(ProviderSafetyFilter).with(filter),
            true,
        );
    }

    /// Order matters for a filter, not for a boundary: the safety layer must
    /// also hold when it sits below the env-filter in the stack.
    #[test]
    fn boundary_holds_whatever_order_the_layers_are_installed_in() {
        let above = RecordingLayer::default();
        let stack = tracing_subscriber::registry().with(above.clone());
        assert_boundary_holds(
            &above,
            stack
                .with(EnvFilter::new("trace"))
                .with(ProviderSafetyFilter),
            true,
        );

        let below = RecordingLayer::default();
        let stack = tracing_subscriber::registry().with(below.clone());
        assert_boundary_holds(
            &below,
            stack
                .with(ProviderSafetyFilter)
                .with(EnvFilter::new("trace")),
            true,
        );
    }

    /// A provider span that merely *exists* leaks its fields through the span
    /// context that sinks print for other records, so the boundary must stop
    /// the span from being constructed at all.
    #[test]
    fn provider_spans_are_never_constructed() {
        let recorder = RecordingLayer::default();
        let subscriber = tracing_subscriber::registry()
            .with(ProviderSafetyFilter)
            .with(EnvFilter::new("trace"))
            .with(recorder.clone());

        tracing::subscriber::with_default(subscriber, || {
            emit_provider_and_application_records!();
        });

        let seen = recorder.snapshot();
        assert!(
            !seen.iter().any(|entry| entry.starts_with("span aws_sigv4")
                || entry.starts_with("span aws_smithy")
                || entry.starts_with("span aws_sdk")),
            "a provider span was constructed, saw {seen:?}"
        );
        // The application's own records still exist, so this is not passing
        // because nothing was emitted at all.
        assert!(
            seen.iter().any(|entry| entry.contains("platform debug")),
            "the recorder saw nothing, saw {seen:?}"
        );
    }

    /// Without the boundary the same permissive filter does leak. This is the
    /// bypass Round 1 shipped; keeping it asserted means a future "simplifying"
    /// refactor that drops `ProviderSafetyFilter` fails here rather than in a
    /// production log.
    #[test]
    fn without_the_boundary_a_permissive_filter_leaks() {
        let recorder = RecordingLayer::default();
        let stack = tracing_subscriber::registry().with(recorder.clone());
        tracing::subscriber::with_default(stack.with(EnvFilter::new("trace")), || {
            emit_provider_and_application_records!();
        });

        let seen = recorder.snapshot();
        assert!(
            seen.iter()
                .any(|entry| entry.contains("provider debug") || entry.contains(SENTINEL)),
            "expected the unfixed pipeline to leak provider DEBUG, saw {seen:?}"
        );
    }

    #[test]
    fn only_debug_and_trace_are_within_the_boundary() {
        assert!(is_provider_suppressed_level(&tracing::Level::TRACE));
        assert!(is_provider_suppressed_level(&tracing::Level::DEBUG));
        assert!(!is_provider_suppressed_level(&tracing::Level::INFO));
        assert!(!is_provider_suppressed_level(&tracing::Level::WARN));
        assert!(!is_provider_suppressed_level(&tracing::Level::ERROR));
    }

    #[test]
    fn suppression_matches_targets_on_module_boundaries() {
        assert!(is_provider_suppressed_target("aws_sigv4"));
        assert!(is_provider_suppressed_target(
            "aws_sigv4::http_request::sign"
        ));
        assert!(is_provider_suppressed_target("h2"));
        // An unrelated application target that merely shares a prefix must keep
        // its diagnostics; a byte prefix match would silence it.
        assert!(!is_provider_suppressed_target("h2o::server"));
        assert!(!is_provider_suppressed_target("hyperion::ingest"));
        assert!(!is_provider_suppressed_target("object_storage::client"));
        assert!(!is_provider_suppressed_target(""));
    }

    /// The directive string is an optimisation, so it must at least stay a
    /// valid filter that keeps the requested service level.
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
