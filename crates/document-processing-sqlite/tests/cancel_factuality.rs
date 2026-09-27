//! Wave 3 (PLAN-0014) cancellation factuality across every durable transition
//! path (failure-matrix row D06 cancel x completion, plus the cancel arms of
//! the outbox/audit rows).
//!
//! Contract under test, from AGENTS.md §9 ("events express facts that have
//! already happened") and `WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE` §13
//! ("cancellation must be audited", cooperative semantics):
//! - a cancellation REQUEST writes a `processing_cancel_requested` audit and
//!   no `document.processing.cancelled.v1` fact event while the job is still
//!   running;
//! - the `cancelled.v1` event and the `processing_cancelled` audit are written
//!   exactly once, at the transition into `cancelled`, whichever path performs
//!   it (immediate AI cancel, step boundary, cancelled failure disposition, or
//!   lease-expiry reclaim).
use chrono::{Duration, Utc};
use document_processing::ports::{
    ClassifiedProcessingFailure, ExecutionFence, ProcessingExecutionUnitOfWork,
    ProcessingFailureDisposition, ProcessingJobClaimPort, ProcessingJobCommandPort,
    TextArtifactReference,
};
use document_processing::{ProcessingJob, ProcessingStepKind};
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
    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .unwrap_or_else(|_| unreachable!());
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

async fn queue_job(
    store: &SqliteProcessingStore,
    tenant: Uuid,
    document: Uuid,
    user: Uuid,
) -> ProcessingJob {
    let job = ProcessingJob::queue(
        tenant,
        document,
        1,
        format!("cancel-{}", Uuid::now_v7()),
        user,
        3,
        Utc::now(),
    )
    .unwrap_or_else(|_| unreachable!());
    store.create(&job).await.unwrap_or_else(|_| unreachable!());
    job
}

async fn claim(store: &SqliteProcessingStore) -> ExecutionFence {
    let claimed = ProcessingJobClaimPort::claim_next(store, "business-test", Utc::now(), 60)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!("job must be claimable"));
    ExecutionFence::new(
        "business-test",
        claimed.lease_token.clone(),
        claimed.fence_version,
    )
}

async fn job_status(pool: &sqlx::SqlitePool, tenant: Uuid, job_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM document_processing_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant.to_string())
    .bind(job_id.to_string())
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| unreachable!("job row must exist"))
}

async fn outbox_count(pool: &sqlx::SqlitePool, tenant: Uuid, event_type: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?1 AND event_type = ?2",
    )
    .bind(tenant.to_string())
    .bind(event_type)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| unreachable!())
}

async fn audit_count(pool: &sqlx::SqlitePool, tenant: Uuid, action: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_processing_audit_events WHERE tenant_id = ?1 AND action = ?2",
    )
    .bind(tenant.to_string())
    .bind(action)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| unreachable!())
}

const CANCELLED_EVENT: &str = "document.processing.cancelled.v1";

/// While the job is still executing, a cancel request is intent only: it must
/// not publish the cancellation fact event and must not claim the fact in the
/// audit trail; the request itself must be audited.
#[tokio::test]
async fn cooperative_cancel_is_intent_only() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let _fence = claim(&store).await;

    let result = store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await;
    assert!(result.is_ok(), "cancel request must succeed: {result:?}");

    assert_eq!(
        job_status(&pool, tenant, job.id()).await,
        "running",
        "cooperative cancel must not fake the terminal state"
    );
    assert_eq!(
        outbox_count(&pool, tenant, CANCELLED_EVENT).await,
        0,
        "cancelled.v1 must not be emitted before the fact"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancelled").await,
        0,
        "the cancellation fact must not be audited before it happened"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1,
        "the cancellation request must be audited exactly once"
    );

    // Repeated requests stay idempotent and add no further evidence.
    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1,
        "duplicate cancel request must not duplicate the intent audit"
    );
}

/// Cancelling a job parked in `WaitingForAi` transitions immediately: this is
/// both intent and fact, so the request audit, exactly one cancelled.v1 event
/// and exactly one fact audit must all be present.
#[tokio::test]
async fn waiting_for_ai_cancel_publishes_fact_once() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let fence = claim(&store).await;
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
        .unwrap_or_else(|_| unreachable!());

    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());

    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(
        outbox_count(&pool, tenant, CANCELLED_EVENT).await,
        1,
        "the immediate cancel must publish the fact exactly once"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1
    );
    assert_eq!(audit_count(&pool, tenant, "processing_cancelled").await, 1);
}

/// The cooperative fact lands at the step boundary: request (intent) writes
/// no event, the transition into cancelled writes the event and the fact
/// audit — across both calls the event appears exactly once.
#[tokio::test]
async fn cancel_fact_lands_exactly_once_at_the_step_boundary() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
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
        .unwrap_or_else(|_| unreachable!());

    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        outbox_count(&pool, tenant, CANCELLED_EVENT).await,
        0,
        "request must not publish the fact event"
    );

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
        .unwrap_or_else(|_| unreachable!());

    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(
        outbox_count(&pool, tenant, CANCELLED_EVENT).await,
        1,
        "the transition must publish the fact exactly once"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancelled").await,
        1,
        "the cancellation must be audited at the transition"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancel_requested").await,
        1
    );
}

/// A worker reporting the Cancelled failure disposition also transitions into
/// cancelled; the fact audit must accompany the already-correct event.
#[tokio::test]
async fn cancelled_failure_disposition_audits_the_fact() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
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
        .unwrap_or_else(|_| unreachable!());
    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());

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
        .unwrap_or_else(|_| unreachable!());

    assert_eq!(job_status(&pool, tenant, job.id()).await, "cancelled");
    assert_eq!(
        outbox_count(&pool, tenant, CANCELLED_EVENT).await,
        1,
        "fact event exactly once"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancelled").await,
        1,
        "the cancellation fact must be audited"
    );
}

/// A crashed worker holding a cancelled job must not silence the fact: the
/// reclaim that moves the job into cancelled publishes the event and the fact
/// audit (cancel-priority reclaim, `SQLite` lane of D02/D06).
#[tokio::test]
async fn reclaim_of_cancelled_expired_job_publishes_the_fact() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let claimed = ProcessingJobClaimPort::claim_next(&store, "crash-test", Utc::now(), 1)
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!("job must be claimable"));
    assert_eq!(claimed.job.id(), job.id());
    store
        .cancel_processing(tenant, job.id(), user, Utc::now())
        .await
        .unwrap_or_else(|_| unreachable!());

    let reclaimed = store
        .reclaim_expired_jobs(Utc::now() + Duration::seconds(10))
        .await
        .unwrap_or_else(|_| unreachable!());

    assert_eq!(reclaimed, 1);
    assert_eq!(
        job_status(&pool, tenant, job.id()).await,
        "cancelled",
        "cancel-priority reclaim must land the job in cancelled"
    );
    assert_eq!(
        outbox_count(&pool, tenant, CANCELLED_EVENT).await,
        1,
        "reclaimed cancellation must publish the fact exactly once"
    );
    assert_eq!(
        audit_count(&pool, tenant, "processing_cancelled").await,
        1,
        "reclaimed cancellation must be audited"
    );
    assert_eq!(
        audit_count(&pool, tenant, "job_reclaimed").await,
        1,
        "the reclaim itself remains auditable"
    );
}
