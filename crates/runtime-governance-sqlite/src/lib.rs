//! `SQLite` Runtime Governance adapter. It is local single-process only.

use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use data_integrity::{
    finding_status_name, FindingStatus, IntegrityError, IntegrityFinding, IntegrityPersistencePort,
    IntegrityQueryPort, IntegrityScanRun, IntegrityScanScope, IntegritySeverity,
    ProcessingIntegrityQuery, ProcessingIntegritySnapshot, ProcessingStepIntegritySnapshot,
    ScanRunStatus, TextArtifactIntegrityState,
};
use data_repair::{
    repair_run_status_name, CreateRepairExecution, CreateRepairResult, RepairCommand, RepairError,
    RepairFailureDisposition, RepairLedgerEntry, RepairPersistencePort, RepairRun, RepairRunStatus,
    RepairStep, RepairStepStatus,
};
use sqlx::{pool::PoolConnection, Sqlite, SqlitePool};
use uuid::Uuid;

#[derive(Clone)]
pub struct SqliteGovernanceStore {
    pool: SqlitePool,
}

impl SqliteGovernanceStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    pub async fn insert_test_run(&self, run: &RepairRun) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,idempotency_key,command,version,created_at,updated_at,next_attempt_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?11) ON CONFLICT(id) DO UPDATE SET status=excluded.status,approved_by=excluded.approved_by,approval_note=excluded.approval_note,command=excluded.command,version=excluded.version,updated_at=excluded.updated_at")
            .bind(run.id().to_string()).bind(run.tenant_id().to_string()).bind(run.finding_id().to_string())
            .bind(repair_run_status_name(run.status())).bind(run.created_by().to_string()).bind(run.approved_by().map(|v| v.to_string()))
            .bind(run.approval_note()).bind(&run.command().idempotency_key)
            .bind(serde_json::to_string(run.command()).map_err(|_| RepairError::Persistence)?)
            .bind(run.version()).bind(run.created_at().to_rfc3339())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    pub async fn insert_test_step(&self, step: &RepairStep) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at,created_at,updated_at) SELECT ?1,tenant_id,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?11 FROM data_repair_runs WHERE id=?2 ON CONFLICT(id) DO UPDATE SET status=excluded.status,attempt_count=excluded.attempt_count,checkpoint=excluded.checkpoint,lease_owner=excluded.lease_owner,lease_token=excluded.lease_token,fence_version=excluded.fence_version,lease_expires_at=excluded.lease_expires_at,next_attempt_at=excluded.next_attempt_at,updated_at=excluded.updated_at")
            .bind(step.id().to_string()).bind(step.run_id().to_string()).bind(step.finding_id().to_string())
            .bind(data_repair::repair_step_status_name(step.status())).bind(i64::from(step.attempt_count())).bind(step.checkpoint().map(ToString::to_string))
            .bind(step.lease_owner()).bind(step.lease_token()).bind(step.fence_version()).bind(step.lease_expires_at().map(|v| v.to_rfc3339()))
            .bind(step.next_attempt_at().to_rfc3339()).execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
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
        let result = sqlx::query("UPDATE data_repair_steps SET status=?1,attempt_count=?2,checkpoint=?3,lease_owner=?4,lease_token=?5,fence_version=?6,lease_expires_at=?7,next_attempt_at=?8,updated_at=?9 WHERE id=?10 AND repair_run_id=?11 AND finding_id=?12 AND fence_version=?13 AND lease_owner=?4 AND lease_token=?5 AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now') AND EXISTS (SELECT 1 FROM data_repair_runs r JOIN data_integrity_findings f ON f.id=r.finding_id AND f.tenant_id=r.tenant_id WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running' AND f.status='repairing')")
            .bind(data_repair::repair_step_status_name(step.status()))
            .bind(i64::from(step.attempt_count()))
            .bind(step.checkpoint().map(ToString::to_string))
            .bind(lease_owner)
            .bind(lease_token)
            .bind(step.fence_version())
            .bind(step.lease_expires_at().map(|value| value.to_rfc3339()))
            .bind(step.next_attempt_at().to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(step.id().to_string())
            .bind(step.run_id().to_string())
            .bind(step.finding_id().to_string())
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

async fn begin_immediate(pool: &SqlitePool) -> Result<PoolConnection<Sqlite>, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await?;
    Ok(connection)
}

async fn commit_immediate(connection: &mut PoolConnection<Sqlite>) -> Result<(), sqlx::Error> {
    sqlx::query("COMMIT").execute(&mut **connection).await?;
    Ok(())
}

async fn rollback_immediate(connection: &mut PoolConnection<Sqlite>) -> Result<(), RepairError> {
    sqlx::query("ROLLBACK")
        .execute(&mut **connection)
        .await
        .map(|_| ())
        .map_err(|_| RepairError::Persistence)
}

fn parse_text_artifact_state(value: &str) -> TextArtifactIntegrityState {
    match value {
        "present" => TextArtifactIntegrityState::Present,
        "missing" => TextArtifactIntegrityState::Missing,
        _ => TextArtifactIntegrityState::Unknown,
    }
}

#[derive(Debug, sqlx::FromRow)]
struct JobRow {
    id: String,
    tenant_id: String,
    status: String,
    job_attempt_count: i64,
    current_step: String,
    content_revision: i64,
    candidate_content_revision: Option<i64>,
    has_candidate: i64,
    has_review: i64,
    review_decision: Option<String>,
    has_active_ai_task: i64,
    has_succeeded_ai_without_candidate: i64,
    terminal_has_lease: i64,
    text_artifact_state: String,
}

#[async_trait]
impl ProcessingIntegrityQuery for SqliteGovernanceStore {
    async fn snapshots(
        &self,
        scope: &IntegrityScanScope,
    ) -> Result<Vec<ProcessingIntegritySnapshot>, IntegrityError> {
        let mut sql = "SELECT j.id,j.tenant_id,j.status,j.attempt_count AS job_attempt_count,j.current_step,j.content_revision,(SELECT CASE WHEN json_type(c.payload,'$.content_revision') IN ('integer','real') THEN CAST(json_extract(c.payload,'$.content_revision') AS INTEGER) ELSE NULL END FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id LIMIT 1) AS candidate_content_revision,EXISTS(SELECT 1 FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id) AS has_candidate,EXISTS(SELECT 1 FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=j.tenant_id AND c.job_id=j.id) AS has_review,(SELECT r.decision FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=j.tenant_id AND c.job_id=j.id LIMIT 1) AS review_decision,EXISTS(SELECT 1 FROM document_ai_tasks a WHERE a.tenant_id=j.tenant_id AND a.job_id=j.id AND a.status IN ('queued','running','retry_scheduled')) AS has_active_ai_task,EXISTS(SELECT 1 FROM document_ai_tasks a WHERE a.tenant_id=j.tenant_id AND a.job_id=j.id AND a.status='succeeded' AND NOT EXISTS(SELECT 1 FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id)) AS has_succeeded_ai_without_candidate,(j.lease_owner IS NOT NULL OR j.lease_token IS NOT NULL) AS terminal_has_lease,CASE WHEN EXISTS(SELECT 1 FROM document_processing_steps s WHERE s.tenant_id=j.tenant_id AND s.job_id=j.id AND s.step_kind='extract_text' AND s.status='succeeded') THEN CASE WHEN EXISTS(SELECT 1 FROM document_processing_steps s WHERE s.tenant_id=j.tenant_id AND s.job_id=j.id AND s.step_kind='extract_text' AND s.status='succeeded' AND json_extract(s.checkpoint_json,'$.text_artifact_reference') IS NOT NULL AND json_extract(s.checkpoint_json,'$.text_artifact_reference') <> '') THEN 'present' ELSE 'missing' END ELSE 'unknown' END AS text_artifact_state FROM document_processing_jobs j WHERE 1=1".to_string();
        if scope.tenant_id.is_some() {
            sql.push_str(" AND j.tenant_id = ?");
        }
        if scope.resource_id.is_some() {
            sql.push_str(" AND j.id = ?");
        }
        let mut query = sqlx::query_as::<_, JobRow>(&sql);
        if let Some(tenant_id) = scope.tenant_id {
            query = query.bind(tenant_id.to_string());
        }
        if let Some(job_id) = scope.resource_id.as_deref() {
            query = query.bind(job_id);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|_| IntegrityError::DependencyUnavailable)?;
        let mut output = Vec::with_capacity(rows.len());
        for row in rows {
            let job_id = Uuid::parse_str(&row.id).map_err(|_| IntegrityError::InvalidFinding)?;
            let tenant_id =
                Uuid::parse_str(&row.tenant_id).map_err(|_| IntegrityError::InvalidFinding)?;
            let steps = sqlx::query_as::<_, (String, String, i64)>(
                "SELECT step_kind,status,attempt_number FROM document_processing_steps WHERE tenant_id=?1 AND job_id=?2 ORDER BY step_kind,attempt_number",
            )
            .bind(&row.tenant_id)
            .bind(&row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| IntegrityError::DependencyUnavailable)?
            .into_iter()
            .map(|(step_kind, status, attempt_number)| ProcessingStepIntegritySnapshot {
                step_kind,
                status,
                attempt_number,
            })
            .collect();
            output.push(ProcessingIntegritySnapshot {
                tenant_id,
                job_id,
                job_status: row.status,
                job_attempt_count: row.job_attempt_count,
                current_step: row.current_step,
                content_revision: row.content_revision,
                candidate_content_revision: row.candidate_content_revision,
                has_candidate: row.has_candidate != 0,
                has_review: row.has_review != 0,
                review_decision: row.review_decision,
                has_active_ai_task: row.has_active_ai_task != 0,
                has_succeeded_ai_without_candidate: row.has_succeeded_ai_without_candidate != 0,
                terminal_has_lease: row.terminal_has_lease != 0,
                steps,
                text_artifact_state: parse_text_artifact_state(&row.text_artifact_state),
            });
        }
        Ok(output)
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
impl IntegrityPersistencePort for SqliteGovernanceStore {
    async fn record_scan_run(&self, run: &IntegrityScanRun) -> Result<(), IntegrityError> {
        sqlx::query("INSERT INTO data_integrity_scan_runs (id,tenant_id,scope,status,started_at,finished_at,rule_count,finding_count,failure_code,created_by,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET status=excluded.status,finished_at=excluded.finished_at,finding_count=excluded.finding_count,failure_code=excluded.failure_code")
            .bind(run.id.to_string()).bind(run.tenant_id.map(|v| v.to_string()))
            .bind(serde_json::to_string(&run.scope).map_err(|_| IntegrityError::Persistence)?)
            .bind(format!("{:?}", run.status).to_lowercase()).bind(run.started_at.map(|v| v.to_rfc3339()))
            .bind(run.finished_at.map(|v| v.to_rfc3339())).bind(i64::from(run.rule_count))
            .bind(i64::try_from(run.finding_count).map_err(|_| IntegrityError::Persistence)?).bind(&run.failure_code)
            .bind(run.created_by.to_string()).bind(Utc::now().to_rfc3339())
            .execute(&self.pool).await.map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn upsert_finding(&self, finding: &IntegrityFinding) -> Result<(), IntegrityError> {
        sqlx::query("INSERT INTO data_integrity_findings (id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?23) ON CONFLICT(tenant_id,rule_id,rule_version,resource_type,resource_id,fingerprint) DO UPDATE SET last_detected_at=excluded.last_detected_at,occurrence_count=data_integrity_findings.occurrence_count+1,status=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN 'open' ELSE excluded.status END,resolved_at=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN NULL ELSE data_integrity_findings.resolved_at END,resolution_reason=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN NULL ELSE data_integrity_findings.resolution_reason END,reopened_at=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN excluded.last_detected_at ELSE data_integrity_findings.reopened_at END,reopen_count=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN data_integrity_findings.reopen_count+1 ELSE data_integrity_findings.reopen_count END,previous_resolution=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN COALESCE(data_integrity_findings.resolution_reason,data_integrity_findings.status) ELSE data_integrity_findings.previous_resolution END,version=data_integrity_findings.version+1,updated_at=excluded.updated_at")
            .bind(finding.id().to_string()).bind(finding.tenant_id().to_string()).bind(finding.rule_id())
            .bind(i64::from(finding.rule_version())).bind(finding.bounded_context()).bind(finding.resource_type()).bind(finding.resource_id())
            .bind(format!("{:?}", finding.severity()).to_lowercase()).bind(finding.fingerprint())
            .bind(finding.detected_state().to_string()).bind(finding.expected_state().to_string()).bind(finding_status_name(finding.status()))
            .bind(finding.repairability()).bind(finding.first_detected_at().to_rfc3339()).bind(finding.last_detected_at().to_rfc3339())
            .bind(i64::try_from(finding.occurrence_count()).map_err(|_| IntegrityError::Persistence)?).bind(finding.resolved_at().map(|v| v.to_rfc3339()))
            .bind(finding.resolution_reason())
            .bind(finding.reopened_at().map(|v| v.to_rfc3339()))
            .bind(i64::try_from(finding.reopen_count()).map_err(|_| IntegrityError::Persistence)?)
            .bind(finding.previous_resolution())
            .bind(finding.version())
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool).await.map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, IntegrityError> {
        let row = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version FROM data_integrity_findings WHERE id=?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| IntegrityError::Persistence)?;
        row.map(FindingRow::into_domain).transpose()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FindingRow {
    id: String,
    tenant_id: String,
    rule_id: String,
    rule_version: i64,
    bounded_context: String,
    resource_type: String,
    resource_id: String,
    severity: String,
    fingerprint: String,
    detected_state: String,
    expected_state: String,
    status: String,
    repairability: String,
    first_detected_at: String,
    last_detected_at: String,
    occurrence_count: i64,
    resolved_at: Option<String>,
    resolution_reason: Option<String>,
    reopened_at: Option<String>,
    reopen_count: i64,
    previous_resolution: Option<String>,
    version: i64,
}

impl FindingRow {
    fn into_domain(self) -> Result<IntegrityFinding, IntegrityError> {
        let parse_uuid =
            |value: &str| Uuid::parse_str(value).map_err(|_| IntegrityError::Persistence);
        let parse_time = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|_| IntegrityError::Persistence)
        };
        IntegrityFinding::rehydrate(
            parse_uuid(&self.id)?,
            parse_uuid(&self.tenant_id)?,
            self.rule_id,
            u32::try_from(self.rule_version).map_err(|_| IntegrityError::Persistence)?,
            self.bounded_context,
            self.resource_type,
            self.resource_id,
            parse_severity(&self.severity)?,
            self.fingerprint,
            serde_json::from_str(&self.detected_state).map_err(|_| IntegrityError::Persistence)?,
            serde_json::from_str(&self.expected_state).map_err(|_| IntegrityError::Persistence)?,
            parse_finding_status(&self.status)?,
            self.repairability,
            parse_time(&self.first_detected_at)?,
            parse_time(&self.last_detected_at)?,
            u64::try_from(self.occurrence_count).map_err(|_| IntegrityError::Persistence)?,
            self.resolved_at.as_deref().map(parse_time).transpose()?,
            self.resolution_reason,
            self.reopened_at.as_deref().map(parse_time).transpose()?,
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
    id: String,
    tenant_id: Option<String>,
    scope: String,
    status: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    rule_count: i64,
    finding_count: i64,
    failure_code: Option<String>,
    created_by: String,
}

impl ScanRunRow {
    fn into_domain(self) -> Result<IntegrityScanRun, IntegrityError> {
        let parse_uuid =
            |value: &str| Uuid::parse_str(value).map_err(|_| IntegrityError::Persistence);
        let parse_time = |value: Option<String>| {
            value
                .as_deref()
                .map(|date| {
                    chrono::DateTime::parse_from_rfc3339(date).map(|v| v.with_timezone(&Utc))
                })
                .transpose()
                .map_err(|_| IntegrityError::Persistence)
        };
        Ok(IntegrityScanRun {
            id: parse_uuid(&self.id)?,
            tenant_id: self.tenant_id.as_deref().map(parse_uuid).transpose()?,
            scope: serde_json::from_str(&self.scope).map_err(|_| IntegrityError::Persistence)?,
            status: parse_scan_run_status(&self.status)?,
            started_at: parse_time(self.started_at)?,
            finished_at: parse_time(self.finished_at)?,
            rule_count: u32::try_from(self.rule_count).map_err(|_| IntegrityError::Persistence)?,
            finding_count: u64::try_from(self.finding_count)
                .map_err(|_| IntegrityError::Persistence)?,
            failure_code: self.failure_code,
            created_by: parse_uuid(&self.created_by)?,
        })
    }
}

#[async_trait]
impl IntegrityQueryPort for SqliteGovernanceStore {
    async fn get_scan_run(
        &self,
        tenant_id: Option<Uuid>,
        id: Uuid,
    ) -> Result<Option<IntegrityScanRun>, IntegrityError> {
        let row = sqlx::query_as::<_, ScanRunRow>(
            "SELECT id,tenant_id,scope,status,started_at,finished_at,rule_count,finding_count,failure_code,created_by FROM data_integrity_scan_runs WHERE id=?1 AND (?2 IS NULL OR tenant_id=?2)",
        )
        .bind(id.to_string())
        .bind(tenant_id.map(|value| value.to_string()))
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
            "SELECT id,tenant_id,scope,status,started_at,finished_at,rule_count,finding_count,failure_code,created_by FROM data_integrity_scan_runs WHERE (?1 IS NULL OR tenant_id=?1) ORDER BY created_at DESC,id DESC LIMIT ?2",
        )
        .bind(tenant_id.map(|value| value.to_string()))
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
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version FROM data_integrity_findings WHERE tenant_id=?1 AND (?2 IS NULL OR status=?2) ORDER BY last_detected_at DESC,id DESC LIMIT ?3",
        )
        .bind(tenant_id.to_string())
        .bind(status.map(|value| format!("{value:?}").to_lowercase()))
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        rows.into_iter().map(FindingRow::into_domain).collect()
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl RepairPersistencePort for SqliteGovernanceStore {
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let finding = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT tenant_id,status,version FROM data_integrity_findings WHERE id=?1",
        )
        .bind(run.finding_id().to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?
        .ok_or(RepairError::Conflict)?;
        if finding.0 != run.tenant_id().to_string() {
            return Err(RepairError::Conflict);
        }
        let existing = sqlx::query_as::<_, RepairRunRow>(
            "SELECT id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,command,created_at,updated_at,version FROM data_repair_runs WHERE tenant_id=?1 AND idempotency_key=?2",
        )
        .bind(run.tenant_id().to_string())
        .bind(&run.command().idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if let Some(existing) = existing {
            let existing_run = existing.into_domain()?;
            if existing_run.command() != run.command() {
                return Err(RepairError::IdempotencyConflict);
            }
            let row = sqlx::query_as::<_, (String, String, String, String, i64, Option<String>, Option<String>, Option<String>, i64, Option<String>, String)>(
                "SELECT id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at FROM data_repair_steps WHERE repair_run_id=?1",
            )
            .bind(existing_run.id().to_string())
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
            let parse_time = |value: &str| {
                DateTime::parse_from_rfc3339(value)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|_| RepairError::Persistence)
            };
            let existing_step = RepairStep::rehydrate(
                Uuid::parse_str(&row.0).map_err(|_| RepairError::Persistence)?,
                Uuid::parse_str(&row.1).map_err(|_| RepairError::Persistence)?,
                Uuid::parse_str(&row.2).map_err(|_| RepairError::Persistence)?,
                parse_step_status(&row.3)?,
                u32::try_from(row.4).map_err(|_| RepairError::Persistence)?,
                row.5
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|_| RepairError::Persistence)?,
                row.6,
                row.7,
                row.8,
                row.9.as_deref().map(parse_time).transpose()?,
                parse_time(&row.10)?,
            )?;
            commit_immediate(&mut connection)
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
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,idempotency_key,command,version,created_at,updated_at,next_attempt_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?11)")
            .bind(run.id().to_string())
            .bind(run.tenant_id().to_string())
            .bind(run.finding_id().to_string())
            .bind(repair_run_status_name(run.status()))
            .bind(run.created_by().to_string())
            .bind(run.approved_by().map(|value| value.to_string()))
            .bind(run.approval_note())
            .bind(&run.command().idempotency_key)
            .bind(serde_json::to_string(run.command()).map_err(|_| RepairError::Persistence)?)
            .bind(run.version())
            .bind(run.created_at().to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,?12)")
            .bind(step.id().to_string())
            .bind(run.tenant_id().to_string())
            .bind(step.run_id().to_string())
            .bind(step.finding_id().to_string())
            .bind(data_repair::repair_step_status_name(step.status()))
            .bind(i64::from(step.attempt_count()))
            .bind(step.checkpoint().map(ToString::to_string))
            .bind(step.lease_owner())
            .bind(step.lease_token())
            .bind(step.fence_version())
            .bind(step.lease_expires_at().map(|value| value.to_rfc3339()))
            .bind(step.next_attempt_at().to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repair_planned',version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=?3 AND status='open' AND version=?4")
            .bind(Utc::now().to_rfc3339())
            .bind(run.finding_id().to_string())
            .bind(run.tenant_id().to_string())
            .bind(expected_finding_version)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
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
        audit_sqlite::append_sqlite_in_transaction(&mut connection, &audit_event)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO outbox_events (event_id,event_type,tenant_id,aggregate_id,aggregate_type,payload,schema_version,occurred_at) VALUES (?1,'runtime.governance.repair-created.v1',?2,?3,'repair_run',?4,'v1',?5)")
            .bind(Uuid::now_v7().to_string())
            .bind(run.tenant_id().to_string())
            .bind(run.id().to_string())
            .bind(serde_json::json!({ "repair_run_id": run.id(), "finding_id": run.finding_id() }).to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(CreateRepairResult {
            run,
            step,
            replayed: false,
        })
    }

    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
            .bind(entry.id().to_string()).bind(entry.tenant_id().to_string()).bind(entry.repair_run_id().to_string()).bind(entry.repair_step_id().to_string()).bind(entry.finding_id().to_string())
            .bind(entry.rule_id()).bind(entry.repair_type()).bind(i64::from(entry.repair_version())).bind(entry.actor_type()).bind(entry.actor_id().to_string()).bind(entry.reason())
            .bind(entry.resource_type()).bind(entry.resource_id()).bind(entry.before_hash()).bind(entry.after_hash()).bind(entry.before_snapshot().to_string()).bind(entry.after_snapshot().to_string())
            .bind(i64::from(entry.rows_affected())).bind(format!("{:?}", entry.result()).to_lowercase()).bind(entry.failure_code()).bind(entry.trace_id())
            .bind(entry.started_at().to_rfc3339()).bind(entry.finished_at().to_rfc3339()).bind(entry.previous_hash()).bind(entry.record_hash())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, RepairError> {
        let row = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version FROM data_integrity_findings WHERE id=?1",
        )
        .bind(id.to_string())
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
        let result = sqlx::query("UPDATE data_integrity_findings SET status='repaired',resolved_at=?1,resolution_reason=?2,version=version+1,updated_at=?1 WHERE id=?3 AND status IN ('open','repair_planned','repairing')")
            .bind(Utc::now().to_rfc3339()).bind(reason).bind(finding_id.to_string())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
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
        let result = sqlx::query("UPDATE data_integrity_findings SET status='needs_manual_review',resolution_reason=?1,version=version+1,updated_at=?2 WHERE id=?3 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(reason).bind(Utc::now().to_rfc3339()).bind(finding_id.to_string())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
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
        let expected = repair_run_status_name(expected_status);
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status='approved',approved_by=?1,approval_note=?2,version=version+1,updated_at=?3 WHERE id=?4 AND tenant_id=?5 AND version=?6 AND status=?7 AND requested_by<>?1")
            .bind(approver.to_string()).bind(&note).bind(Utc::now().to_rfc3339()).bind(run_id.to_string()).bind(tenant_id.to_string()).bind(expected_version).bind(expected)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='approved',updated_at=?1 WHERE repair_run_id=?2 AND tenant_id=?3 AND status='awaiting_approval'")
            .bind(Utc::now().to_rfc3339()).bind(run_id.to_string()).bind(tenant_id.to_string()).execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        commit_immediate(&mut connection)
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status='queued',version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=?3 AND version=?4 AND status=?5 AND approved_by IS NOT NULL")
            .bind(Utc::now().to_rfc3339())
            .bind(run_id.to_string())
            .bind(tenant_id.to_string())
            .bind(expected_version)
            .bind(expected)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='queued',next_attempt_at=?1,updated_at=?1 WHERE repair_run_id=?2 AND tenant_id=?3 AND status='approved'")
            .bind(Utc::now().to_rfc3339())
            .bind(run_id.to_string())
            .bind(tenant_id.to_string())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        }
        commit_immediate(&mut connection)
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let updated = sqlx::query("UPDATE data_repair_runs SET status='cancelled',version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=?3 AND version=?4 AND status=?5 AND status NOT IN ('succeeded','cancelled')")
            .bind(Utc::now().to_rfc3339()).bind(run_id.to_string()).bind(tenant_id.to_string()).bind(expected_version).bind(expected)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='cancelled',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=?1 WHERE repair_run_id=?2 AND tenant_id=?3 AND status NOT IN ('succeeded','cancelled')")
            .bind(Utc::now().to_rfc3339()).bind(run_id.to_string()).bind(tenant_id.to_string()).execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        commit_immediate(&mut connection)
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query("UPDATE data_repair_runs SET status=CASE WHEN approved_by IS NULL THEN 'awaiting_approval' ELSE 'queued' END,version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=?3 AND version=?4 AND status=?5 AND status IN ('cancelled','failed','needs_manual_review')")
            .bind(&now).bind(run_id.to_string()).bind(tenant_id.to_string()).bind(expected_version).bind(expected)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status=CASE WHEN (SELECT approved_by FROM data_repair_runs WHERE id=?1) IS NULL THEN 'awaiting_approval' ELSE 'queued' END,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,next_attempt_at=?2,updated_at=?2 WHERE repair_run_id=?1 AND tenant_id=?3 AND status IN ('cancelled','failed','needs_manual_review')")
            .bind(run_id.to_string()).bind(&now).bind(tenant_id.to_string()).execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        commit_immediate(&mut connection)
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let result: Result<(), RepairError> = async {
            sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
                .bind(entry.id().to_string()).bind(entry.tenant_id().to_string()).bind(entry.repair_run_id().to_string()).bind(entry.repair_step_id().to_string()).bind(entry.finding_id().to_string())
                .bind(entry.rule_id()).bind(entry.repair_type()).bind(i64::from(entry.repair_version())).bind(entry.actor_type()).bind(entry.actor_id().to_string()).bind(entry.reason())
                .bind(entry.resource_type()).bind(entry.resource_id()).bind(entry.before_hash()).bind(entry.after_hash()).bind(entry.before_snapshot().to_string()).bind(entry.after_snapshot().to_string())
                .bind(i64::from(entry.rows_affected())).bind(format!("{:?}", entry.result()).to_lowercase()).bind(entry.failure_code()).bind(entry.trace_id())
                .bind(entry.started_at().to_rfc3339()).bind(entry.finished_at().to_rfc3339()).bind(entry.previous_hash()).bind(entry.record_hash())
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repaired',resolved_at=?1,resolution_reason=?2,version=version+1,updated_at=?1 WHERE id=?3 AND tenant_id=?4 AND resource_type=?5 AND resource_id=?6 AND status IN ('open','repair_planned','repairing')")
                .bind(Utc::now().to_rfc3339())
                .bind("repair_succeeded")
                .bind(entry.finding_id().to_string())
                .bind(run.tenant_id().to_string())
                .bind(entry.resource_type())
                .bind(entry.resource_id())
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if finding_update.rows_affected() != 1 {
                return Err(RepairError::Conflict);
            }
            let step_update = sqlx::query("UPDATE data_repair_steps SET status=?1,attempt_count=?2,checkpoint=?3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=?4,next_attempt_at=?5,updated_at=?6 WHERE id=?7 AND tenant_id=?8 AND repair_run_id=?9 AND finding_id=?10 AND fence_version=?11 AND status='running' AND lease_owner=?12 AND lease_token=?13 AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now') AND EXISTS (SELECT 1 FROM data_repair_runs r WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running')")
                .bind(data_repair::repair_step_status_name(step.status()))
                .bind(i64::from(step.attempt_count())).bind(step.checkpoint().map(ToString::to_string))
                .bind(step.fence_version()).bind(step.next_attempt_at().to_rfc3339()).bind(Utc::now().to_rfc3339())
                .bind(step.id().to_string()).bind(run.tenant_id().to_string())
                .bind(run.id().to_string()).bind(run.finding_id().to_string())
                .bind(expected_fence_version).bind(lease_owner).bind(lease_token)
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            if step_update.rows_affected() != 1 {
                return Err(RepairError::LeaseLost);
            }
            let run_update = sqlx::query("UPDATE data_repair_runs SET status=?1,approved_by=?2,approval_note=?3,updated_at=?4,version=?5 WHERE id=?6 AND tenant_id=?7 AND version=?8 AND status='running'")
                .bind(repair_run_status_name(run.status()))
                .bind(run.approved_by().map(|value| value.to_string())).bind(run.approval_note()).bind(Utc::now().to_rfc3339())
                .bind(run.version()).bind(run.id().to_string()).bind(run.tenant_id().to_string())
                .bind(expected_run_version)
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            if run_update.rows_affected() != 1 {
                return Err(RepairError::Conflict);
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => commit_immediate(&mut connection)
                .await
                .map_err(|_| RepairError::Persistence),
            Err(error) => {
                if rollback_immediate(&mut connection).await.is_err() {
                    return Err(RepairError::Persistence);
                }
                Err(error)
            }
        }
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let result: Result<(), RepairError> = async {
            let finding_status = match run.status() {
                RepairRunStatus::Queued => "repair_planned",
                RepairRunStatus::Cancelled => "open",
                RepairRunStatus::Failed | RepairRunStatus::NeedsManualReview => {
                    "needs_manual_review"
                }
                _ => return Err(RepairError::InvalidTransition),
            };
            let finished_at = (!matches!(run.status(), RepairRunStatus::Queued))
                .then(|| entry.finished_at().to_rfc3339());
            sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
                .bind(entry.id().to_string()).bind(entry.tenant_id().to_string()).bind(entry.repair_run_id().to_string()).bind(entry.repair_step_id().to_string()).bind(entry.finding_id().to_string())
                .bind(entry.rule_id()).bind(entry.repair_type()).bind(i64::from(entry.repair_version())).bind(entry.actor_type()).bind(entry.actor_id().to_string()).bind(entry.reason())
                .bind(entry.resource_type()).bind(entry.resource_id()).bind(entry.before_hash()).bind(entry.after_hash()).bind(entry.before_snapshot().to_string()).bind(entry.after_snapshot().to_string())
                .bind(i64::from(entry.rows_affected())).bind(format!("{:?}", entry.result()).to_lowercase()).bind(entry.failure_code()).bind(entry.trace_id())
                .bind(entry.started_at().to_rfc3339()).bind(entry.finished_at().to_rfc3339()).bind(entry.previous_hash()).bind(entry.record_hash())
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            let finding_update = sqlx::query("UPDATE data_integrity_findings SET status=?1,resolution_reason=?2,version=version+1,updated_at=?3 WHERE id=?4 AND tenant_id=?5 AND resource_type=?6 AND resource_id=?7 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
                .bind(finding_status).bind(entry.failure_code().unwrap_or("repair_failed"))
                .bind(Utc::now().to_rfc3339())
                .bind(entry.finding_id().to_string())
                .bind(run.tenant_id().to_string())
                .bind(entry.resource_type())
                .bind(entry.resource_id())
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if finding_update.rows_affected() != 1 {
                return Err(RepairError::Conflict);
            }
            let step_update = sqlx::query("UPDATE data_repair_steps SET status=?1,attempt_count=?2,checkpoint=?3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=?4,next_attempt_at=?5,failure_code=?6,last_error_category=?7,finished_at=?8,updated_at=?9 WHERE id=?10 AND tenant_id=?11 AND repair_run_id=?12 AND finding_id=?13 AND fence_version=?14 AND status='running' AND lease_owner=?15 AND lease_token=?16 AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now') AND EXISTS (SELECT 1 FROM data_repair_runs r WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running')")
                .bind(data_repair::repair_step_status_name(step.status()))
                .bind(i64::from(step.attempt_count())).bind(step.checkpoint().map(ToString::to_string))
                .bind(step.fence_version()).bind(step.next_attempt_at().to_rfc3339())
                .bind(entry.failure_code()).bind(entry.failure_code()).bind(&finished_at).bind(Utc::now().to_rfc3339())
                .bind(step.id().to_string()).bind(run.tenant_id().to_string())
                .bind(run.id().to_string()).bind(run.finding_id().to_string())
                .bind(expected_fence_version).bind(lease_owner).bind(lease_token)
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            if step_update.rows_affected() != 1 {
                return Err(RepairError::LeaseLost);
            }
            let run_update = sqlx::query("UPDATE data_repair_runs SET status=?1,failure_code=?2,last_error_category=?3,finished_at=?4,updated_at=?5,version=?6 WHERE id=?7 AND tenant_id=?8 AND version=?9 AND status='running'")
                .bind(repair_run_status_name(run.status())).bind(entry.failure_code()).bind(entry.failure_code()).bind(&finished_at).bind(Utc::now().to_rfc3339())
                .bind(run.version()).bind(run.id().to_string()).bind(run.tenant_id().to_string())
                .bind(expected_run_version)
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            if run_update.rows_affected() != 1 {
                return Err(RepairError::Conflict);
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => commit_immediate(&mut connection)
                .await
                .map_err(|_| RepairError::Persistence),
            Err(error) => {
                if rollback_immediate(&mut connection).await.is_err() {
                    return Err(RepairError::Persistence);
                }
                Err(error)
            }
        }
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
            let mut connection = begin_immediate(&self.pool)
                .await
                .map_err(|_| RepairError::Persistence)?;
            let row = sqlx::query_as::<_, (i64, i64, String, String)>(
                "SELECT attempt_count,max_attempts,tenant_id,finding_id FROM data_repair_steps WHERE id=?1 AND repair_run_id=?2 AND status='running' AND lease_owner=?3 AND lease_token=?4 AND fence_version=?5 AND lease_expires_at > ?6",
            )
            .bind(step_id.to_string())
            .bind(run_id.to_string())
            .bind(lease_owner)
            .bind(lease_token)
            .bind(expected_fence_version)
            .bind(now.to_rfc3339())
            .fetch_optional(&mut *connection)
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
                (
                    "failed",
                    "failed",
                    "needs_manual_review",
                    Some(now.to_rfc3339()),
                )
            } else {
                match disposition {
                    RepairFailureDisposition::Retry { .. } => {
                        ("queued", "queued", "repair_planned", None)
                    }
                    RepairFailureDisposition::Permanent => (
                        "failed",
                        "failed",
                        "needs_manual_review",
                        Some(now.to_rfc3339()),
                    ),
                    RepairFailureDisposition::NeedsManualReview => (
                        "needs_manual_review",
                        "needs_manual_review",
                        "needs_manual_review",
                        Some(now.to_rfc3339()),
                    ),
                    RepairFailureDisposition::Cancelled => {
                        ("cancelled", "cancelled", "open", Some(now.to_rfc3339()))
                    }
                    RepairFailureDisposition::LeaseLost => unreachable!(),
                }
            };
            let step_result = sqlx::query("UPDATE data_repair_steps SET status=?1,next_attempt_at=?2,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,failure_code=?3,last_error_category=?3,finished_at=?4,updated_at=?5 WHERE id=?6 AND repair_run_id=?7 AND tenant_id=?8 AND status='running' AND lease_owner=?9 AND lease_token=?10 AND fence_version=?11 AND lease_expires_at > ?5")
                .bind(step_status)
                .bind(next_attempt_at.unwrap_or(now).to_rfc3339())
                .bind(effective_code)
                .bind(&finished_at)
                .bind(now.to_rfc3339())
                .bind(step_id.to_string())
                .bind(run_id.to_string())
                .bind(&row.2)
                .bind(lease_owner)
                .bind(lease_token)
                .bind(expected_fence_version)
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if step_result.rows_affected() != 1 {
                rollback_immediate(&mut connection)
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Err(RepairError::LeaseLost);
            }
            let run_result = sqlx::query("UPDATE data_repair_runs SET status=?1,failure_code=?2,last_error_category=?2,finished_at=?3,version=version+1,updated_at=?4 WHERE id=?5 AND tenant_id=?6 AND status='running'")
                .bind(run_status).bind(effective_code).bind(&finished_at).bind(now.to_rfc3339()).bind(run_id.to_string()).bind(&row.2)
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            if run_result.rows_affected() != 1 {
                rollback_immediate(&mut connection)
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Err(RepairError::Conflict);
            }
            let finding_result = sqlx::query("UPDATE data_integrity_findings SET status=?1,resolution_reason=?2,version=version+1,updated_at=?3 WHERE id=?4 AND tenant_id=?5 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
                .bind(finding_status).bind(effective_code).bind(now.to_rfc3339()).bind(&row.3).bind(&row.2)
                .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
            if finding_result.rows_affected() != 1 {
                rollback_immediate(&mut connection)
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Err(RepairError::Conflict);
            }
            commit_immediate(&mut connection)
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let step_update = sqlx::query("UPDATE data_repair_steps SET status='needs_manual_review',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=?1 WHERE id=?2 AND repair_run_id=?3 AND finding_id=?4 AND status='running' AND lease_owner=?5 AND lease_token=?6 AND fence_version=?7 AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now')")
            .bind(Utc::now().to_rfc3339())
            .bind(step.id().to_string())
            .bind(step.run_id().to_string())
            .bind(step.finding_id().to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(step.fence_version())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::LeaseLost);
        }
        let run_update = sqlx::query_as::<_, (String, String)>("UPDATE data_repair_runs SET status='needs_manual_review',version=version+1,updated_at=?1 WHERE id=?2 AND status='running' RETURNING tenant_id,finding_id")
            .bind(Utc::now().to_rfc3339())
            .bind(step.run_id().to_string())
            .fetch_optional(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let Some((tenant_id, finding_id)) = run_update else {
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        };
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='needs_manual_review',resolution_reason=?1,version=version+1,updated_at=?2 WHERE id=?3 AND tenant_id=?4 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(reason)
            .bind(Utc::now().to_rfc3339())
            .bind(&finding_id)
            .bind(&tenant_id)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::Conflict);
        }
        let tenant_id = Uuid::parse_str(&tenant_id).map_err(|_| RepairError::Persistence)?;
        let finding_id = Uuid::parse_str(&finding_id).map_err(|_| RepairError::Persistence)?;
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
        audit_sqlite::append_sqlite_in_transaction(&mut connection, &audit_event)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO outbox_events (event_id,event_type,tenant_id,aggregate_id,aggregate_type,payload,schema_version,occurred_at) VALUES (?1,'runtime.governance.repair-claim-aborted.v1',?2,?3,'repair_run',?4,'v1',?5)")
            .bind(Uuid::now_v7().to_string())
            .bind(tenant_id.to_string())
            .bind(step.run_id().to_string())
            .bind(serde_json::json!({ "repair_run_id": step.run_id(), "repair_step_id": step.id(), "reason": reason }).to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)
    }

    async fn load_run(&self, id: Uuid) -> Result<Option<RepairRun>, RepairError> {
        let row = sqlx::query_as::<_, RepairRunRow>(
            "SELECT id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,command,created_at,updated_at,version FROM data_repair_runs WHERE id=?1",
        )
        .bind(id.to_string())
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
            "SELECT id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,command,created_at,updated_at,version FROM data_repair_runs WHERE tenant_id=?1 AND idempotency_key=?2",
        )
        .bind(tenant_id.to_string())
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
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let row = sqlx::query_as::<_, RepairStepRow>(
            "SELECT s.id,s.repair_run_id,s.finding_id,s.attempt_count,s.checkpoint,s.fence_version,s.next_attempt_at FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id AND r.tenant_id=s.tenant_id AND r.finding_id=s.finding_id JOIN data_integrity_findings f ON f.id=s.finding_id AND f.tenant_id=s.tenant_id WHERE r.status IN ('queued','running') AND f.status IN ('open','repair_planned','repairing','needs_manual_review') AND (s.status IN ('queued') OR (s.status='running' AND (s.lease_expires_at IS NULL OR s.lease_expires_at <= ?1))) AND s.next_attempt_at <= ?1 ORDER BY s.next_attempt_at,s.created_at,s.id LIMIT 1",
        )
        .bind(now.to_rfc3339())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        let Some(row) = row else {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Ok(None);
        };
        let token = Uuid::now_v7().to_string();
        if row.fence_version < 0 {
            rollback_immediate(&mut connection).await?;
            return Err(RepairError::Persistence);
        }
        let fence = row
            .fence_version
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        let expires = now + chrono::Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE data_repair_steps SET status='running',attempt_count=attempt_count+1,lease_owner=?1,lease_token=?2,fence_version=?3,lease_expires_at=?4,updated_at=?5 WHERE id=?6 AND fence_version=?7")
            .bind(worker_id)
            .bind(&token)
            .bind(fence)
            .bind(expires.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(&row.id)
            .bind(row.fence_version)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if result.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::LeaseLost);
        }
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repairing',version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=(SELECT tenant_id FROM data_repair_runs WHERE id=?3) AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(Utc::now().to_rfc3339())
            .bind(&row.finding_id)
            .bind(&row.repair_run_id)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        let run_update = sqlx::query(
            "UPDATE data_repair_runs SET status='running',version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=(SELECT tenant_id FROM data_repair_steps WHERE repair_run_id=?2 LIMIT 1) AND status IN ('queued','running')",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&row.repair_run_id)
        .execute(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if run_update.rows_affected() != 1 {
            if sqlx::query("ROLLBACK")
                .execute(&mut *connection)
                .await
                .is_err()
            {
                return Err(RepairError::Persistence);
            }
            return Err(RepairError::Conflict);
        }
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let checkpoint = row
            .checkpoint
            .map(|value| serde_json::from_str(&value).map_err(|_| RepairError::Persistence))
            .transpose()?;
        let attempt_count = u32::try_from(row.attempt_count)
            .map_err(|_| RepairError::Persistence)?
            .checked_add(1)
            .ok_or(RepairError::Persistence)?;
        let next_attempt_at = chrono::DateTime::parse_from_rfc3339(&row.next_attempt_at)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| RepairError::Persistence)?;
        Ok(Some(RepairStep::rehydrate(
            Uuid::parse_str(&row.id).map_err(|_| RepairError::Persistence)?,
            Uuid::parse_str(&row.repair_run_id).map_err(|_| RepairError::Persistence)?,
            Uuid::parse_str(&row.finding_id).map_err(|_| RepairError::Persistence)?,
            RepairStepStatus::Running,
            attempt_count,
            checkpoint,
            Some(worker_id.to_string()),
            Some(token),
            fence,
            Some(expires),
            next_attempt_at,
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
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let expires = now + chrono::Duration::seconds(lease_duration_secs);
        let updated = sqlx::query(
            "UPDATE data_repair_steps SET lease_expires_at=?1,updated_at=?2 WHERE id=?3 AND status='running' AND lease_owner=?4 AND lease_token=?5 AND fence_version=?6 AND lease_expires_at > ?2 AND EXISTS (SELECT 1 FROM data_repair_runs r JOIN data_integrity_findings f ON f.id=r.finding_id AND f.tenant_id=r.tenant_id WHERE r.id=data_repair_steps.repair_run_id AND r.tenant_id=data_repair_steps.tenant_id AND r.finding_id=data_repair_steps.finding_id AND r.status='running' AND f.status='repairing')",
        )
        .bind(expires.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(step_id.to_string())
        .bind(lease_owner)
        .bind(lease_token)
        .bind(fence_version)
        .execute(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Err(RepairError::LeaseLost);
        }
        let row = sqlx::query_as::<_, RepairStepRow>(
            "SELECT id,repair_run_id,finding_id,attempt_count,checkpoint,fence_version,next_attempt_at FROM data_repair_steps WHERE id=?1",
        )
        .bind(step_id.to_string())
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let checkpoint = row
            .checkpoint
            .map(|value| serde_json::from_str(&value).map_err(|_| RepairError::Persistence))
            .transpose()?;
        let attempt_count =
            u32::try_from(row.attempt_count).map_err(|_| RepairError::Persistence)?;
        let next_attempt_at = chrono::DateTime::parse_from_rfc3339(&row.next_attempt_at)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| RepairError::Persistence)?;
        Ok(RepairStep::rehydrate(
            Uuid::parse_str(&row.id).map_err(|_| RepairError::Persistence)?,
            Uuid::parse_str(&row.repair_run_id).map_err(|_| RepairError::Persistence)?,
            Uuid::parse_str(&row.finding_id).map_err(|_| RepairError::Persistence)?,
            RepairStepStatus::Running,
            attempt_count,
            checkpoint,
            Some(lease_owner.to_string()),
            Some(lease_token.to_string()),
            row.fence_version,
            Some(expires),
            next_attempt_at,
        )?)
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
            "SELECT 1 FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id AND r.tenant_id=s.tenant_id AND r.finding_id=s.finding_id JOIN data_integrity_findings f ON f.id=s.finding_id AND f.tenant_id=s.tenant_id WHERE s.id=?1 AND s.status='running' AND s.lease_owner=?2 AND s.lease_token=?3 AND s.fence_version=?4 AND s.lease_expires_at > ?5 AND r.status='running' AND f.status='repairing'",
        )
        .bind(step_id.to_string())
        .bind(lease_owner)
        .bind(lease_token)
        .bind(fence_version)
        .bind(now.to_rfc3339())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?;
        owned.map_or(Err(RepairError::LeaseLost), |_| Ok(()))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RepairRunRow {
    id: String,
    tenant_id: String,
    finding_id: String,
    status: String,
    requested_by: String,
    approved_by: Option<String>,
    approval_note: Option<String>,
    command: String,
    created_at: String,
    updated_at: String,
    version: i64,
}

impl RepairRunRow {
    fn into_domain(self) -> Result<RepairRun, RepairError> {
        let parse_uuid = |value: &str| Uuid::parse_str(value).map_err(|_| RepairError::Persistence);
        let parse_time = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|_| RepairError::Persistence)
        };
        RepairRun::rehydrate(
            parse_uuid(&self.id)?,
            parse_uuid(&self.tenant_id)?,
            parse_uuid(&self.finding_id)?,
            serde_json::from_str::<RepairCommand>(&self.command)
                .map_err(|_| RepairError::Persistence)?,
            parse_run_status(&self.status)?,
            parse_uuid(&self.requested_by)?,
            self.approved_by.as_deref().map(parse_uuid).transpose()?,
            self.approval_note,
            parse_time(&self.created_at)?,
            parse_time(&self.updated_at)?,
            self.version,
        )
        .map_err(|_| RepairError::Persistence)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RepairStepRow {
    id: String,
    repair_run_id: String,
    finding_id: String,
    attempt_count: i64,
    checkpoint: Option<String>,
    fence_version: i64,
    next_attempt_at: String,
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
