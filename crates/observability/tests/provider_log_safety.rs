#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 Round 2 — the provider safety boundary must not be a
//! configurable log level.
//!
//! AGENTS §10 forbids storage references, credential identifiers and signed
//! request material in logs and traces. The AWS SDK and its transports emit
//! exactly those values at DEBUG/TRACE. Wave 4 Round 1 pinned the provider
//! targets by appending `<target>=warn` directives to the `EnvFilter` string,
//! and only when `RUST_LOG` was unset, so three ordinary operator actions
//! reopened the leak (this closes gap `OBS-W4-02`, reopened in Round 2 as a
//! security fix rather than a noise optimisation):
//!
//! 1. `RUST_LOG=debug` replaced the whole filter, suppression included;
//! 2. a more specific provider subtarget directive wins anyway, because
//!    `EnvFilter` resolves the most specific matching directive, so
//!    `aws_sigv4::http_request::sign=trace` outranks `aws_sigv4=warn`;
//! 3. span directives behaved the same way.
//!
//! These tests therefore drive the real `observability::init_tracing` path,
//! not a hand-built `EnvFilter`.
//!
//! Isolation: `init_tracing` installs a *process global* subscriber and reads
//! `RUST_LOG` from the *process environment*. Neither can be reset between
//! tests, and mutating the environment of a process whose tests run in
//! parallel is a data race. Every case therefore runs in a fresh child process
//! of this same test binary; the parent only spawns children with an explicit
//! environment and asserts on their captured output.
//!
//! Every sentinel value below is synthetic. None of them is a credential, a
//! bucket or an object key that exists anywhere.

use std::io::Write as _;
use std::process::Command;

/// Marker the child process looks for; never set in the parent.
const CHILD_CASE: &str = "PLAN0014_OBS_CHILD_CASE";

/// Synthetic sentinels. None of these may appear in a child's output.
const SENTINEL_BUCKET: &str = "synth-bucket-plan0014";
const SENTINEL_KEY: &str = "tenants/synth-tenant/documents/synth-doc/revisions/synth-rev/source";
const SENTINEL_ACCESS_KEY_ID: &str = "AKIASYNTHETICPLAN0014";
const SENTINEL_SECRET_ACCESS_KEY: &str = "synth-secret-plan0014-do-not-log";
const SENTINEL_SIGNATURE: &str = "synthsig-plan0014-value";
const SENTINEL_URL: &str = "https://synth-bucket-plan0014.s3.invalid/tenants/synth-tenant/x";

/// Marks that identify a provider record by level, independent of target.
const PROVIDER_DEBUG_MARK: &str = "PLAN0014-PROVIDER-DEBUG";
const PROVIDER_TRACE_MARK: &str = "PLAN0014-PROVIDER-TRACE";
/// Marks the child must still produce.
const APP_DEBUG_MARK: &str = "PLAN0014-APP-DEBUG";
const APP_INFO_MARK: &str = "PLAN0014-APP-INFO";
const PROVIDER_WARN_MARK: &str = "PLAN0014-PROVIDER-WARN";

/// Emit one DEBUG and one TRACE record on a literal target. `tracing` requires
/// a constant `target:`, so the target list is driven by a macro expansion
/// rather than a loop.
macro_rules! emit_on_provider_target {
    ($target:literal) => {{
        let name: &str = $target;
        tracing::debug!(
            target: $target,
            bucket = SENTINEL_BUCKET,
            key = SENTINEL_KEY,
            access_key_id = SENTINEL_ACCESS_KEY_ID,
            secret_access_key = SENTINEL_SECRET_ACCESS_KEY,
            signature = SENTINEL_SIGNATURE,
            signed_url = SENTINEL_URL,
            "{PROVIDER_DEBUG_MARK} {name}"
        );
        tracing::trace!(
            target: $target,
            bucket = SENTINEL_BUCKET,
            key = SENTINEL_KEY,
            access_key_id = SENTINEL_ACCESS_KEY_ID,
            signature = SENTINEL_SIGNATURE,
            "{PROVIDER_TRACE_MARK} {name}"
        );
    }};
}

/// Field names the provider attaches to those records. They are asserted
/// absent as well: a redacted value whose field name still appears has leaked
/// the shape, and the boundary under test does not redact — it drops.
const PROVIDER_FIELD_NAMES: &[&str] = &[
    "access_key_id",
    "secret_access_key",
    "signature",
    "signed_url",
];

/// The application's own target, which must keep its DEBUG output.
const APP_TARGET: &str = "plan0014_app";

/// One child configuration. The parent reads this table to build the child
/// environment, the child reads the same table to build its subscriber, so the
/// two sides cannot disagree about a case.
struct Case {
    name: &'static str,
    /// `RUST_LOG` for the child, or `None` to remove it from the environment.
    rust_log: Option<&'static str>,
    log_level: &'static str,
    log_format: &'static str,
    otlp_endpoint: Option<&'static str>,
    why: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "text-rust-log-unset-debug-level",
        rust_log: None,
        log_level: "debug",
        log_format: "text",
        otlp_endpoint: None,
        why: "no RUST_LOG, service level debug",
    },
    Case {
        name: "text-rust-log-unset-trace-level",
        rust_log: None,
        log_level: "trace",
        log_format: "text",
        otlp_endpoint: None,
        why: "no RUST_LOG, service level trace",
    },
    Case {
        name: "text-rust-log-debug",
        rust_log: Some("debug"),
        log_level: "info",
        log_format: "text",
        otlp_endpoint: None,
        why: "RUST_LOG=debug replaces the whole filter",
    },
    Case {
        name: "text-rust-log-trace",
        rust_log: Some("trace"),
        log_level: "info",
        log_format: "text",
        otlp_endpoint: None,
        why: "RUST_LOG=trace",
    },
    Case {
        name: "text-rust-log-specific-provider-subtargets",
        rust_log: Some(
            "warn,aws_sigv4::http_request::sign=trace,aws_sigv4::http_request=debug,\
             aws_smithy_runtime::client=trace,aws_sdk_s3::endpoint=trace,plan0014_app=debug",
        ),
        log_level: "info",
        log_format: "text",
        otlp_endpoint: None,
        why: "subtarget directives more specific than the pinned parents",
    },
    Case {
        name: "text-rust-log-span-directive",
        rust_log: Some("warn,[signing]=trace,[orchestrating]=trace,plan0014_app=debug"),
        log_level: "info",
        log_format: "text",
        otlp_endpoint: None,
        why: "a span directive must not open a provider span either",
    },
    Case {
        name: "json-rust-log-unset-debug-level",
        rust_log: None,
        log_level: "debug",
        log_format: "json",
        otlp_endpoint: None,
        why: "json format, no RUST_LOG",
    },
    Case {
        name: "json-rust-log-debug",
        rust_log: Some("debug"),
        log_level: "info",
        log_format: "json",
        otlp_endpoint: None,
        why: "json format with RUST_LOG=debug",
    },
    Case {
        name: "text-rust-log-debug-with-otlp",
        rust_log: Some("debug"),
        log_level: "info",
        log_format: "text",
        // Nothing listens here: the point is that the OTLP branch installs the
        // same boundary over the span pipeline, not that it exports. Exporting
        // to a real Collector stays BLOCKED (gap OBS-W4-06).
        otlp_endpoint: Some("http://127.0.0.1:1"),
        why: "OTLP branch: the span layer must carry the same boundary",
    },
];

fn case(name: &str) -> &'static Case {
    CASES
        .iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("unknown case {name}"))
}

/// Run one case in a child process and return its combined output.
fn run_case(name: &str) -> String {
    let spec = case(name);
    let mut command =
        Command::new(std::env::current_exe().expect("the test binary must be runnable as a child"));
    command
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .arg("child_entry_point")
        .env(CHILD_CASE, spec.name);
    match spec.rust_log {
        Some(value) => {
            command.env("RUST_LOG", value);
        }
        None => {
            command.env_remove("RUST_LOG");
        }
    }
    let output = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("spawning the child test process must work");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "child case {name} ({}) exited with {:?}\n--- child output ---\n{text}\n--- end ---",
        spec.why,
        output.status.code()
    );
    text
}

/// Emit the records every case emits. Nothing here is a real value.
fn emit_provider_shaped_records() {
    // The application's own diagnostics must survive untouched.
    tracing::debug!(target: APP_TARGET, "{APP_DEBUG_MARK} application diagnostics");
    tracing::info!(target: APP_TARGET, "{APP_INFO_MARK} application info");
    tracing::debug!(
        target: APP_TARGET,
        request_id = "plan0014-synth-request",
        "{APP_DEBUG_MARK} application debug with a safe field"
    );

    // Targets a real provider path uses, including children deeper than the
    // parent targets Wave 4 pinned. A boundary that only knows the parents is
    // bypassed by the children. This mirrors the SDK's own target shape:
    // `aws_sigv4::http_request::sign`, `aws_smithy_runtime::client::http::*`,
    // `aws_sdk_s3::client::orchestrator` and the transport crates below it.
    emit_on_provider_target!("aws_sdk_s3");
    emit_on_provider_target!("aws_sdk_s3::client::orchestrator");
    emit_on_provider_target!("aws_sdk_s3::endpoint::parts");
    emit_on_provider_target!("aws_smithy_runtime");
    emit_on_provider_target!("aws_smithy_runtime::client::http::hyper_014");
    emit_on_provider_target!("aws_sigv4");
    emit_on_provider_target!("aws_sigv4::http_request::sign");
    emit_on_provider_target!("aws_sigv4::http_request::canonical_request");
    emit_on_provider_target!("aws_runtime::auth");
    emit_on_provider_target!("hyper::client::connect::http");
    emit_on_provider_target!("h2::frame::settings");
    emit_on_provider_target!("rustls::msgs::handshake");
    emit_on_provider_target!("reqwest::connect");

    // Spans carry the same material as events, and the fmt layer prints the
    // fields of enclosing spans on every nested record.
    let signing = tracing::span!(
        target: "aws_sigv4::http_request::sign",
        tracing::Level::DEBUG,
        "signing",
        bucket = SENTINEL_BUCKET,
        access_key_id = SENTINEL_ACCESS_KEY_ID,
    );
    let orchestrating = tracing::span!(
        target: "aws_sdk_s3::client::orchestrator",
        tracing::Level::TRACE,
        "orchestrating",
        key = SENTINEL_KEY,
        signed_url = SENTINEL_URL,
    );
    let signing_guard = signing.enter();
    let orchestrating_guard = orchestrating.enter();
    tracing::debug!(
        target: "aws_smithy_runtime::client::http::hyper_014",
        "{PROVIDER_DEBUG_MARK} nested inside provider spans"
    );
    tracing::debug!(target: APP_TARGET, "{APP_DEBUG_MARK} inside provider spans");
    drop(orchestrating_guard);
    drop(signing_guard);

    // Provider WARN stays: it is the operator's only provider-side signal, and
    // closing DEBUG/TRACE is all this boundary claims to do (gap OBS-W4-07 for
    // WARN/ERROR payload redaction stays open).
    tracing::warn!(
        target: "aws_sdk_s3::client::orchestrator",
        "{PROVIDER_WARN_MARK} retryable provider failure"
    );

    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();
}

/// The child half of every case. In the parent run this test does nothing.
#[test]
fn child_entry_point() {
    let Some(name) = std::env::var_os(CHILD_CASE) else {
        return;
    };
    let spec = case(name.to_string_lossy().as_ref());
    let format = match spec.log_format {
        "json" => observability::LogFormat::Json,
        _ => observability::LogFormat::Text,
    };
    if let Some(endpoint) = spec.otlp_endpoint {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for the OTLP case");
        runtime.block_on(async move {
            let guard = observability::init_tracing(
                "plan0014-child",
                spec.log_level,
                format,
                Some(endpoint),
            )
            .expect("init_tracing must succeed with an OTLP endpoint");
            emit_provider_shaped_records();
            drop(guard);
        });
    } else {
        let guard = observability::init_tracing("plan0014-child", spec.log_level, format, None)
            .expect("init_tracing must succeed without an exporter");
        emit_provider_shaped_records();
        drop(guard);
    }
}

/// Nothing a provider puts at DEBUG/TRACE may reach any sink.
fn assert_no_provider_leak(output: &str, name: &str) {
    let spec = case(name);
    for sentinel in [
        SENTINEL_BUCKET,
        SENTINEL_KEY,
        SENTINEL_ACCESS_KEY_ID,
        SENTINEL_SECRET_ACCESS_KEY,
        SENTINEL_SIGNATURE,
        SENTINEL_URL,
        PROVIDER_DEBUG_MARK,
        PROVIDER_TRACE_MARK,
    ] {
        assert!(
            !output.contains(sentinel),
            "case {name} ({}): provider material reached the log pipeline; \
             the output contains {sentinel:?}\n--- child output ---\n{output}\n--- end ---",
            spec.why
        );
    }
    for field in PROVIDER_FIELD_NAMES {
        assert!(
            !output.contains(field),
            "case {name} ({}): provider field name {field:?} reached the log pipeline, \
             so a provider record was emitted\n--- child output ---\n{output}\n--- end ---",
            spec.why
        );
    }
}

fn assert_application_diagnostics_survive(output: &str, name: &str) {
    let spec = case(name);
    assert!(
        output.contains(APP_DEBUG_MARK),
        "case {name} ({}): the application's own DEBUG output was lost\n--- child output ---\n{output}\n--- end ---",
        spec.why
    );
    assert!(
        output.contains(APP_INFO_MARK),
        "case {name} ({}): the application's own INFO output was lost\n--- child output ---\n{output}\n--- end ---",
        spec.why
    );
    assert!(
        output.contains("plan0014-synth-request"),
        "case {name} ({}): a safe application field was lost\n--- child output ---\n{output}\n--- end ---",
        spec.why
    );
}

fn assert_provider_warn_survives(output: &str, name: &str) {
    let spec = case(name);
    assert!(
        output.contains(PROVIDER_WARN_MARK),
        "case {name} ({}): provider WARN diagnostics were suppressed; this \
         boundary only closes DEBUG/TRACE\n--- child output ---\n{output}\n--- end ---",
        spec.why
    );
}

/// The whole table, one property per test, so a regression names the case that
/// reopened the leak.
fn for_each_case(mut check: impl FnMut(&'static Case, &str)) {
    for spec in CASES {
        check(spec, &run_case(spec.name));
    }
}

#[test]
fn no_case_leaks_provider_debug_or_trace_material() {
    for_each_case(|spec, output| assert_no_provider_leak(output, spec.name));
}

#[test]
fn no_case_loses_the_application_own_debug_output() {
    for_each_case(|spec, output| assert_application_diagnostics_survive(output, spec.name));
}

#[test]
fn no_case_silences_provider_warn_diagnostics() {
    for_each_case(|spec, output| assert_provider_warn_survives(output, spec.name));
}

#[test]
fn rust_log_unset_debug_service_level_does_not_leak() {
    let name = "text-rust-log-unset-debug-level";
    let output = run_case(name);
    assert_no_provider_leak(&output, name);
    assert_application_diagnostics_survive(&output, name);
}

#[test]
fn rust_log_debug_does_not_leak() {
    let name = "text-rust-log-debug";
    let output = run_case(name);
    assert_no_provider_leak(&output, name);
    assert_application_diagnostics_survive(&output, name);
}

#[test]
fn rust_log_trace_does_not_leak() {
    let name = "text-rust-log-trace";
    let output = run_case(name);
    assert_no_provider_leak(&output, name);
    assert_application_diagnostics_survive(&output, name);
}

#[test]
fn more_specific_provider_subtarget_directives_do_not_leak() {
    let name = "text-rust-log-specific-provider-subtargets";
    let output = run_case(name);
    assert_no_provider_leak(&output, name);
}

#[test]
fn provider_span_directives_do_not_leak() {
    let name = "text-rust-log-span-directive";
    let output = run_case(name);
    assert_no_provider_leak(&output, name);
}

#[test]
fn json_format_does_not_change_the_boundary() {
    for name in ["json-rust-log-unset-debug-level", "json-rust-log-debug"] {
        let output = run_case(name);
        assert_no_provider_leak(&output, name);
        assert_application_diagnostics_survive(&output, name);
        // Prove the child really emitted JSON, so this is not a text run.
        assert!(
            output.contains("\"fields\""),
            "case {name} was expected to emit JSON records\n--- child output ---\n{output}\n--- end ---"
        );
    }
}

#[test]
fn otlp_branch_keeps_the_same_boundary_for_span_output() {
    let name = "text-rust-log-debug-with-otlp";
    let output = run_case(name);
    assert_no_provider_leak(&output, name);
    assert_application_diagnostics_survive(&output, name);
}

/// Guard against this suite silently degenerating into a no-op: if the child
/// never produced output, every "must not contain" assertion above would pass.
#[test]
fn child_processes_actually_emit_the_application_records() {
    let output = run_case("text-rust-log-unset-debug-level");
    assert!(
        output.contains(APP_DEBUG_MARK) && output.contains(PROVIDER_WARN_MARK),
        "the child harness itself is broken\n--- child output ---\n{output}\n--- end ---"
    );
}
