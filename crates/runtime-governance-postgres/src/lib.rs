//! `PostgreSQL` Runtime Governance query and persistence adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use data_integrity::{
    DetectedIntegrityIssue, FindingStatus, IntegrityError, IntegrityFinding,
    IntegrityPersistencePort, IntegrityQueryPort, IntegrityRuleDescriptor, IntegrityScanRun,
    IntegrityScanScope, IntegritySeverity, ProcessingIntegrityQuery, ProcessingIntegritySnapshot,
    ProcessingStepIntegritySnapshot, ScanRunStatus, TextArtifactIntegrityState,
};
use data_repair::{
    RepairCommand, RepairError, RepairLedgerEntry, RepairPersistencePort, RepairRun,
    RepairRunStatus, RepairStep,
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
        let rows = sqlx::query_as::<_, ProcessingJobRow>(
            "SELECT j.id, j.tenant_id, j.status, j.current_step, j.content_revision, (SELECT CASE WHEN c.payload->>'content_revision' ~ '^[0-9]+$' THEN (c.payload->>'content_revision')::bigint ELSE NULL END FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id LIMIT 1) AS candidate_content_revision, EXISTS (SELECT 1 FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id) AS has_candidate, EXISTS (SELECT 1 FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=j.tenant_id AND c.job_id=j.id) AS has_review, (SELECT r.decision FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=j.tenant_id AND c.job_id=j.id LIMIT 1) AS review_decision, EXISTS (SELECT 1 FROM document_ai_tasks a WHERE a.tenant_id=j.tenant_id AND a.job_id=j.id AND a.status IN ('queued','running','retry_scheduled')) AS has_active_ai_task, EXISTS (SELECT 1 FROM document_ai_tasks a WHERE a.tenant_id=j.tenant_id AND a.job_id=j.id AND a.status='succeeded' AND NOT EXISTS (SELECT 1 FROM document_extraction_candidates c WHERE c.tenant_id=j.tenant_id AND c.job_id=j.id)) AS has_succeeded_ai_without_candidate, (j.lease_owner IS NOT NULL OR j.lease_token IS NOT NULL) AS terminal_has_lease, CASE WHEN EXISTS (SELECT 1 FROM document_processing_steps s WHERE s.tenant_id=j.tenant_id AND s.job_id=j.id AND s.step_kind='extract_text' AND s.status='succeeded') THEN CASE WHEN EXISTS (SELECT 1 FROM document_processing_steps s WHERE s.tenant_id=j.tenant_id AND s.job_id=j.id AND s.step_kind='extract_text' AND s.status='succeeded' AND s.checkpoint_json->>'text_artifact_reference' IS NOT NULL AND s.checkpoint_json->>'text_artifact_reference' <> '') THEN 'present' ELSE 'missing' END ELSE 'unknown' END AS text_artifact_state FROM document_processing_jobs j WHERE ($1::uuid IS NULL OR j.tenant_id=$1) AND ($2::uuid IS NULL OR j.id=$2)",
        )
        .bind(scope.tenant_id)
        .bind(scope.resource_id.as_deref().and_then(|id| Uuid::parse_str(id).ok()))
        .fetch_all(&self.pool)
        .await
        .map_err(|_| IntegrityError::DependencyUnavailable)?;
        let mut snapshots = Vec::with_capacity(rows.len());
        for row in rows {
            let steps = sqlx::query_as::<_, (String, String)>(
                "SELECT step_kind, status FROM document_processing_steps WHERE tenant_id=$1 AND job_id=$2 ORDER BY step_kind, attempt_number",
            )
            .bind(row.tenant_id)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| IntegrityError::DependencyUnavailable)?
            .into_iter()
            .map(|(step_kind, status)| ProcessingStepIntegritySnapshot { step_kind, status })
            .collect();
            snapshots.push(ProcessingIntegritySnapshot {
                tenant_id: row.tenant_id,
                job_id: row.id,
                job_status: row.status,
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
            .bind(i32::try_from(run.rule_count).unwrap_or(i32::MAX))
            .bind(i64::try_from(run.finding_count).unwrap_or(i64::MAX))
            .bind(&run.failure_code)
            .bind(run.created_by)
            .execute(&self.pool)
            .await
            .map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn upsert_finding(&self, finding: &IntegrityFinding) -> Result<(), IntegrityError> {
        sqlx::query("INSERT INTO data_integrity_findings (id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) ON CONFLICT (tenant_id,rule_id,rule_version,resource_type,resource_id,fingerprint) DO UPDATE SET last_detected_at=EXCLUDED.last_detected_at, occurrence_count=data_integrity_findings.occurrence_count + 1, detected_state=EXCLUDED.detected_state, expected_state=EXCLUDED.expected_state, status=CASE WHEN data_integrity_findings.status IN ('repaired','false_positive') THEN data_integrity_findings.status ELSE EXCLUDED.status END, version=data_integrity_findings.version + 1, updated_at=NOW()")
            .bind(finding.id)
            .bind(finding.tenant_id)
            .bind(&finding.rule_id)
            .bind(i32::try_from(finding.rule_version).unwrap_or(i32::MAX))
            .bind(&finding.bounded_context)
            .bind(&finding.resource_type)
            .bind(&finding.resource_id)
            .bind(format!("{:?}", finding.severity).to_lowercase())
            .bind(&finding.fingerprint)
            .bind(&finding.detected_state)
            .bind(&finding.expected_state)
            .bind(format!("{:?}", finding.status).to_lowercase())
            .bind(&finding.repairability)
            .bind(finding.first_detected_at)
            .bind(finding.last_detected_at)
            .bind(i64::try_from(finding.occurrence_count).unwrap_or(i64::MAX))
            .bind(finding.resolved_at)
            .bind(&finding.resolution_reason)
            .bind(finding.version)
            .execute(&self.pool)
            .await
            .map_err(|_| IntegrityError::Persistence)?;
        Ok(())
    }

    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, IntegrityError> {
        let row = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,version FROM data_integrity_findings WHERE id=$1",
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
    version: i64,
}

impl FindingRow {
    fn into_domain(self) -> Result<IntegrityFinding, IntegrityError> {
        Ok(IntegrityFinding {
            id: self.id,
            tenant_id: self.tenant_id,
            rule_id: self.rule_id,
            rule_version: u32::try_from(self.rule_version)
                .map_err(|_| IntegrityError::Persistence)?,
            bounded_context: self.bounded_context,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            severity: parse_severity(&self.severity),
            fingerprint: self.fingerprint,
            detected_state: self.detected_state,
            expected_state: self.expected_state,
            status: parse_finding_status(&self.status),
            repairability: self.repairability,
            first_detected_at: self.first_detected_at,
            last_detected_at: self.last_detected_at,
            occurrence_count: u64::try_from(self.occurrence_count).unwrap_or(u64::MAX),
            resolved_at: self.resolved_at,
            resolution_reason: self.resolution_reason,
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
            status: match self.status.as_str() {
                "queued" => ScanRunStatus::Queued,
                "running" => ScanRunStatus::Running,
                "failed" => ScanRunStatus::Failed,
                "cancelled" => ScanRunStatus::Cancelled,
                _ => ScanRunStatus::Succeeded,
            },
            started_at: self.started_at,
            finished_at: self.finished_at,
            rule_count: u32::try_from(self.rule_count).unwrap_or(u32::MAX),
            finding_count: u64::try_from(self.finding_count).unwrap_or(u64::MAX),
            failure_code: self.failure_code,
            created_by: self.created_by,
        })
    }
}

#[async_trait]
impl IntegrityQueryPort for PostgresGovernanceStore {
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
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,version FROM data_integrity_findings WHERE tenant_id=$1 AND ($2::text IS NULL OR status=$2) ORDER BY last_detected_at DESC,id DESC LIMIT $3",
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
impl RepairPersistencePort for PostgresGovernanceStore {
    async fn save_run(&self, run: &RepairRun) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_runs (id,tenant_id,finding_id,status,requested_by,approved_by,approval_note,worker_id,lease_token,fence_version,lease_expires_at,attempt_count,checkpoint,next_attempt_at,idempotency_key,command,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,0,NULL,NOW(),$12,$13,$14,$15,NOW()) ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status, approved_by=EXCLUDED.approved_by, approval_note=EXCLUDED.approval_note, command=EXCLUDED.command, updated_at=NOW(), version=EXCLUDED.version")
            .bind(run.id).bind(run.tenant_id).bind(run.finding_id)
            .bind(format!("{:?}", run.status).to_lowercase()).bind(run.created_by)
            .bind(run.approved_by).bind(&run.approval_note).bind(Option::<String>::None).bind(Option::<String>::None)
            .bind(0_i64).bind(Option::<DateTime<Utc>>::None)
            .bind(&run.command.idempotency_key)
            .bind(serde_json::to_value(&run.command).map_err(|_| RepairError::Persistence)?)
            .bind(run.version).bind(run.created_at)
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn save_step(&self, step: &RepairStep) -> Result<(), RepairError> {
        sqlx::query("INSERT INTO data_repair_steps (id,tenant_id,repair_run_id,finding_id,status,attempt_count,checkpoint,lease_owner,lease_token,fence_version,lease_expires_at,next_attempt_at) VALUES ($1,(SELECT tenant_id FROM data_repair_runs WHERE id=$2),$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status, attempt_count=EXCLUDED.attempt_count, checkpoint=EXCLUDED.checkpoint, lease_owner=EXCLUDED.lease_owner, lease_token=EXCLUDED.lease_token, fence_version=EXCLUDED.fence_version, lease_expires_at=EXCLUDED.lease_expires_at, next_attempt_at=EXCLUDED.next_attempt_at, updated_at=NOW()")
            .bind(step.id).bind(step.run_id).bind(step.finding_id)
            .bind(format!("{:?}", step.status).to_lowercase()).bind(i32::try_from(step.attempt_count).unwrap_or(i32::MAX))
            .bind(&step.checkpoint).bind(&step.lease_owner).bind(&step.lease_token)
            .bind(step.fence_version).bind(step.lease_expires_at).bind(step.next_attempt_at)
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn save_step_fenced(
        &self,
        step: &RepairStep,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let result = sqlx::query("UPDATE data_repair_steps SET status=$1,attempt_count=$2,checkpoint=$3,lease_owner=$4,lease_token=$5,fence_version=$6,lease_expires_at=$7,next_attempt_at=$8,updated_at=NOW() WHERE id=$9 AND fence_version=$10 AND lease_owner=$4 AND lease_token=$5")
            .bind(format!("{:?}", step.status).to_lowercase())
            .bind(i32::try_from(step.attempt_count).unwrap_or(i32::MAX))
            .bind(&step.checkpoint)
            .bind(&step.lease_owner)
            .bind(&step.lease_token)
            .bind(step.fence_version)
            .bind(step.lease_expires_at)
            .bind(step.next_attempt_at)
            .bind(step.id)
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
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
            .bind(entry.id).bind(entry.tenant_id).bind(entry.repair_run_id).bind(entry.repair_step_id).bind(entry.finding_id)
            .bind(&entry.rule_id).bind(&entry.repair_type).bind(i32::try_from(entry.repair_version).unwrap_or(i32::MAX))
            .bind(&entry.actor_type).bind(entry.actor_id).bind(&entry.reason).bind(&entry.resource_type).bind(&entry.resource_id)
            .bind(&entry.before_hash).bind(&entry.after_hash).bind(&entry.before_snapshot).bind(&entry.after_snapshot)
            .bind(i32::try_from(entry.rows_affected).unwrap_or(i32::MAX)).bind(format!("{:?}", entry.result).to_lowercase())
            .bind(&entry.failure_code).bind(&entry.trace_id).bind(entry.started_at).bind(entry.finished_at)
            .bind(&entry.previous_hash).bind(&entry.record_hash)
            .execute(&self.pool).await.map_err(|_| RepairError::Persistence)?;
        Ok(())
    }

    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, RepairError> {
        let row = sqlx::query_as::<_, FindingRow>(
            "SELECT id,tenant_id,rule_id,rule_version,bounded_context,resource_type,resource_id,severity,fingerprint,detected_state,expected_state,status,repairability,first_detected_at,last_detected_at,occurrence_count,resolved_at,resolution_reason,version FROM data_integrity_findings WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepairError::Persistence)?;
        row.map(FindingRow::into_domain)
            .transpose()
            .map_err(|_| RepairError::Persistence)
    }

    async fn commit_success(
        &self,
        run: &RepairRun,
        step: &RepairStep,
        entry: &RepairLedgerEntry,
        expected_fence_version: i64,
    ) -> Result<(), RepairError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RepairError::Persistence)?;
        sqlx::query("INSERT INTO data_repair_events (id,tenant_id,repair_run_id,repair_step_id,finding_id,rule_id,repair_type,repair_version,actor_type,actor_id,reason,resource_type,resource_id,before_hash,after_hash,before_snapshot,after_snapshot,rows_affected,result,failure_code,trace_id,started_at,finished_at,previous_hash,record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
            .bind(entry.id).bind(entry.tenant_id).bind(entry.repair_run_id).bind(entry.repair_step_id).bind(entry.finding_id)
            .bind(&entry.rule_id).bind(&entry.repair_type).bind(i32::try_from(entry.repair_version).unwrap_or(i32::MAX))
            .bind(&entry.actor_type).bind(entry.actor_id).bind(&entry.reason).bind(&entry.resource_type).bind(&entry.resource_id)
            .bind(&entry.before_hash).bind(&entry.after_hash).bind(&entry.before_snapshot).bind(&entry.after_snapshot)
            .bind(i32::try_from(entry.rows_affected).unwrap_or(i32::MAX)).bind(format!("{:?}", entry.result).to_lowercase())
            .bind(&entry.failure_code).bind(&entry.trace_id).bind(entry.started_at).bind(entry.finished_at)
            .bind(&entry.previous_hash).bind(&entry.record_hash)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repaired',resolved_at=NOW(),resolution_reason=$1,version=version+1,updated_at=NOW() WHERE id=$2 AND status IN ('open','repair_planned','repairing')")
            .bind("repair_succeeded")
            .bind(entry.finding_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            transaction.rollback().await.ok();
            return Err(RepairError::Conflict);
        }
        let step_update = sqlx::query("UPDATE data_repair_steps SET status=$1,attempt_count=$2,checkpoint=$3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,fence_version=$4,next_attempt_at=$5,updated_at=NOW() WHERE id=$6 AND fence_version=$7")
            .bind(format!("{:?}", step.status).to_lowercase())
            .bind(i32::try_from(step.attempt_count).unwrap_or(i32::MAX))
            .bind(&step.checkpoint).bind(step.fence_version).bind(step.next_attempt_at)
            .bind(step.id).bind(expected_fence_version)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
        if step_update.rows_affected() != 1 {
            transaction.rollback().await.ok();
            return Err(RepairError::LeaseLost);
        }
        sqlx::query("UPDATE data_repair_runs SET status=$1,approved_by=$2,approval_note=$3,updated_at=NOW(),version=$4 WHERE id=$5")
            .bind(format!("{:?}", run.status).to_lowercase())
            .bind(run.approved_by).bind(&run.approval_note).bind(run.version).bind(run.id)
            .execute(&mut *transaction).await.map_err(|_| RepairError::Persistence)?;
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
            "SELECT s.id,s.repair_run_id,s.finding_id,s.attempt_count,s.checkpoint,s.fence_version,s.next_attempt_at FROM data_repair_steps s JOIN data_repair_runs r ON r.id=s.repair_run_id WHERE r.status IN ('approved','queued','running') AND (s.status IN ('approved','queued','awaiting_approval') OR (s.status='running' AND s.lease_expires_at <= $1)) AND s.next_attempt_at <= $1 ORDER BY s.next_attempt_at,s.created_at,s.id FOR UPDATE SKIP LOCKED LIMIT 1",
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
        let fence = row.fence_version.saturating_add(1);
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
            transaction.rollback().await.ok();
            return Err(RepairError::LeaseLost);
        }
        let finding_update = sqlx::query("UPDATE data_integrity_findings SET status='repairing',version=version+1,updated_at=NOW() WHERE id=$1 AND status IN ('open','repair_planned','repairing')")
            .bind(row.finding_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if finding_update.rows_affected() != 1 {
            transaction.rollback().await.ok();
            return Err(RepairError::Conflict);
        }
        transaction
            .commit()
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(Some(RepairStep {
            id: row.id,
            run_id: row.repair_run_id,
            finding_id: row.finding_id,
            status: RepairRunStatus::Running,
            attempt_count: u32::try_from(row.attempt_count)
                .unwrap_or(u32::MAX)
                .saturating_add(1),
            checkpoint: row.checkpoint,
            lease_owner: Some(worker_id.to_string()),
            lease_token: Some(token),
            fence_version: fence,
            lease_expires_at: Some(expires),
            next_attempt_at: row.next_attempt_at,
        }))
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
        Ok(RepairRun {
            id: self.id,
            tenant_id: self.tenant_id,
            finding_id: self.finding_id,
            command,
            status: parse_run_status(&self.status),
            created_by: self.requested_by,
            approved_by: self.approved_by,
            approval_note: self.approval_note,
            created_at: self.created_at,
            updated_at: self.updated_at,
            version: self.version,
        })
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

/// Helper used by scan orchestration to construct a durable Finding without
/// exposing database rows to the domain.
pub fn finding_from_issue(
    descriptor: &IntegrityRuleDescriptor,
    issue: DetectedIntegrityIssue,
    now: DateTime<Utc>,
) -> Result<IntegrityFinding, IntegrityError> {
    IntegrityFinding::from_issue(descriptor, issue, now)
}
