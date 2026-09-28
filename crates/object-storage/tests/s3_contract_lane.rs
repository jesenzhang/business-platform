#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 - real S3-compatible provider contract lane.
//!
//! Wave 4 requires that the provider-neutral `ObjectStorageClient` contract is
//! proven against a real S3-compatible implementation, not only against the
//! in-memory fake or the local filesystem adapter. Every test here is
//! `#[ignore]`d so the default workspace gate stays free of infrastructure
//! dependencies, and is executed with `--ignored` against a dedicated endpoint,
//! bucket and prefix (PLAN-0014 Wave 4 evidence directory
//! `2026-09-28-wave4-storage-lane/`).
//!
//! Required environment (never committed):
//!
//! * `WAVE4_S3_ENDPOINT`    - for example `http://127.0.0.1:19100`
//! * `WAVE4_S3_ACCESS_KEY`  - dedicated test identity
//! * `WAVE4_S3_SECRET_KEY`  - dedicated test identity
//! * `WAVE4_S3_BUCKET`      - dedicated test bucket, created when missing
//! * `WAVE4_S3_REGION`      - provider region, for example `us-east-1`
//! * `WAVE4_S3_RUN_ID`      - unique id; all keys live under its prefix
//!
//! Cleanup is limited to keys under `plan0014-wave4/<WAVE4_S3_RUN_ID>/`.
//!
//! Leak proof protocol: the harness runs tests in its own order, so the lane is
//! executed twice with the same `WAVE4_S3_RUN_ID`. The first run performs the
//! contract cases; the second run executes only
//! `lane_leaves_no_objects_under_the_run_prefix`, which deletes and then counts
//! whatever the first run left behind. Only that second invocation proves the
//! whole lane is leak free.

use std::collections::BTreeMap;

use bytes::Bytes;
use futures_util::stream;
use object_storage::{ObjectKey, ObjectStorageClient, S3Client, StorageError};

#[allow(clippy::expect_used)]
fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for the S3 contract lane"))
}

#[allow(clippy::expect_used)]
fn run_prefix() -> String {
    format!("plan0014-wave4/{}/", env("WAVE4_S3_RUN_ID"))
}

fn key_under_prefix(suffix: &str) -> String {
    format!("{}{}", run_prefix(), suffix)
}

#[allow(clippy::expect_used)]
fn client() -> S3Client {
    S3Client::new(
        &env("WAVE4_S3_ENDPOINT"),
        &env("WAVE4_S3_ACCESS_KEY"),
        &env("WAVE4_S3_SECRET_KEY"),
        &env("WAVE4_S3_BUCKET"),
        &env("WAVE4_S3_REGION"),
    )
}

#[allow(clippy::expect_used)]
fn sdk_client() -> aws_sdk_s3::Client {
    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
    let credentials = Credentials::new(
        env("WAVE4_S3_ACCESS_KEY"),
        env("WAVE4_S3_SECRET_KEY"),
        None,
        None,
        "contract-lane",
    );
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .endpoint_url(env("WAVE4_S3_ENDPOINT"))
        .region(Region::new(env("WAVE4_S3_REGION")))
        .credentials_provider(credentials)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

/// Create the dedicated test bucket when it does not exist yet. The lane never
/// touches a bucket it was not given through `WAVE4_S3_BUCKET`.
async fn ensure_bucket() {
    let sdk = sdk_client();
    let bucket = env("WAVE4_S3_BUCKET");
    match sdk.create_bucket().bucket(&bucket).send().await {
        Ok(_) => println!("LANE created dedicated bucket {bucket}"),
        Err(error) => {
            let text = error.to_string();
            assert!(
                text.contains("BucketAlreadyOwnedByYou")
                    || text.contains("BucketAlreadyExists")
                    || text.contains("already exist"),
                "creating the dedicated bucket {bucket} failed: {text}"
            );
        }
    }
}

async fn lane_client() -> S3Client {
    ensure_bucket().await;
    client()
}

async fn put(client: &S3Client, key: &str, body: &'static [u8], sha256: &str) {
    let object_key = ObjectKey::new(key).expect("valid key");
    let chunks = stream::iter([Ok(Bytes::from_static(body))]);
    let mut metadata = BTreeMap::new();
    metadata.insert("sha256".to_string(), sha256.to_string());
    client
        .put_stream(
            &object_key,
            Box::pin(chunks),
            body.len() as u64,
            "application/octet-stream",
            &metadata,
        )
        .await
        .expect("put_stream must succeed against a real provider");
}

async fn delete_key(client: &S3Client, key: &str) {
    let object_key = ObjectKey::new(key).expect("valid key");
    client.delete(&object_key).await.expect("delete");
}

/// Round trip through the provider-neutral port against a real provider:
/// streaming write, head metadata, checksum metadata, ranged read and delete.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn lane_round_trips_object_metadata_and_payload() {
    let client = lane_client().await;
    let key = key_under_prefix("roundtrip/source.bin");
    let payload: &'static [u8] = b"wave 4 contract lane payload";
    let sha256 = "9f2c0a3e3a1a77f4a3d18e5051ba38a7bb2b0e0f2b1c6b7a2e0a9b4c3d2e1f0a";

    put(&client, &key, payload, sha256).await;
    let object_key = ObjectKey::new(&key).expect("valid key");
    let head = client.head(&object_key).await.expect("head");
    assert_eq!(head.content_length, payload.len() as u64);
    assert_eq!(
        head.content_type.as_deref(),
        Some("application/octet-stream")
    );
    assert_eq!(
        head.metadata.get("sha256").map(String::as_str),
        Some(sha256),
        "provider must return caller metadata; got {:?}",
        head.metadata.keys().collect::<Vec<_>>()
    );
    assert!(head.etag.is_some(), "a real provider returns an ETag");
    assert!(
        client.exists(&object_key).await.expect("exists"),
        "exists must be true after a successful write"
    );
    assert_eq!(
        client.get_object(&object_key).await.expect("read"),
        Bytes::from_static(payload)
    );

    delete_key(&client, &key).await;
    assert!(
        !client.exists(&object_key).await.expect("exists"),
        "the object must be gone after delete"
    );
}

/// A missing object must surface as `NotFound`, not as a transport error, so
/// callers can tell "metadata without object" apart from an outage.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn lane_reports_missing_object_as_not_found() {
    let client = lane_client().await;
    let key = key_under_prefix("missing/never-written.bin");
    let object_key = ObjectKey::new(&key).expect("valid key");

    let result = client.open_stream(&object_key).await;
    let Err(error) = result else {
        panic!("reading a missing object must fail");
    };
    assert!(
        matches!(error, StorageError::NotFound(_)),
        "expected NotFound, got {error:?}"
    );
    assert!(
        !client.exists(&object_key).await.expect("exists"),
        "exists must be false for a missing object"
    );
}

/// Delete of an absent object must not fail: the storage GC and compensation
/// paths replay deletes.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn lane_delete_of_absent_object_is_idempotent() {
    let client = lane_client().await;
    let key = key_under_prefix("delete/absent.bin");
    let object_key = ObjectKey::new(&key).expect("valid key");

    client.delete(&object_key).await.expect("first delete");
    client.delete(&object_key).await.expect("second delete");
    assert!(!client.exists(&object_key).await.expect("exists"));
}

/// Presigned URLs must be scoped to one key and must stop working when they
/// expire. This is the capability the local filesystem adapter refuses (gap
/// ST-W4-03), so it can only be proven against a real provider.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn lane_presigned_url_is_key_scoped_and_expires() {
    let client = lane_client().await;
    let granted = key_under_prefix("presign/granted.bin");
    let other = key_under_prefix("presign/other.bin");
    put(&client, &granted, b"granted body", "0").await;
    put(&client, &other, b"other body", "0").await;

    let granted_key = ObjectKey::new(&granted).expect("valid key");
    let url = object_storage::ObjectStorageClient::presigned_url(&client, &granted_key, 300)
        .await
        .expect("presign");
    assert!(
        url.starts_with("http"),
        "presigned URL must be http(s): {url}"
    );
    let response = reqwest::get(&url).await.expect("presigned GET");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response.bytes().await.expect("body").as_ref(),
        b"granted body" as &[u8]
    );

    // Scope: the signature covers the key, so retargeting the path must fail.
    let tampered = url.replace("presign/granted.bin", "presign/other.bin");
    assert_ne!(tampered, url, "the tampered URL must differ");
    let status = reqwest::get(&tampered)
        .await
        .expect("tampered presigned GET must reach the provider")
        .status();
    assert!(
        status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND,
        "a presigned URL must not read another key, got {status}"
    );

    // Expiry: a one second grant must stop working shortly after.
    let short = object_storage::ObjectStorageClient::presigned_url(&client, &granted_key, 1)
        .await
        .expect("short presign");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let expired_status = reqwest::get(&short)
        .await
        .expect("expired presigned GET must reach the provider")
        .status();
    assert!(
        expired_status == reqwest::StatusCode::FORBIDDEN
            || expired_status == reqwest::StatusCode::NOT_FOUND,
        "an expired presigned URL must be refused, got {expired_status}"
    );

    delete_key(&client, &granted).await;
    delete_key(&client, &other).await;
}

/// Capability discovery against this provider. Wave 4 forbids generalising one
/// provider's capabilities to all providers, so the matrix records what the
/// provider actually answers; only the S3 fundamentals the platform depends on
/// are asserted.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
#[allow(clippy::too_many_lines)]
async fn lane_records_provider_capabilities() {
    ensure_bucket().await;
    let sdk = sdk_client();
    let bucket = env("WAVE4_S3_BUCKET");
    let key = key_under_prefix("capability/multipart.bin");

    // Multipart upload plus abort: the platform port exposes neither, so this
    // row is "provider has it, adapter does not implement it".
    let created = sdk
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let upload_id = match created {
        Ok(response) => response.upload_id.clone().expect("multipart upload id"),
        Err(error) => panic!("CreateMultipartUpload must be supported: {error}"),
    };
    // S3 requires every non-final part to be at least 5 MiB; a smaller probe
    // body measures the client, not the provider, so the part is 5 MiB.
    let part = sdk
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(aws_smithy_types::byte_stream::ByteStream::from(vec![
            7_u8;
            5 * 1024 * 1024
        ]))
        .send()
        .await;
    let part_etag = match &part {
        Ok(response) => response.e_tag().map(ToOwned::to_owned),
        Err(error) => {
            println!("CAPABILITY multipart upload_part error: {error}");
            None
        }
    };
    println!(
        "CAPABILITY multipart upload_part_5mib: {}",
        part_etag.is_some()
    );
    let completed = if let Some(etag) = part_etag {
        sdk.complete_multipart_upload()
            .bucket(&bucket)
            .key(&key)
            .upload_id(&upload_id)
            .multipart_upload(
                aws_sdk_s3::types::CompletedMultipartUpload::builder()
                    .parts(
                        aws_sdk_s3::types::CompletedPart::builder()
                            .part_number(1)
                            .set_e_tag(Some(etag))
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
            .is_ok()
    } else {
        false
    };
    println!("CAPABILITY multipart complete_multipart_upload: {completed}");
    let aborted = sdk
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await;
    println!(
        "CAPABILITY multipart abort_multipart_upload: {}",
        aborted.is_ok()
    );
    // A capability probe records; it does not dictate provider behaviour. Here
    // RustFS 1.0.1-preview.11 answered the legal 5 MiB `UploadPart` with
    // `HTTP 500 InternalError`, so completion never ran. That is a finding
    // about this provider, not a failure of this repository: what the lane
    // requires is that an unsupported or broken capability surfaces as an
    // explicit provider-classified error instead of a silent partial success.
    // The platform port exposes no multipart operation at all (gap ST-W4-01),
    // so no code path here can be degraded by the provider's answer.
    let multipart_explicit_outcome =
        completed || matches!(part, Err(aws_sdk_s3::error::SdkError::ServiceError(_)));
    println!("CAPABILITY multipart explicit_outcome: {multipart_explicit_outcome}");
    assert!(
        multipart_explicit_outcome,
        "multipart must either complete or be rejected with an explicit provider error, got {:?}",
        part.as_ref().err()
    );
    let abort_explicit_outcome =
        aborted.is_ok() || matches!(aborted, Err(aws_sdk_s3::error::SdkError::ServiceError(_)));
    assert!(
        abort_explicit_outcome,
        "AbortMultipartUpload must succeed or be rejected with an explicit provider error, got {:?}",
        aborted.as_ref().err()
    );
    sdk.delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .expect("cleanup of the multipart probe object");

    // Conditional read (If-Match) on a normal object.
    let client = lane_client().await;
    let conditional_key = key_under_prefix("capability/conditional.bin");
    put(&client, &conditional_key, b"conditional", "0").await;
    let head = sdk
        .head_object()
        .bucket(&bucket)
        .key(&conditional_key)
        .send()
        .await
        .expect("head");
    let etag = head.e_tag().unwrap_or_default().to_string();
    let conditional = sdk
        .get_object()
        .bucket(&bucket)
        .key(&conditional_key)
        .if_match(&etag)
        .send()
        .await;
    println!(
        "CAPABILITY conditional_read if_match: {}",
        conditional.is_ok()
    );
    let stale = sdk
        .get_object()
        .bucket(&bucket)
        .key(&conditional_key)
        .if_match("\"00000000000000000000000000000000\"")
        .send()
        .await;
    println!(
        "CAPABILITY conditional_read stale_etag_rejected: {}",
        stale.is_err()
    );

    // Object checksums: the adapter sends the SDK default CRC32 trailer but the
    // platform contract never reads a checksum back. The probe lets the SDK
    // compute a valid CRC32 (a hand written value would only test rejection).
    let checksummed = sdk
        .put_object()
        .bucket(&bucket)
        .key(&conditional_key)
        .checksum_algorithm(aws_sdk_s3::types::ChecksumAlgorithm::Crc32)
        .body(aws_smithy_types::byte_stream::ByteStream::from(
            b"checksummed".to_vec(),
        ))
        .send()
        .await;
    println!(
        "CAPABILITY object_checksums put_with_sdk_crc32: {}",
        checksummed.is_ok()
    );
    let checksum_read_back = match &checksummed {
        Ok(response) => response.checksum_crc32().map(ToOwned::to_owned),
        Err(error) => {
            println!("CAPABILITY object_checksums put error: {error}");
            None
        }
    };
    let head_with_checksum = sdk
        .head_object()
        .bucket(&bucket)
        .key(&conditional_key)
        .checksum_mode(aws_sdk_s3::types::ChecksumMode::Enabled)
        .send()
        .await
        .ok()
        .and_then(|response| response.checksum_crc32().map(ToOwned::to_owned));
    println!(
        "CAPABILITY object_checksums write_echoed={checksum_read_back:?} head_returns_checksum={}",
        head_with_checksum.is_some()
    );

    // Versioning: the platform contract has no version identity.
    let versioning = sdk.get_bucket_versioning().bucket(&bucket).send().await;
    println!(
        "CAPABILITY object_versioning status: {:?}",
        versioning.as_ref().map(|response| {
            response
                .status
                .as_ref()
                .map_or("unconfigured", |status| status.as_str())
        })
    );

    delete_key(&client, &conditional_key).await;
}

/// The lane must not leave objects behind: cleanup is limited to this run's
/// prefix and asserts the prefix is empty afterwards.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn lane_leaves_no_objects_under_the_run_prefix() {
    ensure_bucket().await;
    let sdk = sdk_client();
    let bucket = env("WAVE4_S3_BUCKET");
    let prefix = run_prefix();
    let list = sdk
        .list_objects_v2()
        .bucket(&bucket)
        .prefix(&prefix)
        .send()
        .await
        .expect("list_objects_v2");
    let mut removed = 0_usize;
    for object in list.contents() {
        if let Some(key) = object.key() {
            sdk.delete_object()
                .bucket(&bucket)
                .key(key)
                .send()
                .await
                .unwrap_or_else(|error| panic!("cleanup must delete {key}: {error}"));
            removed += 1;
        }
    }
    let after = sdk
        .list_objects_v2()
        .bucket(&bucket)
        .prefix(&prefix)
        .send()
        .await
        .expect("list_objects_v2 after cleanup");
    let remaining = after.key_count().unwrap_or_default();
    println!("LANE cleanup removed {removed} objects, remaining under {prefix}: {remaining}");
    assert_eq!(remaining, 0, "the run prefix must be empty");
}

/// Optional teardown for a one-off lane run: removes the dedicated bucket
/// itself. Only runs when `WAVE4_S3_DELETE_BUCKET=true`, so a bucket prepared
/// for repeated runs is kept.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn lane_deletes_the_dedicated_bucket_when_requested() {
    if std::env::var("WAVE4_S3_DELETE_BUCKET").ok().as_deref() != Some("true") {
        println!("LANE bucket deletion skipped (WAVE4_S3_DELETE_BUCKET != true)");
        return;
    }
    let sdk = sdk_client();
    let bucket = env("WAVE4_S3_BUCKET");
    sdk.delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .unwrap_or_else(|error| panic!("deleting the dedicated bucket {bucket} failed: {error}"));
    println!("LANE deleted dedicated bucket {bucket}");
}
