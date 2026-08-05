#![allow(clippy::panic, clippy::expect_used, clippy::too_many_lines)]

use chrono::{Duration, Utc};
use document_processing::ports::{
    CompleteAiTaskCommand, ExecutionFence, ProcessingExecutionUnitOfWork, ProcessingJobQuery,
    TextArtifactReference,
};
use document_processing::ports::{ProcessingJobClaimPort, ProcessingJobCommandPort};
use document_processing::{
    DeterministicLocalExtractor, FixedPipelineRunner, ProcessingJob, ProcessingStepKind,
};
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
    .unwrap_or_else(|error| {
        panic!("object key construction failed tenant={tenant} document={document}: {error:?}")
    });
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, created_by, created_at, updated_at) VALUES ($1, $2, 'processing.txt', 'text/plain', $4, 'active', 1, 1, $3, NOW(), NOW())",
    )
    .bind(document)
    .bind(tenant)
    .bind(user)
    .bind(object_key.as_str())
    .execute(pool)
    .await
    .unwrap_or_else(|error| panic!("document setup insert failed tenant={tenant} document={document}: {error:?}"));
    (tenant, document, user, object_key)
}

#[tokio::test]
#[ignore = "requires PostgreSQL, migrations, and MinIO"]
#[allow(clippy::too_many_lines)]
async fn postgres_minio_processing_adapter_round_trip() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    let pool = PgPool::connect(&url)
        .await
        .unwrap_or_else(|error| panic!("PostgreSQL connection failed url={url}: {error:?}"));
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
        .unwrap_or_else(|error| {
            panic!("MinIO put_object failed tenant={tenant} object_key={object_key:?}: {error:?}")
        });
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
    .unwrap_or_else(|error| {
        panic!("processing job construction failed tenant={tenant} document={document}: {error:?}")
    });
    store.create(&job).await.unwrap_or_else(|error| {
        panic!(
            "processing job create failed tenant={tenant} job_id={}: {error:?}",
            job.id()
        )
    });
    let claimed = store
        .claim_next("minio-worker", Utc::now(), 30)
        .await
        .unwrap_or_else(|error| panic!("infrastructure operation failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()))
        .unwrap_or_else(|| panic!("infrastructure claim returned no item tenant={tenant} job_id={} stage=postgres_minio_round_trip", job.id()));
    let bytes = storage
        .get_object(&object_key)
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    let run = FixedPipelineRunner
        .run_inline(
            &claimed.job,
            "text/plain",
            &bytes,
            1024,
            &DeterministicLocalExtractor,
        )
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    let business_fence = ExecutionFence::new(
        "minio-worker",
        claimed.lease_token.clone(),
        claimed.fence_version,
    );
    for step in [
        ProcessingStepKind::ValidateSource,
        ProcessingStepKind::DetectType,
    ] {
        store
            .start_step(tenant, job.id(), step, &business_fence, Utc::now())
            .await
            .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
        store
            .complete_step(tenant, job.id(), step, None, &business_fence, Utc::now())
            .await
            .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    }
    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ExtractText,
            &business_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    let task = store
        .enqueue_ai_and_wait(
            tenant,
            job.id(),
            TextArtifactReference {
                key: format!("processing/{}/text", job.id()),
                content_hash: run.checkpoint.content_hash.clone(),
                content_revision: 1,
                byte_count: run.checkpoint.byte_count,
                line_count: run.checkpoint.line_count,
                character_count: run.checkpoint.character_count,
            },
            &business_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    let ai_claimed_at = Utc::now();
    let ai_claim = store
        .claim_ai_task_for_test(task.id, "minio-ai-worker", ai_claimed_at, 30)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "AI task claim failed tenant={tenant} job_id={} stage=extract_fields: {error:?}",
                job.id()
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "AI task claim returned no task tenant={tenant} job_id={} stage=extract_fields",
                job.id()
            )
        });
    assert_eq!(ai_claim.id, task.id, "AI task claim selected unexpected task tenant={tenant} job_id={} expected_task_id={} actual_task_id={}", job.id(), task.id, ai_claim.id);
    assert_eq!(ai_claim.tenant_id, tenant);
    assert_eq!(ai_claim.job_id, job.id());
    assert_eq!(ai_claim.step_kind, ProcessingStepKind::ExtractFields);
    assert_eq!(ai_claim.status, "running");
    assert_eq!(ai_claim.lease_owner.as_deref(), Some("minio-ai-worker"));
    assert!(!ai_claim
        .lease_token
        .as_deref()
        .unwrap_or_default()
        .is_empty());
    assert!(ai_claim.fence_version > 0);
    assert!(ai_claim
        .lease_expires_at
        .is_some_and(|expires| expires > ai_claimed_at));
    store
        .complete_ai_and_resume(
            CompleteAiTaskCommand {
                tenant_id: tenant,
                job_id: job.id(),
                task_id: task.id,
                fence: ExecutionFence::new(
                    "minio-ai-worker",
                    ai_claim.lease_token.clone().unwrap_or_default(),
                    ai_claim.fence_version,
                ),
                candidate: run.candidate.clone(),
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|error| panic!("complete_ai_and_resume failed tenant={tenant} job_id={} task_id={} stage=extract_fields: {error:?}", job.id(), task.id));
    let candidate_claim = store
        .claim_next("minio-worker-2", Utc::now(), 30)
        .await
        .unwrap_or_else(|error| panic!("infrastructure operation failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()))
        .unwrap_or_else(|| panic!("infrastructure claim returned no item tenant={tenant} job_id={} stage=postgres_minio_round_trip", job.id()));
    let candidate_fence = ExecutionFence::new(
        "minio-worker-2",
        candidate_claim.lease_token,
        candidate_claim.fence_version,
    );
    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateCandidate,
            &candidate_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    store
        .save_candidate_and_wait_for_review(
            tenant,
            job.id(),
            &run.candidate,
            &candidate_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    assert!(store
        .detail(tenant, job.id())
        .await
        .unwrap_or_else(|error| panic!("infrastructure operation failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()))
        .and_then(|detail| detail.candidate)
        .is_some());
    storage
        .delete(&object_key)
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    sqlx::query(
        "DELETE FROM document_processing_audit_events WHERE tenant_id = $1 AND job_id = $2",
    )
    .bind(tenant)
    .bind(job.id())
    .execute(&pool)
    .await
    .unwrap_or_else(|error| {
        panic!(
            "processing audit cleanup failed tenant={tenant} job_id={}: {error:?}",
            job.id()
        )
    });
    sqlx::query("DELETE FROM outbox_events WHERE tenant_id = $1 AND aggregate_id = $2")
        .bind(tenant.to_string())
        .bind(job.id().to_string())
        .execute(&pool)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "outbox cleanup failed tenant={tenant} job_id={}: {error:?}",
                job.id()
            )
        });
    sqlx::query("DELETE FROM audit_events WHERE tenant_id = $1 AND resource_id = $2")
        .bind(tenant)
        .bind(job.id().to_string())
        .execute(&pool)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "audit cleanup failed tenant={tenant} job_id={}: {error:?}",
                job.id()
            )
        });
    sqlx::query("DELETE FROM document_processing_jobs WHERE tenant_id = $1 AND document_id = $2")
        .bind(tenant)
        .bind(document)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
    sqlx::query("DELETE FROM documents WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(document)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("infrastructure step failed tenant={tenant} job_id={} stage=postgres_minio_round_trip: {error:?}", job.id()));
}
