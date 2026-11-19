//! Wave 3 (PLAN-0014) cancellation factuality on the production backend —
//! `PostgreSQL` mirror of `document-processing-sqlite/tests/cancel_factuality.rs`.
//! Requires running `PostgreSQL` with migrations applied. Run with:
//!   cargo test -p document-processing-postgres --test `cancel_factuality_pg` --all-features -- --include-ignored --test-threads=1
//!
//! Contract: the cancel REQUEST writes a `processing_cancel_requested` audit
//! and no fact event while the job is still executing; the `cancelled.v1`
//! event and `processing_cancelled` audit are written exactly once at the
//! transition into Cancelled on every path (immediate AI cancel, step
//! boundary, cancelled disposition, reclaim).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{Duration, Utc};
use document_processing::ports::{
    ClassifiedProcessingFailure, ExecutionFence, ProcessingExecutionUnitOfWork,
    ProcessingFailureDisposition, ProcessingJobClaimPort, ProcessingJobCommandPort,
    TextArtifactReference,
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
    .expect("document fixture must insert");
    (tenant, document, user)
}

async fn cleanup(pool: &PgPool, tenant: Uuid) {
    // Tenant-scoped deletion of only this test's own rows; never a shared
    // database sweep.
    sqlx::query("DELETE FROM document_processing_audit_events WHERE tenant_id = $1")
        .bind(tenant)
        .execute(pool)
        .await
        .expect("audit cleanup");
    // `outbox_events.tenant_id` is the legacy `VARCHAR(100)` column (migration
    // 001) and every production writer binds it as text; binding a `Uuid` here
    // would make PostgreSQL reject the comparison as `varchar = uuid`.
    sqlx::query("DELETE FROM outbox_events WHERE tenant_id = $1")
        .bind(tenant.to_string())
        .execute(pool)
        .await
        .expect("outbox cleanup");
    sqlx::query("DELETE FROM document_ai_tasks WHERE tenant_id = $1")
        .bind(tenant)
        .execute(pool)
        .await
        .expect("ai task cleanup");
    sqlx::query("DELETE FROM document_processing_steps WHERE tenant_id = $1")
        .bind(tenant)
        .execute(pool)
        .await
        .expect("step cleanup");
    sqlx::query("DELETE FROM document_processing_jobs WHERE tenant_id = $1")
        .bind(tenant)
        .execute(pool)
        .await
        .expect("job cleanup");
    sqlx::query("DELETE FROM documents WHERE tenant_id = $1")
        .bind(tenant)
        .execute(pool)
        .await
        .expect("document cleanup");
}

async fn queue_job(
    store: &PostgresProcessingStore,
    tenant: Uuid,
    document: Uuid,
    user: Uuid,
) -> ProcessingJob {
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("cancel-pg-{}", Uuid::now_v7()),
        user,
        3,
        Utc::now(),
    )
    .expect("fixture job");
    store.create(&job).await.expect("job must persist");
    job
}

async fn claim(store: &PostgresProcessingStore) -> ExecutionFence {
    let claimed = ProcessingJobClaimPort::claim_next(store, "business-test", Utc::now(), 60)
        .await
        .expect("claim must succeed")
        .expect("job must be claimable");
    ExecutionFence::new(
        "business-test",
        claimed.lease_token.clone(),
        claimed.fence_version,
    )
}

async fn run_pipeline_to_extract_text(
    store: &PostgresProcessingStore,
    tenant: Uuid,
    job_id: Uuid,
    fence: &ExecutionFence,
) {
    for step in [
        ProcessingStepKind::ValidateSource,
        ProcessingStepKind::DetectType,
    ] {
        store
            .start_step(tenant, job_id, step, fence, Utc::now())
            .await
            .expect("step must start");
        store
            .complete_step(tenant, job_id, step, None, fence, Utc::now())
            .await
            .expect("step must complete");
    }
    store
        .start_step(
            tenant,
            job_id,
            ProcessingStepKind::ExtractText,
            fence,
            Utc::now(),
        )
        .await
        .expect("extract text must start");
}

async fn job_status(pool: &PgPool, tenant: Uuid, job_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM document_processing_jobs WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(job_id)
    .fetch_one(pool)
    .await
    .expect("job row must exist")
}

async fn outbox_count(pool: &PgPool, tenant: Uuid, event_type: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(tenant.to_string())
    .bind(event_type)
    .fetch_one(pool)
    .await
    .expect("outbox count")
}

async fn audit_count(pool: &PgPool, tenant: Uuid, action: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_processing_audit_events WHERE tenant_id = $1 AND action = $2",
    )
    .bind(tenant)
    .bind(action)
    .fetch_one(pool)
    .await
    .expect("audit count")
}

const CANCELLED_EVENT: &str = "document.processing.cancelled.v1";

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
async fn cooperative_cancel_is_intent_only() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/business_platform".into());
    let pool = PgPool::connect(&url).await.expect("pool");
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let _fence = claim(&store).await;

    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .expect("cancel request must succeed");

    assert_eq!(
        job_status(&pool, tenant, job.id()).await,
        "running",
        "cooperative cancel must not fake the terminal state"
    );
    assert_eq!(outbox_count(&pool, tenant, CANCELLED_EVENT).await, 0);
    assert_eq!(audit_count(&pool, tenant, "processing_cancelled").await, 0);
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1
    );
    cleanup(&pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
async fn waiting_for_ai_cancel_publishes_fact_once() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/business_platform".into());
    let pool = PgPool::connect(&url).await.expect("pool");
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let fence = claim(&store).await;
    run_pipeline_to_extract_text(&store, tenant, job.id(), &fence).await;
    store
        .enqueue_ai_and_wait(
            tenant,
            job.id(),
            TextArtifactReference {
                key: format!(
                    "tenants/{tenant}/processing-jobs/{}/artifacts/text.txt",
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
        .expect("ai enqueue must succeed");

    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .expect("cancel must succeed");

    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(outbox_count(&pool, tenant, CANCELLED_EVENT).await, 1);
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1
    );
    assert_eq!(audit_count(&pool, tenant, "processing_cancelled").await, 1);
    cleanup(&pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
async fn cancel_fact_lands_exactly_once_at_the_step_boundary() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/business_platform".into());
    let pool = PgPool::connect(&url).await.expect("pool");
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let fence = claim(&store).await;
    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            &fence,
            Utc::now(),
        )
        .await
        .expect("step must start");
    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .expect("cancel request must succeed");
    assert_eq!(outbox_count(&pool, tenant, CANCELLED_EVENT).await, 0);

    store
        .complete_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            None,
            &fence,
            Utc::now(),
        )
        .await
        .expect("complete step must succeed");

    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(outbox_count(&pool, tenant, CANCELLED_EVENT).await, 1);
    assert_eq!(audit_count(&pool, tenant, "processing_cancelled").await, 1);
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1
    );
    cleanup(&pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
async fn cancelled_failure_disposition_audits_the_fact() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/business_platform".into());
    let pool = PgPool::connect(&url).await.expect("pool");
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let fence = claim(&store).await;
    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            &fence,
            Utc::now(),
        )
        .await
        .expect("step must start");
    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .expect("cancel request must succeed");

    store
        .retry_or_fail_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            ClassifiedProcessingFailure {
                code: "cancelled".to_string(),
                message: None,
                disposition: ProcessingFailureDisposition::Cancelled,
            },
            &fence,
            Utc::now(),
        )
        .await
        .expect("failure must be recorded");

    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(outbox_count(&pool, tenant, CANCELLED_EVENT).await, 1);
    assert_eq!(audit_count(&pool, tenant, "processing_cancelled").await, 1);
    cleanup(&pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL and migrations"]
async fn reclaim_of_cancelled_expired_job_publishes_the_fact() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/business_platform".into());
    let pool = PgPool::connect(&url).await.expect("pool");
    let (tenant, document, user) = setup(&pool).await;
    let store = PostgresProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let _claimed = ProcessingJobClaimPort::claim_next(&store, "crash-test", Utc::now(), 1)
        .await
        .expect("claim must succeed")
        .expect("job must be claimable");
    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .expect("cancel request must succeed");

    let reclaimed = store
        .reclaim_expired_jobs(Utc::now() + Duration::seconds(10))
        .await
        .expect("reclaim must succeed");

    assert_eq!(reclaimed, 1);
    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(outbox_count(&pool, tenant, CANCELLED_EVENT).await, 1);
    assert_eq!(audit_count(&pool, tenant, "processing_cancelled").await, 1);
    assert_eq!(audit_count(&pool, tenant, "job_reclaimed").await, 1);
    cleanup(&pool, tenant).await;
}
