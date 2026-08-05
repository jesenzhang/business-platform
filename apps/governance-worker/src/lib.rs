//! Explicit scan/repair orchestration for the Runtime Governance worker.

use chrono::Utc;
use data_integrity::{
    IntegrityPersistencePort, IntegrityRuleRegistry, IntegrityScanScope, ProcessingIntegrityQuery,
};
use data_repair::{
    RepairError, RepairFailureDisposition, RepairHandlerRegistry, RepairLedgerEntry,
    RepairPersistencePort, RepairResult, RepairRun, RepairRunStatus, RepairStep,
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
    pub max_attempts: u32,
}

fn failure_disposition(error: &RepairError) -> RepairFailureDisposition {
    match error {
        RepairError::Unavailable | RepairError::Persistence => RepairFailureDisposition::Retry {
            backoff: chrono::Duration::seconds(5),
        },
        RepairError::Conflict => RepairFailureDisposition::NeedsManualReview,
        RepairError::LeaseLost => RepairFailureDisposition::LeaseLost,
        RepairError::InvalidDescriptor
        | RepairError::InvalidCommand
        | RepairError::InvalidStoredEnum
        | RepairError::InvalidTransition
        | RepairError::ApprovalRequired
        | RepairError::ApprovalSeparation
        | RepairError::IdempotencyConflict => RepairFailureDisposition::Permanent,
    }
}

fn failure_code(_error: &RepairError, disposition: &RepairFailureDisposition) -> &'static str {
    match disposition {
        RepairFailureDisposition::Retry { .. } => "dependency_unavailable",
        RepairFailureDisposition::Permanent => "permanent_execution_failure",
        RepairFailureDisposition::NeedsManualReview => "execution_conflict",
        RepairFailureDisposition::LeaseLost => "lease_lost",
        RepairFailureDisposition::Cancelled => "cancelled",
    }
}

fn failure_result() -> data_repair::RepairResult {
    data_repair::RepairResult {
        command_id: Uuid::now_v7(),
        resource_version_before: None,
        resource_version_after: None,
        before_hash: "unavailable".to_string(),
        after_hash: "unavailable".to_string(),
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
) -> Result<RepairLedgerEntry, RepairError> {
    RepairLedgerEntry::new(
        Uuid::now_v7(),
        run.tenant_id(),
        run.id(),
        step.id(),
        run.finding_id(),
        finding.rule_id().to_string(),
        run.command().repair_type.clone(),
        run.command().repair_version,
        "repair_job".to_string(),
        run.created_by(),
        run.command().reason.clone(),
        finding.resource_type().to_string(),
        finding.resource_id().to_string(),
        result.before_hash.clone(),
        result.after_hash.clone(),
        json!({ "hash": result.before_hash }),
        json!({ "hash": result.after_hash }),
        result.rows_affected,
        result.outcome,
        failure_code,
        None,
        started_at,
        Utc::now(),
        None,
        None,
    )
}

impl<P, H> RepairWorker<P, H>
where
    P: RepairPersistencePort + 'static,
    H: RepairHandlerRegistry + 'static,
{
    #[allow(clippy::too_many_arguments)]
    async fn persist_failure(
        &self,
        mut run: RepairRun,
        mut step: RepairStep,
        finding: &data_integrity::IntegrityFinding,
        result: RepairResult,
        error: &RepairError,
        started_at: chrono::DateTime<Utc>,
        lease_token: &str,
    ) -> Result<bool, RepairError> {
        let disposition = failure_disposition(error);
        if disposition == RepairFailureDisposition::LeaseLost {
            tracing::warn!(
                worker_id = %self.worker_id,
                step_id = %step.id(),
                run_id = %run.id(),
                "repair lease was lost; worker will not commit a result"
            );
            return Ok(true);
        }

        let expected_run_version = run.version();
        let now = Utc::now();
        let mut code = failure_code(error, &disposition).to_string();
        match disposition {
            RepairFailureDisposition::Retry { backoff }
                if step.attempt_count() < self.max_attempts =>
            {
                run.schedule_retry(now)?;
                step.schedule_retry(now + backoff)?;
            }
            RepairFailureDisposition::Retry { .. } => {
                run.mark_failed(now)?;
                step.fail()?;
                code = "retry_exhausted".to_string();
            }
            RepairFailureDisposition::Permanent => {
                run.mark_failed(now)?;
                step.fail()?;
            }
            RepairFailureDisposition::NeedsManualReview => {
                run.mark_needs_manual_review(now)?;
                step.require_manual_review()?;
            }
            RepairFailureDisposition::Cancelled => {
                run.cancel(now)?;
                step.cancel()?;
            }
            RepairFailureDisposition::LeaseLost => unreachable!(),
        }
        let entry = ledger_entry(&run, &step, finding, &result, Some(code), started_at)?;
        self.persistence
            .commit_failure(
                &run,
                &step,
                &entry,
                expected_run_version,
                step.fence_version(),
                &self.worker_id,
                lease_token,
            )
            .await?;
        Ok(true)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn execute_one(&self) -> Result<bool, RepairError> {
        let now = Utc::now();
        let Some(step) = self
            .persistence
            .claim_step(&self.worker_id, now, self.lease_duration_secs)
            .await?
        else {
            return Ok(false);
        };
        let claim_snapshot = step.clone();
        match self.execute_claimed(step).await {
            Ok(result) => Ok(result),
            Err(error) => {
                eprintln!("[DEBUG-PLAN0005] execute_claimed failed: {error:?}");
                if matches!(error, RepairError::LeaseLost) {
                    tracing::warn!(
                        worker_id = %self.worker_id,
                        step_id = %claim_snapshot.id(),
                        run_id = %claim_snapshot.run_id(),
                        "repair lease was lost before a durable result could be committed"
                    );
                    return Ok(true);
                }
                let reason = match &error {
                    RepairError::Conflict | RepairError::IdempotencyConflict => {
                        "post_claim_conflict"
                    }
                    RepairError::InvalidDescriptor => "post_claim_invalid_descriptor",
                    RepairError::InvalidTransition => "post_claim_invalid_transition",
                    RepairError::ApprovalRequired => "post_claim_approval_required",
                    RepairError::ApprovalSeparation => "post_claim_approval_separation",
                    RepairError::LeaseLost => "post_claim_lease_lost",
                    RepairError::Unavailable => "post_claim_dependency_unavailable",
                    RepairError::Persistence
                    | RepairError::InvalidCommand
                    | RepairError::InvalidStoredEnum => "post_claim_persistence_failure",
                };
                if let Err(abort_error) = self
                    .persistence
                    .abort_claimed_repair(&claim_snapshot, &self.worker_id, reason)
                    .await
                {
                    tracing::error!(
                        worker_id = %self.worker_id,
                        step_id = %claim_snapshot.id(),
                        run_id = %claim_snapshot.run_id(),
                        error = %abort_error,
                        original_error = %error,
                        "failed to durably abort claimed repair"
                    );
                } else {
                    tracing::error!(
                        worker_id = %self.worker_id,
                        step_id = %claim_snapshot.id(),
                        run_id = %claim_snapshot.run_id(),
                        error = %error,
                        "claimed repair moved to manual review after execution failure"
                    );
                }
                Ok(true)
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn execute_claimed(
        &self,
        mut step: data_repair::RepairStep,
    ) -> Result<bool, RepairError> {
        let now = Utc::now();
        let lease_token = step
            .lease_token()
            .ok_or(RepairError::LeaseLost)?
            .to_string();
        eprintln!("[DEBUG-PLAN0005] worker stage=fence1_start");
        self.persistence
            .validate_repair_fence(
                step.id(),
                &self.worker_id,
                &lease_token,
                step.fence_version(),
                now,
            )
            .await?;
        eprintln!("[DEBUG-PLAN0005] worker stage=fence1_ok");
        step = self
            .persistence
            .heartbeat_repair_step(
                step.id(),
                &self.worker_id,
                &lease_token,
                step.fence_version(),
                now,
                self.lease_duration_secs,
            )
            .await?;
        eprintln!("[DEBUG-PLAN0005] worker stage=heartbeat1_ok");
        eprintln!("[DEBUG-PLAN0005] worker stage=load_run_start");
        let Some(mut run) = self.persistence.load_run(step.run_id()).await? else {
            return Err(RepairError::Persistence);
        };
        eprintln!("[DEBUG-PLAN0005] worker stage=load_run_ok");
        if run.command().integrity_finding_id != run.finding_id()
            || run.command().tenant_id != run.tenant_id()
        {
            return Err(RepairError::Conflict);
        }
        eprintln!("[DEBUG-PLAN0005] worker stage=load_finding_start");
        let finding = self
            .persistence
            .load_finding(run.finding_id())
            .await?
            .ok_or(RepairError::Conflict)?;
        eprintln!("[DEBUG-PLAN0005] worker stage=load_finding_ok");
        if finding.tenant_id() != run.tenant_id()
            || finding.resource_type() != run.command().target.resource_type
            || finding.resource_id() != run.command().target.resource_id
            || finding.repairability() != run.command().repair_type
            || matches!(
                finding.status(),
                data_integrity::FindingStatus::Repaired
                    | data_integrity::FindingStatus::FalsePositive
                    | data_integrity::FindingStatus::Stale
            )
        {
            return Err(RepairError::Conflict);
        }
        if !matches!(
            run.status(),
            RepairRunStatus::Queued | RepairRunStatus::Running
        ) {
            return Err(RepairError::InvalidTransition);
        }
        eprintln!("[DEBUG-PLAN0005] worker stage=handler_start");
        let Some(handler) = self
            .handlers
            .get(&run.command().repair_type, run.command().repair_version)
            .await
        else {
            return Err(RepairError::InvalidDescriptor);
        };
        eprintln!("[DEBUG-PLAN0005] worker stage=handler_ok");
        let descriptor = handler.descriptor();
        descriptor.validate()?;
        if descriptor.requires_approval && run.approved_by().is_none() {
            return Err(RepairError::ApprovalRequired);
        }
        if descriptor.requires_approval && run.approved_by() == Some(run.created_by()) {
            return Err(RepairError::ApprovalSeparation);
        }
        let context = data_repair::RepairExecutionContext {
            run_id: run.id(),
            step_id: step.id(),
            worker_id: self.worker_id.clone(),
            fence_version: step.fence_version(),
            lease_token: step
                .lease_token()
                .ok_or(RepairError::LeaseLost)?
                .to_string(),
            now,
            lease_expires_at: step.lease_expires_at().ok_or(RepairError::LeaseLost)?,
        };
        eprintln!("[DEBUG-PLAN0005] worker stage=fence2_start");
        self.persistence
            .validate_repair_fence(
                step.id(),
                &self.worker_id,
                &context.lease_token,
                context.fence_version,
                Utc::now(),
            )
            .await?;
        eprintln!("[DEBUG-PLAN0005] worker stage=fence2_ok");
        let heartbeat_interval = TokioDuration::from_secs(
            self.heartbeat_seconds
                .max(1)
                .min(self.lease_duration_secs.saturating_sub(1).max(1))
                .cast_unsigned(),
        );
        let command_for_execution = run.command().clone();
        let mut execution = std::pin::pin!(handler.execute(&command_for_execution, &context));
        let execution_result = loop {
            tokio::select! {
                result = &mut execution => break result,
                () = sleep(heartbeat_interval) => {
                    step = self.persistence.heartbeat_repair_step(
                        step.id(),
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
                return self
                    .persist_failure(
                        run,
                        step,
                        &finding,
                        failure_result(),
                        &error,
                        now,
                        &context.lease_token,
                    )
                    .await;
            }
        };
        if matches!(
            result.outcome,
            data_repair::RepairOutcome::Conflict | data_repair::RepairOutcome::Failed
        ) {
            let error = if result.outcome == data_repair::RepairOutcome::Conflict {
                RepairError::Conflict
            } else {
                RepairError::Persistence
            };
            return self
                .persist_failure(
                    run,
                    step,
                    &finding,
                    result,
                    &error,
                    now,
                    &context.lease_token,
                )
                .await;
        }
        let command_for_verification = run.command().clone();
        let verification = match handler
            .verify_after_repair(&command_for_verification, &result)
            .await
        {
            Ok(verification) => verification,
            Err(error) => {
                return self
                    .persist_failure(
                        run,
                        step,
                        &finding,
                        result,
                        &error,
                        now,
                        &context.lease_token,
                    )
                    .await;
            }
        };
        let rule_verified = match self.rule_registry.as_ref() {
            Some(registry) => registry
                .verify_finding(&finding)
                .await
                .map_err(|_| RepairError::Unavailable)?,
            None => verification.valid,
        };
        if !verification.valid || !rule_verified {
            return self
                .persist_failure(
                    run,
                    step,
                    &finding,
                    RepairResult {
                        outcome: data_repair::RepairOutcome::Failed,
                        ..result
                    },
                    &RepairError::Conflict,
                    now,
                    &context.lease_token,
                )
                .await;
        }
        let expected_run_version = run.version();
        let mut completed = step;
        completed.succeed()?;
        run.mark_succeeded(Utc::now())?;
        let entry = ledger_entry(&run, &completed, &finding, &result, None, now)?;
        eprintln!("[DEBUG-PLAN0005] worker stage=final_fence_start");
        self.persistence
            .validate_repair_fence(
                completed.id(),
                &self.worker_id,
                &context.lease_token,
                context.fence_version,
                Utc::now(),
            )
            .await?;
        self.persistence
            .commit_success(
                &run,
                &completed,
                &entry,
                expected_run_version,
                context.fence_version,
                &context.worker_id,
                &context.lease_token,
            )
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
    pub fn new(
        query: std::sync::Arc<Q>,
        persistence: std::sync::Arc<P>,
    ) -> Result<Self, data_integrity::IntegrityError> {
        let mut registry = IntegrityRuleRegistry::default();
        for rule in data_integrity::processing_rules(std::sync::Arc::clone(&query)) {
            // Registration is deterministic and descriptors are validated at
            // composition time; an invalid built-in rule is a startup error.
            registry.register(rule)?;
        }
        Ok(Self {
            query,
            persistence,
            registry,
        })
    }

    pub async fn run_explicit_scan(
        &self,
        scope: IntegrityScanScope,
        created_by: Uuid,
    ) -> Result<ScanReport, GovernanceError> {
        run_integrity_scan(&self.registry, self.persistence.as_ref(), scope, created_by).await
    }
}
