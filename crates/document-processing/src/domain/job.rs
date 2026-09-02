use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use super::error::ProcessingDomainError;

const INITIAL_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobVersion(i64);

impl JobVersion {
    pub fn new(value: i64) -> Result<Self, ProcessingDomainError> {
        if value <= 0 {
            return Err(ProcessingDomainError::InvalidIdentity);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }

    fn increment(self) -> Result<Self, ProcessingDomainError> {
        self.0
            .checked_add(1)
            .filter(|value| *value > 0)
            .map(Self)
            .ok_or(ProcessingDomainError::AttemptOverflow)
    }
}

impl fmt::Display for JobVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingJobStatus {
    Queued,
    Running,
    WaitingForAi,
    WaitingForReview,
    Succeeded,
    Failed,
    Cancelled,
    Rejected,
}

impl ProcessingJobStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingForAi => "waiting_for_ai",
            Self::WaitingForReview => "waiting_for_review",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Rejected => "rejected",
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Rejected
        )
    }
}

impl fmt::Display for ProcessingJobStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ProcessingJobStatus {
    type Error = ProcessingDomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting_for_ai" => Ok(Self::WaitingForAi),
            "waiting_for_review" => Ok(Self::WaitingForReview),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "rejected" => Ok(Self::Rejected),
            _ => Err(ProcessingDomainError::InvalidTransition {
                from: value.to_string(),
                action: "parse_status".to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStepKind {
    ValidateSource,
    DetectType,
    ExtractText,
    ExtractFields,
    ValidateCandidate,
    AwaitReview,
}

impl ProcessingStepKind {
    pub const FIXED: [Self; 6] = [
        Self::ValidateSource,
        Self::DetectType,
        Self::ExtractText,
        Self::ExtractFields,
        Self::ValidateCandidate,
        Self::AwaitReview,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ValidateSource => "validate_source",
            Self::DetectType => "detect_type",
            Self::ExtractText => "extract_text",
            Self::ExtractFields => "extract_fields",
            Self::ValidateCandidate => "validate_candidate",
            Self::AwaitReview => "await_review",
        }
    }

    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::ValidateSource => Some(Self::DetectType),
            Self::DetectType => Some(Self::ExtractText),
            Self::ExtractText => Some(Self::ExtractFields),
            Self::ExtractFields => Some(Self::ValidateCandidate),
            Self::ValidateCandidate => Some(Self::AwaitReview),
            Self::AwaitReview => None,
        }
    }
}

impl fmt::Display for ProcessingStepKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ProcessingStepKind {
    type Error = ProcessingDomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::FIXED
            .into_iter()
            .find(|step| step.as_str() == value)
            .ok_or(ProcessingDomainError::InvalidStep)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl ProcessingStepStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

impl fmt::Display for ProcessingStepStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingFailureKind {
    Transient,
    Permanent,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LeaseState {
    owner: String,
    token: String,
    expires_at: DateTime<Utc>,
    fence_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingJob {
    id: Uuid,
    tenant_id: Uuid,
    document_id: Uuid,
    document_revision_id: Option<Uuid>,
    document_content_revision: i64,
    request_key: String,
    status: ProcessingJobStatus,
    current_step: ProcessingStepKind,
    attempt_count: i32,
    max_attempts: i32,
    next_attempt_at: DateTime<Utc>,
    cancel_requested_at: Option<DateTime<Utc>>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    aggregate_version: JobVersion,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    fence_version: i64,
    lease: Option<LeaseState>,
    /// Request/correlation identity of the enqueueing call (e.g. the
    /// originating `X-Request-Id`). Carried into AI tasks, audit records and
    /// worker logs; never a secret and never derived from document content.
    correlation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedPipeline;

impl FixedPipeline {
    #[must_use]
    pub const fn steps() -> &'static [ProcessingStepKind; 6] {
        &ProcessingStepKind::FIXED
    }
}

impl ProcessingJob {
    pub fn queue(
        tenant_id: Uuid,
        document_id: Uuid,
        document_content_revision: i64,
        request_key: String,
        created_by: Uuid,
        max_attempts: i32,
        now: DateTime<Utc>,
    ) -> Result<Self, ProcessingDomainError> {
        if tenant_id.is_nil() || document_id.is_nil() || created_by.is_nil() {
            return Err(ProcessingDomainError::InvalidIdentity);
        }
        if document_content_revision <= 0 {
            return Err(ProcessingDomainError::InvalidContentRevision);
        }
        if request_key.trim().is_empty() {
            return Err(ProcessingDomainError::EmptyRequestKey);
        }
        if max_attempts < 1 {
            return Err(ProcessingDomainError::InvalidMaxAttempts);
        }
        Ok(Self {
            id: Uuid::now_v7(),
            tenant_id,
            document_id,
            document_revision_id: None,
            document_content_revision,
            request_key,
            status: ProcessingJobStatus::Queued,
            current_step: ProcessingStepKind::ValidateSource,
            attempt_count: 0,
            max_attempts,
            next_attempt_at: now,
            cancel_requested_at: None,
            failure_code: None,
            failure_message: None,
            aggregate_version: JobVersion(INITIAL_VERSION),
            created_by,
            created_at: now,
            updated_at: now,
            fence_version: 0,
            lease: None,
            correlation_id: None,
        })
    }

    /// Queue a run against one immutable `DocumentRevision`. A revision can
    /// have many processing jobs; the run identity is deliberately separate.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_for_revision(
        tenant_id: Uuid,
        document_id: Uuid,
        document_revision_id: Uuid,
        document_content_revision: i64,
        request_key: String,
        created_by: Uuid,
        max_attempts: i32,
        now: DateTime<Utc>,
    ) -> Result<Self, ProcessingDomainError> {
        if document_revision_id.is_nil() {
            return Err(ProcessingDomainError::InvalidDocumentRevision);
        }
        let mut job = Self::queue(
            tenant_id,
            document_id,
            document_content_revision,
            request_key,
            created_by,
            max_attempts,
            now,
        )?;
        job.document_revision_id = Some(document_revision_id);
        Ok(job)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: Uuid,
        tenant_id: Uuid,
        document_id: Uuid,
        document_content_revision: i64,
        request_key: String,
        status: ProcessingJobStatus,
        current_step: ProcessingStepKind,
        attempt_count: i32,
        max_attempts: i32,
        next_attempt_at: DateTime<Utc>,
        cancel_requested_at: Option<DateTime<Utc>>,
        failure_code: Option<String>,
        failure_message: Option<String>,
        aggregate_version: JobVersion,
        created_by: Uuid,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        lease: Option<(String, String, DateTime<Utc>, i64)>,
    ) -> Result<Self, ProcessingDomainError> {
        let fence_version = lease.as_ref().map_or(0, |lease| lease.3);
        Self::rehydrate_with_fence(
            id,
            tenant_id,
            document_id,
            document_content_revision,
            request_key,
            status,
            current_step,
            attempt_count,
            max_attempts,
            next_attempt_at,
            cancel_requested_at,
            failure_code,
            failure_message,
            aggregate_version,
            created_by,
            created_at,
            updated_at,
            fence_version,
            lease,
        )
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn rehydrate_with_fence(
        id: Uuid,
        tenant_id: Uuid,
        document_id: Uuid,
        document_content_revision: i64,
        request_key: String,
        status: ProcessingJobStatus,
        current_step: ProcessingStepKind,
        attempt_count: i32,
        max_attempts: i32,
        next_attempt_at: DateTime<Utc>,
        cancel_requested_at: Option<DateTime<Utc>>,
        failure_code: Option<String>,
        failure_message: Option<String>,
        aggregate_version: JobVersion,
        created_by: Uuid,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        fence_version: i64,
        lease: Option<(String, String, DateTime<Utc>, i64)>,
    ) -> Result<Self, ProcessingDomainError> {
        if id.is_nil() || tenant_id.is_nil() || document_id.is_nil() || created_by.is_nil() {
            return Err(ProcessingDomainError::InvalidIdentity);
        }
        if document_content_revision <= 0
            || attempt_count < 0
            || max_attempts < 1
            || fence_version < 0
        {
            return Err(ProcessingDomainError::InvalidContentRevision);
        }
        if request_key.trim().is_empty() {
            return Err(ProcessingDomainError::EmptyRequestKey);
        }
        if request_key.len() > 200
            || failure_code.as_ref().is_some_and(|code| code.len() > 80)
            || failure_message
                .as_ref()
                .is_some_and(|message| message.len() > 4096)
        {
            return Err(ProcessingDomainError::InvalidIdentity);
        }
        if attempt_count > max_attempts || updated_at < created_at || next_attempt_at < created_at {
            return Err(ProcessingDomainError::InvalidTransition {
                from: status.to_string(),
                action: "rehydrate".to_string(),
            });
        }
        let lease = lease.map(|(owner, token, expires_at, fence_version)| LeaseState {
            owner,
            token,
            expires_at,
            fence_version,
        });
        if lease.as_ref().is_some_and(|lease| {
            lease.owner.trim().is_empty()
                || lease.token.is_empty()
                || lease.fence_version < 1
                || lease.fence_version != fence_version
                || lease.expires_at < created_at
        }) {
            return Err(ProcessingDomainError::LeaseLost);
        }
        let requires_lease = status == ProcessingJobStatus::Running;
        let forbids_lease = matches!(
            status,
            ProcessingJobStatus::Queued
                | ProcessingJobStatus::WaitingForAi
                | ProcessingJobStatus::WaitingForReview
                | ProcessingJobStatus::Succeeded
                | ProcessingJobStatus::Failed
                | ProcessingJobStatus::Cancelled
                | ProcessingJobStatus::Rejected
        );
        if (requires_lease && lease.is_none()) || (forbids_lease && lease.is_some()) {
            return Err(ProcessingDomainError::LeaseLost);
        }
        if status == ProcessingJobStatus::WaitingForAi
            && current_step != ProcessingStepKind::ExtractFields
        {
            return Err(ProcessingDomainError::InvalidStep);
        }
        if status == ProcessingJobStatus::WaitingForReview
            && current_step != ProcessingStepKind::AwaitReview
        {
            return Err(ProcessingDomainError::InvalidStep);
        }
        if matches!(
            status,
            ProcessingJobStatus::Queued | ProcessingJobStatus::Running
        ) && current_step == ProcessingStepKind::AwaitReview
        {
            return Err(ProcessingDomainError::InvalidStep);
        }
        if matches!(
            status,
            ProcessingJobStatus::Succeeded | ProcessingJobStatus::Rejected
        ) && current_step != ProcessingStepKind::AwaitReview
        {
            return Err(ProcessingDomainError::InvalidStep);
        }
        Ok(Self {
            id,
            tenant_id,
            document_id,
            document_revision_id: None,
            document_content_revision,
            request_key,
            status,
            current_step,
            attempt_count,
            max_attempts,
            next_attempt_at,
            cancel_requested_at,
            failure_code,
            failure_message,
            aggregate_version,
            created_by,
            created_at,
            updated_at,
            fence_version,
            lease,
            correlation_id: None,
        })
    }

    /// Attach the request/correlation identity of the enqueueing call. Used
    /// by the enqueue composition root and by persistence adapters when
    /// rehydrating the stored column. Blank values normalize to absent and
    /// values are capped at 64 characters — the same bound as the column.
    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: Option<String>) -> Self {
        self.correlation_id = correlation_id.and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.chars().take(64).collect())
        });
        self
    }

    #[must_use]
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub const fn document_id(&self) -> Uuid {
        self.document_id
    }
    #[must_use]
    pub const fn document_content_revision(&self) -> i64 {
        self.document_content_revision
    }

    #[must_use]
    pub const fn document_revision_id(&self) -> Option<Uuid> {
        self.document_revision_id
    }

    pub fn bind_document_revision(
        &mut self,
        revision_id: Uuid,
    ) -> Result<(), ProcessingDomainError> {
        if revision_id.is_nil() {
            return Err(ProcessingDomainError::InvalidDocumentRevision);
        }
        if let Some(existing) = self.document_revision_id {
            if existing != revision_id {
                return Err(ProcessingDomainError::InvalidDocumentRevision);
            }
        } else {
            self.document_revision_id = Some(revision_id);
        }
        Ok(())
    }
    #[must_use]
    pub fn request_key(&self) -> &str {
        &self.request_key
    }
    #[must_use]
    pub const fn status(&self) -> ProcessingJobStatus {
        self.status
    }
    #[must_use]
    pub const fn current_step(&self) -> ProcessingStepKind {
        self.current_step
    }
    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.attempt_count
    }
    #[must_use]
    pub const fn max_attempts(&self) -> i32 {
        self.max_attempts
    }
    #[must_use]
    pub const fn next_attempt_at(&self) -> DateTime<Utc> {
        self.next_attempt_at
    }
    #[must_use]
    pub const fn cancel_requested_at(&self) -> Option<DateTime<Utc>> {
        self.cancel_requested_at
    }
    #[must_use]
    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }
    #[must_use]
    pub fn failure_message(&self) -> Option<&str> {
        self.failure_message.as_deref()
    }
    #[must_use]
    pub const fn aggregate_version(&self) -> JobVersion {
        self.aggregate_version
    }
    #[must_use]
    pub const fn created_by(&self) -> Uuid {
        self.created_by
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    #[must_use]
    pub const fn fence_version(&self) -> i64 {
        self.fence_version
    }

    fn touch(&mut self, now: DateTime<Utc>) -> Result<(), ProcessingDomainError> {
        self.aggregate_version = self.aggregate_version.increment()?;
        self.updated_at = now;
        Ok(())
    }

    fn require_lease(
        &self,
        owner: &str,
        token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        let Some(lease) = self.lease.as_ref() else {
            return Err(ProcessingDomainError::LeaseLost);
        };
        if lease.owner != owner
            || lease.token != token
            || lease.fence_version != fence_version
            || lease.expires_at <= now
        {
            return Err(ProcessingDomainError::LeaseLost);
        }
        Ok(())
    }

    pub fn claim(
        &mut self,
        owner: String,
        token: String,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<i64, ProcessingDomainError> {
        if self.status != ProcessingJobStatus::Queued
            || self.cancel_requested_at.is_some()
            || self.next_attempt_at > now
        {
            return Err(ProcessingDomainError::InvalidTransition {
                from: self.status.to_string(),
                action: "claim".to_string(),
            });
        }
        if owner.trim().is_empty() || token.is_empty() || expires_at <= now {
            return Err(ProcessingDomainError::LeaseLost);
        }
        let fence_version = self
            .fence_version
            .checked_add(1)
            .ok_or(ProcessingDomainError::AttemptOverflow)?;
        self.fence_version = fence_version;
        self.lease = Some(LeaseState {
            owner,
            token,
            expires_at,
            fence_version,
        });
        self.status = ProcessingJobStatus::Running;
        self.touch(now)?;
        Ok(fence_version)
    }

    pub fn heartbeat(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        if expires_at <= now {
            return Err(ProcessingDomainError::LeaseLost);
        }
        if let Some(lease) = self.lease.as_mut() {
            lease.expires_at = expires_at;
        }
        self.touch(now)
    }

    pub fn release(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.lease = None;
        if self.cancel_requested_at.is_some() {
            self.status = ProcessingJobStatus::Cancelled;
        } else if self.status == ProcessingJobStatus::Running {
            self.status = ProcessingJobStatus::Queued;
            self.next_attempt_at = now;
        }
        self.touch(now)
    }

    pub fn reclaim_expired(&mut self, now: DateTime<Utc>) -> Result<bool, ProcessingDomainError> {
        let expired = self
            .lease
            .as_ref()
            .is_some_and(|lease| lease.expires_at <= now);
        if !expired {
            return Ok(false);
        }
        self.lease = None;
        if self.cancel_requested_at.is_some() {
            self.status = ProcessingJobStatus::Cancelled;
        } else if matches!(
            self.status,
            ProcessingJobStatus::Running | ProcessingJobStatus::WaitingForAi
        ) {
            self.status = ProcessingJobStatus::Queued;
            self.next_attempt_at = now;
        }
        self.touch(now)?;
        Ok(true)
    }

    pub fn start_step(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        step: ProcessingStepKind,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.ensure_not_cancel_requested()?;
        if self.status != ProcessingJobStatus::Running || self.current_step != step {
            return Err(ProcessingDomainError::InvalidStep);
        }
        self.touch(now)
    }

    pub fn complete_step(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        step: ProcessingStepKind,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.ensure_not_cancel_requested()?;
        if self.status != ProcessingJobStatus::Running || self.current_step != step {
            return Err(ProcessingDomainError::InvalidStep);
        }
        let Some(next) = step.next() else {
            return Err(ProcessingDomainError::InvalidStep);
        };
        self.current_step = next;
        if next == ProcessingStepKind::AwaitReview {
            self.status = ProcessingJobStatus::WaitingForReview;
            self.lease = None;
        }
        self.touch(now)
    }

    pub fn wait_for_ai(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.ensure_not_cancel_requested()?;
        if self.status != ProcessingJobStatus::Running
            || self.current_step != ProcessingStepKind::ExtractFields
        {
            return Err(ProcessingDomainError::InvalidStep);
        }
        self.status = ProcessingJobStatus::WaitingForAi;
        self.lease = None;
        self.touch(now)
    }

    pub fn resume_from_ai(
        &mut self,
        owner: String,
        token: String,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<i64, ProcessingDomainError> {
        self.ensure_not_cancel_requested()?;
        if self.status != ProcessingJobStatus::WaitingForAi
            || self.current_step != ProcessingStepKind::ExtractFields
        {
            return Err(ProcessingDomainError::InvalidTransition {
                from: self.status.to_string(),
                action: "resume_from_ai".to_string(),
            });
        }
        self.current_step = ProcessingStepKind::ValidateCandidate;
        self.status = ProcessingJobStatus::Queued;
        self.next_attempt_at = now;
        self.touch(now)?;
        self.claim(owner, token, expires_at, now)
    }

    /// Resume after an AI task has committed its candidate. The next worker
    /// claim is intentionally separate so the AI transaction never borrows a
    /// Job lease for the following step.
    pub fn resume_after_ai(&mut self, now: DateTime<Utc>) -> Result<(), ProcessingDomainError> {
        self.ensure_not_cancel_requested()?;
        if self.status != ProcessingJobStatus::WaitingForAi
            || self.current_step != ProcessingStepKind::ExtractFields
        {
            return Err(ProcessingDomainError::InvalidTransition {
                from: self.status.to_string(),
                action: "resume_after_ai".to_string(),
            });
        }
        self.current_step = ProcessingStepKind::ValidateCandidate;
        self.status = ProcessingJobStatus::Queued;
        self.next_attempt_at = now;
        self.touch(now)
    }

    pub fn wait_for_review(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.ensure_not_cancel_requested()?;
        if self.status != ProcessingJobStatus::Running
            || self.current_step != ProcessingStepKind::ValidateCandidate
        {
            return Err(ProcessingDomainError::InvalidStep);
        }
        self.current_step = ProcessingStepKind::AwaitReview;
        self.status = ProcessingJobStatus::WaitingForReview;
        self.lease = None;
        self.touch(now)
    }

    pub fn request_cancel(&mut self, now: DateTime<Utc>) -> Result<(), ProcessingDomainError> {
        if self.status.is_terminal() {
            return Ok(());
        }
        if self.cancel_requested_at.is_none() {
            self.cancel_requested_at = Some(now);
            if matches!(
                self.status,
                ProcessingJobStatus::Queued | ProcessingJobStatus::WaitingForReview
            ) {
                self.status = ProcessingJobStatus::Cancelled;
                self.lease = None;
            }
            self.touch(now)?;
        }
        Ok(())
    }

    pub fn cancel(
        &mut self,
        owner: Option<(&str, &str, i64)>,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        if self.status.is_terminal() {
            return Ok(());
        }
        if let Some((owner, token, fence_version)) = owner {
            self.require_lease(owner, token, fence_version, now)?;
        } else if self.cancel_requested_at.is_none()
            && !matches!(
                self.status,
                ProcessingJobStatus::Queued | ProcessingJobStatus::WaitingForReview
            )
        {
            return Err(ProcessingDomainError::CancelNotRequested);
        }
        self.status = ProcessingJobStatus::Cancelled;
        self.cancel_requested_at.get_or_insert(now);
        self.lease = None;
        self.touch(now)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fail_transient(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        failure_code: String,
        failure_message: Option<String>,
        now: DateTime<Utc>,
        backoff: Duration,
    ) -> Result<ProcessingFailureKind, ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.attempt_count = self
            .attempt_count
            .checked_add(1)
            .ok_or(ProcessingDomainError::AttemptOverflow)?;
        self.failure_code = Some(failure_code);
        self.failure_message = failure_message;
        self.lease = None;
        if self.cancel_requested_at.is_some() {
            self.status = ProcessingJobStatus::Cancelled;
            self.touch(now)?;
            return Ok(ProcessingFailureKind::Cancelled);
        }
        if self.attempt_count >= self.max_attempts {
            self.status = ProcessingJobStatus::Failed;
            self.touch(now)?;
            return Ok(ProcessingFailureKind::Permanent);
        }
        self.status = ProcessingJobStatus::Queued;
        self.next_attempt_at = now + backoff;
        self.touch(now)?;
        Ok(ProcessingFailureKind::Transient)
    }

    pub fn fail_permanent(
        &mut self,
        owner: &str,
        token: &str,
        fence_version: i64,
        failure_code: String,
        failure_message: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingDomainError> {
        self.require_lease(owner, token, fence_version, now)?;
        self.status = ProcessingJobStatus::Failed;
        self.failure_code = Some(failure_code);
        self.failure_message = failure_message;
        self.lease = None;
        if self.cancel_requested_at.is_some() {
            self.status = ProcessingJobStatus::Cancelled;
        }
        self.touch(now)
    }

    fn ensure_not_cancel_requested(&self) -> Result<(), ProcessingDomainError> {
        if self.cancel_requested_at.is_some() {
            return Err(ProcessingDomainError::InvalidTransition {
                from: self.status.to_string(),
                action: "cancel_requested".to_string(),
            });
        }
        Ok(())
    }

    pub fn confirm_review(&mut self, now: DateTime<Utc>) -> Result<(), ProcessingDomainError> {
        if self.status != ProcessingJobStatus::WaitingForReview {
            return Err(ProcessingDomainError::InvalidTransition {
                from: self.status.to_string(),
                action: "confirm_review".to_string(),
            });
        }
        self.status = ProcessingJobStatus::Succeeded;
        self.touch(now)
    }

    pub fn reject_review(&mut self, now: DateTime<Utc>) -> Result<(), ProcessingDomainError> {
        if self.status != ProcessingJobStatus::WaitingForReview {
            return Err(ProcessingDomainError::InvalidTransition {
                from: self.status.to_string(),
                action: "reject_review".to_string(),
            });
        }
        self.status = ProcessingJobStatus::Rejected;
        self.touch(now)
    }

    #[must_use]
    pub fn lease_snapshot(&self) -> Option<(String, String, DateTime<Utc>, i64)> {
        self.lease.as_ref().map(|lease| {
            (
                lease.owner.clone(),
                lease.token.clone(),
                lease.expires_at,
                lease.fence_version,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> ProcessingJob {
        ProcessingJob::queue(
            Uuid::now_v7(),
            Uuid::now_v7(),
            1,
            "request-1".to_string(),
            Uuid::now_v7(),
            3,
            Utc::now() - Duration::seconds(1),
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn claim(job: &mut ProcessingJob, now: DateTime<Utc>) -> i64 {
        job.claim(
            "worker-a".to_string(),
            "lease-a".to_string(),
            now + Duration::seconds(30),
            now,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn fixed_pipeline_has_no_runtime_reordering() {
        assert_eq!(FixedPipeline::steps().len(), 6);
        assert_eq!(
            ProcessingStepKind::ExtractText.next(),
            Some(ProcessingStepKind::ExtractFields)
        );
        assert_eq!(ProcessingStepKind::AwaitReview.next(), None);
    }

    #[test]
    fn state_machine_advances_and_terminal_cannot_resume() {
        let now = Utc::now();
        let mut job = job();
        let fence = claim(&mut job, now);
        for step in [
            ProcessingStepKind::ValidateSource,
            ProcessingStepKind::DetectType,
            ProcessingStepKind::ExtractText,
            ProcessingStepKind::ExtractFields,
            ProcessingStepKind::ValidateCandidate,
        ] {
            job.start_step("worker-a", "lease-a", fence, step, now)
                .unwrap_or_else(|_| unreachable!());
            job.complete_step("worker-a", "lease-a", fence, step, now)
                .unwrap_or_else(|_| unreachable!());
            if step != ProcessingStepKind::ValidateCandidate {
                assert_eq!(job.status(), ProcessingJobStatus::Running);
                job.lease = Some(LeaseState {
                    owner: "worker-a".to_string(),
                    token: "lease-a".to_string(),
                    expires_at: now + Duration::seconds(30),
                    fence_version: fence,
                });
            }
        }
        assert_eq!(job.status(), ProcessingJobStatus::WaitingForReview);
        job.confirm_review(now).unwrap_or_else(|_| unreachable!());
        assert_eq!(job.status(), ProcessingJobStatus::Succeeded);
        assert!(job.request_cancel(now).is_ok());
        assert!(job.confirm_review(now).is_err());
    }

    #[test]
    fn archive_like_state_does_not_change_content_revision_equivalent() {
        let mut job = job();
        let before = job.document_content_revision();
        let now = Utc::now();
        assert!(job.request_cancel(now).is_ok());
        assert_eq!(job.document_content_revision(), before);
    }

    #[test]
    fn stale_fence_is_rejected_after_expiry_reclaim() {
        let now = Utc::now();
        let mut job = job();
        let first_fence = claim(&mut job, now);
        assert!(job.reclaim_expired(now + Duration::seconds(31)).is_ok());
        let second_fence = claim(&mut job, now + Duration::seconds(31));
        assert!(second_fence > first_fence);
        assert!(job
            .start_step(
                "worker-a",
                "lease-a",
                first_fence,
                ProcessingStepKind::ValidateSource,
                now + Duration::seconds(31),
            )
            .is_err());
    }

    #[test]
    fn transient_retry_respects_limit_and_cancellation() {
        let now = Utc::now();
        let mut job = job();
        let fence = claim(&mut job, now);
        assert_eq!(
            job.fail_transient(
                "worker-a",
                "lease-a",
                fence,
                "temporary".to_string(),
                None,
                now,
                Duration::seconds(1),
            )
            .unwrap_or_else(|_| unreachable!()),
            ProcessingFailureKind::Transient
        );
        let fence = claim(&mut job, now + Duration::seconds(1));
        assert!(job.request_cancel(now + Duration::seconds(1)).is_ok());
        assert_eq!(
            job.fail_transient(
                "worker-a",
                "lease-a",
                fence,
                "cancelled".to_string(),
                None,
                now + Duration::seconds(1),
                Duration::seconds(1),
            )
            .unwrap_or_else(|_| unreachable!()),
            ProcessingFailureKind::Cancelled
        );
        assert_eq!(job.status(), ProcessingJobStatus::Cancelled);
    }

    #[test]
    fn cancellation_wins_release_and_expiry_reclaim() {
        let now = Utc::now();
        let mut released = job();
        let fence = claim(&mut released, now);
        released
            .request_cancel(now + Duration::seconds(1))
            .unwrap_or_else(|_| unreachable!());
        released
            .release("worker-a", "lease-a", fence, now + Duration::seconds(1))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(released.status(), ProcessingJobStatus::Cancelled);
        assert!(released.lease_snapshot().is_none());

        let mut reclaimed = job();
        claim(&mut reclaimed, now);
        reclaimed
            .request_cancel(now + Duration::seconds(1))
            .unwrap_or_else(|_| unreachable!());
        reclaimed
            .reclaim_expired(now + Duration::seconds(31))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(reclaimed.status(), ProcessingJobStatus::Cancelled);
        assert!(reclaimed.lease_snapshot().is_none());
    }
}
