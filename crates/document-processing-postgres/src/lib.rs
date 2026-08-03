//! `PostgreSQL` persistence adapter for durable document processing.
//!
//! Production claiming uses row locks with `SKIP LOCKED`; every worker write
//! is fenced by the lease token and monotonically increasing fence version.

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
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresProcessingStore {
    pool: PgPool,
}

impl PostgresProcessingStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

pub type PostgresProcessingRepository = PostgresProcessingStore;

#[derive(Debug, FromRow)]
struct JobRow {
    id: Uuid,
    tenant_id: Uuid,
    document_id: Uuid,
    content_revision: i64,
    request_key: String,
    status: String,
    current_step: String,
    attempt_count: i32,
    max_attempts: i32,
    next_attempt_at: DateTime<Utc>,
    cancel_requested_at: Option<DateTime<Utc>>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    lease_owner: Option<String>,
    lease_token: Option<String>,
    lease_expires_at: Option<DateTime<Utc>>,
    fence_version: i64,
    version: i64,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct CandidateRow {
    payload: serde_json::Value,
}

#[derive(Debug, FromRow)]
struct ReviewRow {
    id: Uuid,
    tenant_id: Uuid,
    candidate_id: Uuid,
    reviewer_id: Uuid,
    decision: String,
    patch: Option<serde_json::Value>,
    comment: Option<String>,
    candidate_version: i64,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct AiTaskRow {
    id: Uuid,
    tenant_id: Uuid,
    job_id: Uuid,
    step_kind: String,
    status: String,
    input_artifact_id: Option<String>,
    attempt_count: i32,
    max_attempts: i32,
    lease_token: Option<String>,
    fence_version: i64,
    lease_expires_at: Option<DateTime<Utc>>,
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

fn to_job(row: JobRow) -> Result<ProcessingJob, ProcessingRepositoryError> {
    let lease = match (row.lease_owner, row.lease_token, row.lease_expires_at) {
        (Some(owner), Some(token), Some(expires_at)) => {
            Some((owner, token, expires_at, row.fence_version))
        }
        (None, None, None) => None,
        _ => return Err(ProcessingRepositoryError::Failed),
    };
    ProcessingJob::rehydrate_with_fence(
        row.id,
        row.tenant_id,
        row.document_id,
        row.content_revision,
        row.request_key,
        ProcessingJobStatus::try_from(row.status.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        ProcessingStepKind::try_from(row.current_step.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        row.attempt_count,
        row.max_attempts,
        row.next_attempt_at,
        row.cancel_requested_at,
        row.failure_code,
        row.failure_message,
        JobVersion::new(row.version).map_err(|_| ProcessingRepositoryError::Failed)?,
        row.created_by,
        row.created_at,
        row.updated_at,
        row.fence_version,
        lease,
    )
    .map_err(|_| ProcessingRepositoryError::Failed)
}

async fn load_job(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    sqlx::query_as::<_, JobRow>("SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(job_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?
        .map(to_job)
        .transpose()
}

async fn load_job_by_id(
    connection: &mut PgConnection,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    sqlx::query_as::<_, JobRow>("SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?
        .map(to_job)
        .transpose()
}

async fn save_job(
    connection: &mut PgConnection,
    job: &ProcessingJob,
    expected_version: i64,
) -> Result<(), ProcessingRepositoryError> {
    let lease = job.lease_snapshot();
    let (owner, token, expires_at) = lease
        .map_or((None, None, None), |(owner, token, expires_at, _fence)| {
            (Some(owner), Some(token), Some(expires_at))
        });
    let result = sqlx::query("UPDATE document_processing_jobs SET status = $1, current_step = $2, attempt_count = $3, max_attempts = $4, next_attempt_at = $5, cancel_requested_at = $6, failure_code = $7, failure_message = $8, lease_owner = $9, lease_token = $10, lease_expires_at = $11, fence_version = $12, version = $13, updated_at = $14 WHERE tenant_id = $15 AND id = $16 AND version = $17")
        .bind(job.status().as_str())
        .bind(job.current_step().as_str())
        .bind(job.attempt_count())
        .bind(job.max_attempts())
        .bind(job.next_attempt_at())
        .bind(job.cancel_requested_at())
        .bind(job.failure_code())
        .bind(job.failure_message())
        .bind(owner)
        .bind(token)
        .bind(expires_at)
        .bind(job.fence_version())
        .bind(job.aggregate_version().value())
        .bind(job.updated_at())
        .bind(job.tenant_id())
        .bind(job.id())
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
    connection: &mut PgConnection,
    job: &ProcessingJob,
) -> Result<(), ProcessingRepositoryError> {
    for step in ProcessingStepKind::FIXED {
        sqlx::query("INSERT INTO document_processing_steps (job_id, tenant_id, step_kind, status, attempt_number, created_at, updated_at) VALUES ($1, $2, $3, $4, 0, $5, $5)")
            .bind(job.id())
            .bind(job.tenant_id())
            .bind(step.as_str())
            .bind(ProcessingStepStatus::Pending.as_str())
            .bind(job.created_at())
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
    }
    Ok(())
}

#[async_trait]
impl ProcessingJobCommandPort for PostgresProcessingStore {
    async fn create(
        &self,
        job: &ProcessingJob,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_sql_error)?;
        let existing = sqlx::query_as::<_, JobRow>("SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE tenant_id = $1 AND document_id = $2 AND request_key = $3 FOR UPDATE")
            .bind(job.tenant_id())
            .bind(job.document_id())
            .bind(job.request_key())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        if let Some(existing) = existing {
            let existing = to_job(existing)?;
            if existing.document_content_revision() != job.document_content_revision() {
                return Err(ProcessingRepositoryError::IdempotencyConflict);
            }
            transaction.commit().await.map_err(map_sql_error)?;
            return Ok(existing);
        }
        sqlx::query("INSERT INTO document_processing_jobs (id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, version, created_by, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)")
            .bind(job.id())
            .bind(job.tenant_id())
            .bind(job.document_id())
            .bind(job.document_content_revision())
            .bind(job.request_key())
            .bind(job.status().as_str())
            .bind(job.current_step().as_str())
            .bind(job.attempt_count())
            .bind(job.max_attempts())
            .bind(job.next_attempt_at())
            .bind(job.aggregate_version().value())
            .bind(job.created_by())
            .bind(job.created_at())
            .bind(job.updated_at())
            .execute(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        insert_steps(&mut transaction, job).await?;
        sqlx::query("INSERT INTO outbox_events (event_id, event_type, tenant_id, aggregate_id, aggregate_type, payload, schema_version, occurred_at) VALUES ($1, 'document.processing.requested.v1', $2, $3, 'document_processing_job', $4, 'v1', $5)")
            .bind(Uuid::now_v7())
            .bind(job.tenant_id().to_string())
            .bind(job.id().to_string())
            .bind(serde_json::json!({"job_id": job.id(), "document_id": job.document_id(), "content_revision": job.document_content_revision()}))
            .bind(job.created_at())
            .execute(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        transaction.commit().await.map_err(map_sql_error)?;
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
        save_job(&mut connection, job, expected_version).await
    }

    async fn request_cancel(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_sql_error)?;
        let Some(mut job) = load_job(&mut transaction, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        job.request_cancel(Utc::now())
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        save_job(&mut transaction, &job, expected).await?;
        transaction.commit().await.map_err(map_sql_error)?;
        Ok(job)
    }
}

#[async_trait]
impl ProcessingJobClaimPort for PostgresProcessingStore {
    async fn claim_next(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError> {
        if worker_id.trim().is_empty() || lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let mut transaction = self.pool.begin().await.map_err(map_sql_error)?;
        let row = sqlx::query_as::<_, JobRow>("SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE status = 'queued' AND next_attempt_at <= $1 AND (lease_expires_at IS NULL OR lease_expires_at <= $1) ORDER BY created_at, id FOR UPDATE SKIP LOCKED LIMIT 1")
            .bind(now)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        let Some(row) = row else {
            transaction.commit().await.map_err(map_sql_error)?;
            return Ok(None);
        };
        let mut job = to_job(row)?;
        let expected = job.aggregate_version().value();
        let token = Uuid::now_v7().to_string();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let fence_version = job
            .claim(worker_id.to_string(), token.clone(), expires_at, now)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        save_job(&mut transaction, &job, expected).await?;
        transaction.commit().await.map_err(map_sql_error)?;
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
        let mut transaction = self.pool.begin().await.map_err(map_sql_error)?;
        let rows = sqlx::query_as::<_, JobRow>("SELECT id, tenant_id, document_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE lease_expires_at IS NOT NULL AND lease_expires_at <= $1 FOR UPDATE SKIP LOCKED")
            .bind(now)
            .fetch_all(&mut *transaction)
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
                save_job(&mut transaction, &job, expected).await?;
                reclaimed = reclaimed.saturating_add(1);
            }
        }
        transaction.commit().await.map_err(map_sql_error)?;
        Ok(reclaimed)
    }
}

#[async_trait]
impl ProcessingJobQuery for PostgresProcessingStore {
    async fn detail(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJobDetail>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(job) = load_job(&mut connection, tenant_id, job_id).await? else {
            return Ok(None);
        };
        let candidate: Option<ExtractionCandidate> = sqlx::query_as::<_, CandidateRow>("SELECT payload FROM document_extraction_candidates WHERE tenant_id = $1 AND job_id = $2")
            .bind(tenant_id)
            .bind(job_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?
            .map(|row| serde_json::from_value(row.payload).map_err(|_| ProcessingRepositoryError::Failed))
            .transpose()?;
        let review = if let Some(candidate) = candidate.as_ref() {
            sqlx::query_as::<_, ReviewRow>("SELECT id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at FROM document_extraction_reviews WHERE tenant_id = $1 AND candidate_id = $2")
                .bind(tenant_id)
                .bind(candidate.id())
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
        let ids = sqlx::query_scalar::<_, Uuid>("SELECT id FROM document_processing_jobs WHERE tenant_id = $1 AND document_id = $2 ORDER BY created_at DESC")
            .bind(tenant_id)
            .bind(document_id)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sql_error)?;
        let mut details = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(detail) = self.detail(tenant_id, id).await? {
                details.push(detail);
            }
        }
        Ok(details)
    }
}

fn to_review(row: ReviewRow) -> Result<CandidateReview, ProcessingRepositoryError> {
    Ok(CandidateReview {
        id: row.id,
        tenant_id: row.tenant_id,
        candidate_id: row.candidate_id,
        reviewer_id: row.reviewer_id,
        decision: match row.decision.as_str() {
            "accepted" => document_processing::ReviewDecision::Accepted,
            "edited" => document_processing::ReviewDecision::Edited,
            "rejected" => document_processing::ReviewDecision::Rejected,
            _ => return Err(ProcessingRepositoryError::Failed),
        },
        patch: row.patch,
        comment: row.comment,
        candidate_version: row.candidate_version,
        created_at: row.created_at,
    })
}

#[async_trait]
impl ProcessingStepStore for PostgresProcessingStore {
    async fn start(
        &self,
        checkpoint: &StepCheckpoint,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'running', started_at = $1, checkpoint_json = $2, updated_at = $1 WHERE tenant_id = $3 AND job_id = $4 AND step_kind = $5 AND attempt_number = $6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = $3 AND id = $4 AND version = $7)")
            .bind(checkpoint.updated_at)
            .bind(&checkpoint.checkpoint_json)
            .bind(checkpoint.tenant_id)
            .bind(checkpoint.job_id)
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
        let result = sqlx::query("UPDATE document_processing_steps SET checkpoint_json = $1, updated_at = $2 WHERE tenant_id = $3 AND job_id = $4 AND step_kind = $5 AND attempt_number = $6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = $3 AND id = $4 AND version = $7)")
            .bind(&checkpoint.checkpoint_json)
            .bind(checkpoint.updated_at)
            .bind(checkpoint.tenant_id)
            .bind(checkpoint.job_id)
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
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'succeeded', finished_at = $1, updated_at = $1 WHERE tenant_id = $2 AND job_id = $3 AND step_kind = $4 AND attempt_number = $5 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = $2 AND id = $3 AND version = $6)")
            .bind(finished_at)
            .bind(tenant_id)
            .bind(job_id)
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
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'failed', failure_code = $1, finished_at = $2, updated_at = $2 WHERE tenant_id = $3 AND job_id = $4 AND step_kind = $5 AND attempt_number = $6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = $3 AND id = $4 AND version = $7)")
            .bind(failure_code)
            .bind(finished_at)
            .bind(tenant_id)
            .bind(job_id)
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
impl CandidateStore for PostgresProcessingStore {
    async fn save_candidate(
        &self,
        candidate: &ExtractionCandidate,
    ) -> Result<(), ProcessingRepositoryError> {
        sqlx::query("INSERT INTO document_extraction_candidates (id, tenant_id, job_id, schema_version, payload, evidence, provider, model, prompt_version, version, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON CONFLICT (tenant_id, job_id) DO UPDATE SET payload = EXCLUDED.payload, evidence = EXCLUDED.evidence")
            .bind(candidate.id())
            .bind(candidate.tenant_id())
            .bind(candidate.job_id())
            .bind(&candidate.schema_version)
            .bind(serde_json::to_value(candidate).map_err(|_| ProcessingRepositoryError::Failed)?)
            .bind(serde_json::to_value(&candidate.evidence).map_err(|_| ProcessingRepositoryError::Failed)?)
            .bind(&candidate.provider)
            .bind(&candidate.model)
            .bind(&candidate.prompt_version)
            .bind(candidate.version())
            .bind(candidate.created_at())
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
        sqlx::query_as::<_, CandidateRow>("SELECT payload FROM document_extraction_candidates WHERE tenant_id = $1 AND job_id = $2")
            .bind(tenant_id)
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sql_error)?
            .map(|row| serde_json::from_value(row.payload).map_err(|_| ProcessingRepositoryError::Failed))
            .transpose()
    }

    async fn save_review(&self, review: &CandidateReview) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("INSERT INTO document_extraction_reviews (id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (tenant_id, candidate_id) DO NOTHING")
            .bind(review.id)
            .bind(review.tenant_id)
            .bind(review.candidate_id)
            .bind(review.reviewer_id)
            .bind(review.decision.as_str())
            .bind(&review.patch)
            .bind(&review.comment)
            .bind(review.candidate_version)
            .bind(review.created_at)
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
        id: row.id,
        tenant_id: row.tenant_id,
        job_id: row.job_id,
        step_kind: ProcessingStepKind::try_from(row.step_kind.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        status: row.status,
        input_artifact_id: row.input_artifact_id,
        attempt_count: row.attempt_count,
        max_attempts: row.max_attempts,
        lease_token: row.lease_token,
        fence_version: row.fence_version,
        lease_expires_at: row.lease_expires_at,
    })
}

#[async_trait]
impl AiTaskPort for PostgresProcessingStore {
    async fn enqueue(&self, task: &AiTask) -> Result<(), ProcessingRepositoryError> {
        sqlx::query("INSERT INTO document_ai_tasks (id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, next_attempt_at, fence_version, created_at, updated_at) VALUES ($1, $2, $3, $4, 'queued', $5, $6, $7, $8, 0, $9, $9)")
            .bind(task.id)
            .bind(task.tenant_id)
            .bind(task.job_id)
            .bind(task.step_kind.as_str())
            .bind(&task.input_artifact_id)
            .bind(task.attempt_count)
            .bind(task.max_attempts)
            .bind(Utc::now())
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
        let mut transaction = self.pool.begin().await.map_err(map_sql_error)?;
        let row = sqlx::query_as::<_, AiTaskRow>("SELECT id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, lease_token, fence_version, lease_expires_at FROM document_ai_tasks WHERE status = 'queued' AND next_attempt_at <= $1 ORDER BY created_at, id FOR UPDATE SKIP LOCKED LIMIT 1")
            .bind(now)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        let Some(row) = row else {
            transaction.commit().await.map_err(map_sql_error)?;
            return Ok(None);
        };
        let fence = row.fence_version.saturating_add(1);
        let token = Uuid::now_v7().to_string();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        sqlx::query("UPDATE document_ai_tasks SET status = 'running', lease_owner = $1, lease_token = $2, lease_expires_at = $3, fence_version = $4, attempt_count = attempt_count + 1, updated_at = $5 WHERE id = $6 AND status = 'queued' AND fence_version = $7")
            .bind(worker_id)
            .bind(&token)
            .bind(expires_at)
            .bind(fence)
            .bind(now)
            .bind(row.id)
            .bind(row.fence_version)
            .execute(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        let updated = sqlx::query_as::<_, AiTaskRow>("SELECT id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, lease_token, fence_version, lease_expires_at FROM document_ai_tasks WHERE id = $1")
            .bind(row.id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_sql_error)?;
        transaction.commit().await.map_err(map_sql_error)?;
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
        let result = sqlx::query("UPDATE document_ai_tasks SET lease_expires_at = $1, updated_at = $2 WHERE id = $3 AND status = 'running' AND lease_owner = $4 AND lease_token = $5 AND fence_version = $6 AND lease_expires_at > $2")
            .bind(now + Duration::seconds(lease_duration_secs))
            .bind(now)
            .bind(task_id)
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
        let result = sqlx::query("UPDATE document_ai_tasks SET status = 'succeeded', output_candidate_id = $1, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = $2 WHERE id = $3 AND status = 'running' AND lease_owner = $4 AND lease_token = $5 AND fence_version = $6")
            .bind(candidate_id)
            .bind(now)
            .bind(task_id)
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
        let result = sqlx::query("UPDATE document_ai_tasks SET status = CASE WHEN attempt_count < max_attempts THEN 'queued' ELSE 'failed' END, failure_code = $1, next_attempt_at = $2, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = $2 WHERE id = $3 AND status = 'running' AND lease_owner = $4 AND lease_token = $5 AND fence_version = $6")
            .bind(failure_code)
            .bind(now)
            .bind(task_id)
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
