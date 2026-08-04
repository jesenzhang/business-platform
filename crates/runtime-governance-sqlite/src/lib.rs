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
    repair_run_status_name, RepairCommand, RepairError, RepairLedgerEntry, RepairPersistencePort,
    RepairRun, RepairRunStatus, RepairStep, RepairStepStatus,
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
            .bind(i64::try_from(run.finding_count).unwrap_or(i64::MAX)).bind(&run.failure_code)
            .bind(run.created_by.to_string()).bind(Utc::now().to_rfc3339())
            .execute(&self.pool).await.map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn upsert_finding(&self, finding: &IntegrityFinding) -> Result<(), IntegrityError> {
        sqlx::query("INSERT INTO data_integrity_findings (id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,reopened_at,reopen_count,previous_resolution,version,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?23) ON CONFLICT(tenant_id,rule_id,rule_version,resource_type,resource_id,fingerprint) DO UPDATE SET last_detected_at=excluded.last_detected_at,occurrence_count=data_integrity_findings.occurrence_count+1,status=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN 'open' ELSE excluded.status END,resolved_at=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN NULL ELSE data_integrity_findings.resolved_at END,resolution_reason=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN NULL ELSE data_integrity_findings.resolution_reason END,reopened_at=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN excluded.last_detected_at ELSE data_integrity_findings.reopened_at END,reopen_count=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN data_integrity_findings.reopen_count+1 ELSE data_integrity_findings.reopen_count END,previous_resolution=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN COALESCE(data_integrity_findings.resolution_reason,data_integrity_findings.status) ELSE data_integrity_findings.previous_resolution END,version=data_integrity_findings.version+1,updated_at=excluded.updated_at")
            .bind(finding.id.to_string()).bind(finding.tenant_id.to_string()).bind(&finding.rule_id)
            .bind(i64::from(finding.rule_version)).bind(&finding.bounded_context).bind(&finding.resource_type).bind(&finding.resource_id)
            .bind(format!("{:?}", finding.severity).to_lowercase()).bind(&finding.fingerprint)
            .bind(finding.detected_state.to_string()).bind(finding.expected_state.to_string()).bind(finding_status_name(finding.status))
            .bind(&finding.repairability).bind(finding.first_detected_at.to_rfc3339()).bind(finding.last_detected_at.to_rfc3339())
            .bind(i64::try_from(finding.occurrence_count).unwrap_or(i64::MAX)).bind(finding.resolved_at.map(|v| v.to_rfc3339()))
            .bind(&finding.resolution_reason)
            .bind(finding.reopened_at.map(|v| v.to_rfc3339()))
            .bind(i64::try_from(finding.reopen_count).unwrap_or(i64::MAX))
            .bind(&finding.previous_resolution)
            .bind(finding.version)
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
        Ok(IntegrityFinding {
            id: parse_uuid(&self.id)?,
            tenant_id: parse_uuid(&self.tenant_id)?,
            rule_id: self.rule_id,
            rule_version: u32::try_from(self.rule_version)
                .map_err(|_| IntegrityError::Persistence)?,
            bounded_context: self.bounded_context,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            severity: parse_severity(&self.severity),
            fingerprint: self.fingerprint,
            detected_state: serde_json::from_str(&self.detected_state)
                .map_err(|_| IntegrityError::Persistence)?,
            expected_state: serde_json::from_str(&self.expected_state)
                .map_err(|_| IntegrityError::Persistence)?,
            status: parse_finding_status(&self.status),
            repairability: self.repairability,
            first_detected_at: parse_time(&self.first_detected_at)?,
            last_detected_at: parse_time(&self.last_detected_at)?,
            occurrence_count: u64::try_from(self.occurrence_count).unwrap_or(u64::MAX),
            resolved_at: self.resolved_at.as_deref().map(parse_time).transpose()?,
            resolution_reason: self.resolution_reason,
            reopened_at: self.reopened_at.as_deref().map(parse_time).transpose()?,
            reopen_count: u64::try_from(self.reopen_count).unwrap_or(u64::MAX),
            previous_resolution: self.previous_resolution,
            version: self.version,
        })
    }
}

fn parse_severity(value: &str) -> IntegritySeverity {
    match value {
        "info" => IntegritySeverity::Info,
        "warning" => IntegritySeverity::Warning,
        "critical" => IntegritySeverity::Critical,
        _ => IntegritySeverity::Error,
    }
}

fn parse_finding_status(value: &str) -> FindingStatus {
    match value {
        "repair_planned" => FindingStatus::RepairPlanned,
        "repairing" => FindingStatus::Repairing,
        "repaired" => FindingStatus::Repaired,
        "ignored" => FindingStatus::Ignored,
        "false_positive" => FindingStatus::FalsePositive,
        "stale" => FindingStatus::Stale,
        "needs_manual_review" => FindingStatus::NeedsManualReview,
        _ => FindingStatus::Open,
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
            status: match self.status.as_str() {
                "queued" => ScanRunStatus::Queued,
                "running" => ScanRunStatus::Running,
                "failed" => ScanRunStatus::Failed,
                "cancelled" => ScanRunStatus::Cancelled,
                _ => ScanRunStatus::Succeeded,
            },
            started_at: parse_time(self.started_at)?,
            finished_at: parse_time(self.finished_at)?,
            rule_count: u32::try_from(self.rule_count).unwrap_or(u32::MAX),
            finding_count: u64::try_from(self.finding_count).unwrap_or(u64::MAX),
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

#[async_trait]
impl RepairPersistencePort for SqliteGovernanceStore {
    async fn create_repair_run(
        &self,
        run: &RepairRun,
        step: &RepairStep,
    ) -> Result<(), RepairError> {
        run.command.validate()?;
        if run.id != step.run_id
            || run.finding_id != step.finding_id
            || run.command.integrity_finding_id != run.finding_id
            || run.command.tenant_id != run.tenant_id
            || step.fence_version < 0
        {
            return Err(RepairError::Conflict);
        }
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,idempotency_key,command,version,created_at,updated_at,next_attempt_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?11)")
            .bind(run.id.to_string())
            .bind(run.tenant_id.to_string())
            .bind(run.finding_id.to_string())
            .bind(repair_run_status_name(run.status))
            .bind(run.created_by.to_string())
            .bind(run.approved_by.map(|value| value.to_string()))
            .bind(&run.approval_note)
            .bind(&run.command.idempotency_key)
            .bind(serde_json::to_string(&run.command).map_err(|_| RepairError::Persistence)?)
            .bind(run.version)
            .bind(run.created_at.to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,?12)")
            .bind(step.id.to_string())
            .bind(run.tenant_id.to_string())
            .bind(step.run_id.to_string())
            .bind(step.finding_id.to_string())
            .bind(data_repair::repair_step_status_name(step.status))
            .bind(i64::from(step.attempt_count))
            .bind(step.checkpoint.as_ref().map(ToString::to_string))
            .bind(&step.lease_owner)
            .bind(&step.lease_token)
            .bind(step.fence_version)
            .bind(step.lease_expires_at.map(|value| value.to_rfc3339()))
            .bind(step.next_attempt_at.to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repair_planned',version=version+1,updated_at=?1 WHERE id=?2 AND tenant_id=?3 AND status='open'")
            .bind(Utc::now().to_rfc3339())
            .bind(run.finding_id.to_string())
            .bind(run.tenant_id.to_string())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let audit_event = repair_created_audit(
            run.tenant_id,
            run.created_by,
            run.id,
            run.finding_id,
            Utc::now(),
        )?;
        audit_sqlite::append_sqlite_in_transaction(&mut connection, &audit_event)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO outbox_events (event_id,event_type,tenant_id,aggregate_id,aggregate_type,payload,schema_version,occurred_at) VALUES (?1,'runtime.governance.repair-created.v1',?2,?3,'repair_run',?4,'v1',?5)")
            .bind(Uuid::now_v7().to_string())
            .bind(run.tenant_id.to_string())
            .bind(run.id.to_string())
            .bind(serde_json::json!({ "repair_run_id": run.id, "finding_id": run.finding_id }).to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn save_run(&self, run: &RepairRun) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,idempotency_key,command,version,created_at,updated_at,next_attempt_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?11) ON CONFLICT(id) DO UPDATE SET status=excluded.status,approved_by=excluded.approved_by,approval_note=excluded.approval_note,command=excluded.command,version=excluded.version,updated_at=excluded.updated_at")
            .bind(run.id.to_string()).bind(run.tenant_id.to_string()).bind(run.finding_id.to_string())
            .bind(repair_run_status_name(run.status)).bind(run.created_by.to_string()).bind(run.approved_by.map(|v| v.to_string()))
            .bind(&run.approval_note).bind(&run.command.idempotency_key)
            .bind(serde_json::to_string(&run.command).map_err(|_| RepairError::Persistence)?)
            .bind(run.version).bind(run.created_at.to_rfc3339())
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn save_step(&self, step: &RepairStep) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at,created_at,updated_at) SELECT ?1,tenant_id,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?11 FROM data_repair_runs WHERE id=?2 ON CONFLICT(id) DO UPDATE SET status=excluded.status,attempt_count=excluded.attempt_count,checkpoint=excluded.checkpoint,lease_owner=excluded.lease_owner,lease_token=excluded.lease_token,fence_version=excluded.fence_version,lease_expires_at=excluded.lease_expires_at,next_attempt_at=excluded.next_attempt_at,updated_at=excluded.updated_at")
            .bind(step.id.to_string()).bind(step.run_id.to_string()).bind(step.finding_id.to_string())
            .bind(data_repair::repair_step_status_name(step.status)).bind(i64::from(step.attempt_count)).bind(step.checkpoint.as_ref().map(ToString::to_string))
            .bind(&step.lease_owner).bind(&step.lease_token).bind(step.fence_version).bind(step.lease_expires_at.map(|v| v.to_rfc3339()))
            .bind(step.next_attempt_at.to_rfc3339()).execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn save_step_fenced(
        &self,
        step: &RepairStep,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let result = sqlx::query("UPDATE data_repair_steps SET status=?1,attempt_count=?2,checkpoint=?3,lease_owner=?4,lease_token=?5,fence_version=?6,lease_expires_at=?7,next_attempt_at=?8,updated_at=?9 WHERE id=?10 AND fence_version=?11 AND lease_owner=?4 AND lease_token=?5 AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now')")
            .bind(data_repair::repair_step_status_name(step.status))
            .bind(i64::from(step.attempt_count))
            .bind(step.checkpoint.as_ref().map(ToString::to_string))
            .bind(&step.lease_owner)
            .bind(&step.lease_token)
            .bind(step.fence_version)
            .bind(step.lease_expires_at.map(|value| value.to_rfc3339()))
            .bind(step.next_attempt_at.to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(step.id.to_string())
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

    async fn append_ledger(&self, entry: &RepairLedgerEntry) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
            .bind(entry.id.to_string()).bind(entry.tenant_id.to_string()).bind(entry.repair_run_id.to_string()).bind(entry.repair_step_id.to_string()).bind(entry.finding_id.to_string())
            .bind(&entry.rule_id).bind(&entry.repair_type).bind(i64::from(entry.repair_version)).bind(&entry.actor_type).bind(entry.actor_id.to_string()).bind(&entry.reason)
            .bind(&entry.resource_type).bind(&entry.resource_id).bind(&entry.before_hash).bind(&entry.after_hash).bind(entry.before_snapshot.to_string()).bind(entry.after_snapshot.to_string())
            .bind(i64::from(entry.rows_affected)).bind(format!("{:?}", entry.result).to_lowercase()).bind(&entry.failure_code).bind(&entry.trace_id)
            .bind(entry.started_at.to_rfc3339()).bind(entry.finished_at.to_rfc3339()).bind(&entry.previous_hash).bind(&entry.record_hash)
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
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='approved',updated_at=?1 WHERE repair_run_id=?2 AND tenant_id=?3 AND status='awaiting_approval'")
            .bind(Utc::now().to_rfc3339()).bind(run_id.to_string()).bind(tenant_id.to_string()).execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
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
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status='cancelled',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=?1 WHERE repair_run_id=?2 AND tenant_id=?3 AND status NOT IN ('succeeded','cancelled')")
            .bind(Utc::now().to_rfc3339()).bind(run_id.to_string()).bind(tenant_id.to_string()).execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
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
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let step = sqlx::query("UPDATE data_repair_steps SET status=CASE WHEN (SELECT approved_by FROM data_repair_runs WHERE id=?1) IS NULL THEN 'awaiting_approval' ELSE 'queued' END,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,next_attempt_at=?2,updated_at=?2 WHERE repair_run_id=?1 AND tenant_id=?3 AND status IN ('cancelled','failed','needs_manual_review')")
            .bind(run_id.to_string()).bind(&now).bind(tenant_id.to_string()).execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
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
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
            .bind(entry.id.to_string()).bind(entry.tenant_id.to_string()).bind(entry.repair_run_id.to_string()).bind(entry.repair_step_id.to_string()).bind(entry.finding_id.to_string())
            .bind(&entry.rule_id).bind(&entry.repair_type).bind(i64::from(entry.repair_version)).bind(&entry.actor_type).bind(entry.actor_id.to_string()).bind(&entry.reason)
            .bind(&entry.resource_type).bind(&entry.resource_id).bind(&entry.before_hash).bind(&entry.after_hash).bind(entry.before_snapshot.to_string()).bind(entry.after_snapshot.to_string())
            .bind(i64::from(entry.rows_affected)).bind(format!("{:?}", entry.result).to_lowercase()).bind(&entry.failure_code).bind(&entry.trace_id)
            .bind(entry.started_at.to_rfc3339()).bind(entry.finished_at.to_rfc3339()).bind(&entry.previous_hash).bind(&entry.record_hash)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repaired',resolved_at=?1,resolution_reason=?2,version=version+1,updated_at=?1 WHERE id=?3 AND tenant_id=?4 AND status IN ('open','repair_planned','repairing')")
            .bind(Utc::now().to_rfc3339())
            .bind("repair_succeeded")
            .bind(entry.finding_id.to_string())
            .bind(run.tenant_id.to_string())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let step_update = sqlx::query("UPDATE data_repair_steps SET status=?1,attempt_count=?2,checkpoint=?3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=?4,next_attempt_at=?5,updated_at=?6 WHERE id=?7 AND tenant_id=?8 AND fence_version=?9 AND status='running' AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now')")
            .bind(data_repair::repair_step_status_name(step.status))
            .bind(i64::from(step.attempt_count)).bind(step.checkpoint.as_ref().map(ToString::to_string))
            .bind(step.fence_version).bind(step.next_attempt_at.to_rfc3339()).bind(Utc::now().to_rfc3339())
            .bind(step.id.to_string()).bind(run.tenant_id.to_string()).bind(expected_fence_version)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            return Err(RepairError::LeaseLost);
        }
        sqlx::query("UPDATE data_repair_runs SET status=?1,approved_by=?2,approval_note=?3,updated_at=?4,version=?5 WHERE id=?6 AND tenant_id=?7")
            .bind(repair_run_status_name(run.status))
            .bind(run.approved_by.map(|value| value.to_string())).bind(&run.approval_note).bind(Utc::now().to_rfc3339())
            .bind(run.version).bind(run.id.to_string()).bind(run.tenant_id.to_string())
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)
    }

    async fn commit_failure(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
            .bind(entry.id.to_string()).bind(entry.tenant_id.to_string()).bind(entry.repair_run_id.to_string()).bind(entry.repair_step_id.to_string()).bind(entry.finding_id.to_string())
            .bind(&entry.rule_id).bind(&entry.repair_type).bind(i64::from(entry.repair_version)).bind(&entry.actor_type).bind(entry.actor_id.to_string()).bind(&entry.reason)
            .bind(&entry.resource_type).bind(&entry.resource_id).bind(&entry.before_hash).bind(&entry.after_hash).bind(entry.before_snapshot.to_string()).bind(entry.after_snapshot.to_string())
            .bind(i64::from(entry.rows_affected)).bind(format!("{:?}", entry.result).to_lowercase()).bind(&entry.failure_code).bind(&entry.trace_id)
            .bind(entry.started_at.to_rfc3339()).bind(entry.finished_at.to_rfc3339()).bind(&entry.previous_hash).bind(&entry.record_hash)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='needs_manual_review',resolution_reason=?1,version=version+1,updated_at=?2 WHERE id=?3 AND tenant_id=?4 AND status IN ('open','repair_planned','repairing','needs_manual_review')")
            .bind(entry.failure_code.as_deref().unwrap_or("repair_failed"))
            .bind(Utc::now().to_rfc3339())
            .bind(entry.finding_id.to_string())
            .bind(run.tenant_id.to_string())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let step_update = sqlx::query("UPDATE data_repair_steps SET status=?1,attempt_count=?2,checkpoint=?3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=?4,next_attempt_at=?5,updated_at=?6 WHERE id=?7 AND tenant_id=?8 AND fence_version=?9 AND status='running' AND lease_expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now')")
            .bind(data_repair::repair_step_status_name(step.status))
            .bind(i64::from(step.attempt_count)).bind(step.checkpoint.as_ref().map(ToString::to_string))
            .bind(step.fence_version).bind(step.next_attempt_at.to_rfc3339()).bind(Utc::now().to_rfc3339())
            .bind(step.id.to_string()).bind(run.tenant_id.to_string()).bind(expected_fence_version)
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::LeaseLost);
        }
        sqlx::query("UPDATE data_repair_runs SET status=?1,updated_at=?2,version=?3 WHERE id=?4 AND tenant_id=?5")
            .bind(repair_run_status_name(run.status)).bind(Utc::now().to_rfc3339())
            .bind(run.version).bind(run.id.to_string()).bind(run.tenant_id.to_string())
            .execute(&mut *connection).await.map_err(|_| RepairError::Persistence)?;
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
            "SELECT s.id,s.repair_run_id,s.finding_id,s.attempt_count,s.checkpoint,s.fence_version,s.next_attempt_at FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id WHERE r.status IN ('approved','queued','running') AND (s.status IN ('approved','queued','awaiting_approval') OR (s.status='running' AND (s.lease_expires_at IS NULL OR s.lease_expires_at <= ?1))) AND s.next_attempt_at <= ?1 ORDER BY s.next_attempt_at,s.created_at,s.id LIMIT 1",
        )
        .bind(now.to_rfc3339())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        let Some(row) = row else {
            sqlx::query("COMMIT").execute(&mut *connection).await.ok();
            return Ok(None);
        };
        let token = Uuid::now_v7().to_string();
        let fence = row.fence_version.saturating_add(1);
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
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::LeaseLost);
        }
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repairing',version=version+1,updated_at=?1 WHERE id=?2 AND status IN ('open','repair_planned','repairing')")
            .bind(Utc::now().to_rfc3339())
            .bind(&row.finding_id)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        let run_update = sqlx::query(
            "UPDATE data_repair_runs SET status='running',version=version+1,updated_at=?1 WHERE id=?2 AND status IN ('approved','queued','running')",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&row.repair_run_id)
        .execute(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if run_update.rows_affected() != 1 {
            sqlx::query("ROLLBACK").execute(&mut *connection).await.ok();
            return Err(RepairError::Conflict);
        }
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(Some(RepairStep {
            id: Uuid::parse_str(&row.id).map_err(|_| RepairError::Persistence)?,
            run_id: Uuid::parse_str(&row.repair_run_id).map_err(|_| RepairError::Persistence)?,
            finding_id: Uuid::parse_str(&row.finding_id).map_err(|_| RepairError::Persistence)?,
            status: RepairStepStatus::Running,
            attempt_count: u32::try_from(row.attempt_count)
                .unwrap_or(u32::MAX)
                .saturating_add(1),
            checkpoint: row
                .checkpoint
                .and_then(|value| serde_json::from_str(&value).ok()),
            lease_owner: Some(worker_id.to_string()),
            lease_token: Some(token),
            fence_version: fence,
            lease_expires_at: Some(expires),
            next_attempt_at: chrono::DateTime::parse_from_rfc3339(&row.next_attempt_at)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|_| RepairError::Persistence)?,
        }))
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
            "UPDATE data_repair_steps SET lease_expires_at=?1,updated_at=?2 WHERE id=?3 AND status='running' AND lease_owner=?4 AND lease_token=?5 AND fence_version=?6 AND lease_expires_at > ?2 AND EXISTS (SELECT 1 FROM data_repair_runs r WHERE r.id=data_repair_steps.repair_run_id AND r.status NOT IN ('cancelled','succeeded'))",
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
            sqlx::query("COMMIT").execute(&mut *connection).await.ok();
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
        Ok(RepairStep {
            id: Uuid::parse_str(&row.id).map_err(|_| RepairError::Persistence)?,
            run_id: Uuid::parse_str(&row.repair_run_id).map_err(|_| RepairError::Persistence)?,
            finding_id: Uuid::parse_str(&row.finding_id).map_err(|_| RepairError::Persistence)?,
            status: RepairStepStatus::Running,
            attempt_count: u32::try_from(row.attempt_count).unwrap_or(u32::MAX),
            checkpoint: row
                .checkpoint
                .and_then(|value| serde_json::from_str(&value).ok()),
            lease_owner: Some(lease_owner.to_string()),
            lease_token: Some(lease_token.to_string()),
            fence_version: row.fence_version,
            lease_expires_at: Some(expires),
            next_attempt_at: chrono::DateTime::parse_from_rfc3339(&row.next_attempt_at)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|_| RepairError::Persistence)?,
        })
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
            "SELECT 1 FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id WHERE s.id=?1 AND s.status='running' AND s.lease_owner=?2 AND s.lease_token=?3 AND s.fence_version=?4 AND s.lease_expires_at > ?5 AND r.status NOT IN ('cancelled','succeeded')",
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
            parse_run_status(&self.status),
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

fn parse_run_status(value: &str) -> RepairRunStatus {
    match value {
        "draft" => RepairRunStatus::Draft,
        "dry_run_completed" => RepairRunStatus::DryRunCompleted,
        "awaiting_approval" => RepairRunStatus::AwaitingApproval,
        "approved" => RepairRunStatus::Approved,
        "queued" => RepairRunStatus::Queued,
        "running" => RepairRunStatus::Running,
        "verifying" => RepairRunStatus::Verifying,
        "succeeded" => RepairRunStatus::Succeeded,
        "cancelled" => RepairRunStatus::Cancelled,
        "needs_manual_review" => RepairRunStatus::NeedsManualReview,
        _ => RepairRunStatus::Failed,
    }
}
