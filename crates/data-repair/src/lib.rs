//! Typed, durable and approval-gated repair contracts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use data_integrity::{IntegrityFinding, IntegritySeverity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairDescriptor {
    pub repair_type: String,
    pub version: u32,
    pub bounded_context: String,
    pub risk_level: RepairRiskLevel,
    pub requires_approval: bool,
    pub supports_automatic_execution: bool,
}

impl RepairDescriptor {
    pub fn validate(&self) -> Result<(), RepairError> {
        if self.repair_type.trim().is_empty() || self.version == 0 {
            return Err(RepairError::InvalidDescriptor);
        }
        if matches!(
            self.risk_level,
            RepairRiskLevel::High | RepairRiskLevel::Critical
        ) && !self.requires_approval
        {
            return Err(RepairError::InvalidDescriptor);
        }
        if self.supports_automatic_execution && !matches!(self.risk_level, RepairRiskLevel::Low) {
            return Err(RepairError::InvalidDescriptor);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairTarget {
    pub resource_type: String,
    pub resource_id: String,
    pub expected_resource_version: Option<i64>,
}

impl RepairTarget {
    pub fn validate(&self) -> Result<(), RepairError> {
        if self.resource_type != "processing_job"
            || self.resource_type.trim().is_empty()
            || self.resource_id.trim().is_empty()
            || self.resource_id.len() > 256
        {
            return Err(RepairError::InvalidCommand);
        }
        Ok(())
    }

    pub fn uuid(&self) -> Result<Uuid, RepairError> {
        if self.resource_type != "processing_job" {
            return Err(RepairError::Conflict);
        }
        Uuid::parse_str(&self.resource_id).map_err(|_| RepairError::Conflict)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairCommand {
    pub idempotency_key: String,
    pub tenant_id: Uuid,
    pub integrity_finding_id: Uuid,
    pub target: RepairTarget,
    pub repair_type: String,
    pub repair_version: u32,
    pub requested_by: Uuid,
    pub reason: String,
    pub batch_limit: u32,
}

impl RepairCommand {
    pub fn validate(&self) -> Result<(), RepairError> {
        if self.tenant_id.is_nil()
            || self.integrity_finding_id.is_nil()
            || self.requested_by.is_nil()
            || self.idempotency_key.trim().is_empty()
            || self.repair_type.trim().is_empty()
            || self.repair_version == 0
            || self.reason.trim().is_empty()
            || self.batch_limit == 0
            || self.batch_limit > 1000
        {
            return Err(RepairError::InvalidCommand);
        }
        self.target.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairPreview {
    pub command_id: Uuid,
    pub descriptor: RepairDescriptor,
    pub finding_id: Uuid,
    pub resource_type: String,
    pub resource_id: String,
    pub before_hash: String,
    pub expected_after_hash: Option<String>,
    pub affected_count: u32,
    #[serde(default)]
    pub resource_version_before: Option<i64>,
    #[serde(default)]
    pub change_summary: String,
    #[serde(default)]
    pub preconditions: Vec<String>,
    #[serde(default = "default_preview_executable")]
    pub executable: bool,
    #[serde(default)]
    pub conflict_reason: Option<String>,
    pub warnings: Vec<String>,
}

fn default_preview_executable() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairResult {
    pub command_id: Uuid,
    pub resource_version_before: Option<i64>,
    pub resource_version_after: Option<i64>,
    pub before_hash: String,
    pub after_hash: String,
    pub rows_affected: u32,
    pub outcome: RepairOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairOutcome {
    Succeeded,
    Noop,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairVerification {
    pub valid: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RepairExecutionContext {
    pub run_id: Uuid,
    pub step_id: Uuid,
    pub worker_id: String,
    pub fence_version: i64,
    pub lease_token: String,
    pub now: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait RepairHandler: Send + Sync {
    fn descriptor(&self) -> RepairDescriptor;
    async fn dry_run(&self, command: &RepairCommand) -> Result<RepairPreview, RepairError>;
    async fn execute(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError>;
    async fn verify(&self, result: &RepairResult) -> Result<RepairVerification, RepairError>;

    /// Re-read owner state after mutation.  Handlers that can perform a
    /// rule-level verification override this method; the compatibility
    /// default preserves the historical result-only contract for test-only
    /// handlers that do not own a verifier.
    async fn verify_after_repair(
        &self,
        _command: &RepairCommand,
        result: &RepairResult,
    ) -> Result<RepairVerification, RepairError> {
        self.verify(result).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairRunStatus {
    Draft,
    DryRunCompleted,
    AwaitingApproval,
    Approved,
    Queued,
    Running,
    Verifying,
    Succeeded,
    Failed,
    Cancelled,
    NeedsManualReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStepStatus {
    Draft,
    AwaitingApproval,
    Approved,
    Queued,
    Running,
    Verifying,
    Succeeded,
    Failed,
    Cancelled,
    NeedsManualReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairFailureDisposition {
    Retry { backoff: chrono::Duration },
    Permanent,
    NeedsManualReview,
    LeaseLost,
    Cancelled,
}

#[must_use]
pub fn repair_run_status_name(status: RepairRunStatus) -> &'static str {
    match status {
        RepairRunStatus::Draft => "draft",
        RepairRunStatus::DryRunCompleted => "dry_run_completed",
        RepairRunStatus::AwaitingApproval => "awaiting_approval",
        RepairRunStatus::Approved => "approved",
        RepairRunStatus::Queued => "queued",
        RepairRunStatus::Running => "running",
        RepairRunStatus::Verifying => "verifying",
        RepairRunStatus::Succeeded => "succeeded",
        RepairRunStatus::Failed => "failed",
        RepairRunStatus::Cancelled => "cancelled",
        RepairRunStatus::NeedsManualReview => "needs_manual_review",
    }
}

#[must_use]
pub fn repair_step_status_name(status: RepairStepStatus) -> &'static str {
    match status {
        RepairStepStatus::Draft => "draft",
        RepairStepStatus::AwaitingApproval => "awaiting_approval",
        RepairStepStatus::Approved => "approved",
        RepairStepStatus::Queued => "queued",
        RepairStepStatus::Running => "running",
        RepairStepStatus::Verifying => "verifying",
        RepairStepStatus::Succeeded => "succeeded",
        RepairStepStatus::Failed => "failed",
        RepairStepStatus::Cancelled => "cancelled",
        RepairStepStatus::NeedsManualReview => "needs_manual_review",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairRun {
    id: Uuid,
    tenant_id: Uuid,
    finding_id: Uuid,
    command: RepairCommand,
    status: RepairRunStatus,
    created_by: Uuid,
    approved_by: Option<Uuid>,
    approval_note: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i64,
}

impl RepairRun {
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        finding_id: Uuid,
        command: RepairCommand,
        status: RepairRunStatus,
        created_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Self, RepairError> {
        Self::rehydrate(
            id, tenant_id, finding_id, command, status, created_by, None, None, now, now, 0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: Uuid,
        tenant_id: Uuid,
        finding_id: Uuid,
        command: RepairCommand,
        status: RepairRunStatus,
        created_by: Uuid,
        approved_by: Option<Uuid>,
        approval_note: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        version: i64,
    ) -> Result<Self, RepairError> {
        command.validate()?;
        if id.is_nil()
            || tenant_id.is_nil()
            || finding_id.is_nil()
            || created_by.is_nil()
            || version < 0
            || command.tenant_id != tenant_id
            || command.integrity_finding_id != finding_id
            || approved_by.is_some_and(|approver| approver == created_by)
            || approval_note
                .as_deref()
                .is_some_and(|note| note.trim().is_empty())
        {
            return Err(RepairError::InvalidCommand);
        }
        Ok(Self {
            id,
            tenant_id,
            finding_id,
            command,
            status,
            created_by,
            approved_by,
            approval_note,
            created_at,
            updated_at,
            version,
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub fn finding_id(&self) -> Uuid {
        self.finding_id
    }
    #[must_use]
    pub fn command(&self) -> &RepairCommand {
        &self.command
    }
    #[must_use]
    pub fn status(&self) -> RepairRunStatus {
        self.status
    }
    #[must_use]
    pub fn created_by(&self) -> Uuid {
        self.created_by
    }
    #[must_use]
    pub fn approved_by(&self) -> Option<Uuid> {
        self.approved_by
    }
    #[must_use]
    pub fn approval_note(&self) -> Option<&str> {
        self.approval_note.as_deref()
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    #[must_use]
    pub fn version(&self) -> i64 {
        self.version
    }

    pub fn approve(&mut self, approver: Uuid, note: String) -> Result<(), RepairError> {
        if approver.is_nil() || approver == self.created_by || note.trim().is_empty() {
            return Err(RepairError::ApprovalSeparation);
        }
        if self.status != RepairRunStatus::AwaitingApproval {
            return Err(RepairError::InvalidTransition);
        }
        self.approved_by = Some(approver);
        self.approval_note = Some(note);
        self.status = RepairRunStatus::Approved;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn queue_for_execution(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if self.status != RepairRunStatus::Approved {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Queued;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn mark_running(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if self.status != RepairRunStatus::Queued {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Running;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn mark_verifying(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if self.status != RepairRunStatus::Running {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Verifying;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn schedule_retry(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairRunStatus::Running | RepairRunStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Queued;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn mark_failed(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairRunStatus::Running | RepairRunStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Failed;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn mark_needs_manual_review(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairRunStatus::Running | RepairRunStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::NeedsManualReview;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn mark_succeeded(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairRunStatus::Running | RepairRunStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Succeeded;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn cancel(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if matches!(
            self.status,
            RepairRunStatus::Succeeded | RepairRunStatus::Cancelled
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairRunStatus::Cancelled;
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }

    pub fn resume(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairRunStatus::Cancelled
                | RepairRunStatus::Failed
                | RepairRunStatus::NeedsManualReview
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = if self.approved_by.is_some() {
            RepairRunStatus::Queued
        } else {
            RepairRunStatus::AwaitingApproval
        };
        self.updated_at = now;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairStep {
    id: Uuid,
    run_id: Uuid,
    finding_id: Uuid,
    status: RepairStepStatus,
    attempt_count: u32,
    checkpoint: Option<Value>,
    lease_owner: Option<String>,
    lease_token: Option<String>,
    fence_version: i64,
    lease_expires_at: Option<DateTime<Utc>>,
    next_attempt_at: DateTime<Utc>,
}

impl RepairStep {
    pub fn new(
        id: Uuid,
        run_id: Uuid,
        finding_id: Uuid,
        status: RepairStepStatus,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<Self, RepairError> {
        Self::rehydrate(
            id,
            run_id,
            finding_id,
            status,
            0,
            None,
            None,
            None,
            0,
            None,
            next_attempt_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: Uuid,
        run_id: Uuid,
        finding_id: Uuid,
        status: RepairStepStatus,
        attempt_count: u32,
        checkpoint: Option<Value>,
        lease_owner: Option<String>,
        lease_token: Option<String>,
        fence_version: i64,
        lease_expires_at: Option<DateTime<Utc>>,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<Self, RepairError> {
        if id.is_nil() || run_id.is_nil() || finding_id.is_nil() || fence_version < 0 {
            return Err(RepairError::Persistence);
        }
        if lease_owner.is_some() != lease_token.is_some()
            || lease_expires_at.is_some() != lease_owner.is_some()
        {
            return Err(RepairError::Persistence);
        }
        Ok(Self {
            id,
            run_id,
            finding_id,
            status,
            attempt_count,
            checkpoint,
            lease_owner,
            lease_token,
            fence_version,
            lease_expires_at,
            next_attempt_at,
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }
    #[must_use]
    pub fn finding_id(&self) -> Uuid {
        self.finding_id
    }
    #[must_use]
    pub fn status(&self) -> RepairStepStatus {
        self.status
    }
    #[must_use]
    pub fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
    #[must_use]
    pub fn checkpoint(&self) -> Option<&Value> {
        self.checkpoint.as_ref()
    }
    #[must_use]
    pub fn lease_owner(&self) -> Option<&str> {
        self.lease_owner.as_deref()
    }
    #[must_use]
    pub fn lease_token(&self) -> Option<&str> {
        self.lease_token.as_deref()
    }
    #[must_use]
    pub fn fence_version(&self) -> i64 {
        self.fence_version
    }
    #[must_use]
    pub fn lease_expires_at(&self) -> Option<DateTime<Utc>> {
        self.lease_expires_at
    }
    #[must_use]
    pub fn next_attempt_at(&self) -> DateTime<Utc> {
        self.next_attempt_at
    }

    pub fn claim(
        &mut self,
        worker_id: String,
        lease_token: String,
        fence_version: i64,
        expires_at: DateTime<Utc>,
    ) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Queued | RepairStepStatus::Running
        ) {
            return Err(RepairError::InvalidTransition);
        }
        if worker_id.trim().is_empty()
            || lease_token.trim().is_empty()
            || fence_version <= self.fence_version
        {
            return Err(RepairError::LeaseLost);
        }
        self.status = RepairStepStatus::Running;
        self.attempt_count = self
            .attempt_count
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        self.lease_owner = Some(worker_id);
        self.lease_token = Some(lease_token);
        self.fence_version = fence_version;
        self.lease_expires_at = Some(expires_at);
        Ok(())
    }

    pub fn heartbeat(
        &mut self,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), RepairError> {
        if self.status != RepairStepStatus::Running
            || self.lease_owner.is_none()
            || self.lease_token.is_none()
            || self.lease_expires_at.is_none_or(|value| value <= now)
            || expires_at <= now
        {
            return Err(RepairError::LeaseLost);
        }
        self.lease_expires_at = Some(expires_at);
        Ok(())
    }

    pub fn request_cancel(&mut self) -> Result<(), RepairError> {
        if matches!(
            self.status,
            RepairStepStatus::Succeeded | RepairStepStatus::Cancelled
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Cancelled;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }

    pub fn approve(&mut self) -> Result<(), RepairError> {
        if self.status != RepairStepStatus::AwaitingApproval {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Approved;
        Ok(())
    }

    pub fn queue_for_execution(&mut self) -> Result<(), RepairError> {
        if self.status != RepairStepStatus::Approved {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Queued;
        Ok(())
    }

    pub fn succeed(&mut self) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Running | RepairStepStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Succeeded;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }

    pub fn fail(&mut self) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Running | RepairStepStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Failed;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }

    pub fn mark_verifying(&mut self) -> Result<(), RepairError> {
        if self.status != RepairStepStatus::Running {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Verifying;
        Ok(())
    }

    pub fn schedule_retry(&mut self, next_attempt_at: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Running | RepairStepStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Queued;
        self.next_attempt_at = next_attempt_at;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }

    pub fn require_manual_review(&mut self) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Running | RepairStepStatus::Verifying | RepairStepStatus::Failed
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::NeedsManualReview;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Queued | RepairStepStatus::Running | RepairStepStatus::Verifying
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Cancelled;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }

    pub fn resume(&mut self, now: DateTime<Utc>) -> Result<(), RepairError> {
        if !matches!(
            self.status,
            RepairStepStatus::Cancelled
                | RepairStepStatus::Failed
                | RepairStepStatus::NeedsManualReview
        ) {
            return Err(RepairError::InvalidTransition);
        }
        self.status = RepairStepStatus::Queued;
        self.next_attempt_at = now;
        self.lease_owner = None;
        self.lease_token = None;
        self.lease_expires_at = None;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairLedgerEntry {
    id: Uuid,
    tenant_id: Uuid,
    repair_run_id: Uuid,
    repair_step_id: Uuid,
    finding_id: Uuid,
    rule_id: String,
    repair_type: String,
    repair_version: u32,
    actor_type: String,
    actor_id: Uuid,
    reason: String,
    resource_type: String,
    resource_id: String,
    before_hash: String,
    after_hash: String,
    before_snapshot: Value,
    after_snapshot: Value,
    rows_affected: u32,
    result: RepairOutcome,
    failure_code: Option<String>,
    trace_id: Option<String>,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    previous_hash: Option<String>,
    record_hash: Option<String>,
}

impl RepairLedgerEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        repair_run_id: Uuid,
        repair_step_id: Uuid,
        finding_id: Uuid,
        rule_id: String,
        repair_type: String,
        repair_version: u32,
        actor_type: String,
        actor_id: Uuid,
        reason: String,
        resource_type: String,
        resource_id: String,
        before_hash: String,
        after_hash: String,
        before_snapshot: Value,
        after_snapshot: Value,
        rows_affected: u32,
        result: RepairOutcome,
        failure_code: Option<String>,
        trace_id: Option<String>,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        previous_hash: Option<String>,
        record_hash: Option<String>,
    ) -> Result<Self, RepairError> {
        Self::rehydrate(
            id,
            tenant_id,
            repair_run_id,
            repair_step_id,
            finding_id,
            rule_id,
            repair_type,
            repair_version,
            actor_type,
            actor_id,
            reason,
            resource_type,
            resource_id,
            before_hash,
            after_hash,
            before_snapshot,
            after_snapshot,
            rows_affected,
            result,
            failure_code,
            trace_id,
            started_at,
            finished_at,
            previous_hash,
            record_hash,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: Uuid,
        tenant_id: Uuid,
        repair_run_id: Uuid,
        repair_step_id: Uuid,
        finding_id: Uuid,
        rule_id: String,
        repair_type: String,
        repair_version: u32,
        actor_type: String,
        actor_id: Uuid,
        reason: String,
        resource_type: String,
        resource_id: String,
        before_hash: String,
        after_hash: String,
        before_snapshot: Value,
        after_snapshot: Value,
        rows_affected: u32,
        result: RepairOutcome,
        failure_code: Option<String>,
        trace_id: Option<String>,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        previous_hash: Option<String>,
        record_hash: Option<String>,
    ) -> Result<Self, RepairError> {
        if id.is_nil()
            || tenant_id.is_nil()
            || repair_run_id.is_nil()
            || repair_step_id.is_nil()
            || finding_id.is_nil()
            || rule_id.trim().is_empty()
            || repair_type.trim().is_empty()
            || repair_version == 0
            || actor_type.trim().is_empty()
            || actor_id.is_nil()
            || reason.trim().is_empty()
            || resource_type.trim().is_empty()
            || resource_id.trim().is_empty()
            || before_hash.trim().is_empty()
            || after_hash.trim().is_empty()
            || finished_at < started_at
        {
            return Err(RepairError::Persistence);
        }
        Ok(Self {
            id,
            tenant_id,
            repair_run_id,
            repair_step_id,
            finding_id,
            rule_id,
            repair_type,
            repair_version,
            actor_type,
            actor_id,
            reason,
            resource_type,
            resource_id,
            before_hash,
            after_hash,
            before_snapshot,
            after_snapshot,
            rows_affected,
            result,
            failure_code,
            trace_id,
            started_at,
            finished_at,
            previous_hash,
            record_hash,
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub fn repair_run_id(&self) -> Uuid {
        self.repair_run_id
    }
    #[must_use]
    pub fn repair_step_id(&self) -> Uuid {
        self.repair_step_id
    }
    #[must_use]
    pub fn finding_id(&self) -> Uuid {
        self.finding_id
    }
    #[must_use]
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }
    #[must_use]
    pub fn repair_type(&self) -> &str {
        &self.repair_type
    }
    #[must_use]
    pub fn repair_version(&self) -> u32 {
        self.repair_version
    }
    #[must_use]
    pub fn actor_type(&self) -> &str {
        &self.actor_type
    }
    #[must_use]
    pub fn actor_id(&self) -> Uuid {
        self.actor_id
    }
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }
    #[must_use]
    pub fn before_hash(&self) -> &str {
        &self.before_hash
    }
    #[must_use]
    pub fn after_hash(&self) -> &str {
        &self.after_hash
    }
    #[must_use]
    pub fn before_snapshot(&self) -> &Value {
        &self.before_snapshot
    }
    #[must_use]
    pub fn after_snapshot(&self) -> &Value {
        &self.after_snapshot
    }
    #[must_use]
    pub fn rows_affected(&self) -> u32 {
        self.rows_affected
    }
    #[must_use]
    pub fn result(&self) -> RepairOutcome {
        self.result
    }
    #[must_use]
    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }
    #[must_use]
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }
    #[must_use]
    pub fn finished_at(&self) -> DateTime<Utc> {
        self.finished_at
    }
    #[must_use]
    pub fn previous_hash(&self) -> Option<&str> {
        self.previous_hash.as_deref()
    }
    #[must_use]
    pub fn record_hash(&self) -> Option<&str> {
        self.record_hash.as_deref()
    }
}

#[derive(Debug, Error)]
pub enum RepairError {
    #[error("repair descriptor is invalid")]
    InvalidDescriptor,
    #[error("repair command is invalid")]
    InvalidCommand,
    #[error("repair transition is invalid")]
    InvalidTransition,
    #[error("approval must be performed by a different principal")]
    ApprovalSeparation,
    #[error("repair requires approval")]
    ApprovalRequired,
    #[error("repair lease was lost")]
    LeaseLost,
    #[error("repair target version conflicts")]
    Conflict,
    #[error("repair idempotency key conflicts with another request")]
    IdempotencyConflict,
    #[error("stored repair data contains an unsupported value")]
    InvalidStoredEnum,
    #[error("repair dependency is unavailable")]
    Unavailable,
    #[error("repair persistence failed")]
    Persistence,
}

#[async_trait]
pub trait RepairHandlerRegistry: Send + Sync {
    async fn get(&self, repair_type: &str, version: u32) -> Option<Box<dyn RepairHandler>>;
}

#[derive(Debug, Clone)]
pub struct CreateRepairExecution {
    pub run: RepairRun,
    pub step: RepairStep,
    pub expected_finding_version: i64,
}

#[derive(Debug, Clone)]
pub struct CreateRepairResult {
    pub run: RepairRun,
    pub step: RepairStep,
    pub replayed: bool,
}

#[async_trait]
pub trait RepairPersistencePort: Send + Sync {
    /// Atomically validate the Finding version, enforce idempotency, create
    /// Run and Step, transition the Finding, and append Audit/Outbox evidence.
    async fn create_repair_execution(
        &self,
        command: CreateRepairExecution,
    ) -> Result<CreateRepairResult, RepairError>;

    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError>;

    /// Load the finding that owns a repair command.  Governance adapters
    /// override this so the worker can resolve the owner resource without
    /// making the processing adapter read governance tables.
    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, RepairError>;

    /// Commit a successful step, immutable ledger entry, and run transition
    /// through one adapter-owned transaction when supported. The lease
    /// identity and expected run version are part of the same CAS boundary;
    /// callers must not rely on a preceding read as the only fence.
    #[allow(clippy::too_many_arguments)]
    async fn commit_success(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_run_version: i64,
        expected_fence_version: i64,
        lease_owner: &str,
        lease_token: &str,
    ) -> Result<(), RepairError>;

    /// Atomically record a failed/conflicted execution, release the fenced
    /// step, and move the run/finding to a recoverable non-success state.
    /// Adapters must implement this with the same transaction boundary as
    /// [`Self::commit_success`].
    #[allow(clippy::too_many_arguments)]
    async fn commit_failure(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_run_version: i64,
        expected_fence_version: i64,
        lease_owner: &str,
        lease_token: &str,
    ) -> Result<(), RepairError>;

    /// Atomically classify a failure after claim, including failures for
    /// which the worker cannot rehydrate a complete run or finding. The
    /// adapter must fence the transition by owner, token, and version.
    #[allow(clippy::too_many_arguments)]
    async fn classify_claimed_failure(
        &self,
        _step_id: Uuid,
        _run_id: Uuid,
        _lease_owner: &str,
        _lease_token: &str,
        _expected_fence_version: i64,
        _disposition: RepairFailureDisposition,
        _failure_code: &str,
        _next_attempt_at: Option<DateTime<Utc>>,
        _now: DateTime<Utc>,
    ) -> Result<(), RepairError> {
        Err(RepairError::Persistence)
    }

    /// Atomically stop a claimed step when post-claim validation or provider
    /// setup fails before a complete run/finding aggregate is available.
    /// Implementations must fence the transition by owner, token, and fence.
    async fn abort_claimed_repair(
        &self,
        _step: &RepairStep,
        _worker_id: &str,
        _reason: &str,
    ) -> Result<(), RepairError> {
        Err(RepairError::Persistence)
    }

    async fn mark_finding_repaired(
        &self,
        finding_id: Uuid,
        reason: &str,
    ) -> Result<(), RepairError>;
    async fn mark_finding_needs_manual_review(
        &self,
        finding_id: Uuid,
        reason: &str,
    ) -> Result<(), RepairError>;
    async fn load_run(&self, id: Uuid) -> Result<Option<RepairRun>, RepairError>;
    async fn load_run_by_idempotency(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<RepairRun>, RepairError>;

    /// Compare-and-swap lifecycle commands. Adapters update the Run and its
    /// Step in one local transaction and return the rehydrated Run.
    async fn approve_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        approver: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
        note: String,
    ) -> Result<RepairRun, RepairError>;

    /// Explicitly move an approved run into the executable queue. Approval
    /// alone must never make a step claimable.
    async fn execute_repair(
        &self,
        _tenant_id: Uuid,
        _run_id: Uuid,
        _expected_version: i64,
        _expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError> {
        Err(RepairError::Persistence)
    }

    async fn cancel_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError>;

    async fn resume_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError>;
    async fn claim_step(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<RepairStep>, RepairError>;

    /// Extend a lease only when every ownership coordinate still matches.
    /// The fail-closed default is safe for contract fakes; production
    /// adapters must implement the durable SQL predicate.
    async fn heartbeat_repair_step(
        &self,
        _step_id: Uuid,
        _lease_owner: &str,
        _lease_token: &str,
        _fence_version: i64,
        _now: DateTime<Utc>,
        _lease_duration_secs: i64,
    ) -> Result<RepairStep, RepairError> {
        Err(RepairError::LeaseLost)
    }

    /// Validate ownership immediately before an owner mutation.  A stale
    /// worker must fail closed even if no replacement worker has reclaimed the
    /// step yet.
    async fn validate_repair_fence(
        &self,
        _step_id: Uuid,
        _lease_owner: &str,
        _lease_token: &str,
        _fence_version: i64,
        _now: DateTime<Utc>,
    ) -> Result<(), RepairError> {
        Err(RepairError::LeaseLost)
    }
}

#[must_use]
pub fn risk_requires_approval(risk: RepairRiskLevel) -> bool {
    matches!(
        risk,
        RepairRiskLevel::Medium | RepairRiskLevel::High | RepairRiskLevel::Critical
    )
}

#[must_use]
pub fn severity_to_risk(severity: IntegritySeverity) -> RepairRiskLevel {
    match severity {
        IntegritySeverity::Info | IntegritySeverity::Warning => RepairRiskLevel::Low,
        IntegritySeverity::Error => RepairRiskLevel::Medium,
        IntegritySeverity::Critical => RepairRiskLevel::High,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_risk_approval_is_separated() {
        let creator = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let finding_id = Uuid::new_v4();
        let mut run = RepairRun::new(
            Uuid::new_v4(),
            tenant_id,
            finding_id,
            RepairCommand {
                idempotency_key: "repair-1".to_string(),
                tenant_id,
                integrity_finding_id: finding_id,
                target: RepairTarget {
                    resource_type: "processing_job".to_string(),
                    resource_id: Uuid::new_v4().to_string(),
                    expected_resource_version: None,
                },
                repair_type: "typed.v1".to_string(),
                repair_version: 1,
                requested_by: creator,
                reason: "fix".to_string(),
                batch_limit: 1,
            },
            RepairRunStatus::AwaitingApproval,
            creator,
            Utc::now(),
        )
        .unwrap_or_else(|_| unreachable!());
        assert!(run.approve(creator, "self".to_string()).is_err());
        assert!(run.approve(Uuid::new_v4(), "approved".to_string()).is_ok());
    }
}
