use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::domain::{
    CandidateReview, ExtractionCandidate, ProcessingJob, ProcessingStepKind, ProcessingStepStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProcessingRepositoryError {
    #[error("processing resource was not found")]
    NotFound,
    #[error("tenant or resource ownership mismatch")]
    TenantMismatch,
    #[error("optimistic version conflict")]
    Conflict,
    #[error("idempotency key conflicts with an existing request")]
    IdempotencyConflict,
    #[error("lease was lost")]
    LeaseLost,
    #[error("database is unavailable")]
    Unavailable,
    #[error("persistence operation failed")]
    Failed,
}

#[derive(Debug, Clone)]
pub struct ClaimedProcessingJob {
    pub job: ProcessingJob,
    pub lease_token: String,
    pub fence_version: i64,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ProcessingJobDetail {
    pub job: ProcessingJob,
    pub candidate: Option<ExtractionCandidate>,
    pub review: Option<CandidateReview>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessingJobCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingJobListRequest {
    pub tenant_id: Uuid,
    pub document_id: Option<Uuid>,
    pub cursor: Option<ProcessingJobCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct ProcessingJobPage {
    pub items: Vec<ProcessingJobDetail>,
    pub next_cursor: Option<ProcessingJobCursor>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessingJobStatusCounts {
    pub queued: u64,
    pub running: u64,
    pub waiting_for_ai: u64,
    pub waiting_for_review: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub rejected: u64,
}
#[derive(Debug, Clone)]
pub struct StepCheckpoint {
    pub job_id: Uuid,
    pub tenant_id: Uuid,
    pub step_kind: ProcessingStepKind,
    pub attempt_number: i32,
    pub checkpoint_json: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

/// Lease identity carried by every durable worker-side business transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFence {
    pub worker_id: String,
    pub lease_token: String,
    pub fence_version: i64,
}

impl ExecutionFence {
    #[must_use]
    pub fn new(
        worker_id: impl Into<String>,
        lease_token: impl Into<String>,
        fence_version: i64,
    ) -> Self {
        Self {
            worker_id: worker_id.into(),
            lease_token: lease_token.into(),
            fence_version,
        }
    }
}

/// Persisted reference and bounded metadata for the extracted text artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextArtifactReference {
    pub key: String,
    pub content_hash: String,
    pub content_revision: i64,
    pub byte_count: u64,
    pub line_count: u32,
    pub character_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingFailureDisposition {
    Retry { backoff: Duration },
    Permanent,
    Cancelled,
    LeaseLost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedProcessingFailure {
    pub code: String,
    pub message: Option<String>,
    pub disposition: ProcessingFailureDisposition,
}

#[derive(Debug, Clone)]
pub struct CompleteAiTaskCommand {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub task_id: Uuid,
    pub fence: ExecutionFence,
    pub candidate: ExtractionCandidate,
}

#[derive(Debug, Clone)]
pub struct FinalizeReviewCommand {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub review: CandidateReview,
}

#[derive(Debug, Clone)]
pub struct FinalizeReviewResult {
    pub job: ProcessingJob,
    pub review: CandidateReview,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct AiTask {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub step_kind: ProcessingStepKind,
    pub status: String,
    pub input_artifact_id: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub lease_owner: Option<String>,
    pub lease_token: Option<String>,
    pub fence_version: i64,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub output_candidate_id: Option<Uuid>,
}

#[async_trait]
pub trait ProcessingJobCommandPort: Send + Sync {
    async fn create(&self, job: &ProcessingJob)
        -> Result<ProcessingJob, ProcessingRepositoryError>;
    async fn load(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJob>, ProcessingRepositoryError>;
    async fn save(
        &self,
        job: &ProcessingJob,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError>;
    async fn request_cancel(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;
}

#[async_trait]
pub trait ProcessingJobClaimPort: Send + Sync {
    async fn claim_next(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError>;
    async fn heartbeat(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError>;
    async fn release(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError>;
    async fn reclaim_expired(&self, now: DateTime<Utc>) -> Result<u64, ProcessingRepositoryError>;
}

#[async_trait]
pub trait ProcessingJobQuery: Send + Sync {
    async fn status_counts(
        &self,
        tenant_id: Uuid,
    ) -> Result<ProcessingJobStatusCounts, ProcessingRepositoryError>;

    async fn detail(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJobDetail>, ProcessingRepositoryError>;

    async fn list(
        &self,
        request: ProcessingJobListRequest,
    ) -> Result<ProcessingJobPage, ProcessingRepositoryError>;

    async fn list_for_document(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<ProcessingJobDetail>, ProcessingRepositoryError>;
}

/// Adapter-only write ports retained for persistence contract tests.
///
/// Application and worker code must use `ProcessingExecutionUnitOfWork` so a
/// caller cannot accidentally split a Job/Step/AI/Candidate/Review write
/// across transactions.
#[allow(dead_code)]
pub(crate) mod legacy {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[allow(clippy::too_many_arguments)]
    #[async_trait]
    pub trait ProcessingStepStore: Send + Sync {
        async fn start(
            &self,
            checkpoint: &StepCheckpoint,
            expected_version: i64,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn checkpoint(
            &self,
            checkpoint: &StepCheckpoint,
            expected_version: i64,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn complete(
            &self,
            job_id: Uuid,
            tenant_id: Uuid,
            step_kind: ProcessingStepKind,
            attempt_number: i32,
            expected_version: i64,
            finished_at: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn fail(
            &self,
            job_id: Uuid,
            tenant_id: Uuid,
            step_kind: ProcessingStepKind,
            attempt_number: i32,
            failure_code: &str,
            expected_version: i64,
            finished_at: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
    }

    #[async_trait]
    pub trait AiTaskPort: Send + Sync {
        async fn enqueue(&self, task: &AiTask) -> Result<(), ProcessingRepositoryError>;
        async fn claim_next(
            &self,
            worker_id: &str,
            now: DateTime<Utc>,
            lease_duration_secs: i64,
        ) -> Result<Option<AiTask>, ProcessingRepositoryError>;
        async fn heartbeat(
            &self,
            task_id: Uuid,
            worker_id: &str,
            lease_token: &str,
            fence_version: i64,
            now: DateTime<Utc>,
            lease_duration_secs: i64,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn complete(
            &self,
            task_id: Uuid,
            worker_id: &str,
            lease_token: &str,
            fence_version: i64,
            candidate_id: Uuid,
            now: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn fail(
            &self,
            task_id: Uuid,
            worker_id: &str,
            lease_token: &str,
            fence_version: i64,
            failure_code: &str,
            now: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
    }

    #[async_trait]
    pub trait CandidateStore: Send + Sync {
        async fn save_candidate(
            &self,
            candidate: &ExtractionCandidate,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn get_candidate(
            &self,
            tenant_id: Uuid,
            job_id: Uuid,
        ) -> Result<Option<ExtractionCandidate>, ProcessingRepositoryError>;
        async fn save_review(
            &self,
            review: &CandidateReview,
        ) -> Result<(), ProcessingRepositoryError>;
    }
}

#[async_trait]
pub trait CandidateQuery: Send + Sync {
    async fn get_candidate(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ExtractionCandidate>, ProcessingRepositoryError>;
}

#[async_trait]
pub trait ProcessingStepQuery: Send + Sync {
    async fn list_steps(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Vec<StoredStep>, ProcessingRepositoryError>;
}

/// Business-level transaction boundary for worker and review transitions.
///
/// Implementations own the database transaction. Callers must not compose the
/// legacy write stores to emulate these methods because that creates crash
/// windows between the Job, Step, Candidate, Review, Audit, and Outbox writes.
#[async_trait]
pub trait ProcessingExecutionUnitOfWork: Send + Sync {
    async fn create_job(
        &self,
        job: &ProcessingJob,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn claim_next_job(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError>;

    async fn claim_next_ai_task(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<AiTask>, ProcessingRepositoryError>;
    async fn start_step(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        expected_step: ProcessingStepKind,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn complete_step(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        completed_step: ProcessingStepKind,
        checkpoint: Option<StepCheckpoint>,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn retry_or_fail_step(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        step: ProcessingStepKind,
        failure: ClassifiedProcessingFailure,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn enqueue_ai_and_wait(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        text_artifact: TextArtifactReference,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<AiTask, ProcessingRepositoryError>;

    async fn complete_ai_and_resume(
        &self,
        completion: CompleteAiTaskCommand,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn fail_ai_task(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        task_id: Uuid,
        failure: ClassifiedProcessingFailure,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<AiTask, ProcessingRepositoryError>;

    async fn save_candidate_and_wait_for_review(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        candidate: &ExtractionCandidate,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn finalize_review(
        &self,
        command: FinalizeReviewCommand,
        now: DateTime<Utc>,
    ) -> Result<FinalizeReviewResult, ProcessingRepositoryError>;

    async fn cancel_processing(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        requested_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError>;

    async fn heartbeat_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError>;

    async fn release_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError>;

    async fn reclaim_expired_jobs(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, ProcessingRepositoryError>;

    async fn heartbeat_ai_task(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError>;

    async fn reclaim_expired_ai_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, ProcessingRepositoryError>;
}

#[derive(Debug, Clone)]
pub struct StoredStep {
    pub step_kind: ProcessingStepKind,
    pub status: ProcessingStepStatus,
    pub attempt_number: i32,
    pub checkpoint_json: Option<serde_json::Value>,
    pub failure_code: Option<String>,
}
