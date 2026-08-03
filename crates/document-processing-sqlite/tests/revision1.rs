use chrono::Utc;
use document_processing::ports::{
    AiTaskPort, CompleteAiTaskCommand, ExecutionFence, FinalizeReviewCommand,
    ProcessingExecutionUnitOfWork, ProcessingJobClaimPort, ProcessingJobCommandPort,
    ProcessingJobQuery, TextArtifactReference,
};
use document_processing::{
    CandidateReview, DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionRequest,
    ProcessingJob, ProcessingStepKind, ReviewDecision,
};
use document_processing_sqlite::{run_migrations, SqliteProcessingStore};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Executor;
use uuid::Uuid;

async fn setup() -> (sqlx::SqlitePool, Uuid, Uuid, Uuid) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap_or_else(|_| unreachable!());
    pool.execute("PRAGMA foreign_keys = ON").await.ok();
    pool.execute("CREATE TABLE documents (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, original_filename TEXT NOT NULL, content_type TEXT NOT NULL, object_key TEXT NOT NULL, status TEXT NOT NULL, version INTEGER NOT NULL, content_revision INTEGER NOT NULL, created_by TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)")
        .await
        .unwrap_or_else(|_| unreachable!());
    pool.execute("CREATE TABLE outbox_events (event_id TEXT PRIMARY KEY, event_type TEXT NOT NULL, tenant_id TEXT NOT NULL, aggregate_id TEXT NOT NULL, aggregate_type TEXT NOT NULL, payload TEXT NOT NULL, schema_version TEXT NOT NULL, occurred_at TEXT NOT NULL, published INTEGER NOT NULL DEFAULT 0)")
        .await
        .unwrap_or_else(|_| unreachable!());
    run_migrations(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    let tenant = Uuid::now_v7();
    let document = Uuid::now_v7();
    let user = Uuid::now_v7();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, created_by, created_at, updated_at) VALUES (?1, ?2, 'source.txt', 'text/plain', 'tenants/source', 'active', 1, 1, ?3, ?4, ?4)")
        .bind(document.to_string())
        .bind(tenant.to_string())
        .bind(user.to_string())
        .bind(now)
        .execute(&pool)
        .await
        .unwrap_or_else(|_| unreachable!());
    (pool, tenant, document, user)
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn revision_one_uow_commits_fixed_pipeline_and_review_atomically() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("revision-{}", Uuid::now_v7()),
        user,
        3,
        Utc::now(),
    )
    .unwrap_or_else(|_| unreachable!());
    store.create(&job).await.unwrap_or_else(|_| unreachable!());
    let claimed = ProcessingJobClaimPort::claim_next(&store, "business-test", Utc::now(), 60)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let fence = ExecutionFence::new(
        "business-test",
        claimed.lease_token.clone(),
        claimed.fence_version,
    );
    for step in [
        ProcessingStepKind::ValidateSource,
        ProcessingStepKind::DetectType,
    ] {
        store
            .start_step(tenant, job.id(), step, &fence, Utc::now())
            .await
            .unwrap_or_else(|_| unreachable!());
        store
            .complete_step(tenant, job.id(), step, None, &fence, Utc::now())
            .await
            .unwrap_or_else(|_| unreachable!());
    }
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
    let claimed_task = AiTaskPort::claim_next(&store, "ai-test", Utc::now(), 60)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let candidate = DeterministicLocalExtractor
        .extract(ExtractionRequest {
            tenant_id: tenant,
            job_id: job.id(),
            content_revision: 1,
            content_type: "text/plain".to_string(),
            text: "Title\nbody".to_string(),
            line_count: 2,
            character_count: 10,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    store
        .complete_ai_and_resume(
            CompleteAiTaskCommand {
                tenant_id: tenant,
                job_id: job.id(),
                task_id: task.id,
                fence: ExecutionFence::new(
                    "ai-test",
                    claimed_task.lease_token.clone().unwrap_or_default(),
                    claimed_task.fence_version,
                ),
                candidate: candidate.clone(),
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    let claimed_job = ProcessingJobClaimPort::claim_next(&store, "business-test-2", Utc::now(), 60)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    let validation_fence = ExecutionFence::new(
        "business-test-2",
        claimed_job.lease_token,
        claimed_job.fence_version,
    );
    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateCandidate,
            &validation_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    store
        .save_candidate_and_wait_for_review(
            tenant,
            job.id(),
            &candidate,
            &validation_fence,
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
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
    pool.execute("DROP TRIGGER IF EXISTS test_processing_review_failure_trigger")
        .await
        .unwrap_or_else(|_| unreachable!());
    pool.execute("CREATE TRIGGER test_processing_review_failure_trigger AFTER INSERT ON document_extraction_reviews WHEN NEW.comment = 'inject-review-failure' BEGIN SELECT RAISE(ABORT, 'injected review failure'); END")
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
                review: rollback_review,
            },
            Utc::now(),
        )
        .await;
    assert!(rollback.is_err());
    let review_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_extraction_reviews WHERE tenant_id = ?1 AND candidate_id = ?2",
    )
    .bind(tenant.to_string())
    .bind(candidate.id().to_string())
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
        document_processing::ProcessingJobStatus::WaitingForReview
    );
    pool.execute("DROP TRIGGER test_processing_review_failure_trigger")
        .await
        .unwrap_or_else(|_| unreachable!());
    let finalized = store
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id: tenant,
                job_id: job.id(),
                review: review.clone(),
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!finalized.replayed);
    assert_eq!(
        finalized.job.status(),
        document_processing::ProcessingJobStatus::Succeeded
    );
    let replay = store
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id: tenant,
                job_id: job.id(),
                review,
            },
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed);

    let cancel_job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("cancel-running-{}", Uuid::now_v7()),
        user,
        3,
        Utc::now(),
    )
    .unwrap_or_else(|_| unreachable!());
    store
        .create(&cancel_job)
        .await
        .unwrap_or_else(|_| unreachable!());
    let cancel_claim =
        ProcessingJobClaimPort::claim_next(&store, "cancel-business", Utc::now(), 60)
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!());
    let cancel_requested = store
        .cancel_processing(tenant, cancel_job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        cancel_requested.status(),
        document_processing::ProcessingJobStatus::Running
    );
    assert!(cancel_requested.cancel_requested_at().is_some());
    let cancelled = store
        .complete_step(
            tenant,
            cancel_job.id(),
            ProcessingStepKind::ValidateSource,
            None,
            &ExecutionFence::new(
                "cancel-business",
                cancel_claim.lease_token,
                cancel_claim.fence_version,
            ),
            Utc::now(),
        )
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        cancelled.status(),
        document_processing::ProcessingJobStatus::Cancelled
    );
    let cancel_replay = store
        .cancel_processing(tenant, cancel_job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        cancel_replay.status(),
        document_processing::ProcessingJobStatus::Cancelled
    );
}
