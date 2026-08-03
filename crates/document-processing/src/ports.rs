use async_trait::async_trait;
use chrono::{DateTime, Utc};
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

#[derive(Debug, Clone)]
pub struct StepCheckpoint {
    pub job_id: Uuid,
    pub tenant_id: Uuid,
    pub step_kind: ProcessingStepKind,
    pub attempt_number: i32,
    pub checkpoint_json: serde_json::Value,
    pub updated_at: DateTime<Utc>,
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
    pub lease_token: Option<String>,
    pub fence_version: i64,
    pub lease_expires_at: Option<DateTime<Utc>>,
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
    async fn detail(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJobDetail>, ProcessingRepositoryError>;
    async fn list_for_document(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<ProcessingJobDetail>, ProcessingRepositoryError>;
}

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
    async fn save_review(&self, review: &CandidateReview) -> Result<(), ProcessingRepositoryError>;
}

#[derive(Debug, Clone)]
pub struct StoredStep {
    pub step_kind: ProcessingStepKind,
    pub status: ProcessingStepStatus,
    pub attempt_number: i32,
    pub checkpoint_json: Option<serde_json::Value>,
    pub failure_code: Option<String>,
}
