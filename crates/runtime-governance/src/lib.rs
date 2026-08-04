//! Application coordination for explicit integrity scans and typed repairs.

use async_trait::async_trait;
use chrono::Utc;
use data_integrity::{
    IntegrityError, IntegrityFinding, IntegrityPersistencePort, IntegrityRuleRegistry,
    IntegrityScanRun, IntegrityScanScope, ScanRunStatus,
};
use data_repair::{RepairCommand, RepairError, RepairHandler, RepairPreview};
use thiserror::Error;
use uuid::Uuid;

pub mod processing_repairs;

#[derive(Debug, Error)]
pub enum GovernanceError {
    #[error("integrity operation failed")]
    Integrity(#[from] IntegrityError),
    #[error("repair operation failed")]
    Repair(#[from] RepairError),
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub run: IntegrityScanRun,
    pub findings: Vec<IntegrityFinding>,
}

/// Type-erased application port used by the management API.  The concrete
/// query/persistence adapters remain selected in the composition root.
#[async_trait]
pub trait IntegrityScanPort: Send + Sync {
    async fn run(
        &self,
        scope: IntegrityScanScope,
        created_by: Uuid,
    ) -> Result<ScanReport, GovernanceError>;
}

pub struct ExplicitIntegrityScanner<Q, P> {
    query: std::sync::Arc<Q>,
    persistence: std::sync::Arc<P>,
}

impl<Q, P> ExplicitIntegrityScanner<Q, P> {
    #[must_use]
    pub fn new(query: std::sync::Arc<Q>, persistence: std::sync::Arc<P>) -> Self {
        Self { query, persistence }
    }
}

#[async_trait]
impl<Q, P> IntegrityScanPort for ExplicitIntegrityScanner<Q, P>
where
    Q: data_integrity::ProcessingIntegrityQuery + 'static,
    P: IntegrityPersistencePort + 'static,
{
    async fn run(
        &self,
        scope: IntegrityScanScope,
        created_by: Uuid,
    ) -> Result<ScanReport, GovernanceError> {
        let mut registry = IntegrityRuleRegistry::default();
        for rule in data_integrity::processing_rules(std::sync::Arc::clone(&self.query)) {
            registry.register(rule)?;
        }
        run_integrity_scan(&registry, self.persistence.as_ref(), scope, created_by).await
    }
}

/// Run only the rules explicitly registered by the composition root. There is
/// no background scheduler hidden in this function.
pub async fn run_integrity_scan(
    registry: &IntegrityRuleRegistry,
    persistence: &dyn IntegrityPersistencePort,
    scope: IntegrityScanScope,
    created_by: Uuid,
) -> Result<ScanReport, GovernanceError> {
    let now = Utc::now();
    let mut run = IntegrityScanRun {
        id: Uuid::now_v7(),
        tenant_id: scope.tenant_id,
        scope: scope.clone(),
        status: ScanRunStatus::Running,
        started_at: Some(now),
        finished_at: None,
        rule_count: u32::try_from(registry.rules().len()).unwrap_or(u32::MAX),
        finding_count: 0,
        failure_code: None,
        created_by,
    };
    persistence.record_scan_run(&run).await?;
    let mut findings = Vec::new();
    for rule in registry.rules() {
        let descriptor = rule.descriptor();
        let issues = match rule.scan(&scope).await {
            Ok(issues) => issues,
            Err(error) => {
                run.status = ScanRunStatus::Failed;
                run.failure_code = Some("integrity_rule_failed".to_string());
                run.finished_at = Some(Utc::now());
                persistence.record_scan_run(&run).await?;
                return Err(error.into());
            }
        };
        for issue in issues {
            let finding = IntegrityFinding::from_issue(&descriptor, issue, now)?;
            if let Err(error) = persistence.upsert_finding(&finding).await {
                run.status = ScanRunStatus::Failed;
                run.failure_code = Some("integrity_persistence_failed".to_string());
                run.finished_at = Some(Utc::now());
                persistence.record_scan_run(&run).await?;
                return Err(error.into());
            }
            findings.push(finding);
        }
    }
    run.finding_count = u64::try_from(findings.len()).unwrap_or(u64::MAX);
    run.status = ScanRunStatus::Succeeded;
    run.finished_at = Some(Utc::now());
    persistence.record_scan_run(&run).await?;
    Ok(ScanReport { run, findings })
}

pub async fn dry_run_repair(
    handler: &dyn RepairHandler,
    command: &RepairCommand,
) -> Result<RepairPreview, GovernanceError> {
    command.validate()?;
    handler.descriptor().validate()?;
    if handler.descriptor().repair_type != command.repair_type
        || handler.descriptor().version != command.repair_version
    {
        return Err(RepairError::InvalidDescriptor.into());
    }
    Ok(handler.dry_run(command).await?)
}
