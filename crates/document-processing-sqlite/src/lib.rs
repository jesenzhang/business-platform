//! `SQLite` persistence adapter for the durable document processing flow.
//!
//! `SQLite` is deliberately local/single-process. Writes use `BEGIN IMMEDIATE`
//! so independent adapter instances sharing a file still serialize the
//! idempotency read and all side effects.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use document_processing::domain::{
    CandidateReview, ExtractionCandidate, JobVersion, ProcessingJob, ProcessingJobStatus,
    ProcessingStepKind, ProcessingStepStatus,
};
use document_processing::ports::{
    AiTask, AiTaskPort, CandidateStore, ClaimedProcessingJob, ProcessingJobClaimPort,
    ProcessingJobCommandPort, ProcessingJobDetail, ProcessingJobQuery, ProcessingRepositoryError,
    ProcessingStepStore, StepCheckpoint,
};
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use uuid::Uuid;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const PROCESSING_MIGRATION_VERSION: i64 = 1;

/// Apply the processing schema without colliding with the Document `SQLite`
/// catalog. `SQLx`'s built-in migrator uses the global `_sqlx_migrations` table,
/// while the two bounded contexts intentionally keep independent catalogs in
/// the same local database.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_processing_migrations (version INTEGER PRIMARY KEY, checksum BLOB NOT NULL, applied_at TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT version FROM document_processing_migrations WHERE version = ?1",
    )
    .bind(PROCESSING_MIGRATION_VERSION)
    .fetch_optional(pool)
    .await?;
    if applied.is_some() {
        return Ok(());
    }

    // A test or an older local process may have applied the SQLx migrator
    // directly. Adopt that already-created schema into the independent
    // catalog instead of attempting to create duplicate tables.
    let schema_exists = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'document_processing_jobs'",
    )
    .fetch_optional(pool)
    .await?
    .is_some();
    if schema_exists {
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(PROCESSING_MIGRATION_VERSION)
        .bind(include_bytes!("../migrations/001_document_processing.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
        return Ok(());
    }

    let mut transaction = pool.begin().await?;
    sqlx::raw_sql(include_str!("../migrations/001_document_processing.sql"))
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
    )
    .bind(PROCESSING_MIGRATION_VERSION)
    .bind(include_bytes!("../migrations/001_document_processing.sql").as_slice())
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

#[derive(Clone)]
pub struct SqliteProcessingStore {
    pool: SqlitePool,
}

impl SqliteProcessingStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

pub type SqliteProcessingRepository = SqliteProcessingStore;

#[derive(Debug, FromRow)]
struct JobRow {
    id: String,
    tenant_id: String,
    document_id: String,
    content_revision: i64,
    request_key: String,
    status: String,
    current_step: String,
    attempt_count: i32,
    max_attempts: i32,
    next_attempt_at: String,
    cancel_requested_at: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    lease_owner: Option<String>,
    lease_token: Option<String>,
    lease_expires_at: Option<String>,
    fence_version: i64,
    version: i64,
    created_by: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct CandidateRow {
    payload: String,
}

#[derive(Debug, FromRow)]
struct ReviewRow {
    id: String,
    tenant_id: String,
    candidate_id: String,
    reviewer_id: String,
    decision: String,
    patch: Option<String>,
    comment: Option<String>,
    candidate_version: i64,
    created_at: String,
}

#[derive(Debug, FromRow)]
struct AiTaskRow {
    id: String,
    tenant_id: String,
    job_id: String,
    step_kind: String,
    status: String,
    input_artifact_id: Option<String>,
    attempt_count: i32,
    max_attempts: i32,
    lease_token: Option<String>,
    fence_version: i64,
    lease_expires_at: Option<String>,
}

#[allow(clippy::needless_pass_by_value)]
fn map_sql_error(error: sqlx::Error) -> ProcessingRepositoryError {
    match error {
        sqlx::Error::PoolClosed | sqlx::Error::PoolTimedOut | sqlx::Error::Io(_) => {
            ProcessingRepositoryError::Unavailable
        }
        _ => ProcessingRepositoryError::Failed,
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, ProcessingRepositoryError> {
    Uuid::parse_str(value).map_err(|_| ProcessingRepositoryError::Failed)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| ProcessingRepositoryError::Failed)
}

fn parse_optional_time(
    value: Option<String>,
) -> Result<Option<DateTime<Utc>>, ProcessingRepositoryError> {
    value.map(|value| parse_time(&value)).transpose()
}

fn to_job(row: JobRow) -> Result<ProcessingJob, ProcessingRepositoryError> {
    let lease = match (row.lease_owner, row.lease_token, row.lease_expires_at) {
        (Some(owner), Some(token), Some(expires_at)) => {
            Some((owner, token, parse_time(&expires_at)?, row.fence_version))
        }
        (None, None, None) => None,
        _ => return Err(ProcessingRepositoryError::Failed),
    };
    ProcessingJob::rehydrate_with_fence(
        parse_uuid(&row.id)?,
        parse_uuid(&row.tenant_id)?,
        parse_uuid(&row.document_id)?,
        row.content_revision,
        row.request_key,
        ProcessingJobStatus::try_from(row.status.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        ProcessingStepKind::try_from(row.current_step.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        row.attempt_count,
        row.max_attempts,
        parse_time(&row.next_attempt_at)?,
        parse_optional_time(row.cancel_requested_at)?,
        row.failure_code,
        row.failure_message,
        JobVersion::new(row.version).map_err(|_| ProcessingRepositoryError::Failed)?,
        parse_uuid(&row.created_by)?,
        parse_time(&row.created_at)?,
        parse_time(&row.updated_at)?,
        row.fence_version,
        lease,
    )
    .map_err(|_| ProcessingRepositoryError::Failed)
}

async fn load_job(
    connection: &mut SqliteConnection,
    tenant_id: Uuid,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id.to_string())
    .bind(job_id.to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    row.map(to_job).transpose()
}

async fn load_job_by_id(
    connection: &mut SqliteConnection,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE id = ?1",
    )
    .bind(job_id.to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    row.map(to_job).transpose()
}

async fn save_job(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    expected_version: i64,
) -> Result<(), ProcessingRepositoryError> {
    let lease = job.lease_snapshot();
    let (lease_owner, lease_token, lease_expires_at) =
        lease.map_or((None, None, None), |(owner, token, expires_at, _fence)| {
            (Some(owner), Some(token), Some(expires_at.to_rfc3339()))
        });
    let result = sqlx::query(
        "UPDATE document_processing_jobs SET status = ?1, current_step = ?2, attempt_count = ?3, max_attempts = ?4, next_attempt_at = ?5, cancel_requested_at = ?6, failure_code = ?7, failure_message = ?8, lease_owner = ?9, lease_token = ?10, lease_expires_at = ?11, fence_version = ?12, version = ?13, updated_at = ?14 WHERE tenant_id = ?15 AND id = ?16 AND version = ?17",
    )
    .bind(job.status().as_str())
    .bind(job.current_step().as_str())
    .bind(job.attempt_count())
    .bind(job.max_attempts())
    .bind(job.next_attempt_at().to_rfc3339())
    .bind(job.cancel_requested_at().map(|value| value.to_rfc3339()))
    .bind(job.failure_code())
    .bind(job.failure_message())
    .bind(lease_owner)
    .bind(lease_token)
    .bind(lease_expires_at)
    .bind(job.fence_version())
    .bind(job.aggregate_version().value())
    .bind(job.updated_at().to_rfc3339())
    .bind(job.tenant_id().to_string())
    .bind(job.id().to_string())
    .bind(expected_version)
    .execute(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    if result.rows_affected() != 1 {
        return Err(ProcessingRepositoryError::Conflict);
    }
    Ok(())
}

async fn insert_steps(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
) -> Result<(), ProcessingRepositoryError> {
    for step in ProcessingStepKind::FIXED {
        sqlx::query(
            "INSERT INTO document_processing_steps (job_id, tenant_id, step_kind, status, attempt_number, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
        )
        .bind(job.id().to_string())
        .bind(job.tenant_id().to_string())
        .bind(step.as_str())
        .bind(ProcessingStepStatus::Pending.as_str())
        .bind(job.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    }
    Ok(())
}

#[async_trait]
impl ProcessingJobCommandPort for SqliteProcessingStore {
    async fn create(
        &self,
        job: &ProcessingJob,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let existing = sqlx::query_as::<_, JobRow>(
            "SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE tenant_id = ?1 AND document_id = ?2 AND request_key = ?3",
        )
        .bind(job.tenant_id().to_string())
        .bind(job.document_id().to_string())
        .bind(job.request_key())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        if let Some(existing) = existing {
            let existing = to_job(existing)?;
            if existing.document_content_revision() != job.document_content_revision() {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                return Err(ProcessingRepositoryError::IdempotencyConflict);
            }
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            return Ok(existing);
        }
        let result = sqlx::query(
            "INSERT INTO document_processing_jobs (id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, NULL, NULL, NULL, NULL, 0, ?11, ?12, ?13, ?14)",
        )
        .bind(job.id().to_string())
        .bind(job.tenant_id().to_string())
        .bind(job.document_id().to_string())
        .bind(job.document_content_revision())
        .bind(job.request_key())
        .bind(job.status().as_str())
        .bind(job.current_step().as_str())
        .bind(job.attempt_count())
        .bind(job.max_attempts())
        .bind(job.next_attempt_at().to_rfc3339())
        .bind(job.aggregate_version().value())
        .bind(job.created_by().to_string())
        .bind(job.created_at().to_rfc3339())
        .bind(job.updated_at().to_rfc3339())
        .execute(&mut *connection)
        .await;
        if let Err(error) = result {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(map_sql_error(error));
        }
        insert_steps(&mut connection, job).await?;
        sqlx::query(
            "INSERT INTO outbox_events (event_id, event_type, tenant_id, aggregate_id, aggregate_type, payload, schema_version, occurred_at, published) VALUES (?1, 'document.processing.requested.v1', ?2, ?3, 'document_processing_job', ?4, 'v1', ?5, 0)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(job.tenant_id().to_string())
        .bind(job.id().to_string())
        .bind(
            serde_json::json!({
                "job_id": job.id(),
                "document_id": job.document_id(),
                "content_revision": job.document_content_revision()
            })
            .to_string(),
        )
        .bind(job.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        Ok(job.clone())
    }

    async fn load(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        load_job(&mut connection, tenant_id, job_id).await
    }

    async fn save(
        &self,
        job: &ProcessingJob,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let result = save_job(&mut connection, job, expected_version).await;
        match result {
            Ok(()) => {
                sqlx::query("COMMIT")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sql_error)?;
                Ok(())
            }
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                Err(error)
            }
        }
    }

    async fn request_cancel(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let Some(mut job) = load_job(&mut connection, tenant_id, job_id).await? else {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        if let Err(error) = job.request_cancel(Utc::now()) {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            let _ = error;
            return Err(ProcessingRepositoryError::Failed);
        }
        save_job(&mut connection, &job, expected).await?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        Ok(job)
    }
}

#[async_trait]
impl ProcessingJobClaimPort for SqliteProcessingStore {
    async fn claim_next(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError> {
        if worker_id.trim().is_empty() || lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let row = sqlx::query_as::<_, JobRow>(
            "SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE status = 'queued' AND next_attempt_at <= ?1 AND (lease_expires_at IS NULL OR lease_expires_at <= ?1) ORDER BY created_at, id LIMIT 1",
        )
        .bind(now.to_rfc3339())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        let Some(row) = row else {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            return Ok(None);
        };
        let mut job = to_job(row)?;
        let expected = job.aggregate_version().value();
        let token = Uuid::now_v7().to_string();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let fence_version = job
            .claim(worker_id.to_string(), token.clone(), expires_at, now)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        save_job(&mut connection, &job, expected).await?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        Ok(Some(ClaimedProcessingJob {
            job,
            lease_token: token,
            fence_version,
            lease_expires_at: expires_at,
        }))
    }

    async fn heartbeat(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(mut job) = load_job_by_id(&mut connection, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        job.heartbeat(worker_id, lease_token, fence_version, expires_at, now)
            .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job(&mut connection, &job, expected).await?;
        Ok(expires_at)
    }

    async fn release(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(mut job) = load_job_by_id(&mut connection, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        job.release(worker_id, lease_token, fence_version, now)
            .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job(&mut connection, &job, expected).await
    }

    async fn reclaim_expired(&self, now: DateTime<Utc>) -> Result<u64, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let rows = sqlx::query_as::<_, JobRow>(
            "SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE lease_expires_at IS NOT NULL AND lease_expires_at <= ?1",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        let mut reclaimed = 0_u64;
        for row in rows {
            let mut job = to_job(row)?;
            let expected = job.aggregate_version().value();
            if job
                .reclaim_expired(now)
                .map_err(|_| ProcessingRepositoryError::Failed)?
            {
                save_job(&mut connection, &job, expected).await?;
                reclaimed = reclaimed.saturating_add(1);
            }
        }
        Ok(reclaimed)
    }
}

#[async_trait]
impl ProcessingJobQuery for SqliteProcessingStore {
    async fn detail(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJobDetail>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(job) = load_job(&mut connection, tenant_id, job_id).await? else {
            return Ok(None);
        };
        let candidate: Option<ExtractionCandidate> = sqlx::query_as::<_, CandidateRow>(
            "SELECT payload FROM document_extraction_candidates WHERE tenant_id = ?1 AND job_id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?
        .map(|row| serde_json::from_str(&row.payload).map_err(|_| ProcessingRepositoryError::Failed))
        .transpose()?;
        let review = if let Some(candidate) = candidate.as_ref() {
            sqlx::query_as::<_, ReviewRow>(
                "SELECT id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at FROM document_extraction_reviews WHERE tenant_id = ?1 AND candidate_id = ?2",
            )
            .bind(tenant_id.to_string())
            .bind(candidate.id().to_string())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?
            .map(to_review)
            .transpose()?
        } else {
            None
        };
        Ok(Some(ProcessingJobDetail {
            job,
            candidate,
            review,
        }))
    }

    async fn list_for_document(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<ProcessingJobDetail>, ProcessingRepositoryError> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM document_processing_jobs WHERE tenant_id = ?1 AND document_id = ?2 ORDER BY created_at DESC",
        )
        .bind(tenant_id.to_string())
        .bind(document_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sql_error)?;
        let mut details = Vec::with_capacity(ids.len());
        for id in ids {
            let id = parse_uuid(&id)?;
            if let Some(detail) = self.detail(tenant_id, id).await? {
                details.push(detail);
            }
        }
        Ok(details)
    }
}

fn to_review(row: ReviewRow) -> Result<CandidateReview, ProcessingRepositoryError> {
    Ok(CandidateReview {
        id: parse_uuid(&row.id)?,
        tenant_id: parse_uuid(&row.tenant_id)?,
        candidate_id: parse_uuid(&row.candidate_id)?,
        reviewer_id: parse_uuid(&row.reviewer_id)?,
        decision: match row.decision.as_str() {
            "accepted" => document_processing::ReviewDecision::Accepted,
            "edited" => document_processing::ReviewDecision::Edited,
            "rejected" => document_processing::ReviewDecision::Rejected,
            _ => return Err(ProcessingRepositoryError::Failed),
        },
        patch: row
            .patch
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| ProcessingRepositoryError::Failed)
            })
            .transpose()?,
        comment: row.comment,
        candidate_version: row.candidate_version,
        created_at: parse_time(&row.created_at)?,
    })
}

#[async_trait]
impl ProcessingStepStore for SqliteProcessingStore {
    async fn start(
        &self,
        checkpoint: &StepCheckpoint,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'running', started_at = ?1, checkpoint_json = ?2, updated_at = ?1 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?3 AND id = ?4 AND version = ?7)")
            .bind(checkpoint.updated_at.to_rfc3339())
            .bind(checkpoint.checkpoint_json.to_string())
            .bind(checkpoint.tenant_id.to_string())
            .bind(checkpoint.job_id.to_string())
            .bind(checkpoint.step_kind.as_str())
            .bind(checkpoint.attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(ProcessingRepositoryError::Conflict)
        }
    }

    async fn checkpoint(
        &self,
        checkpoint: &StepCheckpoint,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET checkpoint_json = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?3 AND id = ?4 AND version = ?7)")
            .bind(checkpoint.checkpoint_json.to_string())
            .bind(checkpoint.updated_at.to_rfc3339())
            .bind(checkpoint.tenant_id.to_string())
            .bind(checkpoint.job_id.to_string())
            .bind(checkpoint.step_kind.as_str())
            .bind(checkpoint.attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::NotFound);
        }
        Ok(())
    }

    async fn complete(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
        step_kind: ProcessingStepKind,
        attempt_number: i32,
        expected_version: i64,
        finished_at: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'succeeded', finished_at = ?1, updated_at = ?1 WHERE tenant_id = ?2 AND job_id = ?3 AND step_kind = ?4 AND attempt_number = ?5 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?2 AND id = ?3 AND version = ?6)")
            .bind(finished_at.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(job_id.to_string())
            .bind(step_kind.as_str())
            .bind(attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::NotFound);
        }
        Ok(())
    }

    async fn fail(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
        step_kind: ProcessingStepKind,
        attempt_number: i32,
        failure_code: &str,
        expected_version: i64,
        finished_at: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'failed', failure_code = ?1, finished_at = ?2, updated_at = ?2 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?3 AND id = ?4 AND version = ?7)")
            .bind(failure_code)
            .bind(finished_at.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(job_id.to_string())
            .bind(step_kind.as_str())
            .bind(attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::NotFound);
        }
        Ok(())
    }
}

#[async_trait]
impl CandidateStore for SqliteProcessingStore {
    async fn save_candidate(
        &self,
        candidate: &ExtractionCandidate,
    ) -> Result<(), ProcessingRepositoryError> {
        let payload =
            serde_json::to_string(candidate).map_err(|_| ProcessingRepositoryError::Failed)?;
        let evidence = serde_json::to_string(&candidate.evidence)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        sqlx::query("INSERT INTO document_extraction_candidates (id, tenant_id, job_id, schema_version, payload, evidence, provider, model, prompt_version, version, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT(tenant_id, job_id) DO UPDATE SET payload = excluded.payload, evidence = excluded.evidence")
            .bind(candidate.id().to_string())
            .bind(candidate.tenant_id().to_string())
            .bind(candidate.job_id().to_string())
            .bind(&candidate.schema_version)
            .bind(payload)
            .bind(evidence)
            .bind(&candidate.provider)
            .bind(&candidate.model)
            .bind(&candidate.prompt_version)
            .bind(candidate.version())
            .bind(candidate.created_at().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        Ok(())
    }

    async fn get_candidate(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ExtractionCandidate>, ProcessingRepositoryError> {
        let row = sqlx::query_as::<_, CandidateRow>(
            "SELECT payload FROM document_extraction_candidates WHERE tenant_id = ?1 AND job_id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sql_error)?;
        row.map(|row| {
            serde_json::from_str(&row.payload).map_err(|_| ProcessingRepositoryError::Failed)
        })
        .transpose()
    }

    async fn save_review(&self, review: &CandidateReview) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("INSERT INTO document_extraction_reviews (id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(tenant_id, candidate_id) DO NOTHING")
            .bind(review.id.to_string())
            .bind(review.tenant_id.to_string())
            .bind(review.candidate_id.to_string())
            .bind(review.reviewer_id.to_string())
            .bind(review.decision.as_str())
            .bind(review.patch.as_ref().map(ToString::to_string))
            .bind(&review.comment)
            .bind(review.candidate_version)
            .bind(review.created_at.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::Conflict);
        }
        Ok(())
    }
}

fn to_ai_task(row: AiTaskRow) -> Result<AiTask, ProcessingRepositoryError> {
    Ok(AiTask {
        id: parse_uuid(&row.id)?,
        tenant_id: parse_uuid(&row.tenant_id)?,
        job_id: parse_uuid(&row.job_id)?,
        step_kind: ProcessingStepKind::try_from(row.step_kind.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        status: row.status,
        input_artifact_id: row.input_artifact_id,
        attempt_count: row.attempt_count,
        max_attempts: row.max_attempts,
        lease_token: row.lease_token,
        fence_version: row.fence_version,
        lease_expires_at: row
            .lease_expires_at
            .map(|value| parse_time(&value))
            .transpose()?,
    })
}

#[async_trait]
impl AiTaskPort for SqliteProcessingStore {
    async fn enqueue(&self, task: &AiTask) -> Result<(), ProcessingRepositoryError> {
        sqlx::query("INSERT INTO document_ai_tasks (id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, next_attempt_at, fence_version, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6, ?7, ?8, 0, ?9, ?9)")
            .bind(task.id.to_string())
            .bind(task.tenant_id.to_string())
            .bind(task.job_id.to_string())
            .bind(task.step_kind.as_str())
            .bind(&task.input_artifact_id)
            .bind(task.attempt_count)
            .bind(task.max_attempts)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        Ok(())
    }

    async fn claim_next(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<AiTask>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let row = sqlx::query_as::<_, AiTaskRow>("SELECT id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, lease_token, fence_version, lease_expires_at FROM document_ai_tasks WHERE status = 'queued' AND next_attempt_at <= ?1 ORDER BY created_at, id LIMIT 1")
            .bind(now.to_rfc3339())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let Some(row) = row else {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            return Ok(None);
        };
        let id = row.id.clone();
        let fence = row.fence_version.saturating_add(1);
        let token = Uuid::now_v7().to_string();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        sqlx::query("UPDATE document_ai_tasks SET status = 'running', lease_owner = ?1, lease_token = ?2, lease_expires_at = ?3, fence_version = ?4, attempt_count = attempt_count + 1, updated_at = ?5 WHERE id = ?6 AND status = 'queued' AND fence_version = ?7")
            .bind(worker_id)
            .bind(&token)
            .bind(expires_at.to_rfc3339())
            .bind(fence)
            .bind(now.to_rfc3339())
            .bind(&id)
            .bind(row.fence_version)
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let updated = sqlx::query_as::<_, AiTaskRow>("SELECT id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, lease_token, fence_version, lease_expires_at FROM document_ai_tasks WHERE id = ?1")
            .bind(id)
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        to_ai_task(updated).map(Some)
    }

    async fn heartbeat(
        &self,
        task_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE document_ai_tasks SET lease_expires_at = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_token = ?5 AND fence_version = ?6 AND lease_expires_at > ?2")
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(task_id.to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(())
    }

    async fn complete(
        &self,
        task_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        candidate_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_ai_tasks SET status = 'succeeded', output_candidate_id = ?1, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_token = ?5 AND fence_version = ?6")
            .bind(candidate_id.to_string())
            .bind(now.to_rfc3339())
            .bind(task_id.to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(())
    }

    async fn fail(
        &self,
        task_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_ai_tasks SET status = CASE WHEN attempt_count < max_attempts THEN 'queued' ELSE 'failed' END, failure_code = ?1, next_attempt_at = ?2, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_token = ?5 AND fence_version = ?6")
            .bind(failure_code)
            .bind(now.to_rfc3339())
            .bind(task_id.to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(())
    }
}
