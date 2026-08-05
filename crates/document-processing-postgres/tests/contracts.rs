use chrono::{Duration, Utc};
use document_processing::ports::{
    CompleteAiTaskCommand, ExecutionFence, FinalizeReviewCommand, ProcessingExecutionUnitOfWork,
    ProcessingJobClaimPort, ProcessingJobCommandPort, ProcessingJobQuery,
    ProcessingRepositoryError, TextArtifactReference,
};
use document_processing::{
    CandidateReview, DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionRequest,
    ProcessingJob, ProcessingJobStatus, ProcessingStepKind, ReviewDecision,
};
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
    let detail = store
        .detail(tenant, job.id())
        .await
        .unwrap_or_else(|_| unreachable!("processing job detail failed"));
    assert!(detail.is_some());

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
    let fence = ExecutionFence::new(
        "crashed-worker",
        first.lease_token.clone(),
        first.fence_version,
    );
    store
        .start_step(
            tenant,
            first.job.id(),
            ProcessingStepKind::ValidateSource,
            &fence,
            now,
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    store
        .complete_step(
            tenant,
            first.job.id(),
            ProcessingStepKind::ValidateSource,
            None,
            &fence,
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

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
async fn postgres_processing_contract_reclaims_expired_ai_task() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    let pool = PgPool::connect(&url)
        .await
        .unwrap_or_else(|_| unreachable!());
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let created_at = Utc::now() - Duration::seconds(1);
    let now = Utc::now();
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("ai-reclaim-{}", Uuid::now_v7()),
        user,
        3,
        created_at,
    )
    .unwrap_or_else(|_| unreachable!());
    store.create(&job).await.unwrap_or_else(|_| unreachable!());

    sqlx::query(
        "UPDATE document_processing_jobs SET status = 'waiting_for_ai', current_step = 'extract_fields', updated_at = $1 WHERE tenant_id = $2 AND id = $3",
    )
    .bind(now)
    .bind(tenant)
    .bind(job.id())
    .execute(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());

    let task_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO document_ai_tasks (id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, next_attempt_at, lease_owner, lease_token, lease_expires_at, fence_version, created_at, updated_at) VALUES ($1, $2, $3, 'extract_fields', 'running', 'artifact-key', 0, 3, $4, 'crashed-ai-worker', 'stale-token', $5, 1, $6, $6)",
    )
    .bind(task_id)
    .bind(tenant)
    .bind(job.id())
    .bind(now)
    .bind(now - Duration::seconds(1))
    .bind(now)
    .execute(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());

    assert_eq!(
        ProcessingExecutionUnitOfWork::reclaim_expired_ai_tasks(&store, now)
            .await
            .unwrap_or_else(|_| unreachable!()),
        1
    );
    let (status, lease_owner, lease_token, lease_expires_at): (
        String,
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<Utc>>,
    ) = sqlx::query_as(
        "SELECT status, lease_owner, lease_token, lease_expires_at FROM document_ai_tasks WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    assert_eq!(status, "queued");
    assert!(lease_owner.is_none());
    assert!(lease_token.is_none());
    assert!(lease_expires_at.is_none());

    sqlx::query("DELETE FROM document_processing_jobs WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(job.id())
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

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
#[allow(clippy::too_many_lines)]
async fn postgres_processing_revision_one_uow_is_atomic_and_replayable() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform".to_string()
    });
    let pool = PgPool::connect(&url)
        .await
        .unwrap_or_else(|_| unreachable!());
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let now = Utc::now() - Duration::seconds(1);
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("uow-{}", Uuid::now_v7()),
        user,
        3,
        now,
    )
    .unwrap_or_else(|_| unreachable!());
    store.create(&job).await.unwrap_or_else(|_| unreachable!());
    let claimed = store
        .claim_next("uow-business", Utc::now(), 30)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let mut claimed = claimed;
    for step in [
        ProcessingStepKind::ValidateSource,
        ProcessingStepKind::DetectType,
    ] {
        let fence = ExecutionFence::new(
            "uow-business",
            claimed.lease_token.clone(),
            claimed.fence_version,
        );
        store
            .start_step(tenant, job.id(), step, &fence, Utc::now())
            .await
            .unwrap_or_else(|_| unreachable!());
        store
            .complete_step(tenant, job.id(), step, None, &fence, Utc::now())
            .await
            .unwrap_or_else(|_| unreachable!());
        store
            .release(
                job.id(),
                "uow-business",
                &claimed.lease_token,
                claimed.fence_version,
                Utc::now(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        claimed = store
            .claim_next("uow-business", Utc::now(), 30)
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!());
        assert_eq!(claimed.job.current_step(), step.next().unwrap_or(step));
    }

    let fence = ExecutionFence::new(
        "uow-business",
        claimed.lease_token.clone(),
        claimed.fence_version,
    );
    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ExtractText,
            &fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    let task = store
        .enqueue_ai_and_wait(
            tenant,
            job.id(),
            TextArtifactReference {
                key: format!(
                    "tenants/{tenant}/processing-jobs/{}/artifacts/text/hash.txt",
                    job.id()
                ),
                content_hash: "hash".to_string(),
                content_revision: 1,
                byte_count: 10,
                line_count: 1,
                character_count: 10,
            },
            &fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    let waiting = store
        .detail(tenant, job.id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    assert_eq!(waiting.job.status(), ProcessingJobStatus::WaitingForAi);

    let ai_claim = store
        .claim_next_ai_task("uow-ai", Utc::now(), 30)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let ai_fence = ExecutionFence::new(
        "uow-ai",
        ai_claim.lease_token.clone().unwrap_or_default(),
        ai_claim.fence_version,
    );
    let candidate = DeterministicLocalExtractor
        .extract(ExtractionRequest {
            tenant_id: tenant,
            job_id: job.id(),
            content_revision: 1,
            content_type: "text/plain".to_string(),
            text: "UoW title\nbody".to_string(),
            line_count: 2,
            character_count: 14,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    let resumed = store
        .complete_ai_and_resume(
            CompleteAiTaskCommand {
                tenant_id: tenant,
                job_id: job.id(),
                task_id: task.id,
                fence: ai_fence,
                candidate: candidate.clone(),
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        resumed.current_step(),
        ProcessingStepKind::ValidateCandidate
    );
    assert_eq!(resumed.status(), ProcessingJobStatus::Queued);

    let candidate_claim = store
        .claim_next("uow-business", Utc::now(), 30)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let candidate_fence = ExecutionFence::new(
        "uow-business",
        candidate_claim.lease_token.clone(),
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
        .unwrap_or_else(|_| unreachable!());
    let review_job = store
        .save_candidate_and_wait_for_review(
            tenant,
            job.id(),
            &candidate,
            &candidate_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(review_job.status(), ProcessingJobStatus::WaitingForReview);
    let review = CandidateReview {
        id: Uuid::now_v7(),
        tenant_id: tenant,
        candidate_id: candidate.id(),
        reviewer_id: user,
        decision: ReviewDecision::Accepted,
        patch: None,
        comment: None,
        candidate_version: candidate.version(),
        created_at: Utc::now(),
    };
    sqlx::query("DROP TRIGGER IF EXISTS test_processing_review_failure_trigger ON document_extraction_reviews")
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    sqlx::query("CREATE OR REPLACE FUNCTION test_processing_review_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.comment = 'inject-review-failure' THEN RAISE EXCEPTION 'injected review failure'; END IF; RETURN NEW; END; $$")
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    sqlx::query("CREATE TRIGGER test_processing_review_failure_trigger AFTER INSERT ON document_extraction_reviews FOR EACH ROW EXECUTE FUNCTION test_processing_review_failure()")
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    let mut rollback_review = review.clone();
    rollback_review.id = Uuid::now_v7();
    rollback_review.comment = Some("inject-review-failure".to_string());
    let rollback = store
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id: tenant,
                job_id: job.id(),
                idempotency_key: "review-contract-rollback".to_string(),
                request_fingerprint: "b".repeat(64),
                review: rollback_review,
            },
            Utc::now(),
        )
        .await;
    assert!(rollback.is_err());
    let review_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_extraction_reviews WHERE tenant_id = $1 AND candidate_id = $2",
    )
    .bind(tenant)
    .bind(candidate.id())
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    assert_eq!(review_count, 0);
    let after_rollback = store
        .detail(tenant, job.id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        after_rollback.job.status(),
        ProcessingJobStatus::WaitingForReview
    );
    sqlx::query(
        "DROP TRIGGER test_processing_review_failure_trigger ON document_extraction_reviews",
    )
    .execute(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    sqlx::query("DROP FUNCTION test_processing_review_failure()")
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    let finalized = store
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id: tenant,
                job_id: job.id(),
                idempotency_key: "review-contract-1".to_string(),
                request_fingerprint: "a".repeat(64),
                review: review.clone(),
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!finalized.replayed);
    assert_eq!(finalized.job.status(), ProcessingJobStatus::Succeeded);
    let replayed = store
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id: tenant,
                job_id: job.id(),
                idempotency_key: "review-contract-1".to_string(),
                request_fingerprint: "a".repeat(64),
                review: review.clone(),
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replayed.replayed);
    let conflict = store
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id: tenant,
                job_id: job.id(),
                idempotency_key: "review-contract-1".to_string(),
                request_fingerprint: "c".repeat(64),
                review,
            },
            Utc::now(),
        )
        .await;
    assert!(matches!(
        conflict,
        Err(ProcessingRepositoryError::IdempotencyConflict)
    ));

    let stale = store
        .complete_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateCandidate,
            None,
            &fence,
            Utc::now(),
        )
        .await;
    assert!(matches!(
        stale,
        Err(document_processing::ProcessingRepositoryError::LeaseLost)
    ));

    let cancelled_job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("cancel-{}", Uuid::now_v7()),
        user,
        3,
        Utc::now(),
    )
    .unwrap_or_else(|_| unreachable!());
    store
        .create(&cancelled_job)
        .await
        .unwrap_or_else(|_| unreachable!());
    let cancelled = store
        .cancel_processing(tenant, cancelled_job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(cancelled.status(), ProcessingJobStatus::Cancelled);
    let replay_cancel = store
        .cancel_processing(tenant, cancelled_job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(replay_cancel.status(), ProcessingJobStatus::Cancelled);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_processing_audit_events WHERE tenant_id = $1 AND job_id = $2",
    )
    .bind(tenant)
    .bind(job.id())
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    assert!(audit_count >= 8);
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = $1 AND aggregate_id = $2",
    )
    .bind(tenant.to_string())
    .bind(job.id().to_string())
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|_| unreachable!());
    assert!(outbox_count >= 8);

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
