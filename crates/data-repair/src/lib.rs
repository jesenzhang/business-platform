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
pub struct RepairCommand {
    pub idempotency_key: String,
    pub tenant_id: Uuid,
    pub finding_id: Uuid,
    pub repair_type: String,
    pub repair_version: u32,
    pub requested_by: Uuid,
    pub reason: String,
    pub expected_resource_version: Option<i64>,
    pub batch_limit: u32,
}

impl RepairCommand {
    pub fn validate(&self) -> Result<(), RepairError> {
        if self.tenant_id.is_nil()
            || self.finding_id.is_nil()
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
    pub warnings: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairRun {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub finding_id: Uuid,
    pub command: RepairCommand,
    pub status: RepairRunStatus,
    pub created_by: Uuid,
    pub approved_by: Option<Uuid>,
    pub approval_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

impl RepairRun {
    pub fn approve(&mut self, approver: Uuid, note: String) -> Result<(), RepairError> {
        if approver.is_nil() || approver == self.created_by {
            return Err(RepairError::ApprovalSeparation);
        }
        if self.status != RepairRunStatus::AwaitingApproval {
            return Err(RepairError::InvalidTransition);
        }
        self.approved_by = Some(approver);
        self.approval_note = Some(note);
        self.status = RepairRunStatus::Approved;
        self.version = self.version.saturating_add(1);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairStep {
    pub id: Uuid,
    pub run_id: Uuid,
    pub finding_id: Uuid,
    pub status: RepairRunStatus,
    pub attempt_count: u32,
    pub checkpoint: Option<Value>,
    pub lease_owner: Option<String>,
    pub lease_token: Option<String>,
    pub fence_version: i64,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub next_attempt_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairLedgerEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub repair_run_id: Uuid,
    pub repair_step_id: Uuid,
    pub finding_id: Uuid,
    pub rule_id: String,
    pub repair_type: String,
    pub repair_version: u32,
    pub actor_type: String,
    pub actor_id: Uuid,
    pub reason: String,
    pub resource_type: String,
    pub resource_id: String,
    pub before_hash: String,
    pub after_hash: String,
    pub before_snapshot: Value,
    pub after_snapshot: Value,
    pub rows_affected: u32,
    pub result: RepairOutcome,
    pub failure_code: Option<String>,
    pub trace_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub previous_hash: Option<String>,
    pub record_hash: Option<String>,
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
    #[error("repair dependency is unavailable")]
    Unavailable,
    #[error("repair persistence failed")]
    Persistence,
}

#[async_trait]
pub trait RepairHandlerRegistry: Send + Sync {
    async fn get(&self, repair_type: &str, version: u32) -> Option<Box<dyn RepairHandler>>;
}

#[async_trait]
pub trait RepairPersistencePort: Send + Sync {
    async fn save_run(&self, run: &RepairRun) -> Result<(), RepairError>;
    async fn save_step(&self, step: &RepairStep) -> Result<(), RepairError>;
    /// Persist a worker transition only when the caller still owns the same
    /// fence.  Adapters override this with an optimistic `WHERE` predicate.
    async fn save_step_fenced(
        &self,
        step: &RepairStep,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        if step.fence_version != expected_fence_version {
            return Err(RepairError::LeaseLost);
        }
        self.save_step(step).await
    }
    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError>;

    /// Load the finding that owns a repair command.  Governance adapters
    /// override this so the worker can resolve the owner resource without
    /// making the processing adapter read governance tables.
    async fn load_finding(&self, _id: Uuid) -> Result<Option<IntegrityFinding>, RepairError> {
        Ok(None)
    }

    /// Commit a successful step, immutable ledger entry, and run transition
    /// through one adapter-owned transaction when supported.
    async fn commit_success(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        self.append_ledger(entry).await?;
        self.mark_finding_repaired(entry.finding_id, "repair_succeeded")
            .await?;
        self.save_step_fenced(step, expected_fence_version).await?;
        self.save_run(run).await
    }

    /// Mark a finding repaired as part of the adapter-owned completion
    /// transaction.  The default is intentionally a no-op for pure worker
    /// contract fakes; database adapters provide the durable transition.
    async fn mark_finding_repaired(
        &self,
        _finding_id: Uuid,
        _reason: &str,
    ) -> Result<(), RepairError> {
        Ok(())
    }
    async fn load_run(&self, id: Uuid) -> Result<Option<RepairRun>, RepairError>;
    async fn load_run_by_idempotency(
        &self,
        _tenant_id: Uuid,
        _idempotency_key: &str,
    ) -> Result<Option<RepairRun>, RepairError> {
        Ok(None)
    }
    async fn claim_step(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<RepairStep>, RepairError>;
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
        let mut run = RepairRun {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            finding_id: Uuid::new_v4(),
            command: RepairCommand {
                idempotency_key: "repair-1".to_string(),
                tenant_id: Uuid::new_v4(),
                finding_id: Uuid::new_v4(),
                repair_type: "typed.v1".to_string(),
                repair_version: 1,
                requested_by: creator,
                reason: "fix".to_string(),
                expected_resource_version: None,
                batch_limit: 1,
            },
            status: RepairRunStatus::AwaitingApproval,
            created_by: creator,
            approved_by: None,
            approval_note: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 0,
        };
        assert!(run.approve(creator, "self".to_string()).is_err());
        assert!(run.approve(Uuid::new_v4(), "approved".to_string()).is_ok());
    }
}
