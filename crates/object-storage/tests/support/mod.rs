#![allow(dead_code)]
//! Shared harness for the real-provider S3 lane
//! (`tests/s3_contract_lane.rs`, `tests/s3_lane_cleanup.rs`,
//! `tests/s3_lane_fault_injection.rs`) and for the deterministic pagination rules
//! (`tests/s3_lane_pagination.rs`).
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
//! Round 3 tightened what a listing in this harness is allowed to **claim**. A
//! page is only the next page if the provider said so **and** handed back a
//! cursor this scan has not already used; a listing that cannot prove it reached
//! the end returns an error instead of an empty remainder. That is not
//! fastidiousness: cleanup's whole value is the sentence "nothing was left
//! behind", and an unfinished listing has not earned it. See
//! [`next_upload_cursor`], [`next_object_token`] and
//! [`CleanupReport::listing_incomplete`].
//!
//! Scope discipline: cleanup only ever addresses keys under
//! `plan0014-wave4/<WAVE4_S3_RUN_ID>/`. The bucket itself is never deleted
//! unless the operator asks for it *and* the bucket is not on the protected
//! list; CI's shared `contract-test-bucket` is protected by default.

use std::collections::{BTreeMap, BTreeSet};
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
pub const MAX_LIST_PAGES: usize = 500;

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

/// Create `count` incomplete uploads on **one** key and report each one to the
/// parent on the structured channel.
///
/// One key with several uploads is the shape that forces a `(key, upload-id)`
/// cursor: the key marker has to repeat and only the upload-id moves. Creating
/// them on separate keys would test a pagination path the lane already walked.
///
/// Every created id is emitted as it is learned, before this function returns,
/// so a child that dies part way through still tells the parent what exists.
pub async fn leave_incomplete_uploads(sdk: &Client, key: &str, count: usize) -> Vec<CreatedUpload> {
    let mut created = Vec::with_capacity(count);
    for _ in 0..count {
        let response = sdk
            .create_multipart_upload()
            .bucket(bucket())
            .key(key)
            .send()
            .await
            .expect("CreateMultipartUpload must be supported by a provider in this lane");
        let upload = CreatedUpload {
            bucket: bucket(),
            run_id: run_id(),
            key: key.to_owned(),
            upload_id: response
                .upload_id
                .clone()
                .expect("the provider returns an upload id"),
        };
        emit_created_upload(&upload);
        created.push(upload);
    }
    created
}

/// Abort one known upload without going through the listing.
///
/// This is the compensation for the case the listing did not find: the parent
/// knows the key and id from the child's own record, so the upload need not be
/// left in the bucket just because discovery failed. It is deliberately **not**
/// a way to pass the scenario — aborting what you created says nothing about
/// whether a client could have found it.
pub async fn compensating_abort(upload: &CreatedUpload) -> Result<(), String> {
    sdk_client()
        .abort_multipart_upload()
        .bucket(&upload.bucket)
        .key(&upload.key)
        .upload_id(&upload.upload_id)
        .send()
        .await
        .map(|_| ())
        .map_err(|error| {
            format!(
                "abort_multipart_upload {}#{}: {error}",
                upload.key, upload.upload_id
            )
        })
}

/// One `ListMultipartUploads` cursor: the **pair** the next request must send.
///
/// S3 orders uploads by `(key, upload-id)`. A single key with more uploads than
/// one page therefore repeats its key marker and moves only the upload-id, which
/// is the normal, legal case — not a stalled scan. Round 2 compared the key
/// alone and rejected exactly that, and an opaque upload id cannot be ordered
/// locally as a substitute.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UploadCursor {
    pub key_marker: String,
    pub upload_id_marker: String,
}

impl UploadCursor {
    #[must_use]
    pub fn new(key_marker: &str, upload_id_marker: &str) -> Self {
        Self {
            key_marker: key_marker.to_owned(),
            upload_id_marker: upload_id_marker.to_owned(),
        }
    }
}

/// A marker half that is absent or blank is absent: neither can be sent.
fn marker_half(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Validate the cursor for the next page of `ListMultipartUploads`.
///
/// Returns `Ok(None)` only when the provider said the listing is complete. Every
/// other way a page can end — a truncated page with no usable pair, a pair that
/// was already used — is an error, because cleanup must never read a partial
/// listing as "there is nothing left to delete".
///
/// Both halves must come from the provider. Deriving a cursor from the page's
/// last upload instead would be the listing function inventing its own progress:
/// it cannot tell a provider that never sends markers apart from one that has
/// nothing more to give, and "the marker I made up repeats, so stop" would be
/// the scan's own bookkeeping deciding when the truth is unknown. `IsTruncated`
/// is the provider's answer to "is there more"; the cursor is its answer to "from
/// where". A scan that has to supply the second answer has not been told enough.
///
/// `seen` holds every cursor this scan has already used, so a provider that
/// repeats a cursor **or** cycles through a longer set is caught on the repeat
/// rather than after 500 pages.
pub fn next_upload_cursor(
    truncated: bool,
    next_key_marker: Option<&str>,
    next_upload_id_marker: Option<&str>,
    seen: &mut BTreeSet<UploadCursor>,
    prefix: &str,
    page: usize,
) -> Result<Option<UploadCursor>, String> {
    if !truncated {
        return Ok(None);
    }
    let key = marker_half(next_key_marker);
    let upload_id = marker_half(next_upload_id_marker);
    let Some(cursor) = key
        .zip(upload_id)
        .map(|(key, upload_id)| UploadCursor::new(key, upload_id))
    else {
        return Err(format!(
            "list_multipart_uploads page {page} under {prefix} reported IsTruncated=true \
             without a usable (key-marker, upload-id-marker) pair (key-marker {}, \
             upload-id-marker {}); the upload listing is incomplete and is not reported \
             as the whole answer",
            next_key_marker.unwrap_or("<absent>"),
            next_upload_id_marker.unwrap_or("<absent>")
        ));
    };
    if !seen.insert(cursor.clone()) {
        return Err(format!(
            "list_multipart_uploads page {page} under {prefix} returned a cursor this scan \
             had already used (key-marker {}, upload-id-marker {}); the upload listing is \
             cycling and is not reported as the whole answer",
            cursor.key_marker, cursor.upload_id_marker
        ));
    }
    Ok(Some(cursor))
}

/// Validate the continuation token for the next page of `ListObjectsV2`.
///
/// Stricter than the upload side on purpose. `IsTruncated` is the only thing
/// that says "there is more", and Round 2's real run proved this provider does
/// answer it for objects (its six-object cleanup needed two pages, so a page
/// carried `IsTruncated=true`). Requiring `Some(false)` to finish costs nothing
/// there and closes the hole where an unanswered question was read as "done".
/// The upload listing is not held to the same standard because Round 2 had one
/// upload — one page — and therefore produced no evidence either way about how
/// this provider answers `IsTruncated` for multipart uploads.
pub fn next_object_token(
    truncated: Option<bool>,
    next_token: Option<&str>,
    seen: &mut BTreeSet<String>,
    prefix: &str,
    page: usize,
) -> Result<Option<String>, String> {
    match truncated {
        None => Err(format!(
            "list_objects_v2 page {page} under {prefix} did not answer IsTruncated, so the \
             scan cannot be declared complete"
        )),
        Some(false) => Ok(None),
        Some(true) => {
            let Some(token) = marker_half(next_token) else {
                return Err(format!(
                    "list_objects_v2 page {page} under {prefix} reported IsTruncated=true but \
                     returned no continuation token; the object listing is incomplete and is \
                     not reported as the whole answer"
                ));
            };
            if !seen.insert(token.to_owned()) {
                return Err(format!(
                    "list_objects_v2 page {page} under {prefix} repeated continuation token \
                     {token:?}; the object listing is cycling and is not reported as the \
                     whole answer"
                ));
            }
            Ok(Some(token.to_owned()))
        }
    }
}

/// One full listing pass: what it found, and how many provider round trips it
/// took. The page count is part of the result because "this run walked three
/// pages" is the evidence a multi-page claim needs.
#[derive(Debug, Clone)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub pages: usize,
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
    Ok(list_all_keys_with_pages(sdk, bucket, prefix).await?.items)
}

/// [`list_all_keys`], also reporting how many pages it walked.
pub async fn list_all_keys_with_pages(
    sdk: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Listing<String>, String> {
    let mut keys = Vec::new();
    let mut token: Option<String> = None;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut pages = 0_usize;
    loop {
        if pages >= MAX_LIST_PAGES {
            return Err(format!(
                "listing {prefix} exceeded the {MAX_LIST_PAGES}-page bound; the object \
                 listing is incomplete and is not reported as the whole answer"
            ));
        }
        pages += 1;
        let page = pages;
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
        token = next_object_token(
            response.is_truncated(),
            response.next_continuation_token(),
            &mut seen,
            prefix,
            page,
        )?;
        if token.is_none() {
            return Ok(Listing { items: keys, pages });
        }
    }
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
    Ok(list_all_incomplete_uploads_with_pages(sdk, bucket, prefix)
        .await?
        .items)
}

/// [`list_all_incomplete_uploads`], also reporting how many pages it walked.
pub async fn list_all_incomplete_uploads_with_pages(
    sdk: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Listing<(String, String)>, String> {
    let mut uploads = Vec::new();
    let mut cursor: Option<UploadCursor> = None;
    let mut seen: BTreeSet<UploadCursor> = BTreeSet::new();
    let mut pages = 0_usize;
    loop {
        if pages >= MAX_LIST_PAGES {
            return Err(format!(
                "listing incomplete uploads under {prefix} exceeded the {MAX_LIST_PAGES}-page \
                 bound; the upload listing is incomplete and is not reported as the whole answer"
            ));
        }
        pages += 1;
        let page = pages;
        let mut request = sdk
            .list_multipart_uploads()
            .bucket(bucket)
            .prefix(prefix)
            .max_uploads(page_size());
        if let Some(cursor) = cursor.as_ref() {
            request = request
                .key_marker(&cursor.key_marker)
                .upload_id_marker(&cursor.upload_id_marker);
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
        // Every upload on this page carried both halves (checked above), so the
        // only thing left to ask is what the provider says comes next.
        cursor = next_upload_cursor(
            response.is_truncated() == Some(true),
            response.next_key_marker(),
            response.next_upload_id_marker(),
            &mut seen,
            prefix,
            page,
        )?;
        if cursor.is_none() {
            return Ok(Listing {
                items: uploads,
                pages,
            });
        }
    }
}

/// What one cleanup pass did, and what it could not do.
#[derive(Debug, Default)]
pub struct CleanupReport {
    pub prefix: String,
    pub objects_deleted: usize,
    pub uploads_aborted: usize,
    /// Provider round trips the delete pass needed. A multi-page claim that
    /// never walked more than one page is not a multi-page claim.
    pub object_pages: usize,
    pub upload_pages: usize,
    pub remaining_objects: Vec<String>,
    pub remaining_uploads: Vec<(String, String)>,
    /// Failures are collected, never swallowed: a provider that refuses a
    /// delete has to surface as a cleanup failure, not as a green step.
    pub errors: Vec<String>,
    /// At least one listing failed, so this pass never saw the whole scope.
    ///
    /// Tracked separately from `errors` because it changes what the pass is
    /// allowed to *claim*: `remaining_objects = 0` after a failed listing means
    /// "we did not finish looking", not "there is nothing there". Keeping it as
    /// its own field means the render says so out loud instead of letting an
    /// empty residue list read as zero-residue evidence.
    pub listing_incomplete: bool,
    pub injected_fault: bool,
}

impl CleanupReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
            && self.remaining_objects.is_empty()
            && self.remaining_uploads.is_empty()
            && !self.listing_incomplete
            && !self.injected_fault
    }

    pub fn render(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::new();
        let _ = writeln!(
            text,
            "CLEANUP prefix={} objects_deleted={} uploads_aborted={} remaining_objects={} \
             remaining_uploads={} errors={} object_pages={} upload_pages={}",
            self.prefix,
            self.objects_deleted,
            self.uploads_aborted,
            self.remaining_objects.len(),
            self.remaining_uploads.len(),
            self.errors.len(),
            self.object_pages,
            self.upload_pages,
        );
        for error in &self.errors {
            let _ = writeln!(text, "CLEANUP ERROR: {error}");
        }
        if self.listing_incomplete {
            let _ = writeln!(
                text,
                "CLEANUP LISTING INCOMPLETE: a listing under {} failed, so this pass did \
                 not enumerate the whole scope and cannot claim zero residue",
                self.prefix
            );
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

    let uploads = match list_all_incomplete_uploads_with_pages(&sdk, &bucket, &prefix).await {
        Ok(listing) => {
            report.upload_pages = listing.pages;
            listing.items
        }
        Err(error) => {
            report.errors.push(error);
            report.listing_incomplete = true;
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

    let keys = match list_all_keys_with_pages(&sdk, &bucket, &prefix).await {
        Ok(listing) => {
            report.object_pages = listing.pages;
            listing.items
        }
        Err(error) => {
            report.errors.push(error);
            report.listing_incomplete = true;
            Vec::new()
        }
    };
    for key in keys {
        match sdk.delete_object().bucket(&bucket).key(&key).send().await {
            Ok(_) => report.objects_deleted += 1,
            Err(error) => report.errors.push(format!("delete_object {key}: {error}")),
        }
    }

    // Verify from the provider, not from the delete calls that were issued. A
    // failed verification listing is recorded as an incomplete listing, which
    // `ok()` refuses: an empty residue list that came from a listing that did
    // not finish is not evidence of an empty scope.
    match list_all_keys_with_pages(&sdk, &bucket, &prefix).await {
        Ok(listing) => report.remaining_objects = listing.items,
        Err(error) => {
            report.errors.push(error);
            report.listing_incomplete = true;
        }
    }
    match list_all_incomplete_uploads_with_pages(&sdk, &bucket, &prefix).await {
        Ok(listing) => report.remaining_uploads = listing.items,
        Err(error) => {
            report.errors.push(error);
            report.listing_incomplete = true;
        }
    }
    report
}

/// Print the report and fail the step if anything is left or anything failed.
pub fn report_and_fail(report: &CleanupReport) {
    print!("{}", report.render());
    assert!(
        report.ok(),
        "final cleanup did not leave the run scope empty for prefix {}: {} object(s), \
         {} incomplete upload(s), {} error(s), listing incomplete={}. A listing that \
         failed is never read as zero residue. Cleanup failures are reported as their own \
         failure and are never folded into a capability observation.",
        report.prefix,
        report.remaining_objects.len(),
        report.remaining_uploads.len(),
        report.errors.len(),
        report.listing_incomplete,
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

// ---------------------------------------------------------------------------
// What a child actually created: a channel separate from the log
// ---------------------------------------------------------------------------

/// Prefix of the machine-readable line a child prints for every upload it
/// really created.
///
/// The Round 2 fault scenario printed `FAULT_ARMED left incomplete upload
/// <key>#<id>` inside its ordinary log and then only checked that
/// `ListMultipartUploads` was **non-empty**. That has two holes: a log line is
/// free text the parent greps rather than data it can assert on, and "non-empty"
/// is satisfied by *any* upload, so the scenario could pass without ever
/// proving the upload it created was the one the listing found.
pub const CREATED_UPLOAD_MARKER: &str = "PLAN0014-CREATED-UPLOAD";

/// One upload a child process created, reported to the parent as data.
///
/// Synthetic test data only: the bucket, run id and key are the lane's own
/// generated names, and the upload id is whatever the provider handed back for
/// that synthetic key. `run_id` and `bucket` travel with the record so the
/// parent can refuse a record belonging to a different run before it acts on
/// any of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CreatedUpload {
    pub bucket: String,
    pub run_id: String,
    pub key: String,
    pub upload_id: String,
}

impl CreatedUpload {
    #[must_use]
    pub fn pair(&self) -> (String, String) {
        (self.key.clone(), self.upload_id.clone())
    }
}

/// Print one creation record on its own line, apart from the narrative log.
#[allow(clippy::print_literal)]
pub fn emit_created_upload(upload: &CreatedUpload) {
    let rendered = serde_json::to_string(upload).unwrap_or_else(|error| {
        panic!("the created-upload record is serialisable by construction: {error}")
    });
    println!("{CREATED_UPLOAD_MARKER} {rendered}");
}

/// Read back the records in a child's output.
///
/// Returns the records it could parse **and** the malformed marker lines: a
/// truncated or corrupt record is a harness defect, and silently ignoring it
/// would put the parent back into the position of guessing from a log line what
/// the child did.
///
/// The marker is located anywhere in the line, not only at its start. The test
/// harness writes `test <name> ... ` and the child's first `println!` continues
/// on that same line, so a line-start rule drops exactly the first record —
/// which is how the first Round 3 lane run came to report "the child created one
/// upload; it reported none".
#[must_use]
pub fn parse_created_uploads(output: &str) -> (Vec<CreatedUpload>, Vec<String>) {
    let mut parsed = Vec::new();
    let mut malformed = Vec::new();
    for line in output.lines() {
        let Some(at) = line.find(CREATED_UPLOAD_MARKER) else {
            continue;
        };
        let payload = line[at + CREATED_UPLOAD_MARKER.len()..].trim();
        match serde_json::from_str(payload) {
            Ok(upload) => parsed.push(upload),
            Err(error) => malformed.push(format!("{error}: {payload}")),
        }
    }
    (parsed, malformed)
}

/// Did the provider's listing actually contain what the child created?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryOutcome {
    /// Every created upload appears in the listing, matched on the exact
    /// `(key, upload-id)` pair — not on "the listing was non-empty".
    Visible,
    /// The listing is missing at least one upload whose key and id are known.
    /// The discovery capability is therefore **not** verified.
    NotDiscovered {
        missing: Vec<(String, String)>,
        listed: usize,
    },
    /// The child reported nothing it created, so the parent has no expectation
    /// to check. The scenario proves nothing and must not be counted.
    Unreported,
}

impl DiscoveryOutcome {
    /// Whether the scenario may be reported as a pass. Anything else is a
    /// failure or a blocked scenario — never a green line.
    #[must_use]
    pub fn counts_as_pass(&self) -> bool {
        matches!(self, Self::Visible)
    }
}

/// Compare what the child says it created with what the provider listed.
///
/// Pure so the negative fixtures (a provider that lists nothing, a provider
/// that lists a *different* upload) can be tested deterministically instead of
/// waiting for a real provider to misbehave on schedule. The lane's real
/// scenario calls this same function, so the fixtures test the judgement the
/// lane actually makes.
#[must_use]
pub fn judge_upload_discovery(
    created: &[(String, String)],
    listed: &[(String, String)],
) -> DiscoveryOutcome {
    if created.is_empty() {
        return DiscoveryOutcome::Unreported;
    }
    let missing: Vec<(String, String)> = created
        .iter()
        .filter(|pair| !listed.contains(pair))
        .cloned()
        .collect();
    if missing.is_empty() {
        DiscoveryOutcome::Visible
    } else {
        DiscoveryOutcome::NotDiscovered {
            missing,
            listed: listed.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// Scripted provider: deterministic SDK responses, no network, no trust store
// ---------------------------------------------------------------------------

/// One canned HTTP answer for [`ScriptedProvider`].
#[derive(Debug, Clone)]
pub struct ScriptedResponse {
    pub status: u16,
    pub body: String,
}

impl ScriptedResponse {
    #[must_use]
    pub fn ok(body: String) -> Self {
        Self { status: 200, body }
    }
}

/// Whether a scripted page answers `IsTruncated` at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truncation {
    Answered(bool),
    Unanswered,
}

impl Truncation {
    fn render(self) -> String {
        match self {
            Self::Answered(truncated) => format!("<IsTruncated>{truncated}</IsTruncated>"),
            Self::Unanswered => String::new(),
        }
    }
}

fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// A page of `ListMultipartUploads`.
///
/// The two marker halves are separate fields on purpose: a provider that
/// returns one and not the other is a case the cursor rules have to reject, and
/// a builder that always sets both could not express it.
#[derive(Debug, Clone)]
pub struct UploadsPage {
    pub uploads: Vec<(String, String)>,
    pub truncated: Truncation,
    pub next_key_marker: Option<String>,
    pub next_upload_id_marker: Option<String>,
}

impl UploadsPage {
    /// A truncated page that hands back a complete `(key, upload-id)` cursor.
    #[must_use]
    pub fn truncated_to(uploads: &[(String, String)], next: (&str, &str)) -> Self {
        Self {
            uploads: uploads.to_vec(),
            truncated: Truncation::Answered(true),
            next_key_marker: Some(next.0.to_owned()),
            next_upload_id_marker: Some(next.1.to_owned()),
        }
    }

    /// The last page.
    #[must_use]
    pub fn complete(uploads: &[(String, String)]) -> Self {
        Self {
            uploads: uploads.to_vec(),
            truncated: Truncation::Answered(false),
            next_key_marker: None,
            next_upload_id_marker: None,
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        text.push_str(
            "<ListMultipartUploadsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Bucket>scripted-bucket</Bucket><KeyMarker></KeyMarker><UploadIdMarker></UploadIdMarker>",
        );
        if let Some(marker) = &self.next_key_marker {
            let _ = write!(
                text,
                "<NextKeyMarker>{}</NextKeyMarker>",
                xml_escape(marker)
            );
        }
        if let Some(marker) = &self.next_upload_id_marker {
            let _ = write!(
                text,
                "<NextUploadIdMarker>{}</NextUploadIdMarker>",
                xml_escape(marker)
            );
        }
        text.push_str("<MaxUploads>3</MaxUploads>");
        text.push_str(&self.truncated.render());
        for (key, upload_id) in &self.uploads {
            let _ = write!(
                text,
                "<Upload><Key>{}</Key><UploadId>{}</UploadId><StorageClass>STANDARD</StorageClass>\
                 <Initiated>2026-01-01T00:00:00.000Z</Initiated></Upload>",
                xml_escape(key),
                xml_escape(upload_id)
            );
        }
        text.push_str("</ListMultipartUploadsResult>");
        text
    }
}

/// A page of `ListObjectsV2`.
#[derive(Debug, Clone)]
pub struct ObjectsPage {
    pub keys: Vec<String>,
    pub truncated: Truncation,
    pub next_token: Option<String>,
}

impl ObjectsPage {
    #[must_use]
    pub fn truncated_to<S: AsRef<str>>(keys: &[S], next_token: &str) -> Self {
        Self {
            keys: keys.iter().map(|key| key.as_ref().to_owned()).collect(),
            truncated: Truncation::Answered(true),
            next_token: Some(next_token.to_owned()),
        }
    }

    #[must_use]
    pub fn complete<S: AsRef<str>>(keys: &[S]) -> Self {
        Self {
            keys: keys.iter().map(|key| key.as_ref().to_owned()).collect(),
            truncated: Truncation::Answered(false),
            next_token: None,
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        text.push_str(
            "<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Name>scripted-bucket</Name><MaxKeys>3</MaxKeys>",
        );
        if let Some(token) = &self.next_token {
            let _ = write!(
                text,
                "<NextContinuationToken>{}</NextContinuationToken>",
                xml_escape(token)
            );
        }
        let _ = write!(text, "<KeyCount>{}</KeyCount>", self.keys.len());
        text.push_str(&self.truncated.render());
        for key in &self.keys {
            let _ = write!(
                text,
                "<Contents><Key>{}</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified>\
                 <ETag>&quot;scripted&quot;</ETag><Size>7</Size>\
                 <StorageClass>STANDARD</StorageClass></Contents>",
                xml_escape(key)
            );
        }
        text.push_str("</ListBucketResult>");
        text
    }
}

#[derive(Debug, Default)]
struct ScriptedState {
    pending: std::collections::VecDeque<ScriptedResponse>,
    requests: Vec<String>,
}

/// A provider that answers with exactly the pages a test wrote, in order.
///
/// The connector is installed in place of the SDK's HTTPS connector, so a
/// scripted run opens no socket and never reads the operating system's root
/// certificate store — the same seam `src/client.rs` uses for its offline
/// client. That is what lets these tests drive the **real** listing functions:
/// the SDK parses real S3 XML and hands the parsed response to the production
/// pagination code, so there is no second copy of the algorithm to test.
#[derive(Debug, Default, Clone)]
pub struct ScriptedProvider {
    state: std::sync::Arc<std::sync::Mutex<ScriptedState>>,
}

impl ScriptedProvider {
    #[must_use]
    pub fn new(pages: Vec<ScriptedResponse>) -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(ScriptedState {
                pending: pages.into(),
                requests: Vec::new(),
            })),
        }
    }

    /// Upload pages, in order.
    #[must_use]
    pub fn uploads(pages: Vec<UploadsPage>) -> Self {
        Self::new(
            pages
                .into_iter()
                .map(|page| ScriptedResponse::ok(page.render()))
                .collect(),
        )
    }

    /// Object pages, in order.
    #[must_use]
    pub fn objects(pages: Vec<ObjectsPage>) -> Self {
        Self::new(
            pages
                .into_iter()
                .map(|page| ScriptedResponse::ok(page.render()))
                .collect(),
        )
    }

    /// Every request the SDK actually sent, as `METHOD path?query`.
    ///
    /// The tests read this to prove the *pair* of markers reached the wire —
    /// a listing that ignored the upload-id half would still return the right
    /// items if the fixture were generous about it.
    #[must_use]
    pub fn requests(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .clone()
    }

    /// Pages the code under test never asked for.
    #[must_use]
    pub fn unanswered(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .len()
    }

    #[must_use]
    pub fn client(&self) -> Client {
        use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
        use aws_smithy_runtime_api::client::http::http_client_fn;
        use aws_smithy_runtime_api::client::http::SharedHttpConnector;

        let state = std::sync::Arc::clone(&self.state);
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            // Never connected to: the scripted connector replaces the transport.
            .endpoint_url("http://127.0.0.1:1")
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new(
                "plan0014-scripted-access",
                "plan0014-scripted-secret",
                None,
                None,
                "scripted-test",
            ))
            .force_path_style(true)
            .http_client(http_client_fn(move |_settings, _components| {
                SharedHttpConnector::new(ScriptedConnector {
                    state: std::sync::Arc::clone(&state),
                })
            }))
            .build();
        Client::from_conf(config)
    }
}

/// The connector behind [`ScriptedProvider`]: pops the next scripted page and
/// records the request that asked for it.
#[derive(Debug)]
struct ScriptedConnector {
    state: std::sync::Arc<std::sync::Mutex<ScriptedState>>,
}

impl aws_smithy_runtime_api::client::http::HttpConnector for ScriptedConnector {
    fn call(
        &self,
        request: aws_smithy_runtime_api::client::orchestrator::HttpRequest,
    ) -> aws_smithy_runtime_api::client::http::HttpConnectorFuture {
        use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
        use aws_smithy_runtime_api::client::result::ConnectorError;
        use aws_smithy_runtime_api::http::StatusCode;

        let scripted = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state
                .requests
                .push(format!("{} {}", request.method(), request.uri()));
            state.pending.pop_front()
        };
        let outcome = match scripted {
            None => Err(ConnectorError::other(
                "scripted provider ran out of pages".into(),
                None,
            )),
            Some(response) => match StatusCode::try_from(response.status) {
                Ok(status) => Ok(HttpResponse::new(
                    status,
                    aws_smithy_types::body::SdkBody::from(response.body),
                )),
                Err(error) => Err(ConnectorError::other(
                    format!("scripted page had an unusable status: {error}").into(),
                    None,
                )),
            },
        };
        aws_smithy_runtime_api::client::http::HttpConnectorFuture::new(std::future::ready(outcome))
    }
}
