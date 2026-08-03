use chrono::{Duration, Utc};
use document_processing::ports::{
    CandidateStore, ProcessingJobClaimPort, ProcessingJobCommandPort,
};
use document_processing::{DeterministicLocalExtractor, FixedPipelineRunner, ProcessingJob};
use document_processing_postgres::PostgresProcessingStore;
use object_storage::{ObjectKey, ObjectStorageClient, S3Client};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> (Uuid, Uuid, Uuid, ObjectKey) {
    let tenant = Uuid::now_v7();
    let document = Uuid::now_v7();
    let user = Uuid::now_v7();
    let object_key = ObjectKey::new(format!(
        "tenants/{tenant}/documents/{document}/v1/processing.txt"
    ))
    .unwrap_or_else(|_| unreachable!());
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, created_by, created_at, updated_at) VALUES ($1, $2, 'processing.txt', 'text/plain', $4, 'active', 1, 1, $3, NOW(), NOW())",
    )
    .bind(document)
    .bind(tenant)
    .bind(user)
    .bind(object_key.as_str())
    .execute(pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    (tenant, document, user, object_key)
}

#[tokio::test]
#[ignore = "requires PostgreSQL, migrations, and MinIO"]
#[allow(clippy::too_many_lines)]
async fn postgres_minio_processing_candidate_round_trip() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    let pool = PgPool::connect(&url)
        .await
        .unwrap_or_else(|_| unreachable!());
    let endpoint =
        std::env::var("MINIO_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".to_string());
    let access_key = std::env::var("MINIO_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let secret_key = std::env::var("MINIO_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let bucket =
        std::env::var("MINIO_BUCKET").unwrap_or_else(|_| "contract-test-bucket".to_string());
    let storage = S3Client::new(&endpoint, &access_key, &secret_key, &bucket, "us-east-1");
    let (tenant, document, user, object_key) = setup(&pool).await;
    storage
        .put_object(
            &object_key,
            bytes::Bytes::from_static(b"MinIO title\nbody"),
            "text/plain",
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    let store = PostgresProcessingStore::new(pool.clone());
    let now = Utc::now() - Duration::seconds(1);
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("minio-{}", Uuid::now_v7()),
        user,
        3,
        now,
    )
    .unwrap_or_else(|_| unreachable!());
    store.create(&job).await.unwrap_or_else(|_| unreachable!());
    let claimed = store
        .claim_next("minio-worker", Utc::now(), 30)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let bytes = storage
        .get_object(&object_key)
        .await
        .unwrap_or_else(|_| unreachable!());
    let run = FixedPipelineRunner
        .run_inline(
            &claimed.job,
            "text/plain",
            &bytes,
            1024,
            &DeterministicLocalExtractor,
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    store
        .save_candidate(&run.candidate)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(store
        .get_candidate(tenant, job.id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .is_some());
    storage
        .delete(&object_key)
        .await
        .unwrap_or_else(|_| unreachable!());
    sqlx::query("DELETE FROM document_processing_jobs WHERE tenant_id = $1 AND document_id = $2")
        .bind(tenant)
        .bind(document)
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    sqlx::query("DELETE FROM documents WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(document)
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
}
