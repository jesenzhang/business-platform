//! `PostgreSQL` Runtime Governance query and persistence adapter.

use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use data_integrity::{
    finding_status_name, DetectedIntegrityIssue, FindingStatus, IntegrityError, IntegrityFinding,
    IntegrityPersistencePort, IntegrityQueryPort, IntegrityRuleDescriptor, IntegrityScanRun,
    IntegrityScanScope, IntegritySeverity, ProcessingIntegrityQuery, ProcessingIntegritySnapshot,
    ProcessingStepIntegritySnapshot, ScanRunStatus, TextArtifactIntegrityState,
};
use data_repair::{
    repair_run_status_name, CreateRepairExecution, CreateRepairResult, RepairCommand, RepairError,
    RepairFailureDisposition, RepairLedgerEntry, RepairPersistencePort, RepairRun, RepairRunStatus,
    RepairStep, RepairStepStatus,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresGovernanceStore {
    pool: PgPool,
}

impl PostgresGovernanceStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    pub async fn insert_test_run(&self, run: &RepairRun) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,worker_id,lease_token,fence_version,lease_expires_at,attempt_count,checkpoint,next_attempt_at,idempotency_key,command,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,0,NULL,NOW(),$12,$13,$14,$15,NOW()) ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status, approved_by=EXCLUDED.approved_by, approval_note=EXCLUDED.approval_note, command=EXCLUDED.command, updated_at=NOW(), version=EXCLUDED.version")
            .bind(run.id()).bind(run.tenant_id()).bind(run.finding_id())
            .bind(repair_run_status_name(run.status())).bind(run.created_by())
            .bind(run.approved_by()).bind(run.approval_note()).bind(Option::<String>::None).bind(Option::<String>::None)
            .bind(0_i64).bind(Option::<DateTime<Utc>>::None)
            .bind(&run.command().idempotency_key)
            .bind(serde_json::to_value(run.command()).map_err(|_| RepairError::Persistence)?)
            .bind(run.version()).bind(run.created_at())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    pub async fn insert_test_step(&self, step: &RepairStep) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at) VALUES ($1,(SELECT tenant_id FROM data_repair_runs WHERE id=$2),$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status, attempt_count=EXCLUDED.attempt_count, checkpoint=EXCLUDED.checkpoint, lease_owner=EXCLUDED.lease_owner, lease_token=EXCLUDED.lease_token, fence_version=EXCLUDED.fence_version, lease_expires_at=EXCLUDED.lease_expires_at, next_attempt_at=EXCLUDED.next_attempt_at, updated_at=NOW()")
            .bind(step.id()).bind(step.run_id()).bind(step.finding_id())
            .bind(data_repair::repair_step_status_name(step.status())).bind(i32::try_from(step.attempt_count()).map_err(|_| RepairError::Persistence)?)
            .bind(step.checkpoint()).bind(step.lease_owner()).bind(step.lease_token())
            .bind(step.fence_version()).bind(step.lease_expires_at()).bind(step.next_attempt_at())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    pub async fn update_test_step_fenced(
        &self,
        step: &RepairStep,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let lease_owner = step.lease_owner().ok_or(RepairError::LeaseLost)?;
        let lease_token = step.lease_token().ok_or(RepairError::LeaseLost)?;
        if expected_fence_version <= 0 || step.fence_version() != expected_fence_version {
            return Err(RepairError::LeaseLost);
        }
        let result = sqlx::query("UPDATE data_repair_steps SET status=$1,attempt_count=$2,checkpoint=$3,lease_owner=$4,lease_token=$5,fence_version=$6,lease_expires_at=$7,next_attempt_at=$8,updated_at=NOW() WHERE id=$9 AND repair_run_id=$10 AND finding_id=$11 AND fence_version=$12 AND lease_owner=$4 AND lease_token=$5 AND lease_expires_at > NOW() AND EXISTS (SELECT 1 FROM data_repair_runs r JOIN data_integrity_findings f ON f.id=r.finding_id AND f.tenant_id=r.tenant_id WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running' AND f.status='repairing')")
            .bind(data_repair::repair_step_status_name(step.status()))
            .bind(i32::try_from(step.attempt_count()).map_err(|_| RepairError::Persistence)?)
            .bind(step.checkpoint())
            .bind(lease_owner)
            .bind(lease_token)
            .bind(step.fence_version())
            .bind(step.lease_expires_at())
            .bind(step.next_attempt_at())
            .bind(step.id())
            .bind(step.run_id())
            .bind(step.finding_id())
            .bind(expected_fence_version)
            .execute(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(RepairError::LeaseLost)
        }
    }
}

fn repair_created_audit(
    tenant_id: Uuid,
    requested_by: Uuid,
    run_id: Uuid,
    finding_id: Uuid,
    now: DateTime<Utc>,
) -> Result<AuditEvent, RepairError> {
    AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        AuditActor {
            actor_type: AuditActorType::User,
            actor_id: requested_by,
        },
        AuditAction::new("repair.created").map_err(|_| RepairError::Persistence)?,
        AuditResource::new("repair_run", run_id.to_string())
            .map_err(|_| RepairError::Persistence)?,
        run_id,
        None,
        None,
        None,
        Some("repair_run_created".to_string()),
        AuditResult::Succeeded,
        None,
        None,
        None,
        Vec::new(),
        serde_json::json!({ "finding_id": finding_id }),
        "audit.v1",
        now,
    )
    .map_err(|_| RepairError::Persistence)
}

fn validate_commit_inputs(
    run: &RepairRun,
    step: &RepairStep,
    entry: &RepairLedgerEntry,
    expected_run_version: i64,
    expected_fence_version: i64,
    lease_owner: &str,
    lease_token: &str,
) -> Result<(), RepairError> {
    let valid_identity = run.id() == step.run_id()
        && run.finding_id() == step.finding_id()
        && run.tenant_id() == run.command().tenant_id
        && run.command().integrity_finding_id == run.finding_id()
        && entry.tenant_id() == run.tenant_id()
        && entry.repair_run_id() == run.id()
        && entry.repair_step_id() == step.id()
        && entry.finding_id() == run.finding_id()
        && entry.rule_id().trim() != ""
        && entry.resource_type() == run.command().target.resource_type
        && entry.resource_id() == run.command().target.resource_id
        && entry.repair_type() == run.command().repair_type
        && entry.repair_version() == run.command().repair_version;
    if !valid_identity || expected_run_version < 0 || expected_fence_version <= 0 {
        return Err(RepairError::Conflict);
    }
    if run.version()
        != expected_run_version
            .checked_add(1)
            .ok_or(RepairError::Conflict)?
    {
        return Err(RepairError::Conflict);
    }
    if step.fence_version() != expected_fence_version
        || lease_owner.trim().is_empty()
        || lease_token.trim().is_empty()
    {
        return Err(RepairError::LeaseLost);
    }
    Ok(())
}

fn parse_text_artifact_state(value: &str) -> TextArtifactIntegrityState {
    match value {
        "present" => TextArtifactIntegrityState::Present,
        "missing" => TextArtifactIntegrityState::Missing,
        _ => TextArtifactIntegrityState::Unknown,
    }
}

#[derive(Debug, sqlx::FromRow)]
#[allow(clippy::struct_excessive_bools)]
struct ProcessingJobRow {
    id: Uuid,
    tenant_id: Uuid,
    status: String,
    job_attempt_count: i32,
    current_step: String,
    content_revision: i64,
    candidate_content_revision: Option<i64>,
    has_candidate: bool,
    has_review: bool,
    review_decision: Option<String>,
    has_active_ai_task: bool,
    has_succeeded_ai_without_candidate: bool,
    terminal_has_lease: bool,
    text_artifact_state: String,
}

#[async_trait]
impl ProcessingIntegrityQuery for PostgresGovernanceStore {
    async fn snapshots(
        &self,
        scope: &IntegrityScanScope,
    ) -> Result<Vec<ProcessingIntegritySnapshot>, IntegrityError> {
        let resource_id = scope
            .resource_id
            .as_deref()
            .map(|id| Uuid::parse_str(id).map_err(|_| IntegrityError::InvalidFinding))
            .transpose()?;
        let rows = sqlx::query_as::<_, ProcessingJobRow>(
            "SELECT j.id, j.tenant_id, j.status, j.attempt_count AS job_attempt_count, j.current_step, j.content_revision, (SELECT CASE WHEN c.payload->>'content_revision' ~ '^[0-9]+$' THEN (c.payload->>'content_revision')::bigint ELSE NULL END FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id LIMIT 1) AS candidate_content_revision, EXISTS (SELECT 1 FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id) AS has_candidate, EXISTS (SELECT 1 FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=j.tenant_id AND c.job_id=j.id) AS has_review, (SELECT r.decision FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=j.tenant_id AND c.job_id=j.id LIMIT 1) AS review_decision, EXISTS (SELECT 1 FROM document_ai_tasks a WHERE a.tenant_id=j.tenant_id AND a.job_id=j.id AND a.status IN ('queued','running','retry_scheduled')) AS has_active_ai_task, EXISTS (SELECT 1 FROM document_ai_tasks a WHERE a.tenant_id=j.tenant_id AND a.job_id=j.id AND a.status='succeeded' AND NOT EXISTS (SELECT 1 FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id)) AS has_succeeded_ai_without_candidate, (j.lease_owner IS NOT NULL OR j.lease_token IS NOT NULL) AS terminal_has_lease, CASE WHEN EXISTS (SELECT 1 FROM document_processing_steps s WHERE s.tenant_id=j.tenant_id AND s.job_id=j.id AND s.step_kind='extract_text' AND s.status='succeeded') THEN CASE WHEN EXISTS (SELECT 1 FROM document_processing_steps s WHERE s.tenant_id=j.tenant_id AND s.job_id=j.id AND s.step_kind='extract_text' AND s.status='succeeded' AND s.checkpoint_json->>'text_artifact_reference' IS NOT NULL AND s.checkpoint_json->>'text_artifact_reference' <> '') THEN 'present' ELSE 'missing' END ELSE 'unknown' END AS text_artifact_state FROM document_processing_jobs j WHERE ($1::uuid IS NULL OR j.tenant_id=$1) AND ($2::uuid IS NULL OR j.id=$2)",
        )
        .bind(scope.tenant_id)
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        let mut snapshots = Vec::with_capacity(rows.len());
        for row in rows {
            let steps = sqlx::query_as::<_, (String, String, i32)>(
                "SELECT step_kind, status, attempt_number FROM document_processing_steps WHERE tenant_id=$1 AND job_id=$2 ORDER BY step_kind, attempt_number",
            )
            .bind(row.tenant_id)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| IntegrityError::DependencyUnavailable)?
            .into_iter()
            .map(|(step_kind, status, attempt_number)| ProcessingStepIntegritySnapshot {
                step_kind,
                status,
                attempt_number: i64::from(attempt_number),
            })
            .collect();
            snapshots.push(ProcessingIntegritySnapshot {
                tenant_id: row.tenant_id,
                job_id: row.id,
                job_status: row.status,
                job_attempt_count: i64::from(row.job_attempt_count),
                current_step: row.current_step,
                content_revision: row.content_revision,
                candidate_content_revision: row.candidate_content_revision,
                has_candidate: row.has_candidate,
                has_review: row.has_review,
                review_decision: row.review_decision,
                has_active_ai_task: row.has_active_ai_task,
                has_succeeded_ai_without_candidate: row.has_succeeded_ai_without_candidate,
                terminal_has_lease: row.terminal_has_lease,
                steps,
                text_artifact_state: parse_text_artifact_state(&row.text_artifact_state),
            });
        }
        Ok(snapshots)
    }

    async fn snapshot(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingIntegritySnapshot>, IntegrityError> {
        Ok(self
            .snapshots(&IntegrityScanScope {
                tenant_id: Some(tenant_id),
                resource_type: Some("processing_job".to_string()),
                resource_id: Some(job_id.to_string()),
            })
            .await?
            .into_iter()
            .next())
    }
}

#[async_trait]
impl IntegrityPersistencePort for PostgresGovernanceStore {
    async fn record_scan_run(&self, run: &IntegrityScanRun) -> Result<(), IntegrityError> {
        sqlx::query("INSERT INTO data_integrity_scan_runs (id, tenant_id, scope, status, started_at, finished_at, rule_count, finding_count, failure_code, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status, finished_at=EXCLUDED.finished_at, finding_count=EXCLUDED.finding_count, failure_code=EXCLUDED.failure_code")
            .bind(run.id)
            .bind(run.tenant_id)
            .bind(serde_json::to_value(&run.scope).map_err(|_| IntegrityError::Persistence)?)
            .bind(format!("{:?}", run.status).to_lowercase())
            .bind(run.started_at)
            .bind(run.finished_at)
            .bind(i32::try_from(run.rule_count).map_err(|_| IntegrityError::Persistence)?)
            .bind(i64::try_from(run.finding_count).map_err(|_| IntegrityError::Persistence)?)
            .bind(&run.failure_code)
            .bind(run.created_by)
            .execute(&self.pool)
            .await
            .map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn upsert_finding(&self, finding: &IntegrityFinding) -> Result<(), IntegrityError> {
        sqlx::query("INSERT INTO data_integrity_findings (id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22) ON CONFLICT (tenant_id,rule_id,rule_version,resource_type,resource_id,fingerprint) DO UPDATE SET last_detected_at=EXCLUDED.last_detected_at, occurrence_count=data_integrity_findings.occurrence_count + 1, detected_state=EXCLUDED.detected_state, expected_state=EXCLUDED.expected_state, status=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN 'open' ELSE EXCLUDED.status END, resolved_at=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN NULL ELSE data_integrity_findings.resolved_at END, resolution_reason=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN NULL ELSE data_integrity_findings.resolution_reason END, reopened_at=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN EXCLUDED.last_detected_at ELSE data_integrity_findings.reopened_at END, reopen_count=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN data_integrity_findings.reopen_count + 1 ELSE data_integrity_findings.reopen_count END, previous_resolution=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN COALESCE(data_integrity_findings.resolution_reason,data_integrity_findings.status) ELSE data_integrity_findings.previous_resolution END, version=data_integrity_findings.version + 1, updated_at=NOW()")
            .bind(finding.id())
            .bind(finding.tenant_id())
            .bind(finding.rule_id())
            .bind(i32::try_from(finding.rule_version()).map_err(|_| IntegrityError::Persistence)?)
            .bind(finding.bounded_context())
            .bind(finding.resource_type())
            .bind(finding.resource_id())
            .bind(format!("{:?}", finding.severity()).to_lowercase())
            .bind(finding.fingerprint())
            .bind(finding.detected_state())
            .bind(finding.expected_state())
            .bind(finding_status_name(finding.status()))
            .bind(finding.repairability())
            .bind(finding.first_detected_at())
            .bind(finding.last_detected_at())
            .bind(i64::try_from(finding.occurrence_count()).map_err(|_| IntegrityError::Persistence)?)
            .bind(finding.resolved_at())
            .bind(finding.resolution_reason())
            .bind(finding.reopened_at())
            .bind(i64::try_from(finding.reopen_count()).map_err(|_| IntegrityError::Persistence)?)
            .bind(finding.previous_resolution())
            .bind(finding.version())
            .execute(&self.pool)
            .await
            .map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, IntegrityError> {
        let row = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version FROM data_integrity_findings WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| IntegrityError::Persistence)?;
        row.map(FindingRow::into_domain).transpose()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FindingRow {
    id: Uuid,
    tenant_id: Uuid,
    rule_id: String,
    rule_version: i32,
    bounded_context: String,
    resource_type: String,
    resource_id: String,
    severity: String,
    fingerprint: String,
    detected_state: serde_json::Value,
    expected_state: serde_json::Value,
    status: String,
    repairability: String,
    first_detected_at: DateTime<Utc>,
    last_detected_at: DateTime<Utc>,
    occurrence_count: i64,
    resolved_at: Option<DateTime<Utc>>,
    resolution_reason: Option<String>,
    reopened_at: Option<DateTime<Utc>>,
    reopen_count: i64,
    previous_resolution: Option<String>,
    version: i64,
}

impl FindingRow {
    fn into_domain(self) -> Result<IntegrityFinding, IntegrityError> {
        IntegrityFinding::rehydrate(
            self.id,
            self.tenant_id,
            self.rule_id,
            u32::try_from(self.rule_version).map_err(|_| IntegrityError::Persistence)?,
            self.bounded_context,
            self.resource_type,
            self.resource_id,
            parse_severity(&self.severity)?,
            self.fingerprint,
            self.detected_state,
            self.expected_state,
            parse_finding_status(&self.status)?,
            self.repairability,
            self.first_detected_at,
            self.last_detected_at,
            u64::try_from(self.occurrence_count).map_err(|_| IntegrityError::Persistence)?,
            self.resolved_at,
            self.resolution_reason,
            self.reopened_at,
            u64::try_from(self.reopen_count).map_err(|_| IntegrityError::Persistence)?,
            self.previous_resolution,
            self.version,
        )
    }
}

fn parse_severity(value: &str) -> Result<IntegritySeverity, IntegrityError> {
    match value {
        "info" => Ok(IntegritySeverity::Info),
        "warning" => Ok(IntegritySeverity::Warning),
        "error" => Ok(IntegritySeverity::Error),
        "critical" => Ok(IntegritySeverity::Critical),
        _ => Err(IntegrityError::Persistence),
    }
}

fn parse_finding_status(value: &str) -> Result<FindingStatus, IntegrityError> {
    match value {
        "open" => Ok(FindingStatus::Open),
        "repair_planned" => Ok(FindingStatus::RepairPlanned),
        "repairing" => Ok(FindingStatus::Repairing),
        "repaired" => Ok(FindingStatus::Repaired),
        "ignored" => Ok(FindingStatus::Ignored),
        "false_positive" => Ok(FindingStatus::FalsePositive),
        "stale" => Ok(FindingStatus::Stale),
        "needs_manual_review" => Ok(FindingStatus::NeedsManualReview),
        _ => Err(IntegrityError::Persistence),
    }
}

fn parse_scan_run_status(value: &str) -> Result<ScanRunStatus, IntegrityError> {
    match value {
        "queued" => Ok(ScanRunStatus::Queued),
        "running" => Ok(ScanRunStatus::Running),
        "succeeded" => Ok(ScanRunStatus::Succeeded),
        "failed" => Ok(ScanRunStatus::Failed),
        "cancelled" => Ok(ScanRunStatus::Cancelled),
        _ => Err(IntegrityError::Persistence),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ScanRunRow {
    id: Uuid,
    tenant_id: Option<Uuid>,
    scope: serde_json::Value,
    status: String,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    rule_count: i32,
    finding_count: i64,
    failure_code: Option<String>,
    created_by: Uuid,
}

impl ScanRunRow {
    fn into_domain(self) -> Result<IntegrityScanRun, IntegrityError> {
        let scope = serde_json::from_value(self.scope).map_err(|_| IntegrityError::Persistence)?;
        Ok(IntegrityScanRun {
            id: self.id,
            tenant_id: self.tenant_id,
            scope,
            status: parse_scan_run_status(&self.status)?,
            started_at: self.started_at,
            finished_at: self.finished_at,
            rule_count: u32::try_from(self.rule_count).map_err(|_| IntegrityError::Persistence)?,
            finding_count: u64::try_from(self.finding_count)
                .map_err(|_| IntegrityError::Persistence)?,
            failure_code: self.failure_code,
            created_by: self.created_by,
        })
    }
}

#[async_trait]
impl IntegrityQueryPort for PostgresGovernanceStore {
    async fn count_unresolved(&self, tenant_id: Uuid) -> Result<u64, IntegrityError> {
        if tenant_id.is_nil() {
            return Err(IntegrityError::InvalidFinding);
        }
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM data_integrity_findings WHERE tenant_id=$1 AND status NOT IN ('repaired','ignored','false_positive')",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        u64::try_from(count).map_err(|_| IntegrityError::Persistence)
    }
    async fn get_scan_run(
        &self,
        tenant_id: Option<Uuid>,
        id: Uuid,
    ) -> Result<Option<IntegrityScanRun>, IntegrityError> {
        let row = sqlx::query_as::<_, ScanRunRow>(
            "SELECT id,tenant_id,scope,status,started_at,finished_at,rule_count,finding_count,failure_code,created_by FROM data_integrity_scan_runs WHERE id=$1 AND ($2::uuid IS NULL OR tenant_id=$2)",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        row.map(ScanRunRow::into_domain).transpose()
    }

    async fn list_scan_runs(
        &self,
        tenant_id: Option<Uuid>,
        limit: u16,
    ) -> Result<Vec<IntegrityScanRun>, IntegrityError> {
        let rows = sqlx::query_as::<_, ScanRunRow>(
            "SELECT id,tenant_id,scope,status,started_at,finished_at,rule_count,finding_count,failure_code,created_by FROM data_integrity_scan_runs WHERE ($1::uuid IS NULL OR tenant_id=$1) ORDER BY created_at DESC,id DESC LIMIT $2",
        )
        .bind(tenant_id)
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        rows.into_iter().map(ScanRunRow::into_domain).collect()
    }

    async fn list_findings(
        &self,
        tenant_id: Uuid,
        status: Option<FindingStatus>,
        limit: u16,
    ) -> Result<Vec<IntegrityFinding>, IntegrityError> {
        if tenant_id.is_nil() {
            return Err(IntegrityError::InvalidFinding);
        }
        let rows = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version FROM data_integrity_findings WHERE tenant_id=$1 AND ($2::text IS NULL OR status=$2) ORDER BY last_detected_at DESC,id DESC LIMIT $3",
        )
        .bind(tenant_id)
        .bind(status.map(|value| format!("{value:?}").to_lowercase()))
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        rows.into_iter().map(FindingRow::into_domain).collect()
    }
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl RepairPersistencePort for PostgresGovernanceStore {
    async fn create_repair_execution(
        &self,
        command: CreateRepairExecution,
    ) -> Result<CreateRepairResult, RepairError> {
        let CreateRepairExecution {
            run,
            step,
            expected_finding_version,
        } = command;
        run.command().validate()?;
        if run.id() != step.run_id()
            || run.finding_id() != step.finding_id()
            || run.command().integrity_finding_id != run.finding_id()
            || run.command().tenant_id != run.tenant_id()
            || step.fence_version() < 0
            || expected_finding_version < 0
        {
            return Err(RepairError::Conflict);
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let finding = sqlx::query_as::<_, (Uuid, String, i64)>(
            "SELECT tenant_id,status,version FROM data_integrity_findings WHERE id=$1 FOR UPDATE",
        )
        .bind(run.finding_id())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| RepairError::Persistence)?
        .ok_or(RepairError::Conflict)?;
        if finding.0 != run.tenant_id() {
            return Err(RepairError::Conflict);
        }
        let existing = sqlx::query_as::<_, RepairRunRow>(
            "SELECT id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,command,created_at,updated_at,version FROM data_repair_runs WHERE tenant_id=$1 AND idempotency_key=$2",
        )
        .bind(run.tenant_id())
        .bind(&run.command().idempotency_key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if let Some(existing) = existing {
            let existing_run = existing.into_domain()?;
            if existing_run.command() != run.command() {
                return Err(RepairError::IdempotencyConflict);
            }
            let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, String, i32, Option<serde_json::Value>, Option<String>, Option<String>, i64, Option<DateTime<Utc>>, DateTime<Utc>)>(
                "SELECT id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at FROM data_repair_steps WHERE repair_run_id=$1",
            )
            .bind(existing_run.id())
            .fetch_one(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
            let existing_step = RepairStep::rehydrate(
                row.0,
                row.1,
                row.2,
                parse_step_status(&row.3)?,
                u32::try_from(row.4).map_err(|_| RepairError::Persistence)?,
                row.5,
                row.6,
                row.7,
                row.8,
                row.9,
                row.10,
            )?;
            transaction
                .commit()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Ok(CreateRepairResult {
                run: existing_run,
                step: existing_step,
                replayed: true,
            });
        }
        if finding.1 != "open" || finding.2 != expected_finding_version {
            return Err(RepairError::Conflict);
        }
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,worker_id,lease_token,fence_version,lease_expires_at,attempt_count,checkpoint,next_attempt_at,idempotency_key,command,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,NULL,NULL,0,NULL,0,NULL,NOW(),$8,$9,$10,$11,$11)")
            .bind(run.id())
            .bind(run.tenant_id())
            .bind(run.finding_id())
            .bind(repair_run_status_name(run.status()))
            .bind(run.created_by())
            .bind(run.approved_by())
            .bind(run.approval_note())
            .bind(&run.command().idempotency_key)
            .bind(serde_json::to_value(run.command()).map_err(|_| RepairError::Persistence)?)
            .bind(run.version())
            .bind(run.created_at())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(step.id())
            .bind(run.tenant_id())
            .bind(step.run_id())
            .bind(step.finding_id())
            .bind(data_repair::repair_step_status_name(step.status()))
            .bind(i32::try_from(step.attempt_count()).map_err(|_| RepairError::Persistence)?)
            .bind(step.checkpoint())
            .bind(step.lease_owner())
            .bind(step.lease_token())
            .bind(step.fence_version())
            .bind(step.lease_expires_at())
            .bind(step.next_attempt_at())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repair_planned',version=version+1,updated_at=NOW() WHERE id=$1 AND tenant_id=$2 AND status='open' AND version=$3")
            .bind(run.finding_id())
            .bind(run.tenant_id())
            .bind(expected_finding_version)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let audit_event = repair_created_audit(
            run.tenant_id(),
            run.created_by(),
            run.id(),
            run.finding_id(),
            Utc::now(),
        )?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &audit_event)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO outbox_events (event_id,event_type,tenant_id,aggregate_id,aggregate_type,payload,schema_version,occurred_at) VALUES ($1,'runtime.governance.repair-created.v1',$2,$3,'repair_run',$4,'v1',$5)")
            .bind(Uuid::now_v7())
            .bind(run.tenant_id().to_string())
            .bind(run.id().to_string())
            .bind(serde_json::json!({ "repair_run_id": run.id(), "finding_id": run.finding_id() }))
            .bind(Utc::now())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(CreateRepairResult {
            run,
            step,
            replayed: false,
        })
    }

    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
            .bind(entry.id()).bind(entry.tenant_id()).bind(entry.repair_run_id()).bind(entry.repair_step_id()).bind(entry.finding_id())
            .bind(entry.rule_id()).bind(entry.repair_type()).bind(i32::try_from(entry.repair_version()).map_err(|_| RepairError::Persistence)?)
            .bind(entry.actor_type()).bind(entry.actor_id()).bind(entry.reason()).bind(entry.resource_type()).bind(entry.resource_id())
            .bind(entry.before_hash()).bind(entry.after_hash()).bind(entry.before_snapshot()).bind(entry.after_snapshot())
            .bind(i32::try_from(entry.rows_affected()).map_err(|_| RepairError::Persistence)?).bind(format!("{:?}", entry.result()).to_lowercase())
            .bind(entry.failure_code()).bind(entry.trace_id()).bind(entry.started_at()).bind(entry.finished_at())
            .bind(entry.previous_hash()).bind(entry.record_hash())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, RepairError> {
        let row = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version FROM data_integrity_findings WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?;
        row.map(FindingRow::into_domain)
            .transpose()
            .map_err(|_| RepairError::Persistence)
    }

    async fn mark_finding_repaired(
        &self,
        finding_id: Uuid,
        reason: &str,
    ) -> Result<(), RepairError> {
        let result = sqlx::query("UPDATE data_integrity_findings SET status='repaired',resolved_at=NOW(),resolution_reason=$1,version=version+1,updated_at=NOW() WHERE id=$2 AND status IN ('open','repair_planned','repairing')")
            .bind(reason)
            .bind(finding_id)
            .execute(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(RepairError::Conflict)
        }
    }

    async fn mark_finding_needs_manual_review(
        &self,
        finding_id: Uuid,
        reason: &str,
    ) -> Result<(), RepairError> {
        let result = sqlx::query("UPDATE data_integrity_findings SET status='needs_manual_review',resolution_reason=$1,version=version+1,updated_at=NOW() WHERE id=$2 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(reason).bind(finding_id).execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(RepairError::Conflict)
        }
    }

    async fn approve_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        approver: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
        note: String,
    ) -> Result<RepairRun, RepairError> {
        if tenant_id.is_nil() || run_id.is_nil() || approver.is_nil() || expected_version < 0 {
            return Err(RepairError::Conflict);
        }
        if approver.is_nil() {
            return Err(RepairError::ApprovalSeparation);
        }
        let expected = repair_run_status_name(expected_status);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status='approved',approved_by=$1,approval_note=$2,version=version+1,updated_at=NOW() WHERE id=$3 AND tenant_id=$4 AND version=$5 AND status=$6 AND requested_by<>$1")
            .bind(approver).bind(&note).bind(run_id).bind(tenant_id).bind(expected_version).bind(expected)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='approved',updated_at=NOW() WHERE repair_run_id=$1 AND tenant_id=$2 AND status='awaiting_approval'")
            .bind(run_id).bind(tenant_id).execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        self.load_run(run_id).await?.ok_or(RepairError::Persistence)
    }

    async fn execute_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError> {
        if tenant_id.is_nil() || run_id.is_nil() || expected_version < 0 {
            return Err(RepairError::Conflict);
        }
        let expected = repair_run_status_name(expected_status);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status='queued',version=version+1,updated_at=NOW() WHERE id=$1 AND tenant_id=$2 AND version=$3 AND status=$4 AND approved_by IS NOT NULL")
            .bind(run_id)
            .bind(tenant_id)
            .bind(expected_version)
            .bind(expected)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='queued',next_attempt_at=NOW(),updated_at=NOW() WHERE repair_run_id=$1 AND tenant_id=$2 AND status='approved'")
            .bind(run_id)
            .bind(tenant_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        self.load_run(run_id).await?.ok_or(RepairError::Persistence)
    }

    async fn cancel_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError> {
        if tenant_id.is_nil() || run_id.is_nil() || expected_version < 0 {
            return Err(RepairError::Conflict);
        }
        let expected = repair_run_status_name(expected_status);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status='cancelled',version=version+1,updated_at=NOW() WHERE id=$1 AND tenant_id=$2 AND version=$3 AND status=$4 AND status NOT IN ('succeeded','cancelled')")
            .bind(run_id).bind(tenant_id).bind(expected_version).bind(expected)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='cancelled',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=NOW() WHERE repair_run_id=$1 AND tenant_id=$2 AND status NOT IN ('succeeded','cancelled')")
            .bind(run_id).bind(tenant_id).execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        self.load_run(run_id).await?.ok_or(RepairError::Persistence)
    }

    async fn resume_repair(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        expected_version: i64,
        expected_status: RepairRunStatus,
    ) -> Result<RepairRun, RepairError> {
        if tenant_id.is_nil() || run_id.is_nil() || expected_version < 0 {
            return Err(RepairError::Conflict);
        }
        let expected = repair_run_status_name(expected_status);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status=CASE WHEN approved_by IS NULL THEN 'awaiting_approval' ELSE 'queued' END,version=version+1,updated_at=NOW() WHERE id=$1 AND tenant_id=$2 AND version=$3 AND status=$4 AND status IN ('cancelled','failed','needs_manual_review')")
            .bind(run_id).bind(tenant_id).bind(expected_version).bind(expected)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status=CASE WHEN (SELECT approved_by FROM data_repair_runs WHERE id=$1) IS NULL THEN 'awaiting_approval' ELSE 'queued' END,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NOW(),updated_at=NOW() WHERE repair_run_id=$1 AND tenant_id=$2 AND status IN ('cancelled','failed','needs_manual_review')")
            .bind(run_id).bind(tenant_id).execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        self.load_run(run_id).await?.ok_or(RepairError::Persistence)
    }

    async fn commit_success(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_run_version: i64,
        expected_fence_version: i64,
        lease_owner: &str,
        lease_token: &str,
    ) -> Result<(), RepairError> {
        validate_commit_inputs(
            run,
            step,
            entry,
            expected_run_version,
            expected_fence_version,
            lease_owner,
            lease_token,
        )?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
            .bind(entry.id()).bind(entry.tenant_id()).bind(entry.repair_run_id()).bind(entry.repair_step_id()).bind(entry.finding_id())
            .bind(entry.rule_id()).bind(entry.repair_type()).bind(i32::try_from(entry.repair_version()).map_err(|_| RepairError::Persistence)?)
            .bind(entry.actor_type()).bind(entry.actor_id()).bind(entry.reason()).bind(entry.resource_type()).bind(entry.resource_id())
            .bind(entry.before_hash()).bind(entry.after_hash()).bind(entry.before_snapshot()).bind(entry.after_snapshot())
            .bind(i32::try_from(entry.rows_affected()).map_err(|_| RepairError::Persistence)?).bind(format!("{:?}", entry.result()).to_lowercase())
            .bind(entry.failure_code()).bind(entry.trace_id()).bind(entry.started_at()).bind(entry.finished_at())
            .bind(entry.previous_hash()).bind(entry.record_hash())
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repaired',resolved_at=NOW(),resolution_reason=$1,version=version+1,updated_at=NOW() WHERE id=$2 AND tenant_id=$3 AND resource_type=$4 AND resource_id=$5 AND status IN ('open','repair_planned','repairing')")
            .bind("repair_succeeded")
            .bind(entry.finding_id())
            .bind(run.tenant_id())
            .bind(entry.resource_type())
            .bind(entry.resource_id())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step_update = sqlx::query("UPDATE data_repair_steps SET status=$1,attempt_count=$2,checkpoint=$3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=$4,next_attempt_at=$5,updated_at=NOW() WHERE id=$6 AND tenant_id=$7 AND repair_run_id=$8 AND finding_id=$9 AND fence_version=$10 AND status='running' AND lease_owner=$11 AND lease_token=$12 AND lease_expires_at > NOW() AND EXISTS (SELECT 1 FROM data_repair_runs r WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running')")
            .bind(data_repair::repair_step_status_name(step.status()))
            .bind(i32::try_from(step.attempt_count()).map_err(|_| RepairError::Persistence)?)
            .bind(step.checkpoint()).bind(step.fence_version()).bind(step.next_attempt_at())
            .bind(step.id()).bind(run.tenant_id()).bind(run.id()).bind(run.finding_id())
            .bind(expected_fence_version).bind(lease_owner).bind(lease_token)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::LeaseLost);
        }
        let run_update = sqlx::query("UPDATE data_repair_runs SET status=$1,approved_by=$2,approval_note=$3,updated_at=NOW(),version=$4 WHERE id=$5 AND tenant_id=$6 AND version=$7 AND status='running'")
            .bind(repair_run_status_name(run.status()))
            .bind(run.approved_by()).bind(run.approval_note()).bind(run.version()).bind(run.id()).bind(run.tenant_id())
            .bind(expected_run_version)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if run_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)
    }

    async fn commit_failure(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_run_version: i64,
        expected_fence_version: i64,
        lease_owner: &str,
        lease_token: &str,
    ) -> Result<(), RepairError> {
        validate_commit_inputs(
            run,
            step,
            entry,
            expected_run_version,
            expected_fence_version,
            lease_owner,
            lease_token,
        )?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let finding_status = match run.status() {
            RepairRunStatus::Queued => "repair_planned",
            RepairRunStatus::Cancelled => "open",
            RepairRunStatus::Failed | RepairRunStatus::NeedsManualReview => "needs_manual_review",
            _ => return Err(RepairError::InvalidTransition),
        };
        let finished_at =
            (!matches!(run.status(), RepairRunStatus::Queued)).then_some(entry.finished_at());
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
            .bind(entry.id()).bind(entry.tenant_id()).bind(entry.repair_run_id()).bind(entry.repair_step_id()).bind(entry.finding_id())
            .bind(entry.rule_id()).bind(entry.repair_type()).bind(i32::try_from(entry.repair_version()).map_err(|_| RepairError::Persistence)?)
            .bind(entry.actor_type()).bind(entry.actor_id()).bind(entry.reason()).bind(entry.resource_type()).bind(entry.resource_id())
            .bind(entry.before_hash()).bind(entry.after_hash()).bind(entry.before_snapshot()).bind(entry.after_snapshot())
            .bind(i32::try_from(entry.rows_affected()).map_err(|_| RepairError::Persistence)?).bind(format!("{:?}", entry.result()).to_lowercase())
            .bind(entry.failure_code()).bind(entry.trace_id()).bind(entry.started_at()).bind(entry.finished_at())
            .bind(entry.previous_hash()).bind(entry.record_hash())
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status=$1,resolution_reason=$2,version=version+1,updated_at=NOW() WHERE id=$3 AND tenant_id=$4 AND resource_type=$5 AND resource_id=$6 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(finding_status).bind(entry.failure_code().unwrap_or("repair_failed"))
            .bind(entry.finding_id())
            .bind(run.tenant_id())
            .bind(entry.resource_type())
            .bind(entry.resource_id())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step_update = sqlx::query("UPDATE data_repair_steps SET status=$1,attempt_count=$2,checkpoint=$3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=$4,next_attempt_at=$5,failure_code=$6,last_error_category=$7,finished_at=$8,updated_at=NOW() WHERE id=$9 AND tenant_id=$10 AND repair_run_id=$11 AND finding_id=$12 AND fence_version=$13 AND status='running' AND lease_owner=$14 AND lease_token=$15 AND lease_expires_at > NOW() AND EXISTS (SELECT 1 FROM data_repair_runs r WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running')")
            .bind(data_repair::repair_step_status_name(step.status()))
            .bind(i32::try_from(step.attempt_count()).map_err(|_| RepairError::Persistence)?)
            .bind(step.checkpoint()).bind(step.fence_version()).bind(step.next_attempt_at())
            .bind(entry.failure_code()).bind(entry.failure_code()).bind(finished_at)
            .bind(step.id()).bind(run.tenant_id()).bind(run.id()).bind(run.finding_id())
            .bind(expected_fence_version).bind(lease_owner).bind(lease_token)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::LeaseLost);
        }
        let run_update = sqlx::query("UPDATE data_repair_runs SET status=$1,failure_code=$2,last_error_category=$3,finished_at=$4,updated_at=NOW(),version=$5 WHERE id=$6 AND tenant_id=$7 AND version=$8 AND status='running'")
            .bind(repair_run_status_name(run.status())).bind(entry.failure_code()).bind(entry.failure_code()).bind(finished_at)
            .bind(run.version()).bind(run.id()).bind(run.tenant_id())
            .bind(expected_run_version)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if run_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)
    }

    async fn classify_claimed_failure(
        &self,
        step_id: Uuid,
        run_id: Uuid,
        lease_owner: &str,
        lease_token: &str,
        expected_fence_version: i64,
        disposition: RepairFailureDisposition,
        failure_code: &str,
        next_attempt_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<(), RepairError> {
        if step_id.is_nil()
            || run_id.is_nil()
            || lease_owner.trim().is_empty()
            || lease_token.trim().is_empty()
            || expected_fence_version <= 0
            || failure_code.trim().is_empty()
            || failure_code.len() > 128
        {
            return Err(RepairError::Conflict);
        }
        if matches!(disposition, RepairFailureDisposition::LeaseLost) {
            return Err(RepairError::LeaseLost);
        }
        {
            let mut transaction = self
                .pool
                .begin()
                .await
                .map_err(|_| RepairError::Persistence)?;
            let row = sqlx::query_as::<_, (i32, i32, Uuid, Uuid)>(
                "SELECT attempt_count,max_attempts,tenant_id,finding_id FROM data_repair_steps WHERE id=$1 AND repair_run_id=$2 AND status='running' AND lease_owner=$3 AND lease_token=$4 AND fence_version=$5 AND lease_expires_at > $6 FOR UPDATE",
            )
            .bind(step_id)
            .bind(run_id)
            .bind(lease_owner)
            .bind(lease_token)
            .bind(expected_fence_version)
            .bind(now)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?
            .ok_or(RepairError::LeaseLost)?;
            let retry_exhausted =
                matches!(disposition, RepairFailureDisposition::Retry { .. }) && row.0 >= row.1;
            let effective_code = if retry_exhausted {
                "retry_exhausted"
            } else {
                failure_code
            };
            let (step_status, run_status, finding_status, finished_at) = if retry_exhausted {
                ("failed", "failed", "needs_manual_review", Some(now))
            } else {
                match disposition {
                    RepairFailureDisposition::Retry { .. } => {
                        ("queued", "queued", "repair_planned", None)
                    }
                    RepairFailureDisposition::Permanent => {
                        ("failed", "failed", "needs_manual_review", Some(now))
                    }
                    RepairFailureDisposition::NeedsManualReview => (
                        "needs_manual_review",
                        "needs_manual_review",
                        "needs_manual_review",
                        Some(now),
                    ),
                    RepairFailureDisposition::Cancelled => {
                        ("cancelled", "cancelled", "open", Some(now))
                    }
                    RepairFailureDisposition::LeaseLost => unreachable!(),
                }
            };
            let step_result = sqlx::query("UPDATE data_repair_steps SET status=$1,next_attempt_at=$2,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,failure_code=$3,last_error_category=$3,finished_at=$4,updated_at=$5 WHERE id=$6 AND repair_run_id=$7 AND tenant_id=$8 AND status='running' AND lease_owner=$9 AND lease_token=$10 AND fence_version=$11 AND lease_expires_at > $5")
                .bind(step_status)
                .bind(next_attempt_at.unwrap_or(now))
                .bind(effective_code)
                .bind(finished_at)
                .bind(now)
                .bind(step_id)
                .bind(run_id)
                .bind(row.2)
                .bind(lease_owner)
                .bind(lease_token)
                .bind(expected_fence_version)
                .execute(&mut *transaction)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if step_result.rows_affected() != 1 {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Err(RepairError::LeaseLost);
            }
            let run_result = sqlx::query("UPDATE data_repair_runs SET status=$1,failure_code=$2,last_error_category=$2,finished_at=$3,version=version+1,updated_at=$4 WHERE id=$5 AND tenant_id=$6 AND status='running'")
                .bind(run_status)
                .bind(effective_code)
                .bind(finished_at)
                .bind(now)
                .bind(run_id)
                .bind(row.2)
                .execute(&mut *transaction)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if run_result.rows_affected() != 1 {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Err(RepairError::Conflict);
            }
            let finding_result = sqlx::query("UPDATE data_integrity_findings SET status=$1,resolution_reason=$2,version=version+1,updated_at=$3 WHERE id=$4 AND tenant_id=$5 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
                .bind(finding_status)
                .bind(effective_code)
                .bind(now)
                .bind(row.3)
                .bind(row.2)
                .execute(&mut *transaction)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if finding_result.rows_affected() != 1 {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Err(RepairError::Conflict);
            }
            transaction
                .commit()
                .await
                .map_err(|_| RepairError::Persistence)
        }
    }
    async fn abort_claimed_repair(
        &self,
        step: &RepairStep,
        worker_id: &str,
        reason: &str,
    ) -> Result<(), RepairError> {
        if worker_id.trim().is_empty() || reason.trim().is_empty() || reason.len() > 128 {
            return Err(RepairError::Conflict);
        }
        let lease_token = step.lease_token().ok_or(RepairError::LeaseLost)?;
        let lease_owner = step.lease_owner().ok_or(RepairError::LeaseLost)?;
        if lease_owner != worker_id {
            return Err(RepairError::LeaseLost);
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let step_update = sqlx::query("UPDATE data_repair_steps SET status='needs_manual_review',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=NOW() WHERE id=$1 AND repair_run_id=$2 AND finding_id=$3 AND status='running' AND lease_owner=$4 AND lease_token=$5 AND fence_version=$6 AND lease_expires_at > NOW()")
            .bind(step.id())
            .bind(step.run_id())
            .bind(step.finding_id())
            .bind(worker_id)
            .bind(lease_token)
            .bind(step.fence_version())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::LeaseLost);
        }
        let run_update = sqlx::query_as::<_, (Uuid, Uuid)>("UPDATE data_repair_runs SET status='needs_manual_review',version=version+1,updated_at=NOW() WHERE id=$1 AND status='running' RETURNING tenant_id,finding_id")
            .bind(step.run_id())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let Some((tenant_id, finding_id)) = run_update else {
            transaction
                .rollback()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        };
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='needs_manual_review',resolution_reason=$1,version=version+1,updated_at=NOW() WHERE id=$2 AND tenant_id=$3 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(reason)
            .bind(finding_id)
            .bind(tenant_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        }
        let audit_event = AuditEvent::new(
            Uuid::now_v7(),
            tenant_id,
            AuditActor {
                actor_type: AuditActorType::RepairJob,
                actor_id: Uuid::now_v7(),
            },
            AuditAction::new("repair.claim-aborted").map_err(|_| RepairError::Persistence)?,
            AuditResource::new("repair_run", step.run_id().to_string())
                .map_err(|_| RepairError::Persistence)?,
            step.run_id(),
            None,
            None,
            None,
            Some(reason.to_string()),
            AuditResult::Failed,
            Some("repair_claim_aborted".to_string()),
            None,
            None,
            Vec::new(),
            serde_json::json!({ "repair_step_id": step.id(), "finding_id": finding_id }),
            "audit.v1",
            Utc::now(),
        )
        .map_err(|_| RepairError::Persistence)?;
        audit_postgres::append_postgres_in_transaction(&mut transaction, &audit_event)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO outbox_events (event_id,event_type,tenant_id,aggregate_id,aggregate_type,payload,schema_version,occurred_at) VALUES ($1,'runtime.governance.repair-claim-aborted.v1',$2,$3,'repair_run',$4,'v1',$5)")
            .bind(Uuid::now_v7())
            .bind(tenant_id.to_string())
            .bind(step.run_id().to_string())
            .bind(serde_json::json!({ "repair_run_id": step.run_id(), "repair_step_id": step.id(), "reason": reason }))
            .bind(Utc::now())
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)
    }

    async fn load_run(&self, id: Uuid) -> Result<Option<RepairRun>, RepairError> {
        let row = sqlx::query_as::<_, RepairRunRow>(
            "SELECT id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,command,created_at,updated_at,version FROM data_repair_runs WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?;
        row.map(RepairRunRow::into_domain).transpose()
    }

    async fn load_run_by_idempotency(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<RepairRun>, RepairError> {
        let row = sqlx::query_as::<_, RepairRunRow>(
            "SELECT id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,command,created_at,updated_at,version FROM data_repair_runs WHERE tenant_id=$1 AND idempotency_key=$2",
        )
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?;
        row.map(RepairRunRow::into_domain).transpose()
    }

    async fn claim_step(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<RepairStep>, RepairError> {
        if worker_id.trim().is_empty() || lease_duration_secs <= 0 {
            return Err(RepairError::LeaseLost);
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        let row = sqlx::query_as::<_, RepairStepRow>(
            "SELECT s.id,s.repair_run_id,s.finding_id,s.attempt_count,s.checkpoint,s.fence_version,s.next_attempt_at FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id AND r.tenant_id=s.tenant_id AND r.finding_id=s.finding_id JOIN data_integrity_findings f ON f.id=s.finding_id AND f.tenant_id=s.tenant_id WHERE r.status IN ('queued','running') AND f.status IN ('open','repair_planned','repairing','needs_manual_review') AND (s.status IN ('queued') OR (s.status='running' AND s.lease_expires_at <= $1)) AND s.next_attempt_at <= $1 ORDER BY s.next_attempt_at,s.created_at,s.id FOR UPDATE OF s SKIP LOCKED LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| RepairError::Persistence)?;
        let Some(row) = row else {
            transaction
                .commit()
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Ok(None);
        };
        let token = Uuid::now_v7().to_string();
        if row.fence_version < 0 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Persistence);
        }
        let fence = row
            .fence_version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        let expires = now + chrono::Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE data_repair_steps SET status='running',attempt_count=attempt_count+1,lease_owner=$1,lease_token=$2,fence_version=$3,lease_expires_at=$4,updated_at=$5 WHERE id=$6 AND fence_version=$7")
            .bind(worker_id)
            .bind(&token)
            .bind(fence)
            .bind(expires)
            .bind(now)
            .bind(row.id)
            .bind(row.fence_version)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if result.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::LeaseLost);
        }
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repairing',version=version+1,updated_at=NOW() WHERE id=$1 AND tenant_id=(SELECT tenant_id FROM data_repair_runs WHERE id=$2) AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(row.finding_id)
            .bind(row.repair_run_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let run_update = sqlx::query(
            "UPDATE data_repair_runs SET status='running',version=version+1,updated_at=NOW() WHERE id=$1 AND tenant_id=(SELECT tenant_id FROM data_repair_steps WHERE repair_run_id=$1 LIMIT 1) AND status IN ('queued','running')",
        )
        .bind(row.repair_run_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if run_update.rows_affected() != 1 {
            if transaction.rollback().await.is_err() {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(Some(RepairStep::rehydrate(
            row.id,
            row.repair_run_id,
            row.finding_id,
            RepairStepStatus::Running,
            u32::try_from(row.attempt_count)
                .map_err(|_| RepairError::Persistence)?
                .checked_add(1)
                .ok_or(RepairError::Persistence)?,
            row.checkpoint,
            Some(worker_id.to_string()),
            Some(token),
            fence,
            Some(expires),
            row.next_attempt_at,
        )?))
    }

    async fn heartbeat_repair_step(
        &self,
        step_id: Uuid,
        lease_owner: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<RepairStep, RepairError> {
        if step_id.is_nil()
            || lease_owner.trim().is_empty()
            || lease_token.trim().is_empty()
            || fence_version < 0
            || lease_duration_secs <= 0
        {
            return Err(RepairError::LeaseLost);
        }
        let expires = now + chrono::Duration::seconds(lease_duration_secs);
        let row = sqlx::query_as::<_, RepairStepRow>(
            "UPDATE data_repair_steps SET lease_expires_at=$1,updated_at=$2 WHERE id=$3 AND status='running' AND lease_owner=$4 AND lease_token=$5 AND fence_version=$6 AND lease_expires_at > $2 AND EXISTS (SELECT 1 FROM data_repair_runs r JOIN data_integrity_findings f ON f.id=r.finding_id AND f.tenant_id=r.tenant_id WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running' AND f.status='repairing') RETURNING id,repair_run_id,finding_id,attempt_count,checkpoint,fence_version,next_attempt_at",
        )
        .bind(expires)
        .bind(now)
        .bind(step_id)
        .bind(lease_owner)
        .bind(lease_token)
        .bind(fence_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?
        .ok_or(RepairError::LeaseLost)?;
        RepairStep::rehydrate(
            row.id,
            row.repair_run_id,
            row.finding_id,
            RepairStepStatus::Running,
            u32::try_from(row.attempt_count).map_err(|_| RepairError::Persistence)?,
            row.checkpoint,
            Some(lease_owner.to_string()),
            Some(lease_token.to_string()),
            row.fence_version,
            Some(expires),
            row.next_attempt_at,
        )
    }

    async fn validate_repair_fence(
        &self,
        step_id: Uuid,
        lease_owner: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), RepairError> {
        let owned = sqlx::query_scalar::<_, i64>(
            "SELECT 1::bigint FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id AND r.tenant_id=s.tenant_id AND r.finding_id=s.finding_id JOIN data_integrity_findings f ON f.id=s.finding_id AND f.tenant_id=s.tenant_id WHERE s.id=$1 AND s.status='running' AND s.lease_owner=$2 AND s.lease_token=$3 AND s.fence_version=$4 AND s.lease_expires_at > $5 AND r.status='running' AND f.status='repairing'",
        )
        .bind(step_id)
        .bind(lease_owner)
        .bind(lease_token)
        .bind(fence_version)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?;
        owned.map_or(Err(RepairError::LeaseLost), |_| Ok(()))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RepairRunRow {
    id: Uuid,
    tenant_id: Uuid,
    finding_id: Uuid,
    status: String,
    requested_by: Uuid,
    approved_by: Option<Uuid>,
    approval_note: Option<String>,
    command: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i64,
}

impl RepairRunRow {
    fn into_domain(self) -> Result<RepairRun, RepairError> {
        let command: RepairCommand =
            serde_json::from_value(self.command).map_err(|_| RepairError::Persistence)?;
        RepairRun::rehydrate(
            self.id,
            self.tenant_id,
            self.finding_id,
            command,
            parse_run_status(&self.status)?,
            self.requested_by,
            self.approved_by,
            self.approval_note,
            self.created_at,
            self.updated_at,
            self.version,
        )
        .map_err(|_| RepairError::Persistence)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RepairStepRow {
    id: Uuid,
    repair_run_id: Uuid,
    finding_id: Uuid,
    attempt_count: i32,
    checkpoint: Option<serde_json::Value>,
    fence_version: i64,
    next_attempt_at: DateTime<Utc>,
}

fn parse_run_status(value: &str) -> Result<RepairRunStatus, RepairError> {
    match value {
        "draft" => Ok(RepairRunStatus::Draft),
        "dry_run_completed" => Ok(RepairRunStatus::DryRunCompleted),
        "awaiting_approval" => Ok(RepairRunStatus::AwaitingApproval),
        "approved" => Ok(RepairRunStatus::Approved),
        "queued" => Ok(RepairRunStatus::Queued),
        "running" => Ok(RepairRunStatus::Running),
        "verifying" => Ok(RepairRunStatus::Verifying),
        "succeeded" => Ok(RepairRunStatus::Succeeded),
        "failed" => Ok(RepairRunStatus::Failed),
        "cancelled" => Ok(RepairRunStatus::Cancelled),
        "needs_manual_review" => Ok(RepairRunStatus::NeedsManualReview),
        _ => Err(RepairError::InvalidStoredEnum),
    }
}

fn parse_step_status(value: &str) -> Result<RepairStepStatus, RepairError> {
    match value {
        "draft" => Ok(RepairStepStatus::Draft),
        "awaiting_approval" => Ok(RepairStepStatus::AwaitingApproval),
        "approved" => Ok(RepairStepStatus::Approved),
        "queued" => Ok(RepairStepStatus::Queued),
        "running" => Ok(RepairStepStatus::Running),
        "verifying" => Ok(RepairStepStatus::Verifying),
        "succeeded" => Ok(RepairStepStatus::Succeeded),
        "failed" => Ok(RepairStepStatus::Failed),
        "cancelled" => Ok(RepairStepStatus::Cancelled),
        "needs_manual_review" => Ok(RepairStepStatus::NeedsManualReview),
        _ => Err(RepairError::InvalidStoredEnum),
    }
}

/// Helper used by scan orchestration to construct a durable Finding without
/// exposing database rows to the domain.
pub fn finding_from_issue(
    descriptor: &IntegrityRuleDescriptor,
    issue: DetectedIntegrityIssue,
    now: DateTime<Utc>,
) -> Result<IntegrityFinding, IntegrityError> {
    IntegrityFinding::from_issue(descriptor, issue, now)
}
