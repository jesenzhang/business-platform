//! Wave 3 (PLAN-0014) adapter-level lease/fence/reclaim evidence (failure
//! matrix rows D01 stale worker rejected, D02 lease/reclaim/heartbeat/fence
//! consistency, D05 duplicate dispatch) on the local `SQLite` deployment.
//!
//! Pins the fencing contract from `WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE`
//! §10 against the adapter itself (not only the fake ports): a stale fence
//! must be rejected on every fenced mutation, rejection must leave zero
//! audit/outbox evidence, reclaim only fires on expiry, and the successor
//! lease always carries a strictly higher fence version.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{DateTime, Duration, Utc};
use document_processing::ports::{
    ClassifiedProcessingFailure, ExecutionFence, ProcessingExecutionUnitOfWork,
    ProcessingFailureDisposition, ProcessingJobClaimPort, ProcessingJobCommandPort,
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
        format!("fence-{}", Uuid::now_v7()),
        user,
        3,
        Utc::now(),
    )
    .expect("fixture job");
    store.create(&job).await.expect("job must persist");
    job
}

struct Claimed {
    fence: ExecutionFence,
    fence_version: i64,
}

async fn claim_at(
    store: &SqliteProcessingStore,
    worker: &str,
    now: DateTime<Utc>,
    lease_secs: i64,
) -> Claimed {
    let claimed = ProcessingJobClaimPort::claim_next(store, worker, now, lease_secs)
        .await
        .expect("claim must succeed")
        .expect("job must be claimable");
    let fence_version = claimed.fence_version;
    Claimed {
        fence: ExecutionFence::new(worker, claimed.lease_token.clone(), fence_version),
        fence_version,
    }
}

async fn evidence_counts(pool: &sqlx::SqlitePool, tenant: Uuid) -> (i64, i64) {
    let outbox =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?1")
            .bind(tenant.to_string())
            .fetch_one(pool)
            .await
            .expect("outbox count");
    let audit = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM document_processing_audit_events WHERE tenant_id = ?1",
    )
    .bind(tenant.to_string())
    .fetch_one(pool)
    .await
    .expect("audit count");
    (outbox, audit)
}

async fn job_version(pool: &sqlx::SqlitePool, tenant: Uuid, job_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT version FROM document_processing_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant.to_string())
    .bind(job_id.to_string())
    .fetch_one(pool)
    .await
    .expect("job row must exist")
}

fn permanent_failure() -> ClassifiedProcessingFailure {
    ClassifiedProcessingFailure {
        code: "probe".to_string(),
        message: None,
        disposition: ProcessingFailureDisposition::Permanent,
    }
}

/// After the lease dies and a successor claims the job, every fenced mutation
/// attempted with the stale fence must fail closed with `LeaseLost` and must
/// write no outbox/audit rows and not bump the aggregate version.
#[tokio::test]
async fn stale_fence_is_rejected_everywhere_without_evidence() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let stale = claim_at(&store, "worker-stale", Utc::now(), 1).await;

    // Lease dies; the recovery scan requeues the job.
    let reclaimed = store
        .reclaim_expired_jobs(Utc::now() + Duration::seconds(10))
        .await
        .expect("reclaim must succeed");
    assert_eq!(reclaimed, 1);

    // Successor claims with a strictly higher fence version.
    let fresh = claim_at(
        &store,
        "worker-fresh",
        Utc::now() + Duration::seconds(20),
        60,
    )
    .await;
    assert!(
        fresh.fence_version > stale.fence_version,
        "successor fence must strictly increase: {} -> {}",
        stale.fence_version,
        fresh.fence_version
    );

    let (outbox_before, audit_before) = evidence_counts(&pool, tenant).await;
    let version_before = job_version(&pool, tenant, job.id()).await;

    let start = store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            &stale.fence,
            Utc::now(),
        )
        .await;
    let complete = store
        .complete_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            None,
            &stale.fence,
            Utc::now(),
        )
        .await;
    let fail = store
        .retry_or_fail_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            permanent_failure(),
            &stale.fence,
            Utc::now(),
        )
        .await;
    let heartbeat = store
        .heartbeat_job(tenant, job.id(), &stale.fence, Utc::now(), 60)
        .await;
    let release = store
        .release_job(tenant, job.id(), &stale.fence, Utc::now())
        .await;

    for (name, outcome) in [
        ("start_step", start.map(|_| ())),
        ("complete_step", complete.map(|_| ())),
        ("retry_or_fail_step", fail.map(|_| ())),
        ("heartbeat_job", heartbeat.map(|_| ())),
        ("release_job", release),
    ] {
        assert!(
            matches!(
                outcome,
                Err(document_processing::ports::ProcessingRepositoryError::LeaseLost)
            ),
            "{name} with a stale fence must fail with LeaseLost, got {outcome:?}"
        );
    }

    let (outbox_after, audit_after) = evidence_counts(&pool, tenant).await;
    assert_eq!(
        (outbox_after, audit_after),
        (outbox_before, audit_before),
        "rejected stale-fence mutations must write no outbox or audit evidence"
    );
    assert_eq!(
        job_version(&pool, tenant, job.id()).await,
        version_before,
        "rejected stale-fence mutations must not bump the aggregate version"
    );
}

/// An unexpired lease must not be reclaimed by the recovery scan, and while
/// a valid lease is active no second worker may obtain the job.
#[tokio::test]
async fn active_lease_is_neither_reclaimed_nor_double_dispatched() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let _held = claim_at(&store, "worker-held", Utc::now(), 60).await;

    let reclaimed_now = store
        .reclaim_expired_jobs(Utc::now())
        .await
        .expect("reclaim scan must succeed");
    assert_eq!(reclaimed_now, 0, "live lease must not be reclaimed");

    let second = ProcessingJobClaimPort::claim_next(&store, "worker-second", Utc::now(), 60)
        .await
        .expect("scan must succeed");
    assert!(
        second.is_none(),
        "a job under a live lease must not be dispatched twice (job {})",
        job.id()
    );
}

/// Once the lease expires, heartbeat must fail closed so the old worker
/// cannot extend a dead lease.
#[tokio::test]
async fn heartbeat_after_lease_expiry_fails_closed() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let held = claim_at(&store, "worker-slow", Utc::now(), 1).await;

    let heartbeat = store
        .heartbeat_job(
            tenant,
            job.id(),
            &held.fence,
            Utc::now() + Duration::seconds(10),
            60,
        )
        .await;
    assert!(
        matches!(
            heartbeat,
            Err(document_processing::ports::ProcessingRepositoryError::LeaseLost)
        ),
        "heartbeat on an expired lease must be LeaseLost, got {heartbeat:?}"
    );
}

/// The successor worker's fenced mutations must succeed after a reclaim —
/// fencing must not lock the job out permanently (recovery works, not just
/// rejection).
#[tokio::test]
async fn successor_can_progress_after_reclaim() {
    let (pool, tenant, document, user) = setup().await;
    let store = SqliteProcessingStore::new(pool.clone());
    let job = queue_job(&store, tenant, document, user).await;
    let _stale = claim_at(&store, "worker-crashed", Utc::now(), 1).await;
    store
        .reclaim_expired_jobs(Utc::now() + Duration::seconds(10))
        .await
        .expect("reclaim must succeed");
    let fresh = claim_at(
        &store,
        "worker-successor",
        Utc::now() + Duration::seconds(20),
        60,
    )
    .await;

    store
        .start_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            &fresh.fence,
            Utc::now(),
        )
        .await
        .expect("successor must be able to start the step");
    store
        .complete_step(
            tenant,
            job.id(),
            ProcessingStepKind::ValidateSource,
            None,
            &fresh.fence,
            Utc::now(),
        )
        .await
        .expect("successor must be able to complete the step");

    let step_status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM document_processing_steps WHERE tenant_id = ?1 AND job_id = ?2 AND step_kind = 'validate_source'",
    )
    .bind(tenant.to_string())
    .bind(job.id().to_string())
    .fetch_optional(&pool)
    .await
    .expect("step query must run");
    assert_eq!(
        step_status.as_deref(),
        Some("succeeded"),
        "successor progress must be durably recorded"
    );
}
