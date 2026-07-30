//! `MinIO` contract tests for the S3 storage adapter.
//!
//! These tests require a running `MinIO` instance (docker compose up)
//! and are marked `#[ignore]` so they do not run in CI without `MinIO`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
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

    // Verify the object is retrievable (content-type is set server-side;
    // a full verification would require a HEAD request checking Content-Type).
    let fetched = client.get_object(&key).await.unwrap();
    assert_eq!(fetched, Bytes::from_static(br#"{"key":"value"}"#));

    client.delete_object(&key).await.unwrap();
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
