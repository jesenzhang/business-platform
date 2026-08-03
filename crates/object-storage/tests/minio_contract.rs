//! `MinIO` contract tests for the S3 storage adapter.
//!
//! These tests require a running `MinIO` instance (docker compose up)
//! and are marked `#[ignore]` so they do not run in CI without `MinIO`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use bytes::Bytes;
use futures_util::{stream, StreamExt};
use object_storage::{ObjectKey, ObjectStorageClient, S3Client};

const ENDPOINT: &str = "http://localhost:9000";
const ACCESS_KEY: &str = "minioadmin";
const SECRET_KEY: &str = "minioadmin";
const BUCKET: &str = "contract-test-bucket";
const REGION: &str = "us-east-1";

fn test_client() -> S3Client {
    S3Client::new(ENDPOINT, ACCESS_KEY, SECRET_KEY, BUCKET, REGION)
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn put_and_get_roundtrip() {
    let client = test_client();
    let key = ObjectKey::new("contract/roundtrip/hello.txt").unwrap();
    let data = Bytes::from_static(b"hello minio");

    client
        .put_object(&key, data.clone(), "text/plain")
        .await
        .unwrap();

    let fetched = client.get_object(&key).await.unwrap();
    assert_eq!(fetched, data);

    // cleanup
    client.delete_object(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn delete_removes_object() {
    let client = test_client();
    let key = ObjectKey::new("contract/delete/target.txt").unwrap();
    let data = Bytes::from_static(b"to be deleted");

    client.put_object(&key, data, "text/plain").await.unwrap();
    assert!(client.object_exists(&key).await.unwrap());

    client.delete_object(&key).await.unwrap();
    assert!(!client.object_exists(&key).await.unwrap());
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn exists_returns_false_for_missing() {
    let client = test_client();
    let key = ObjectKey::new("contract/missing/no-such-key.txt").unwrap();

    assert!(!client.object_exists(&key).await.unwrap());
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn get_missing_returns_not_found() {
    let client = test_client();
    let key = ObjectKey::new("contract/missing/ghost.txt").unwrap();

    let err = client.get_object(&key).await.unwrap_err();
    assert!(
        matches!(err, object_storage::StorageError::NotFound(_)),
        "expected NotFound, got: {err}"
    );
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn presigned_url_is_valid() {
    let client = test_client();
    let key = ObjectKey::new("contract/presign/secret.txt").unwrap();
    let data = Bytes::from_static(b"presigned content");

    client.put_object(&key, data, "text/plain").await.unwrap();

    let url = client.presigned_url(&key, 300).await.unwrap();
    assert!(
        url.starts_with("http"),
        "presigned URL should be http(s): {url}"
    );
    assert!(
        url.contains("X-Amz-Signature"),
        "URL should contain signature: {url}"
    );
    assert!(
        url.contains("X-Amz-Expires=300"),
        "URL should contain expiry: {url}"
    );
    let response = reqwest::get(&url).await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.bytes().await.unwrap(),
        Bytes::from_static(b"presigned content")
    );

    // cleanup
    client.delete_object(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn special_character_keys() {
    let client = test_client();
    let key =
        ObjectKey::new("contract/special/文件 with spaces & (parens) [brackets].txt").unwrap();
    let data = Bytes::from_static(b"special chars");

    client
        .put_object(&key, data.clone(), "application/octet-stream")
        .await
        .unwrap();

    let fetched = client.get_object(&key).await.unwrap();
    assert_eq!(fetched, data);

    assert!(client.object_exists(&key).await.unwrap());

    client.delete_object(&key).await.unwrap();
    assert!(!client.object_exists(&key).await.unwrap());
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn content_type_is_preserved() {
    let client = test_client();
    let key = ObjectKey::new("contract/content-type/data.json").unwrap();
    let data = Bytes::from_static(br#"{"key":"value"}"#);

    client
        .put_object(&key, data, "application/json")
        .await
        .unwrap();

    let head = client.head(&key).await.unwrap();
    assert_eq!(head.content_type.as_deref(), Some("application/json"));

    client.delete_object(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn user_metadata_is_preserved() {
    let client = test_client();
    let key = ObjectKey::new("contract/metadata/data.bin").unwrap();
    let metadata = BTreeMap::from([
        ("checksum".to_string(), "sha256:test".to_string()),
        ("source".to_string(), "contract".to_string()),
    ]);
    client
        .put_stream(
            &key,
            Box::pin(stream::once(async { Ok(Bytes::from_static(b"metadata")) })),
            8,
            "application/octet-stream",
            &metadata,
        )
        .await
        .unwrap();

    let head = client.head(&key).await.unwrap();
    assert_eq!(
        head.metadata.get("checksum"),
        Some(&"sha256:test".to_string())
    );
    assert_eq!(head.metadata.get("source"), Some(&"contract".to_string()));
    client.delete(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn large_object_is_streamed_without_small_object_buffering() {
    const CHUNK_SIZE: usize = 1024 * 1024;
    const CHUNKS: usize = 20;
    let client = test_client();
    let key = ObjectKey::new("contract/stream/large.bin").unwrap();
    let chunks = (0..CHUNKS).map(|_| Ok(Bytes::from(vec![0x5a; CHUNK_SIZE])));
    client
        .put_stream(
            &key,
            Box::pin(stream::iter(chunks)),
            (CHUNK_SIZE * CHUNKS) as u64,
            "application/octet-stream",
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    let object = client.open_stream(&key).await.unwrap();
    assert_eq!(object.metadata.content_length, (CHUNK_SIZE * CHUNKS) as u64);
    let received = object
        .body
        .map(|chunk| chunk.unwrap().len())
        .fold(0usize, |total, length| async move { total + length })
        .await;
    assert_eq!(received, CHUNK_SIZE * CHUNKS);
    client.delete(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "requires running MinIO (docker compose up)"]
async fn delete_is_idempotent() {
    let client = test_client();
    let key = ObjectKey::new("contract/idempotent/already-gone.txt").unwrap();

    // Deleting a non-existent object should succeed.
    client.delete_object(&key).await.unwrap();
    client.delete_object(&key).await.unwrap();
}
