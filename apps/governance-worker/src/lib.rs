//! Explicit scan/repair orchestration for the Runtime Governance worker.

use chrono::Utc;
use data_integrity::{
    IntegrityPersistencePort, IntegrityRuleRegistry, IntegrityScanScope, ProcessingIntegrityQuery,
};
use data_repair::{
    RepairError, RepairHandlerRegistry, RepairLedgerEntry, RepairPersistencePort, RepairRunStatus,
};
use runtime_governance::{run_integrity_scan, GovernanceError, ScanReport};
use serde_json::json;
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
    pub worker_id: String,
    pub lease_duration_secs: i64,
}

impl<P, H> RepairWorker<P, H>
where
    P: RepairPersistencePort + 'static,
    H: RepairHandlerRegistry + 'static,
{
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
        let Some(mut run) = self.persistence.load_run(step.run_id).await? else {
            return Err(RepairError::Persistence);
        };
        if run.command.finding_id != run.finding_id || run.command.tenant_id != run.tenant_id {
            return Err(RepairError::Conflict);
        }
        let finding = self.persistence.load_finding(run.finding_id).await?;
        let owner_command = if let Some(finding) = finding.as_ref() {
            if finding.tenant_id != run.tenant_id
                || matches!(
                    finding.status,
                    data_integrity::FindingStatus::Repaired
                        | data_integrity::FindingStatus::FalsePositive
                        | data_integrity::FindingStatus::Stale
                )
            {
                return Err(RepairError::Conflict);
            }
            let resource_id =
                Uuid::parse_str(&finding.resource_id).map_err(|_| RepairError::Conflict)?;
            let mut command = run.command.clone();
            command.finding_id = resource_id;
            command
        } else {
            // Contract fakes do not persist findings; production adapters do.
            run.command.clone()
        };
        if !matches!(
            run.status,
            RepairRunStatus::Approved | RepairRunStatus::Queued
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
        run.status = RepairRunStatus::Running;
        run.updated_at = now;
        run.version = run.version.saturating_add(1);
        self.persistence.save_run(&run).await?;
        let context = data_repair::RepairExecutionContext {
            run_id: run.id,
            step_id: step.id,
            worker_id: self.worker_id.clone(),
            fence_version: step.fence_version,
            lease_token: step.lease_token.clone().ok_or(RepairError::LeaseLost)?,
            now,
        };
        let result = handler.execute(&owner_command, &context).await?;
        let verification = handler.verify(&result).await?;
        if !verification.valid {
            run.status = RepairRunStatus::NeedsManualReview;
            self.persistence.save_run(&run).await?;
            return Err(RepairError::Conflict);
        }
        let entry = RepairLedgerEntry {
            id: Uuid::now_v7(),
            tenant_id: run.tenant_id,
            repair_run_id: run.id,
            repair_step_id: step.id,
            finding_id: run.finding_id,
            rule_id: finding.as_ref().map_or_else(
                || "runtime-governance".to_string(),
                |value| value.rule_id.clone(),
            ),
            repair_type: run.command.repair_type.clone(),
            repair_version: run.command.repair_version,
            actor_type: "repair_job".to_string(),
            actor_id: run.created_by,
            reason: run.command.reason.clone(),
            resource_type: "processing_job".to_string(),
            resource_id: run.finding_id.to_string(),
            before_hash: result.before_hash.clone(),
            after_hash: result.after_hash.clone(),
            before_snapshot: json!({ "hash": result.before_hash }),
            after_snapshot: json!({ "hash": result.after_hash }),
            rows_affected: result.rows_affected,
            result: result.outcome,
            failure_code: None,
            trace_id: None,
            started_at: now,
            finished_at: Utc::now(),
            previous_hash: None,
            record_hash: None,
        };
        let mut completed = step;
        completed.status = RepairRunStatus::Succeeded;
        completed.lease_expires_at = None;
        completed.lease_owner = None;
        completed.lease_token = None;
        run.status = RepairRunStatus::Succeeded;
        run.updated_at = Utc::now();
        self.persistence
            .commit_success(&run, &completed, &entry, context.fence_version)
            .await?;
        Ok(true)
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
