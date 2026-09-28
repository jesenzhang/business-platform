//! PLAN-0014 Wave 4 - storage references must not enter the log stream.
//!
//! `S3Client::put_stream` put the bucket name on its tracing span and logged the
//! full object key at DEBUG. AGENTS §10 and the PLAN-0003/PLAN-0004 rules keep
//! object keys, bucket names and storage paths out of logs and traces, so the
//! crate that owns those values must never emit them.
//!
//! Wave 4 gaps ST-W4-02 (key/bucket in logs) - failing-first reproduction.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};

use bytes::Bytes;
use futures_util::stream;
use object_storage::{ObjectKey, ObjectStorageClient, S3Client, StorageError};
use tracing::field::{Field, Visit};
use tracing::metadata::Metadata;
use tracing::span::{Attributes, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Subscriber};

#[derive(Clone, Default)]
struct Captured {
    target: String,
    name: String,
    fields: Vec<(String, String)>,
}

impl fmt::Display for Captured {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fields = self
            .fields
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        write!(formatter, "{}::{} {fields}", self.target, self.name)
    }
}

#[derive(Default)]
struct CollectingVisitor {
    fields: Vec<(String, String)>,
}

impl Visit for CollectingVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

#[derive(Default)]
struct CapturingSubscriber {
    captured: Arc<Mutex<Vec<Captured>>>,
    next_id: AtomicU64,
}

impl CapturingSubscriber {
    fn capture(&self, target: &str, name: &str, visitor: CollectingVisitor) {
        let mut guard = self
            .captured
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.push(Captured {
            target: target.to_string(),
            name: name.to_string(),
            fields: visitor.fields,
        });
    }
}

impl Subscriber for CapturingSubscriber {
    fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
        Interest::always()
    }

    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &Attributes<'_>) -> tracing::span::Id {
        let mut visitor = CollectingVisitor::default();
        span.record(&mut visitor);
        let metadata = span.metadata();
        self.capture(metadata.target(), metadata.name(), visitor);
        tracing::span::Id::from_u64(self.next_id.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut visitor = CollectingVisitor::default();
        event.record(&mut visitor);
        let metadata = event.metadata();
        self.capture(metadata.target(), metadata.name(), visitor);
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

#[allow(clippy::expect_used)]
fn install() -> Arc<Mutex<Vec<Captured>>> {
    static INIT: Once = Once::new();
    let subscriber = CapturingSubscriber::default();
    let captured = Arc::clone(&subscriber.captured);
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
    captured
}

fn our_records(captured: &Arc<Mutex<Vec<Captured>>>) -> Vec<Captured> {
    let guard = captured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .iter()
        .filter(|record| record.target.starts_with("object_storage"))
        .cloned()
        .collect()
}

fn foreign_records(captured: &Arc<Mutex<Vec<Captured>>>) -> Vec<Captured> {
    let guard = captured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .iter()
        .filter(|record| !record.target.starts_with("object_storage"))
        .cloned()
        .collect()
}

/// The object storage crate must not put object keys or bucket names into any
/// span or event field it emits itself.
#[tokio::test]
#[allow(clippy::expect_used)]
async fn storage_references_never_enter_the_log_stream() {
    let captured = install();
    // Unroutable port: the operation must fail, but the span and the debug
    // event are recorded before the request leaves the process.
    let client = S3Client::new(
        "http://127.0.0.1:1",
        "wave4-access",
        "wave4-secret",
        "plan0014-wave4-bucket",
        "us-east-1",
    );
    let key = ObjectKey::new("tenants/tenant-1/documents/doc-1/revisions/rev-1/source")
        .expect("valid key");
    let chunks = stream::iter([Ok(Bytes::from_static(b"payload"))]);
    let result: Result<(), StorageError> = client
        .put_stream(
            &key,
            Box::pin(chunks),
            7,
            "application/octet-stream",
            &BTreeMap::new(),
        )
        .await;
    assert!(
        result.is_err(),
        "the upload must fail against a dead endpoint"
    );

    let ours = our_records(&captured);
    assert!(
        !ours.is_empty(),
        "the object storage crate must emit at least one record for this path"
    );
    for record in &ours {
        let rendered = record.to_string();
        for secret in ["tenants/", "revisions/", "plan0014-wave4-bucket", "source"] {
            assert!(
                !rendered.contains(secret),
                "object storage emitted a storage reference: {rendered}"
            );
        }
    }

    // This subscriber captures everything on purpose, including the third-party
    // records, so it can say nothing about whether they *would* be logged in
    // production: it bypasses every filter. What it can say is that the capture
    // really did see the provider's own records, which is what makes the
    // assertion above non-vacuous — and it is the reason the SDK material has to
    // be dropped at the observability boundary rather than trusted to the SDK.
    // The negative assertion over those third-party records, including the
    // bucket, object key, credential identifier and signed URL, is
    // `tests/sdk_log_hygiene.rs`, which runs the same port through the
    // production `observability::init_tracing`.
    let foreign = foreign_records(&captured);
    assert!(
        !foreign.is_empty(),
        "the capture saw no third-party records at all, so the provider-side \
         premise of this test would be unverified"
    );
    let leaking: Vec<String> = foreign
        .iter()
        .map(ToString::to_string)
        .filter(|rendered| {
            rendered.contains("tenants/") || rendered.contains("plan0014-wave4-bucket")
        })
        .collect();
    println!(
        "third-party records captured ({}), of which {} carry a storage reference \
         that the observability boundary must drop: {}",
        foreign.len(),
        leaking.len(),
        if leaking.is_empty() {
            "none on this SDK version".to_string()
        } else {
            "recorded; see tests/sdk_log_hygiene.rs for the assertion".to_string()
        }
    );
}
