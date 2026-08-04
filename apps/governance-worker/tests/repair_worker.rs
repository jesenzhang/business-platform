use async_trait::async_trait;
use chrono::Utc;
use data_repair::{
    RepairCommand, RepairDescriptor, RepairError, RepairExecutionContext, RepairHandler,
    RepairHandlerRegistry, RepairLedgerEntry, RepairOutcome, RepairPersistencePort, RepairPreview,
    RepairResult, RepairRiskLevel, RepairRun, RepairRunStatus, RepairStep, RepairVerification,
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

    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ledger
            .push(entry.clone());
        Ok(())
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
        step.status = RepairRunStatus::Running;
        step.lease_owner = Some(worker_id.to_string());
        step.lease_token = Some("fenced-token".to_string());
        step.fence_version = 1;
        Ok(Some(step))
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
            finding_id: command.finding_id,
            resource_type: "processing_job".to_string(),
            resource_id: command.finding_id.to_string(),
            before_hash: "before".to_string(),
            expected_after_hash: Some("after".to_string()),
            affected_count: 1,
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
        finding_id,
        repair_type: "typed.v1".to_string(),
        repair_version: 1,
        requested_by: Uuid::new_v4(),
        reason: "test".to_string(),
        expected_resource_version: Some(1),
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
            status: RepairRunStatus::Queued,
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
        worker_id: "worker-a".to_string(),
        lease_duration_secs: 30,
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
    assert_eq!(state.step.status, RepairRunStatus::Succeeded);
    assert_eq!(state.ledger.len(), 1);
}
