#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 Round 2 — a real AWS SDK operation, run through the
//! production observability initialisation, must not put storage references or
//! credential material into the log stream.
//!
//! `tests/log_hygiene.rs` proves that `object_storage` itself never emits a
//! storage reference. It installs a capture-everything subscriber on purpose,
//! which is also why it cannot say anything about the *third-party* records it
//! captures: that subscriber bypasses every filter, so it sees exactly what the
//! SDK writes. This file closes the other half of AGENTS §10 by putting the
//! same SDK calls behind `observability::init_tracing` — the function every
//! composition root calls — and asserting negatively: the synthetic bucket,
//! object key, credential identifier and signed URL below may not appear in the
//! process output at any log level an operator can configure.
//!
//! No network and no certificate store: the endpoint is unroutable
//! (`127.0.0.1:1`, plain HTTP), so the SDK resolves the endpoint, signs the
//! request, logs it, and only then fails on connect. Everything under test
//! happens before the socket, and nothing here depends on the host trust store,
//! which a restricted container may not be able to read.
//!
//! Isolation: `init_tracing` installs a process-global subscriber and reads
//! `RUST_LOG` from the process environment, so each case runs in a fresh child
//! process of this test binary — the same harness pattern as
//! `crates/observability/tests/provider_log_safety.rs`.
//!
//! All values below are synthetic and exist nowhere.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::process::Command;

use bytes::Bytes;
use futures_util::stream;
use object_storage::{ObjectKey, ObjectStorageClient, S3Client};

const CHILD_CASE: &str = "PLAN0014_SDK_LOG_CHILD_CASE";

const SYNTH_ACCESS_KEY_ID: &str = "AKIASYNTHETICPLAN0014";
const SYNTH_SECRET_ACCESS_KEY: &str = "synthsecretplan0014donotlog";
const SYNTH_BUCKET: &str = "plan0014-synthetic-bucket";
const SYNTH_KEY: &str =
    "tenants/plan0014-synth-tenant/documents/plan0014-synth-doc/revisions/plan0014-synth-rev/source.bin";
/// A distinctive segment of the key, asserted on its own so a partial path in
/// the output is still a failure.
const SYNTH_KEY_SEGMENT: &str = "plan0014-synth-rev";

/// The SDK's own signed-URL and authorization vocabulary. A pipeline that drops
/// the record cannot produce any of these. None of them may appear in a
/// non-control child's output, not even as a field name.
const SDK_LEAK_MARKERS: &[&str] = &[
    "X-Amz-Signature",
    "X-Amz-Credential",
    "X-Amz-Security-Token",
    "authorization",
    "access_key_id",
    "secret_access_key",
];

/// The application's own DEBUG line from `S3Client::put_stream`. Keeping it is
/// the other half of the requirement: closing OBS-W4-02 must not turn into
/// "no diagnostics at all".
const OWN_DEBUG_MESSAGE: &str = "streaming upload to object storage";

/// Emitted by the child after each port operation, so the parent can tell a
/// boundary that worked apart from a child that did nothing. Deliberately free
/// of the vocabulary in [`SDK_LEAK_MARKERS`].
const OPERATION_MARK: &str = "PLAN0014-SDK-OP";

struct Case {
    name: &'static str,
    /// `RUST_LOG` for the child, or `None` to remove it from the environment.
    rust_log: Option<&'static str>,
    log_level: &'static str,
    log_format: &'static str,
    /// The negative control: the same records with no safety boundary. It must
    /// leak, which is what stops every assertion below being satisfied by an
    /// SDK that happened to log nothing.
    without_boundary: bool,
    why: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "text-rust-log-unset-debug-level",
        rust_log: None,
        log_level: "debug",
        log_format: "text",
        without_boundary: false,
        why: "no RUST_LOG, service level debug",
    },
    Case {
        name: "text-rust-log-debug",
        rust_log: Some("debug"),
        log_level: "info",
        log_format: "text",
        without_boundary: false,
        why: "RUST_LOG=debug replaces the whole filter",
    },
    Case {
        name: "text-rust-log-trace",
        rust_log: Some("trace"),
        log_level: "info",
        log_format: "text",
        without_boundary: false,
        why: "RUST_LOG=trace",
    },
    Case {
        name: "text-rust-log-provider-subtargets",
        rust_log: Some(
            "warn,aws_sdk_s3=trace,aws_smithy_runtime=trace,aws_smithy_http_client=trace,\
             aws_sigv4=trace,hyper=trace,h2=trace,rustls=trace,object_storage=debug",
        ),
        log_level: "info",
        log_format: "text",
        without_boundary: false,
        why: "every provider target explicitly turned up to trace",
    },
    Case {
        name: "json-rust-log-debug",
        rust_log: Some("debug"),
        log_level: "info",
        log_format: "json",
        without_boundary: false,
        why: "json output must be filtered the same way",
    },
    Case {
        name: "control-no-boundary-trace",
        rust_log: Some("trace"),
        log_level: "trace",
        log_format: "text",
        without_boundary: true,
        why: "control: without the boundary the same SDK call must leak",
    },
];

fn case(name: &str) -> &'static Case {
    CASES
        .iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("unknown case {name}"))
}

fn run_case(name: &str) -> String {
    let spec = case(name);
    let mut command = Command::new(std::env::current_exe().expect("current exe"));
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

/// One pass over the port against an unroutable endpoint.
///
/// Kept to one network operation on purpose: the SDK retries a refused
/// connection with backoff, and the signing/endpoint records that matter are
/// emitted by any operation. The full port surface is exercised by the real
/// provider lane (`tests/s3_contract_lane.rs`).
async fn drive_the_port() {
    let client = S3Client::new(
        "http://127.0.0.1:1",
        SYNTH_ACCESS_KEY_ID,
        SYNTH_SECRET_ACCESS_KEY,
        SYNTH_BUCKET,
        "us-east-1",
    );
    let key = ObjectKey::new(SYNTH_KEY).expect("synthetic key must be valid");
    let mut metadata = BTreeMap::new();
    metadata.insert("sha256".to_string(), "0".repeat(64));

    let payload: &[u8] = b"synthetic payload";
    let chunks = stream::iter([Ok(Bytes::from_static(payload))]);
    let put = client
        .put_stream(
            &key,
            Box::pin(chunks),
            payload.len() as u64,
            "application/octet-stream",
            &metadata,
        )
        .await;
    assert!(put.is_err(), "the unroutable endpoint must fail the upload");
    println!("{OPERATION_MARK} put_stream");

    // A presigned URL is produced locally. It carries the bucket, the object
    // key, the credential identifier and a signature: exactly the material
    // AGENTS §10 keeps out of logs. Asserting its shape here, without printing
    // it, proves the negative assertions have something to catch — otherwise
    // "no leak" could just mean "nothing was produced".
    let url = client
        .presign(&key, 300)
        .await
        .expect("presigning must succeed against a configured endpoint");
    assert!(
        url.contains(SYNTH_BUCKET)
            && url.contains(SYNTH_KEY)
            && url.contains(SYNTH_ACCESS_KEY_ID)
            && url.contains("X-Amz-Signature"),
        "the synthetic presigned URL must itself carry the material under test, \
         otherwise this suite measures nothing"
    );
    println!("{OPERATION_MARK} presign");

    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();
}

/// The child half of every case; a no-op in the parent run.
#[test]
fn child_entry_point() {
    let Some(name) = std::env::var_os(CHILD_CASE) else {
        return;
    };
    let spec = case(name.to_string_lossy().as_ref());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime for the SDK case");
    if spec.without_boundary {
        // The control: a plain fmt subscriber with the most permissive filter
        // and no safety boundary. If this stops leaking, the SDK no longer
        // writes this material and the assertions in the other cases would pass
        // for the wrong reason.
        let filter = tracing_subscriber::EnvFilter::new("trace");
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .init();
        runtime.block_on(drive_the_port());
    } else {
        let format = match spec.log_format {
            "json" => observability::LogFormat::Json,
            _ => observability::LogFormat::Text,
        };
        let guard =
            observability::init_tracing("plan0014-sdk-log-hygiene", spec.log_level, format, None)
                .expect("init_tracing must succeed");
        runtime.block_on(drive_the_port());
        drop(guard);
    }
}

/// The sentinels must stay distinguishable, or a "no leak" assertion could
/// match a value that is allowed to appear.
#[test]
fn sentinels_do_not_contain_each_other() {
    let sentinels = [
        SYNTH_BUCKET,
        SYNTH_KEY,
        SYNTH_ACCESS_KEY_ID,
        SYNTH_SECRET_ACCESS_KEY,
    ];
    for sentinel in sentinels {
        for other in sentinels {
            if sentinel != other {
                assert!(
                    !sentinel.contains(other),
                    "sentinel {sentinel} contains the distinct sentinel {other}"
                );
            }
        }
    }
    // The harness' own markers must not collide with the vocabulary they look
    // for, or the test would report its own output as a leak.
    for marker in SDK_LEAK_MARKERS {
        assert!(
            !OPERATION_MARK.contains(marker) && !OWN_DEBUG_MESSAGE.contains(marker),
            "the harness emits {marker:?} itself"
        );
    }
}

fn assert_no_leak(output: &str, name: &str) {
    let spec = case(name);
    for sentinel in [
        SYNTH_BUCKET,
        SYNTH_KEY,
        SYNTH_KEY_SEGMENT,
        SYNTH_ACCESS_KEY_ID,
        SYNTH_SECRET_ACCESS_KEY,
    ] {
        assert!(
            !output.contains(sentinel),
            "case {name} ({}): a storage reference or credential identifier \
             ({sentinel:?}) reached the log pipeline\n--- child output ---\n{output}\n--- end ---",
            spec.why
        );
    }
    for marker in SDK_LEAK_MARKERS {
        assert!(
            !output.contains(marker),
            "case {name} ({}): the log pipeline carries the SDK marker {marker:?}, \
             so a provider record was emitted\n--- child output ---\n{output}\n--- end ---",
            spec.why
        );
    }
}

fn assert_positive_controls(output: &str, name: &str) {
    let spec = case(name);
    for operation in ["put_stream", "presign"] {
        assert!(
            output.contains(&format!("{OPERATION_MARK} {operation}")),
            "case {name} ({}): the port operation {operation} never ran, so the \
             negative assertions would pass vacuously\n--- child output ---\n{output}\n--- end ---",
            spec.why
        );
    }
    assert!(
        output.contains(OWN_DEBUG_MESSAGE),
        "case {name} ({}): the application's own DEBUG diagnostic was lost; the \
         boundary must not take the crate's own logging with it\n--- child output ---\n{output}\n--- end ---",
        spec.why
    );
}

/// Every configured case in one pass: each child is a real SDK round trip, so
/// cases are not run once per property.
#[test]
fn every_configured_case_keeps_storage_references_out_of_the_log_stream() {
    for spec in CASES {
        let output = run_case(spec.name);
        if spec.without_boundary {
            assert!(
                output.contains(SYNTH_BUCKET)
                    && output.contains(SYNTH_KEY_SEGMENT)
                    && output.contains(SYNTH_ACCESS_KEY_ID),
                "the control case must leak: without the safety boundary the SDK \
                 is expected to log the bucket, the key and the credential \
                 identifier. If it no longer does, the other cases in this file \
                 have stopped proving anything.\n--- child output ---\n{output}\n--- end ---"
            );
            assert_positive_controls(&output, spec.name);
            continue;
        }
        assert_no_leak(&output, spec.name);
        assert_positive_controls(&output, spec.name);
        if spec.log_format == "json" {
            assert!(
                output.contains("\"fields\""),
                "case {} did not emit JSON\n--- child output ---\n{output}\n--- end ---",
                spec.why
            );
        }
    }
}

/// Named for the review: the specific operator action that reopened OBS-W4-02
/// in Round 1 was setting `RUST_LOG` to a more permissive value.
#[test]
fn turning_every_provider_target_to_trace_does_not_leak() {
    let name = "text-rust-log-provider-subtargets";
    let output = run_case(name);
    assert_no_leak(&output, name);
    assert_positive_controls(&output, name);
}
