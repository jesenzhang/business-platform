#![allow(dead_code)]
//! Shared harness for the real-provider S3 lane
//! (`tests/s3_contract_lane.rs`, `tests/s3_lane_cleanup.rs`,
//! `tests/s3_lane_fault_injection.rs`).
//!
//! PLAN-0014 Wave 4 Round 2 splits the lane into two explicit steps:
//!
//! * **contract execution** — exercises the provider-neutral port and records
//!   provider capabilities;
//! * **final cleanup** — removes everything this run created and asserts the
//!   run scope is empty.
//!
//! They are separate test targets because they must run as separate CI steps
//! with separate exit codes: a cleanup failure has to be reported as a cleanup
//! failure, and a cleanup success must not be able to mask a contract failure
//! (and vice versa). This module holds the provider plumbing both need.
//!
//! Scope discipline: cleanup only ever addresses keys under
//! `plan0014-wave4/<WAVE4_S3_RUN_ID>/`. The bucket itself is never deleted
//! unless the operator asks for it *and* the bucket is not on the protected
//! list; CI's shared `contract-test-bucket` is protected by default.

use std::collections::BTreeMap;
use std::process::Command;

use aws_sdk_s3::Client;
use bytes::Bytes;
use futures_util::stream;
use object_storage::{ObjectKey, ObjectStorageClient, S3Client};

/// Root for everything this lane writes into the bucket.
pub const PREFIX_ROOT: &str = "plan0014-wave4";
/// Root for objects the lane writes deliberately *outside* its own prefix, to
/// prove cleanup does not wander.
pub const SENTINEL_ROOT: &str = "plan0014-wave4-sentinel";
/// Buckets cleanup must never delete. `contract-test-bucket` is the bucket the
/// CI job provisions once and reuses across runs and jobs.
pub const PROTECTED_BUCKETS: &[&str] = &["contract-test-bucket"];

/// Marker a deliberately-failing child prints before it panics, so the parent
/// can tell "the injected fault happened" from "the harness broke".
pub const FAULT_ARMED: &str = "PLAN0014-FAULT-ARMED";

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for the S3 provider lane"))
}

pub fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// The run id, validated.
///
/// Round 1 accepted any value, including an empty one: with an empty run id the
/// prefix degenerates to `plan0014-wave4/`, i.e. *every* concurrent lane run's
/// scope, so one job's cleanup could delete another's evidence. An unset,
/// empty or whitespace-only run id therefore stops the lane.
#[allow(clippy::expect_used)]
pub fn run_id() -> String {
    let raw = env("WAVE4_S3_RUN_ID");
    let trimmed = raw.trim();
    assert!(
        !trimmed.is_empty(),
        "WAVE4_S3_RUN_ID must be a non-empty value: cleanup is scoped by it, and an \
         empty value would widen the scope to every lane run under {PREFIX_ROOT}/"
    );
    assert!(
        !trimmed.contains('/') && trimmed != "." && trimmed != "..",
        "WAVE4_S3_RUN_ID must be a single path segment, got {trimmed:?}"
    );
    trimmed.to_string()
}

/// The only prefix cleanup is allowed to delete under.
pub fn run_prefix() -> String {
    format!("{PREFIX_ROOT}/{}/", run_id())
}

/// Where the out-of-prefix sentinel objects live, keyed by run so parallel runs
/// do not overwrite each other's sentinel.
pub fn sentinel_prefix() -> String {
    format!("{SENTINEL_ROOT}/{}/", run_id())
}

pub fn bucket() -> String {
    env("WAVE4_S3_BUCKET")
}

pub fn key_under_prefix(suffix: &str) -> String {
    format!("{}{}", run_prefix(), suffix)
}

/// Page size for every cleanup listing. Deliberately small and configurable
/// (`WAVE4_S3_CLEANUP_PAGE_SIZE`, default 3) so the pagination path is the one
/// actually executed by a lane run, rather than the convenience path a provider
/// with a 1000-key default would hide behind.
#[allow(clippy::expect_used)]
pub fn page_size() -> i32 {
    optional_env("WAVE4_S3_CLEANUP_PAGE_SIZE").map_or(3, |value| {
        value
            .parse::<i32>()
            .unwrap_or_else(|_| panic!("WAVE4_S3_CLEANUP_PAGE_SIZE must be an integer"))
    })
}

/// Hard bound on listing pages: a provider that ignores the pagination
/// parameters must not turn cleanup into an infinite loop.
const MAX_PAGES: usize = 500;

#[allow(clippy::expect_used)]
pub fn sdk_client() -> Client {
    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
    let credentials = Credentials::new(
        env("WAVE4_S3_ACCESS_KEY"),
        env("WAVE4_S3_SECRET_KEY"),
        None,
        None,
        "provider-lane",
    );
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .endpoint_url(env("WAVE4_S3_ENDPOINT"))
        .region(Region::new(env("WAVE4_S3_REGION")))
        .credentials_provider(credentials)
        .force_path_style(true)
        .build();
    Client::from_conf(config)
}

#[allow(clippy::expect_used)]
pub fn port_client() -> S3Client {
    S3Client::new(
        &env("WAVE4_S3_ENDPOINT"),
        &env("WAVE4_S3_ACCESS_KEY"),
        &env("WAVE4_S3_SECRET_KEY"),
        &env("WAVE4_S3_BUCKET"),
        &env("WAVE4_S3_REGION"),
    )
}

/// Create the bucket when the operator pointed the lane at one that does not
/// exist yet. Never deletes one.
pub async fn ensure_bucket() {
    let sdk = sdk_client();
    let name = bucket();
    match sdk.create_bucket().bucket(&name).send().await {
        Ok(_) => println!("LANE created bucket {name}"),
        Err(error) => {
            let text = error.to_string();
            assert!(
                text.contains("BucketAlreadyOwnedByYou")
                    || text.contains("BucketAlreadyExists")
                    || text.contains("already exist"),
                "creating the bucket {name} failed: {text}"
            );
        }
    }
}

pub async fn put_object(client: &S3Client, key: &str, body: &[u8]) {
    let object_key = ObjectKey::new(key).expect("valid key");
    let chunks = stream::iter([Ok(Bytes::from(copy_of(body)))]);
    let mut metadata = BTreeMap::new();
    metadata.insert("sha256".to_string(), "0".repeat(64));
    client
        .put_stream(
            &object_key,
            Box::pin(chunks),
            body.len() as u64,
            "application/octet-stream",
            &metadata,
        )
        .await
        .unwrap_or_else(|error| panic!("put_stream must succeed against a real provider: {error}"));
}

fn copy_of(body: &[u8]) -> Vec<u8> {
    body.to_vec()
}

/// Write `count` small objects under `prefix`. Callers pass more than
/// [`page_size`] so the cleanup path that has to follow a continuation token is
/// the path under test.
pub async fn send_probe_objects(client: &S3Client, prefix: &str, label: &str, count: usize) {
    for index in 0..count {
        let key = format!("{prefix}{label}-{index:03}.bin");
        put_object(client, &key, format!("probe {label} {index}").as_bytes()).await;
    }
}

/// More than one page of objects, by construction.
#[must_use]
pub fn more_than_a_page() -> usize {
    usize::try_from(page_size()).unwrap_or(3) * 2
}

/// Create a multipart upload under `prefix` and leave it incomplete, the way an
/// interrupted client does. Returns the key and upload id.
pub async fn leave_an_incomplete_upload(sdk: &Client, key: &str) -> (String, String) {
    let created = sdk
        .create_multipart_upload()
        .bucket(bucket())
        .key(key)
        .send()
        .await
        .expect("CreateMultipartUpload must be supported by a provider in this lane");
    let upload_id = created
        .upload_id
        .clone()
        .expect("the provider returns an upload id");
    (key.to_owned(), upload_id)
}

/// Every key under `prefix`, following continuation tokens.
///
/// A single `ListObjectsV2` call returns at most 1000 keys and the lane used to
/// assume that was the whole answer: with more than one page, cleanup deleted
/// the first page and still reported a clean prefix.
pub async fn list_all_keys(
    sdk: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    let mut token: Option<String> = None;
    for page in 0..MAX_PAGES {
        let mut request = sdk
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .max_keys(page_size());
        if let Some(token) = token.as_ref() {
            request = request.continuation_token(token);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("list_objects_v2 page {page} under {prefix}: {error}"))?;
        keys.extend(
            response
                .contents()
                .iter()
                .filter_map(|object| object.key().map(str::to_owned)),
        );
        token = response.next_continuation_token().map(str::to_owned);
        if token.is_none() || response.is_truncated() != Some(true) {
            return Ok(keys);
        }
    }
    Err(format!("listing {prefix} exceeded {MAX_PAGES} pages"))
}

/// Every incomplete multipart upload under `prefix`, following markers.
///
/// An incomplete upload is invisible to `ListObjectsV2` and keeps consuming
/// capacity forever; a bucket policy lifecycle rule is the usual remedy, and
/// the lane cannot rely on one existing.
pub async fn list_all_incomplete_uploads(
    sdk: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut uploads = Vec::new();
    let mut key_marker: Option<String> = None;
    let mut upload_id_marker: Option<String> = None;
    for page in 0..MAX_PAGES {
        let mut request = sdk
            .list_multipart_uploads()
            .bucket(bucket)
            .prefix(prefix)
            .max_uploads(page_size());
        if let Some(marker) = key_marker.as_ref() {
            request = request.key_marker(marker);
        }
        if let Some(marker) = upload_id_marker.as_ref() {
            request = request.upload_id_marker(marker);
        }
        let response = request.send().await.map_err(|error| {
            format!("list_multipart_uploads page {page} under {prefix}: {error}")
        })?;
        for upload in response.uploads() {
            match (upload.key(), upload.upload_id()) {
                (Some(key), Some(upload_id)) => {
                    uploads.push((key.to_owned(), upload_id.to_owned()));
                }
                _ => {
                    return Err(format!(
                        "provider returned an upload without key or id under {prefix}"
                    ));
                }
            }
        }
        if response.is_truncated() != Some(true) {
            return Ok(uploads);
        }
        let next_key = response
            .next_key_marker()
            .or(response.uploads().last().and_then(|upload| upload.key()))
            .map(str::to_owned);
        let next_upload = response
            .next_upload_id_marker()
            .or(response
                .uploads()
                .last()
                .and_then(|upload| upload.upload_id()))
            .map(str::to_owned);
        match (next_key, next_upload) {
            (Some(key), Some(upload_id)) if Some(&key) != key_marker.as_ref() => {
                key_marker = Some(key);
                upload_id_marker = Some(upload_id);
            }
            _ => {
                return Err(format!(
                    "provider truncated the upload listing under {prefix} without advancing the marker"
                ));
            }
        }
    }
    Err(format!(
        "listing incomplete uploads under {prefix} exceeded {MAX_PAGES} pages"
    ))
}

/// What one cleanup pass did, and what it could not do.
#[derive(Debug, Default)]
pub struct CleanupReport {
    pub prefix: String,
    pub objects_deleted: usize,
    pub uploads_aborted: usize,
    pub remaining_objects: Vec<String>,
    pub remaining_uploads: Vec<(String, String)>,
    /// Failures are collected, never swallowed: a provider that refuses a
    /// delete has to surface as a cleanup failure, not as a green step.
    pub errors: Vec<String>,
    pub injected_fault: bool,
}

impl CleanupReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
            && self.remaining_objects.is_empty()
            && self.remaining_uploads.is_empty()
            && !self.injected_fault
    }

    pub fn render(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::new();
        let _ = writeln!(
            text,
            "CLEANUP prefix={} objects_deleted={} uploads_aborted={} remaining_objects={} \
             remaining_uploads={} errors={}",
            self.prefix,
            self.objects_deleted,
            self.uploads_aborted,
            self.remaining_objects.len(),
            self.remaining_uploads.len(),
            self.errors.len(),
        );
        for error in &self.errors {
            let _ = writeln!(text, "CLEANUP ERROR: {error}");
        }
        for key in &self.remaining_objects {
            let _ = writeln!(text, "CLEANUP RESIDUE OBJECT: {key}");
        }
        for (key, upload_id) in &self.remaining_uploads {
            let _ = writeln!(text, "CLEANUP RESIDUE UPLOAD: {key}#{upload_id}");
        }
        if self.injected_fault {
            let _ = writeln!(
                text,
                "CLEANUP INJECTED FAULT: WAVE4_S3_CLEANUP_SIMULATE_FAILURE=true"
            );
        }
        text
    }
}

/// Delete everything this run created under its own prefix, then verify the
/// scope is empty.
///
/// Returns a report instead of panicking so the caller can print the full
/// picture before deciding; [`report_and_fail`] is the usual ending.
pub async fn cleanup_run_prefix() -> CleanupReport {
    let report = run_cleanup().await;
    print!("{}", report.render());
    report
}

async fn run_cleanup() -> CleanupReport {
    let mut report = CleanupReport {
        prefix: run_prefix(),
        injected_fault: optional_env("WAVE4_S3_CLEANUP_SIMULATE_FAILURE").as_deref()
            == Some("true"),
        ..Default::default()
    };
    let sdk = sdk_client();
    let bucket = bucket();
    let prefix = report.prefix.clone();

    let uploads = match list_all_incomplete_uploads(&sdk, &bucket, &prefix).await {
        Ok(uploads) => uploads,
        Err(error) => {
            report.errors.push(error);
            Vec::new()
        }
    };
    for (key, upload_id) in uploads {
        match sdk
            .abort_multipart_upload()
            .bucket(&bucket)
            .key(&key)
            .upload_id(&upload_id)
            .send()
            .await
        {
            Ok(_) => report.uploads_aborted += 1,
            Err(error) => report
                .errors
                .push(format!("abort_multipart_upload {key}#{upload_id}: {error}")),
        }
    }

    let keys = match list_all_keys(&sdk, &bucket, &prefix).await {
        Ok(keys) => keys,
        Err(error) => {
            report.errors.push(error);
            Vec::new()
        }
    };
    for key in keys {
        match sdk.delete_object().bucket(&bucket).key(&key).send().await {
            Ok(_) => report.objects_deleted += 1,
            Err(error) => report.errors.push(format!("delete_object {key}: {error}")),
        }
    }

    // Verify from the provider, not from the delete calls that were issued.
    match list_all_keys(&sdk, &bucket, &prefix).await {
        Ok(keys) => report.remaining_objects = keys,
        Err(error) => report.errors.push(error),
    }
    match list_all_incomplete_uploads(&sdk, &bucket, &prefix).await {
        Ok(uploads) => report.remaining_uploads = uploads,
        Err(error) => report.errors.push(error),
    }
    report
}

/// Print the report and fail the step if anything is left or anything failed.
pub fn report_and_fail(report: &CleanupReport) {
    print!("{}", report.render());
    assert!(
        report.ok(),
        "final cleanup did not leave the run scope empty for prefix {}: {} object(s), \
         {} incomplete upload(s), {} error(s). Cleanup failures are reported as their own \
         failure and are never folded into a capability observation.",
        report.prefix,
        report.remaining_objects.len(),
        report.remaining_uploads.len(),
        report.errors.len(),
    );
}

/// Refuse a bucket teardown that targets a shared bucket.
///
/// The guard runs *before* cleanup touches anything, not after: an operator who
/// asked to delete a bucket CI depends on should be refused before a single
/// object is removed, not at the end of an otherwise successful pass.
pub fn refuse_protected_bucket_deletion() {
    if optional_env("WAVE4_S3_DELETE_BUCKET").as_deref() != Some("true") {
        return;
    }
    let name = bucket();
    assert!(
        !PROTECTED_BUCKETS.contains(&name.as_str()),
        "refusing to delete bucket {name}: it is on the protected list \
         ({PROTECTED_BUCKETS:?}) that CI shares across jobs"
    );
}

/// Delete the bucket itself — only on explicit request, and never for a bucket
/// on the protected list. CI reuses one provisioned bucket across jobs, so the
/// default must be "never delete the bucket".
pub async fn maybe_delete_bucket() {
    refuse_protected_bucket_deletion();
    if optional_env("WAVE4_S3_DELETE_BUCKET").as_deref() != Some("true") {
        println!("CLEANUP bucket deletion not requested (WAVE4_S3_DELETE_BUCKET != true)");
        return;
    }
    let name = bucket();
    let sdk = sdk_client();
    sdk.delete_bucket()
        .bucket(&name)
        .send()
        .await
        .unwrap_or_else(|error| panic!("deleting bucket {name} failed: {error}"));
    println!("CLEANUP deleted bucket {name}");
}

/// Write a sentinel object outside the run prefix. Cleanup must leave it alone.
pub async fn write_sentinel(body: &str) -> String {
    let key = format!("{}keep.txt", sentinel_prefix());
    let sdk = sdk_client();
    sdk.put_object()
        .bucket(bucket())
        .key(&key)
        .body(aws_smithy_types::byte_stream::ByteStream::from(
            body.as_bytes().to_vec(),
        ))
        .send()
        .await
        .unwrap_or_else(|error| panic!("writing the sentinel {key} failed: {error}"));
    key
}

pub async fn read_sentinel(key: &str) -> String {
    let sdk = sdk_client();
    let response = sdk
        .get_object()
        .bucket(bucket())
        .key(key)
        .send()
        .await
        .unwrap_or_else(|error| panic!("reading the sentinel {key} failed: {error}"));
    let body = response.body.collect().await.expect("sentinel body");
    String::from_utf8(body.to_vec()).expect("the sentinel is utf-8")
}

/// Spawn this same test binary with the given case, overriding the environment.
/// Used by the guard and fault tests, which need a *separate process* because
/// they assert on exit codes.
#[must_use]
pub struct ChildRun {
    pub name: String,
    pub code: Option<i32>,
    pub output: String,
}

impl ChildRun {
    pub fn succeeded(&self) -> bool {
        self.code == Some(0)
    }
}

#[allow(clippy::expect_used)]
pub fn spawn_case(name: &str, overrides: &[(&str, Option<&str>)]) -> ChildRun {
    let mut command = Command::new(std::env::current_exe().expect("current exe"));
    command
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .arg("--include-ignored")
        .arg(name);
    for (key, value) in overrides {
        match value {
            Some(value) => {
                command.env(key, value);
            }
            None => {
                command.env_remove(key);
            }
        }
    }
    let output = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("spawning the child test process must work");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    ChildRun {
        name: name.to_string(),
        code: output.status.code(),
        output: text,
    }
}
