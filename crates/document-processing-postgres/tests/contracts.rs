use chrono::{Duration, Utc};
use document_processing::ports::{
    ProcessingJobClaimPort, ProcessingJobCommandPort, ProcessingJobQuery, ProcessingStepStore,
    StepCheckpoint,
};
use document_processing::{ProcessingJob, ProcessingStepKind};
use document_processing_postgres::PostgresProcessingStore;
use sqlx::PgPool;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let tenant = Uuid::now_v7();
    let document = Uuid::now_v7();
    let user = Uuid::now_v7();
    let object_key = format!("tenants/{tenant}/documents/{document}/v1/processing.txt");
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, created_by, created_at, updated_at) VALUES ($1, $2, 'processing.txt', 'text/plain', $4, 'active', 1, 1, $3, NOW(), NOW())",
    )
    .bind(document)
    .bind(tenant)
    .bind(user)
    .bind(object_key)
    .execute(pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    (tenant, document, user)
}

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
#[allow(clippy::too_many_lines)]
async fn postgres_processing_contract_claims_and_restarts() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    let pool = PgPool::connect(&url)
        .await
        .unwrap_or_else(|_| unreachable!());
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let now = Utc::now();
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("contract-{}", Uuid::now_v7()),
        user,
        3,
        now - Duration::seconds(1),
    )
    .unwrap_or_else(|_| unreachable!());
    assert!(store.create(&job).await.is_ok());
    assert!(store.create(&job).await.is_ok());
    let claimed = store
        .claim_next("worker-a", now, 30)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    assert!(store
        .heartbeat(
            claimed.job.id(),
            "worker-a",
            &claimed.lease_token,
            claimed.fence_version,
            now,
            30,
        )
        .await
        .is_ok());
    assert!(store
        .release(
            claimed.job.id(),
            "worker-a",
            &claimed.lease_token,
            claimed.fence_version,
            now,
        )
        .await
        .is_ok());
    assert!(store.request_cancel(tenant, claimed.job.id()).await.is_ok());
    assert!(store
        .detail(tenant, job.id())
        .await
        .ok()
        .flatten()
        .is_some());

    let claim_job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("claim-{}", Uuid::now_v7()),
        user,
        3,
        now - Duration::seconds(1),
    )
    .unwrap_or_else(|_| unreachable!());
    store
        .create(&claim_job)
        .await
        .unwrap_or_else(|_| unreachable!());
    let mut workers = Vec::new();
    for index in 0..10 {
        let store = store.clone();
        workers.push(tokio::spawn(async move {
            store
                .claim_next(&format!("claim-worker-{index}"), now, 30)
                .await
                .unwrap_or_else(|_| unreachable!())
        }));
    }
    let mut successful_claims = 0;
    for worker in workers {
        if worker.await.unwrap_or_else(|_| unreachable!()).is_some() {
            successful_claims += 1;
        }
    }
    assert_eq!(successful_claims, 1);

    let recovery_job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("recovery-{}", Uuid::now_v7()),
        user,
        3,
        now - Duration::seconds(1),
    )
    .unwrap_or_else(|_| unreachable!());
    store
        .create(&recovery_job)
        .await
        .unwrap_or_else(|_| unreachable!());
    let first = store
        .claim_next("crashed-worker", now, 1)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let mut checkpointed = first.job.clone();
    let expected = checkpointed.aggregate_version().value();
    checkpointed
        .start_step(
            "crashed-worker",
            &first.lease_token,
            first.fence_version,
            ProcessingStepKind::ValidateSource,
            now,
        )
        .unwrap_or_else(|_| unreachable!());
    store
        .save(&checkpointed, expected)
        .await
        .unwrap_or_else(|_| unreachable!());
    store
        .start(
            &StepCheckpoint {
                job_id: checkpointed.id(),
                tenant_id: tenant,
                step_kind: ProcessingStepKind::ValidateSource,
                attempt_number: checkpointed.attempt_count(),
                checkpoint_json: serde_json::json!({"content_hash":"test"}),
                updated_at: now,
            },
            checkpointed.aggregate_version().value(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    let expected = checkpointed.aggregate_version().value();
    checkpointed
        .complete_step(
            "crashed-worker",
            &first.lease_token,
            first.fence_version,
            ProcessingStepKind::ValidateSource,
            now,
        )
        .unwrap_or_else(|_| unreachable!());
    store
        .save(&checkpointed, expected)
        .await
        .unwrap_or_else(|_| unreachable!());
    store
        .complete(
            checkpointed.id(),
            tenant,
            ProcessingStepKind::ValidateSource,
            checkpointed.attempt_count(),
            checkpointed.aggregate_version().value(),
            now,
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    let reclaimed_at = now + Duration::seconds(2);
    assert_eq!(store.reclaim_expired(reclaimed_at).await.unwrap_or(0), 1);
    let second = store
        .claim_next("recovery-worker", reclaimed_at, 30)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    assert_eq!(second.job.current_step(), ProcessingStepKind::DetectType);
    assert!(store
        .heartbeat(
            first.job.id(),
            "crashed-worker",
            &first.lease_token,
            first.fence_version,
            reclaimed_at,
            30,
        )
        .await
        .is_err());
    assert!(store
        .release(
            second.job.id(),
            "recovery-worker",
            &second.lease_token,
            second.fence_version,
            reclaimed_at,
        )
        .await
        .is_ok());

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
