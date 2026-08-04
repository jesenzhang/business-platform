use async_trait::async_trait;
use chrono::Utc;
use data_integrity::IntegrityFinding;
use data_repair::{
    RepairCommand, RepairDescriptor, RepairError, RepairExecutionContext, RepairHandler,
    RepairHandlerRegistry, RepairLedgerEntry, RepairOutcome, RepairPersistencePort, RepairPreview,
    RepairResult, RepairRiskLevel, RepairRun, RepairRunStatus, RepairStep, RepairStepStatus,
    RepairTarget, RepairVerification,
};
use governance_worker::RepairWorker;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
struct FakePersistence {
    state: Arc<Mutex<FakeState>>,
}

struct FakeState {
    run: RepairRun,
    step: RepairStep,
    claimed: bool,
    ledger: Vec<RepairLedgerEntry>,
}

#[async_trait]
impl RepairPersistencePort for FakePersistence {
    async fn save_run(&self, run: &RepairRun) -> Result<(), RepairError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .run = run.clone();
        Ok(())
    }

    async fn save_step(&self, step: &RepairStep) -> Result<(), RepairError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .step = step.clone();
        Ok(())
    }

    async fn save_step_fenced(
        &self,
        step: &RepairStep,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.step.fence_version != expected_fence_version
            || state.step.lease_owner.is_none()
            || state.step.lease_token.is_none()
            || step.fence_version != expected_fence_version
        {
            return Err(RepairError::LeaseLost);
        }
        state.step = step.clone();
        Ok(())
    }

    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ledger
            .push(entry.clone());
        Ok(())
    }

    async fn load_finding(&self, _id: Uuid) -> Result<Option<IntegrityFinding>, RepairError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let run = &state.run;
        Ok(Some(IntegrityFinding {
            id: run.finding_id,
            tenant_id: run.tenant_id,
            rule_id: "TEST-RULE".to_string(),
            rule_version: 1,
            bounded_context: "document-processing".to_string(),
            resource_type: run.command.target.resource_type.clone(),
            resource_id: run.command.target.resource_id.clone(),
            severity: data_integrity::IntegritySeverity::Warning,
            fingerprint: "test-fingerprint".to_string(),
            detected_state: serde_json::json!({}),
            expected_state: serde_json::json!({}),
            status: data_integrity::FindingStatus::Open,
            repairability: run.command.repair_type.clone(),
            first_detected_at: run.created_at,
            last_detected_at: run.updated_at,
            occurrence_count: 1,
            resolved_at: None,
            resolution_reason: None,
            reopened_at: None,
            reopen_count: 0,
            previous_resolution: None,
            version: 0,
        }))
    }

    async fn mark_finding_repaired(
        &self,
        _finding_id: Uuid,
        _reason: &str,
    ) -> Result<(), RepairError> {
        Ok(())
    }

    async fn mark_finding_needs_manual_review(
        &self,
        _finding_id: Uuid,
        _reason: &str,
    ) -> Result<(), RepairError> {
        Ok(())
    }

    async fn commit_success(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        self.save_step_fenced(step, expected_fence_version).await?;
        self.append_ledger(entry).await?;
        self.save_run(run).await
    }

    async fn commit_failure(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        self.save_step_fenced(step, expected_fence_version).await?;
        self.append_ledger(entry).await?;
        self.save_run(run).await
    }

    async fn load_run(&self, _id: Uuid) -> Result<Option<RepairRun>, RepairError> {
        Ok(Some(
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .run
                .clone(),
        ))
    }

    async fn load_run_by_idempotency(
        &self,
        _tenant_id: Uuid,
        _idempotency_key: &str,
    ) -> Result<Option<RepairRun>, RepairError> {
        Ok(None)
    }

    async fn approve_repair(
        &self,
        _tenant_id: Uuid,
        _run_id: Uuid,
        approver: Uuid,
        _expected_version: i64,
        _expected_status: RepairRunStatus,
        note: String,
    ) -> Result<RepairRun, RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.run.approve(approver, note)?;
        state.step.status = RepairStepStatus::Approved;
        Ok(state.run.clone())
    }

    async fn cancel_repair(
        &self,
        _tenant_id: Uuid,
        _run_id: Uuid,
        _expected_version: i64,
        _expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.run.status = RepairRunStatus::Cancelled;
        state.run.version = state.run.version.saturating_add(1);
        state.step.status = RepairStepStatus::Cancelled;
        Ok(state.run.clone())
    }

    async fn resume_repair(
        &self,
        _tenant_id: Uuid,
        _run_id: Uuid,
        _expected_version: i64,
        _expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.run.status = if state.run.approved_by.is_some() {
            RepairRunStatus::Queued
        } else {
            RepairRunStatus::AwaitingApproval
        };
        state.run.version = state.run.version.saturating_add(1);
        state.step.status = if state.run.approved_by.is_some() {
            RepairStepStatus::Queued
        } else {
            RepairStepStatus::AwaitingApproval
        };
        Ok(state.run.clone())
    }

    async fn claim_step(
        &self,
        worker_id: &str,
        _now: chrono::DateTime<Utc>,
        _lease_duration_secs: i64,
    ) -> Result<Option<RepairStep>, RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.claimed {
            return Ok(None);
        }
        state.claimed = true;
        let mut step = state.step.clone();
        step.status = RepairStepStatus::Running;
        step.lease_owner = Some(worker_id.to_string());
        step.lease_token = Some("fenced-token".to_string());
        step.fence_version = 1;
        step.lease_expires_at = Some(Utc::now() + chrono::Duration::seconds(30));
        state.step = step.clone();
        Ok(Some(step))
    }

    async fn heartbeat_repair_step(
        &self,
        step_id: Uuid,
        lease_owner: &str,
        lease_token: &str,
        fence_version: i64,
        now: chrono::DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<RepairStep, RepairError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut step = state.step.clone();
        if step.id != step_id
            || step.lease_owner.as_deref() != Some(lease_owner)
            || step.lease_token.as_deref() != Some(lease_token)
            || step.fence_version != fence_version
        {
            return Err(RepairError::LeaseLost);
        }
        step.lease_expires_at = Some(now + chrono::Duration::seconds(lease_duration_secs));
        Ok(step)
    }

    async fn validate_repair_fence(
        &self,
        step_id: Uuid,
        lease_owner: &str,
        lease_token: &str,
        fence_version: i64,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), RepairError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let step = &state.step;
        if step.id == step_id
            && step.lease_owner.as_deref() == Some(lease_owner)
            && step.lease_token.as_deref() == Some(lease_token)
            && step.fence_version == fence_version
            && step.lease_expires_at.is_none_or(|expires| expires > now)
        {
            Ok(())
        } else {
            Err(RepairError::LeaseLost)
        }
    }
}

struct FakeHandler;

#[async_trait]
impl RepairHandler for FakeHandler {
    fn descriptor(&self) -> RepairDescriptor {
        RepairDescriptor {
            repair_type: "typed.v1".to_string(),
            version: 1,
            bounded_context: "document-processing".to_string(),
            risk_level: RepairRiskLevel::Low,
            requires_approval: false,
            supports_automatic_execution: true,
        }
    }

    async fn dry_run(&self, command: &RepairCommand) -> Result<RepairPreview, RepairError> {
        Ok(RepairPreview {
            command_id: Uuid::now_v7(),
            descriptor: self.descriptor(),
            finding_id: command.integrity_finding_id,
            resource_type: "processing_job".to_string(),
            resource_id: command.target.resource_id.clone(),
            before_hash: "before".to_string(),
            expected_after_hash: Some("after".to_string()),
            affected_count: 1,
            resource_version_before: Some(1),
            change_summary: "test repair".to_string(),
            preconditions: Vec::new(),
            executable: true,
            conflict_reason: None,
            warnings: Vec::new(),
        })
    }

    async fn execute(
        &self,
        _command: &RepairCommand,
        _context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        Ok(RepairResult {
            command_id: Uuid::now_v7(),
            resource_version_before: Some(1),
            resource_version_after: Some(2),
            before_hash: "before".to_string(),
            after_hash: "after".to_string(),
            rows_affected: 1,
            outcome: RepairOutcome::Succeeded,
        })
    }

    async fn verify(&self, _result: &RepairResult) -> Result<RepairVerification, RepairError> {
        Ok(RepairVerification {
            valid: true,
            message: "verified".to_string(),
        })
    }
}

struct FakeRegistry;

#[async_trait]
impl RepairHandlerRegistry for FakeRegistry {
    async fn get(&self, repair_type: &str, version: u32) -> Option<Box<dyn RepairHandler>> {
        (repair_type == "typed.v1" && version == 1).then(|| Box::new(FakeHandler) as _)
    }
}

fn fixture() -> (RepairRun, RepairStep) {
    let tenant_id = Uuid::new_v4();
    let finding_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let command = RepairCommand {
        idempotency_key: "worker-test".to_string(),
        tenant_id,
        integrity_finding_id: finding_id,
        target: RepairTarget {
            resource_type: "processing_job".to_string(),
            resource_id: Uuid::new_v4().to_string(),
            expected_resource_version: Some(1),
        },
        repair_type: "typed.v1".to_string(),
        repair_version: 1,
        requested_by: Uuid::new_v4(),
        reason: "test".to_string(),
        batch_limit: 1,
    };
    let now = Utc::now();
    (
        RepairRun {
            id: run_id,
            tenant_id,
            finding_id,
            command,
            status: RepairRunStatus::Queued,
            created_by: Uuid::new_v4(),
            approved_by: None,
            approval_note: None,
            created_at: now,
            updated_at: now,
            version: 0,
        },
        RepairStep {
            id: Uuid::new_v4(),
            run_id,
            finding_id,
            status: RepairStepStatus::Queued,
            attempt_count: 0,
            checkpoint: None,
            lease_owner: None,
            lease_token: None,
            fence_version: 0,
            lease_expires_at: None,
            next_attempt_at: now,
        },
    )
}

#[tokio::test]
async fn repair_worker_executes_once_and_records_ledger() {
    let (run, step) = fixture();
    let state = Arc::new(Mutex::new(FakeState {
        run,
        step,
        claimed: false,
        ledger: Vec::new(),
    }));
    let worker = RepairWorker {
        persistence: Arc::new(FakePersistence {
            state: Arc::clone(&state),
        }),
        handlers: Arc::new(FakeRegistry),
        rule_registry: None,
        worker_id: "worker-a".to_string(),
        lease_duration_secs: 30,
        heartbeat_seconds: 5,
    };
    assert!(worker
        .execute_one()
        .await
        .unwrap_or_else(|_| unreachable!()));
    assert!(!worker
        .execute_one()
        .await
        .unwrap_or_else(|_| unreachable!()));
    let state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(state.run.status, RepairRunStatus::Succeeded);
    assert_eq!(state.step.status, RepairStepStatus::Succeeded);
    assert_eq!(state.ledger.len(), 1);
}
