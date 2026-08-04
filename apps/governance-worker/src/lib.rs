//! Explicit scan/repair orchestration for the Runtime Governance worker.

use chrono::Utc;
use data_integrity::{
    IntegrityPersistencePort, IntegrityRuleRegistry, IntegrityScanScope, ProcessingIntegrityQuery,
};
use data_repair::{
    RepairError, RepairHandlerRegistry, RepairLedgerEntry, RepairPersistencePort, RepairResult,
    RepairRunStatus, RepairStepStatus,
};
use runtime_governance::{run_integrity_scan, GovernanceError, ScanReport};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

pub struct GovernanceWorker<Q, P> {
    pub query: std::sync::Arc<Q>,
    pub persistence: std::sync::Arc<P>,
    pub registry: IntegrityRuleRegistry,
}

/// Durable repair execution loop.  It claims one fenced step at a time and
/// stops safely when no approved work is available; callers may invoke it from
/// an explicit wake-up or a bounded worker loop, never from a generic scheduler.
pub struct RepairWorker<P, H> {
    pub persistence: std::sync::Arc<P>,
    pub handlers: std::sync::Arc<H>,
    /// Production composition supplies the immutable rule registry. `None`
    /// is retained only for isolated handler-contract fakes.
    pub rule_registry: Option<std::sync::Arc<IntegrityRuleRegistry>>,
    pub worker_id: String,
    pub lease_duration_secs: i64,
    pub heartbeat_seconds: i64,
}

fn failure_result() -> data_repair::RepairResult {
    data_repair::RepairResult {
        command_id: Uuid::now_v7(),
        resource_version_before: None,
        resource_version_after: None,
        before_hash: String::new(),
        after_hash: String::new(),
        rows_affected: 0,
        outcome: data_repair::RepairOutcome::Failed,
    }
}

fn ledger_entry(
    run: &data_repair::RepairRun,
    step: &data_repair::RepairStep,
    finding: &data_integrity::IntegrityFinding,
    result: &data_repair::RepairResult,
    failure_code: Option<String>,
    started_at: chrono::DateTime<Utc>,
) -> RepairLedgerEntry {
    RepairLedgerEntry {
        id: Uuid::now_v7(),
        tenant_id: run.tenant_id,
        repair_run_id: run.id,
        repair_step_id: step.id,
        finding_id: run.finding_id,
        rule_id: finding.rule_id.clone(),
        repair_type: run.command.repair_type.clone(),
        repair_version: run.command.repair_version,
        actor_type: "repair_job".to_string(),
        actor_id: run.created_by,
        reason: run.command.reason.clone(),
        resource_type: finding.resource_type.clone(),
        resource_id: finding.resource_id.clone(),
        before_hash: result.before_hash.clone(),
        after_hash: result.after_hash.clone(),
        before_snapshot: json!({ "hash": result.before_hash }),
        after_snapshot: json!({ "hash": result.after_hash }),
        rows_affected: result.rows_affected,
        result: result.outcome,
        failure_code,
        trace_id: None,
        started_at,
        finished_at: Utc::now(),
        previous_hash: None,
        record_hash: None,
    }
}

impl<P, H> RepairWorker<P, H>
where
    P: RepairPersistencePort + 'static,
    H: RepairHandlerRegistry + 'static,
{
    #[allow(clippy::too_many_lines)]
    pub async fn execute_one(&self) -> Result<bool, RepairError> {
        let now = Utc::now();
        let Some(mut step) = self
            .persistence
            .claim_step(&self.worker_id, now, self.lease_duration_secs)
            .await?
        else {
            return Ok(false);
        };
        let lease_token = step.lease_token.clone().ok_or(RepairError::LeaseLost)?;
        self.persistence
            .validate_repair_fence(
                step.id,
                &self.worker_id,
                &lease_token,
                step.fence_version,
                now,
            )
            .await?;
        step = self
            .persistence
            .heartbeat_repair_step(
                step.id,
                &self.worker_id,
                &lease_token,
                step.fence_version,
                now,
                self.lease_duration_secs,
            )
            .await?;
        let Some(mut run) = self.persistence.load_run(step.run_id).await? else {
            return Err(RepairError::Persistence);
        };
        if run.command.integrity_finding_id != run.finding_id
            || run.command.tenant_id != run.tenant_id
        {
            return Err(RepairError::Conflict);
        }
        let finding = self
            .persistence
            .load_finding(run.finding_id)
            .await?
            .ok_or(RepairError::Conflict)?;
        if finding.tenant_id != run.tenant_id
            || finding.resource_type != run.command.target.resource_type
            || finding.resource_id != run.command.target.resource_id
            || finding.repairability != run.command.repair_type
            || matches!(
                finding.status,
                data_integrity::FindingStatus::Repaired
                    | data_integrity::FindingStatus::FalsePositive
                    | data_integrity::FindingStatus::Stale
            )
        {
            return Err(RepairError::Conflict);
        }
        if !matches!(
            run.status,
            RepairRunStatus::Approved | RepairRunStatus::Queued | RepairRunStatus::Running
        ) {
            return Err(RepairError::InvalidTransition);
        }
        let Some(handler) = self
            .handlers
            .get(&run.command.repair_type, run.command.repair_version)
            .await
        else {
            return Err(RepairError::InvalidDescriptor);
        };
        let descriptor = handler.descriptor();
        descriptor.validate()?;
        if descriptor.requires_approval && run.approved_by.is_none() {
            return Err(RepairError::ApprovalRequired);
        }
        if descriptor.requires_approval && run.approved_by == Some(run.created_by) {
            return Err(RepairError::ApprovalSeparation);
        }
        let context = data_repair::RepairExecutionContext {
            run_id: run.id,
            step_id: step.id,
            worker_id: self.worker_id.clone(),
            fence_version: step.fence_version,
            lease_token: step.lease_token.clone().ok_or(RepairError::LeaseLost)?,
            now,
            lease_expires_at: step.lease_expires_at.ok_or(RepairError::LeaseLost)?,
        };
        self.persistence
            .validate_repair_fence(
                step.id,
                &self.worker_id,
                &context.lease_token,
                context.fence_version,
                Utc::now(),
            )
            .await?;
        let heartbeat_interval = TokioDuration::from_secs(
            self.heartbeat_seconds
                .max(1)
                .min(self.lease_duration_secs.saturating_sub(1).max(1))
                .cast_unsigned(),
        );
        let mut execution = std::pin::pin!(handler.execute(&run.command, &context));
        let execution_result = loop {
            tokio::select! {
                result = &mut execution => break result,
                () = sleep(heartbeat_interval) => {
                    step = self.persistence.heartbeat_repair_step(
                        step.id,
                        &self.worker_id,
                        &context.lease_token,
                        context.fence_version,
                        Utc::now(),
                        self.lease_duration_secs,
                    ).await?;
                }
            }
        };
        let result = match execution_result {
            Ok(result) => result,
            Err(error) => {
                let mut failed_run = run.clone();
                failed_run.status = RepairRunStatus::Failed;
                failed_run.updated_at = Utc::now();
                failed_run.version = failed_run.version.saturating_add(1);
                let mut failed_step = step.clone();
                failed_step.status = RepairStepStatus::Failed;
                failed_step.lease_expires_at = None;
                failed_step.lease_owner = None;
                failed_step.lease_token = None;
                let failure = failure_result();
                let entry = ledger_entry(
                    &failed_run,
                    &failed_step,
                    &finding,
                    &failure,
                    Some("owner_mutation_failed".to_string()),
                    now,
                );
                self.persistence
                    .commit_failure(&failed_run, &failed_step, &entry, context.fence_version)
                    .await?;
                return Err(error);
            }
        };
        if matches!(
            result.outcome,
            data_repair::RepairOutcome::Conflict | data_repair::RepairOutcome::Failed
        ) {
            let mut failed_run = run.clone();
            failed_run.status = RepairRunStatus::Failed;
            failed_run.updated_at = Utc::now();
            failed_run.version = failed_run.version.saturating_add(1);
            let mut failed_step = step.clone();
            failed_step.status = RepairStepStatus::Failed;
            failed_step.lease_expires_at = None;
            failed_step.lease_owner = None;
            failed_step.lease_token = None;
            let entry = ledger_entry(
                &failed_run,
                &failed_step,
                &finding,
                &result,
                Some(
                    if result.outcome == data_repair::RepairOutcome::Conflict {
                        "owner_mutation_conflict"
                    } else {
                        "owner_mutation_failed"
                    }
                    .to_string(),
                ),
                now,
            );
            self.persistence
                .commit_failure(&failed_run, &failed_step, &entry, context.fence_version)
                .await?;
            return Err(if result.outcome == data_repair::RepairOutcome::Conflict {
                RepairError::Conflict
            } else {
                RepairError::Persistence
            });
        }
        let verification = handler.verify_after_repair(&run.command, &result).await?;
        let rule_verified = match self.rule_registry.as_ref() {
            Some(registry) => registry
                .verify_finding(&finding)
                .await
                .map_err(|_| RepairError::Unavailable)?,
            None => verification.valid,
        };
        if !verification.valid || !rule_verified {
            run.status = RepairRunStatus::NeedsManualReview;
            run.updated_at = Utc::now();
            run.version = run.version.saturating_add(1);
            let mut review_step = step.clone();
            review_step.status = RepairStepStatus::NeedsManualReview;
            review_step.lease_expires_at = None;
            review_step.lease_owner = None;
            review_step.lease_token = None;
            let entry = ledger_entry(
                &run,
                &review_step,
                &finding,
                &RepairResult {
                    outcome: data_repair::RepairOutcome::Failed,
                    ..result.clone()
                },
                Some("verification_failed".to_string()),
                now,
            );
            self.persistence
                .commit_failure(&run, &review_step, &entry, context.fence_version)
                .await?;
            return Err(RepairError::Conflict);
        }
        let mut completed = step;
        completed.status = RepairStepStatus::Succeeded;
        completed.lease_expires_at = None;
        completed.lease_owner = None;
        completed.lease_token = None;
        run.status = RepairRunStatus::Succeeded;
        run.updated_at = Utc::now();
        run.version = run.version.saturating_add(1);
        let entry = ledger_entry(&run, &completed, &finding, &result, None, now);
        self.persistence
            .validate_repair_fence(
                completed.id,
                &self.worker_id,
                &context.lease_token,
                context.fence_version,
                Utc::now(),
            )
            .await?;
        self.persistence
            .commit_success(&run, &completed, &entry, context.fence_version)
            .await?;
        Ok(true)
    }

    /// Bounded, dedicated repair consumer. It never claims more than one
    /// step per poll and sleeps when the queue is empty, so it is not a
    /// generic scheduler or a busy loop.
    pub async fn run_loop(
        &self,
        poll_interval: TokioDuration,
        heartbeat_seconds: i64,
        batch_size: u32,
        once: bool,
        stop: Arc<AtomicBool>,
    ) -> Result<(), RepairError> {
        let heartbeat_seconds = heartbeat_seconds.max(1);
        let batch_size = batch_size.clamp(1, 1_000);
        loop {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let mut processed = 0_u32;
            while processed < batch_size && !stop.load(Ordering::Acquire) {
                if !self.execute_one().await? {
                    break;
                }
                processed = processed.saturating_add(1);
                if once {
                    break;
                }
            }
            if once || stop.load(Ordering::Acquire) {
                break;
            }
            if processed == 0 {
                sleep(poll_interval).await;
            } else {
                // Keep a bounded heartbeat cadence while draining a queue;
                // this also prevents an accidental tight loop on a hot queue.
                sleep(TokioDuration::from_secs(
                    heartbeat_seconds.min(1).cast_unsigned(),
                ))
                .await;
            }
        }
        Ok(())
    }
}

impl<Q, P> GovernanceWorker<Q, P>
where
    Q: ProcessingIntegrityQuery + 'static,
    P: IntegrityPersistencePort + 'static,
{
    pub fn new(query: std::sync::Arc<Q>, persistence: std::sync::Arc<P>) -> Self {
        let mut registry = IntegrityRuleRegistry::default();
        for rule in data_integrity::processing_rules(std::sync::Arc::clone(&query)) {
            // Registration is deterministic and descriptors are validated at
            // composition time; an invalid built-in rule is a startup error.
            if registry.register(rule).is_err() {
                tracing::error!("built-in processing integrity rule registration failed");
            }
        }
        Self {
            query,
            persistence,
            registry,
        }
    }

    pub async fn run_explicit_scan(
        &self,
        scope: IntegrityScanScope,
        created_by: Uuid,
    ) -> Result<ScanReport, GovernanceError> {
        run_integrity_scan(&self.registry, self.persistence.as_ref(), scope, created_by).await
    }
}
