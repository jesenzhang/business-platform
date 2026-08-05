use async_trait::async_trait;
use chrono::Utc;
use data_integrity::IntegrityFinding;
use data_repair::{
    CreateRepairExecution, CreateRepairResult, RepairCommand, RepairDescriptor, RepairError,
    RepairExecutionContext, RepairHandler, RepairHandlerRegistry, RepairLedgerEntry, RepairOutcome,
    RepairPersistencePort, RepairPreview, RepairResult, RepairRiskLevel, RepairRun,
    RepairRunStatus, RepairStep, RepairStepStatus, RepairTarget, RepairVerification,
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
    fail_load_run: bool,
    fail_load_finding: bool,
}

impl FakePersistence {
    fn persist_run(&self, run: &RepairRun) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .run = run.clone();
    }

    fn persist_step_fenced(
        &self,
        step: &RepairStep,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.step.fence_version() != expected_fence_version
            || state.step.lease_owner().is_none()
            || state.step.lease_token().is_none()
            || step.fence_version() != expected_fence_version
        {
            return Err(RepairError::LeaseLost);
        }
        state.step = step.clone();
        Ok(())
    }
}

#[async_trait]
impl RepairPersistencePort for FakePersistence {
    async fn create_repair_execution(
        &self,
        _command: CreateRepairExecution,
    ) -> Result<CreateRepairResult, RepairError> {
        Err(RepairError::Persistence)
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
        if state.fail_load_finding {
            return Err(RepairError::Persistence);
        }
        let run = &state.run;
        Ok(Some(
            IntegrityFinding::rehydrate(
                run.finding_id(),
                run.tenant_id(),
                "TEST-RULE".to_string(),
                1,
                "document-processing".to_string(),
                run.command().target.resource_type.clone(),
                run.command().target.resource_id.clone(),
                data_integrity::IntegritySeverity::Warning,
                "test-fingerprint".to_string(),
                serde_json::json!({}),
                serde_json::json!({}),
                data_integrity::FindingStatus::Open,
                run.command().repair_type.clone(),
                run.created_at(),
                run.updated_at(),
                1,
                None,
                None,
                None,
                0,
                None,
                0,
            )
            .map_err(|_| RepairError::Persistence)?,
        ))
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
        _expected_run_version: i64,
        expected_fence_version: i64,
        _lease_owner: &str,
        _lease_token: &str,
    ) -> Result<(), RepairError> {
        self.persist_step_fenced(step, expected_fence_version)?;
        self.append_ledger(entry).await?;
        self.persist_run(run);
        Ok(())
    }

    async fn commit_failure(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        _expected_run_version: i64,
        expected_fence_version: i64,
        _lease_owner: &str,
        _lease_token: &str,
    ) -> Result<(), RepairError> {
        self.persist_step_fenced(step, expected_fence_version)?;
        self.append_ledger(entry).await?;
        self.persist_run(run);
        Ok(())
    }

    async fn classify_claimed_failure(
        &self,
        step_id: Uuid,
        run_id: Uuid,
        lease_owner: &str,
        lease_token: &str,
        expected_fence_version: i64,
        disposition: data_repair::RepairFailureDisposition,
        _failure_code: &str,
        next_attempt_at: Option<chrono::DateTime<Utc>>,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), RepairError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.step.id() != step_id
            || state.step.run_id() != run_id
            || state.step.lease_owner() != Some(lease_owner)
            || state.step.lease_token() != Some(lease_token)
            || state.step.fence_version() != expected_fence_version
        {
            return Err(RepairError::LeaseLost);
        }
        match disposition {
            data_repair::RepairFailureDisposition::Retry { .. } => {
                state.step.schedule_retry(next_attempt_at.unwrap_or(now))?;
                state.run.schedule_retry(now)?;
            }
            data_repair::RepairFailureDisposition::Permanent => {
                state.step.fail()?;
                state.run.mark_failed(now)?;
            }
            data_repair::RepairFailureDisposition::NeedsManualReview => {
                state.step.require_manual_review()?;
                state.run.mark_needs_manual_review(now)?;
            }
            data_repair::RepairFailureDisposition::Cancelled => {
                state.step.cancel()?;
                state.run.cancel(now)?;
            }
            data_repair::RepairFailureDisposition::LeaseLost => return Err(RepairError::LeaseLost),
        }
        Ok(())
    }
    async fn load_run(&self, _id: Uuid) -> Result<Option<RepairRun>, RepairError> {
        if self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .fail_load_run
        {
            return Err(RepairError::Persistence);
        }
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
        state.step.approve()?;
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
        state.run.cancel(Utc::now())?;
        state.step.request_cancel()?;
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
        state.run.resume(Utc::now())?;
        state.step.resume(Utc::now())?;
        Ok(state.run.clone())
    }

    async fn claim_step(
        &self,
        worker_id: &str,
        now: chrono::DateTime<Utc>,
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
        step.claim(
            worker_id.to_string(),
            "fenced-token".to_string(),
            step.fence_version().saturating_add(1),
            Utc::now() + chrono::Duration::seconds(30),
        )?;
        state.step = step.clone();
        let run = state.run.clone();
        state.run = RepairRun::rehydrate(
            run.id(),
            run.tenant_id(),
            run.finding_id(),
            run.command().clone(),
            RepairRunStatus::Running,
            run.created_by(),
            run.approved_by(),
            run.approval_note().map(ToString::to_string),
            run.created_at(),
            now,
            run.version().saturating_add(1),
        )?;
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
        if step.id() != step_id
            || step.lease_owner() != Some(lease_owner)
            || step.lease_token() != Some(lease_token)
            || step.fence_version() != fence_version
        {
            return Err(RepairError::LeaseLost);
        }
        step.heartbeat(now, now + chrono::Duration::seconds(lease_duration_secs))?;
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
        if step.id() == step_id
            && step.lease_owner() == Some(lease_owner)
            && step.lease_token() == Some(lease_token)
            && step.fence_version() == fence_version
            && step.lease_expires_at().is_none_or(|expires| expires > now)
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
    let run = RepairRun::new(
        run_id,
        tenant_id,
        finding_id,
        command,
        RepairRunStatus::Queued,
        Uuid::new_v4(),
        now,
    )
    .unwrap_or_else(|_| unreachable!());
    let step = RepairStep::new(
        Uuid::new_v4(),
        run_id,
        finding_id,
        RepairStepStatus::Queued,
        now,
    )
    .unwrap_or_else(|_| unreachable!());
    (run, step)
}

#[tokio::test]
async fn repair_worker_executes_once_and_records_ledger() {
    let (run, step) = fixture();
    let state = Arc::new(Mutex::new(FakeState {
        run,
        step,
        claimed: false,
        ledger: Vec::new(),
        fail_load_run: false,
        fail_load_finding: false,
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
        max_attempts: 3,
    };
    let first = worker.execute_one().await;
    assert!(first.is_ok(), "first repair execution failed: {first:?}");
    assert!(first.unwrap_or_else(|_| unreachable!()));
    assert!(!worker
        .execute_one()
        .await
        .unwrap_or_else(|_| unreachable!()));
    let state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(state.run.status(), RepairRunStatus::Succeeded);
    assert_eq!(state.step.status(), RepairStepStatus::Succeeded);
    assert_eq!(state.ledger.len(), 1);
}

#[tokio::test]
async fn transient_post_claim_load_failure_is_requeued() {
    let (run, step) = fixture();
    let state = Arc::new(Mutex::new(FakeState {
        run,
        step,
        claimed: false,
        ledger: Vec::new(),
        fail_load_run: true,
        fail_load_finding: false,
    }));
    let worker = RepairWorker {
        persistence: Arc::new(FakePersistence {
            state: Arc::clone(&state),
        }),
        handlers: Arc::new(FakeRegistry),
        rule_registry: None,
        worker_id: "worker-transient".to_string(),
        lease_duration_secs: 30,
        heartbeat_seconds: 5,
        max_attempts: 3,
    };

    assert!(worker.execute_one().await.is_ok());
    let state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(state.run.status(), RepairRunStatus::Queued);
    assert_eq!(state.step.status(), RepairStepStatus::Queued);
    assert!(state.step.lease_owner().is_none());
    assert!(state.step.lease_token().is_none());
    assert!(state.step.next_attempt_at() > Utc::now());
}
