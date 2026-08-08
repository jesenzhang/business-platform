//! `SQLite` persistence adapter for the durable document processing flow.
//!
//! `SQLite` is deliberately local/single-process. Writes use `BEGIN IMMEDIATE`
//! so independent adapter instances sharing a file still serialize the
//! idempotency read and all side effects.

use async_trait::async_trait;
use audit::{
    audit_chain_genesis, AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource,
    AuditResult,
};
use chrono::{DateTime, Duration, Utc};
use data_repair::{
    RepairCommand, RepairError, RepairExecutionContext, RepairOutcome, RepairPreview, RepairResult,
    RepairVerification,
};
use document_processing::domain::{
    CandidateReview, ExtractionCandidate, JobVersion, ProcessingJob, ProcessingJobStatus,
    ProcessingStepKind, ProcessingStepStatus,
};
use document_processing::ports::{
    AiTask, CandidateQuery, ClaimedProcessingJob, ClassifiedProcessingFailure,
    CompleteAiTaskCommand, ExecutionFence, FinalizeReviewCommand, FinalizeReviewResult,
    ProcessingExecutionUnitOfWork, ProcessingFailureDisposition, ProcessingJobClaimPort,
    ProcessingJobCommandPort, ProcessingJobCursor, ProcessingJobDetail, ProcessingJobListRequest,
    ProcessingJobPage, ProcessingJobQuery, ProcessingJobStatusCounts, ProcessingRepositoryError,
    ProcessingStepQuery, StepCheckpoint, StoredStep, TextArtifactReference,
};
use runtime_governance::processing_repairs::ProcessingRepairPort;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::pool::PoolConnection;
use sqlx::sqlite::SqliteTransactionManager;
use sqlx::{FromRow, Sqlite, SqliteConnection, SqlitePool, TransactionManager};
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

#[allow(dead_code, clippy::too_many_arguments)]
mod legacy {
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use document_processing::domain::{CandidateReview, ExtractionCandidate, ProcessingStepKind};
    use document_processing::ports::{AiTask, ProcessingRepositoryError, StepCheckpoint};
    use uuid::Uuid;

    #[async_trait]
    pub trait ProcessingStepStore: Send + Sync {
        async fn start(
            &self,
            checkpoint: &StepCheckpoint,
            expected_version: i64,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn checkpoint(
            &self,
            checkpoint: &StepCheckpoint,
            expected_version: i64,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn complete(
            &self,
            job_id: Uuid,
            tenant_id: Uuid,
            step_kind: ProcessingStepKind,
            attempt_number: i32,
            expected_version: i64,
            finished_at: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn fail(
            &self,
            job_id: Uuid,
            tenant_id: Uuid,
            step_kind: ProcessingStepKind,
            attempt_number: i32,
            failure_code: &str,
            expected_version: i64,
            finished_at: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
    }

    #[async_trait]
    pub trait AiTaskPort: Send + Sync {
        async fn enqueue(&self, task: &AiTask) -> Result<(), ProcessingRepositoryError>;
        async fn claim_next(
            &self,
            worker_id: &str,
            now: DateTime<Utc>,
            lease_duration_secs: i64,
        ) -> Result<Option<AiTask>, ProcessingRepositoryError>;
        async fn heartbeat(
            &self,
            task_id: Uuid,
            worker_id: &str,
            lease_token: &str,
            fence_version: i64,
            now: DateTime<Utc>,
            lease_duration_secs: i64,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn complete(
            &self,
            task_id: Uuid,
            worker_id: &str,
            lease_token: &str,
            fence_version: i64,
            candidate_id: Uuid,
            now: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn fail(
            &self,
            task_id: Uuid,
            worker_id: &str,
            lease_token: &str,
            fence_version: i64,
            failure_code: &str,
            now: DateTime<Utc>,
        ) -> Result<(), ProcessingRepositoryError>;
    }

    #[async_trait]
    pub trait CandidateStore: Send + Sync {
        async fn save_candidate(
            &self,
            candidate: &ExtractionCandidate,
        ) -> Result<(), ProcessingRepositoryError>;
        async fn get_candidate(
            &self,
            tenant_id: Uuid,
            job_id: Uuid,
        ) -> Result<Option<ExtractionCandidate>, ProcessingRepositoryError>;
        async fn save_review(
            &self,
            review: &CandidateReview,
        ) -> Result<(), ProcessingRepositoryError>;
    }
}

use self::legacy::{AiTaskPort, CandidateStore, ProcessingStepStore};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Apply the processing schema without colliding with the Document `SQLite`
/// catalog. `SQLx`'s built-in migrator uses the global `_sqlx_migrations` table,
/// while the two bounded contexts intentionally keep independent catalogs in
/// the same local database.
#[allow(clippy::too_many_lines)]
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_processing_migrations (version INTEGER PRIMARY KEY, checksum BLOB NOT NULL, applied_at TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;

    // A test or an older local process may have applied the SQLx migrator
    // directly. Adopt that already-created schema into the independent
    // catalog instead of attempting to create duplicate tables.
    let schema_exists = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'document_processing_jobs'",
    )
    .fetch_optional(pool)
    .await?
    .is_some();
    if applied < 1 && schema_exists {
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(1_i64)
        .bind(include_bytes!("../migrations/001_document_processing.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    } else if applied < 1 {
        let mut transaction = pool.begin().await?;
        sqlx::raw_sql(include_str!("../migrations/001_document_processing.sql"))
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(1_i64)
        .bind(include_bytes!("../migrations/001_document_processing.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;
    if applied < 2 {
        let revision_column_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('document_ai_tasks') WHERE name = 'cancel_requested_at'",
        )
        .fetch_one(pool)
        .await?
            > 0;
        let mut transaction = pool.begin().await?;
        if !revision_column_exists {
            sqlx::raw_sql(include_str!("../migrations/002_execution_correctness.sql"))
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(2_i64)
        .bind(include_bytes!("../migrations/002_execution_correctness.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;
    if applied < 3 {
        let mut transaction = pool.begin().await?;
        sqlx::raw_sql(include_str!(
            "../migrations/003_runtime_audit_integrity_repair.sql"
        ))
        .execute(&mut *transaction)
        .await?;
        ensure_audit_columns(&mut transaction).await?;
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(3_i64)
        .bind(include_bytes!("../migrations/003_runtime_audit_integrity_repair.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;
    if applied < 4 {
        let mut transaction = pool.begin().await?;
        let columns =
            sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('audit_events')")
                .fetch_all(&mut *transaction)
                .await?;
        if !columns.iter().any(|column| column == "stream_sequence") {
            sqlx::raw_sql(include_str!(
                "../migrations/004_runtime_governance_revision1.sql"
            ))
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(4_i64)
        .bind(include_bytes!("../migrations/004_runtime_governance_revision1.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    // Older Revision-1 development builds could have recorded migration 4
    // after the shared audit columns already existed, which skipped the raw
    // file. Reconcile the finding recurrence columns independently so a
    // restart cannot leave an adapter/query schema half-upgraded.
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;
    if applied < 5 {
        let mut transaction = pool.begin().await?;
        sqlx::raw_sql(include_str!(
            "../migrations/005_document_processing_review_idempotency.sql"
        ))
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(5_i64)
        .bind(include_bytes!(
            "../migrations/005_document_processing_review_idempotency.sql"
        ).as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;
    if applied < 6 {
        let mut transaction = pool.begin().await?;
        sqlx::raw_sql(include_str!(
            "../migrations/006_runtime_governance_revision2.sql"
        ))
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(6_i64)
        .bind(
            include_bytes!("../migrations/006_runtime_governance_revision2.sql").as_slice(),
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM document_processing_migrations",
    )
    .fetch_one(pool)
    .await?;
    if applied < 7 {
        let column_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('document_processing_jobs') WHERE name = 'document_revision_id'",
        )
        .fetch_one(pool)
        .await?
            > 0;
        let mut transaction = pool.begin().await?;
        if !column_exists {
            sqlx::raw_sql(include_str!(
                "../migrations/007_document_revision_binding.sql"
            ))
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "INSERT INTO document_processing_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
        )
        .bind(7_i64)
        .bind(include_bytes!("../migrations/007_document_revision_binding.sql").as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    ensure_revision1_finding_columns(pool).await?;
    Ok(())
}

async fn ensure_revision1_finding_columns(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_table_info('data_integrity_findings')",
    )
    .fetch_all(pool)
    .await?;
    let additions = [
        ("reopened_at", "TEXT"),
        ("reopen_count", "INTEGER NOT NULL DEFAULT 0"),
        ("previous_resolution", "TEXT"),
    ];
    for (name, definition) in additions {
        if !columns.iter().any(|column| column == name) {
            sqlx::query(&format!(
                "ALTER TABLE data_integrity_findings ADD COLUMN {name} {definition}"
            ))
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

async fn ensure_audit_columns(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let columns =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('audit_events')")
            .fetch_all(&mut *connection)
            .await?;
    let additions = [
        ("operation_id", "TEXT"),
        ("actor_type", "TEXT NOT NULL DEFAULT 'user'"),
        ("actor_id", "TEXT"),
        ("correlation_id", "TEXT"),
        ("causation_id", "TEXT"),
        ("reason", "TEXT"),
        ("result", "TEXT NOT NULL DEFAULT 'succeeded'"),
        ("failure_code", "TEXT"),
        ("before_hash", "TEXT"),
        ("after_hash", "TEXT"),
        ("changed_fields", "TEXT NOT NULL DEFAULT '[]'"),
        ("schema_version", "TEXT NOT NULL DEFAULT 'audit.v1'"),
        ("previous_hash", "TEXT"),
        ("record_hash", "TEXT"),
        ("occurred_at", "TEXT"),
    ];
    for (name, definition) in additions {
        if !columns.iter().any(|column| column == name) {
            let sql = format!("ALTER TABLE audit_events ADD COLUMN {name} {definition}");
            sqlx::query(&sql).execute(&mut *connection).await?;
        }
    }
    sqlx::query("UPDATE audit_events SET occurred_at = created_at WHERE occurred_at IS NULL")
        .execute(&mut *connection)
        .await?;
    Ok(())
}

#[derive(Clone)]
pub struct SqliteProcessingStore {
    pool: SqlitePool,
}

fn state_hash(value: impl AsRef<str>) -> String {
    let digest = Sha256::digest(value.as_ref().as_bytes());
    format!("{digest:x}")
}

fn ensure_live_repair_context(context: &RepairExecutionContext) -> Result<(), RepairError> {
    if context.run_id.is_nil()
        || context.step_id.is_nil()
        || context.worker_id.trim().is_empty()
        || context.lease_token.trim().is_empty()
        || context.fence_version < 0
        || context.lease_expires_at <= Utc::now()
    {
        Err(RepairError::LeaseLost)
    } else {
        Ok(())
    }
}

impl SqliteProcessingStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn preview_processing_repair<F>(
        &self,
        command: &RepairCommand,
        summary: &str,
        predicate: F,
    ) -> Result<RepairPreview, RepairError>
    where
        F: FnOnce(&ProcessingJob) -> bool,
    {
        command.validate()?;
        let resource_id = command.target.uuid()?;
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|_| RepairError::Unavailable)?;
        let Some(job) = load_job(&mut connection, command.tenant_id, resource_id)
            .await
            .map_err(|_| RepairError::Persistence)?
        else {
            return Err(RepairError::Conflict);
        };
        let version = job.aggregate_version().value();
        let version_ok = command
            .target
            .expected_resource_version
            .is_none_or(|expected| expected == version);
        let executable = version_ok && predicate(&job);
        let conflict_reason = if !version_ok {
            Some("resource version does not match the dry-run precondition".to_string())
        } else if !executable {
            Some("owner state does not satisfy this repair's preconditions".to_string())
        } else {
            None
        };
        let before_hash = state_hash(format!(
            "processing-job:{}:{}:{}:{}",
            job.id(),
            version,
            job.status().as_str(),
            job.current_step().as_str()
        ));
        Ok(RepairPreview {
            command_id: Uuid::now_v7(),
            descriptor: data_repair::RepairDescriptor {
                repair_type: command.repair_type.clone(),
                version: command.repair_version,
                bounded_context: "document-processing".to_string(),
                risk_level: data_repair::RepairRiskLevel::Low,
                requires_approval: false,
                supports_automatic_execution: true,
            },
            finding_id: command.integrity_finding_id,
            resource_type: command.target.resource_type.clone(),
            resource_id: command.target.resource_id.clone(),
            before_hash,
            expected_after_hash: executable.then(|| {
                state_hash(format!(
                    "processing-job:{resource_id}:{version_plus}",
                    version_plus = version.saturating_add(1)
                ))
            }),
            affected_count: u32::from(executable),
            resource_version_before: Some(version),
            change_summary: summary.to_string(),
            preconditions: vec![
                "tenant and target identity match".to_string(),
                "owner resource version matches expected version".to_string(),
            ],
            executable,
            conflict_reason,
            warnings: vec!["dry run reads owner state and performs no mutation".to_string()],
        })
    }
}

pub type SqliteProcessingRepository = SqliteProcessingStore;

#[derive(Debug, FromRow)]
struct JobRow {
    id: String,
    tenant_id: String,
    document_id: String,
    document_revision_id: Option<String>,
    content_revision: i64,
    request_key: String,
    status: String,
    current_step: String,
    attempt_count: i32,
    max_attempts: i32,
    next_attempt_at: String,
    cancel_requested_at: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    lease_owner: Option<String>,
    lease_token: Option<String>,
    lease_expires_at: Option<String>,
    fence_version: i64,
    version: i64,
    created_by: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct CandidateRow {
    payload: String,
}

#[derive(Debug, FromRow)]
struct StepRow {
    step_kind: String,
    status: String,
    attempt_number: i32,
    checkpoint_json: Option<String>,
    failure_code: Option<String>,
}

#[derive(Debug, FromRow)]
struct ReviewRow {
    id: String,
    tenant_id: String,
    candidate_id: String,
    reviewer_id: String,
    decision: String,
    patch: Option<String>,
    comment: Option<String>,
    candidate_version: i64,
    created_at: String,
    request_fingerprint: String,
}

#[derive(Debug, FromRow)]
struct AiTaskRow {
    id: String,
    tenant_id: String,
    job_id: String,
    step_kind: String,
    status: String,
    input_artifact_id: Option<String>,
    attempt_count: i32,
    max_attempts: i32,
    next_attempt_at: String,
    cancel_requested_at: Option<String>,
    lease_owner: Option<String>,
    lease_token: Option<String>,
    fence_version: i64,
    lease_expires_at: Option<String>,
    output_candidate_id: Option<String>,
}

const AI_TASK_COLUMNS: &str = "id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, lease_owner, lease_token, fence_version, lease_expires_at, output_candidate_id";

#[allow(clippy::needless_pass_by_value)]
fn map_sql_error(error: sqlx::Error) -> ProcessingRepositoryError {
    match error {
        sqlx::Error::PoolClosed | sqlx::Error::PoolTimedOut | sqlx::Error::Io(_) => {
            ProcessingRepositoryError::Unavailable
        }
        _ => ProcessingRepositoryError::Failed,
    }
}

async fn rollback(
    connection: &mut PoolConnection<Sqlite>,
) -> Result<(), ProcessingRepositoryError> {
    sqlx::query("ROLLBACK")
        .execute(&mut **connection)
        .await
        .map(|_| ())
        .map_err(map_sql_error)
}

fn parse_uuid(value: &str) -> Result<Uuid, ProcessingRepositoryError> {
    Uuid::parse_str(value).map_err(|_| ProcessingRepositoryError::Failed)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| ProcessingRepositoryError::Failed)
}

fn parse_optional_time(
    value: Option<String>,
) -> Result<Option<DateTime<Utc>>, ProcessingRepositoryError> {
    value.map(|value| parse_time(&value)).transpose()
}

fn to_job(row: JobRow) -> Result<ProcessingJob, ProcessingRepositoryError> {
    let lease = match (row.lease_owner, row.lease_token, row.lease_expires_at) {
        (Some(owner), Some(token), Some(expires_at)) => {
            Some((owner, token, parse_time(&expires_at)?, row.fence_version))
        }
        (None, None, None) => None,
        _ => return Err(ProcessingRepositoryError::Failed),
    };
    let mut job = ProcessingJob::rehydrate_with_fence(
        parse_uuid(&row.id)?,
        parse_uuid(&row.tenant_id)?,
        parse_uuid(&row.document_id)?,
        row.content_revision,
        row.request_key,
        ProcessingJobStatus::try_from(row.status.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        ProcessingStepKind::try_from(row.current_step.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        row.attempt_count,
        row.max_attempts,
        parse_time(&row.next_attempt_at)?,
        parse_optional_time(row.cancel_requested_at)?,
        row.failure_code,
        row.failure_message,
        JobVersion::new(row.version).map_err(|_| ProcessingRepositoryError::Failed)?,
        parse_uuid(&row.created_by)?,
        parse_time(&row.created_at)?,
        parse_time(&row.updated_at)?,
        row.fence_version,
        lease,
    )
    .map_err(|_| ProcessingRepositoryError::Failed)?;
    if let Some(revision_id) = row.document_revision_id {
        job.bind_document_revision(parse_uuid(&revision_id)?)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
    }
    Ok(job)
}

async fn load_job(
    connection: &mut SqliteConnection,
    tenant_id: Uuid,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id.to_string())
    .bind(job_id.to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    row.map(to_job).transpose()
}

async fn load_job_by_id(
    connection: &mut SqliteConnection,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE id = ?1",
    )
    .bind(job_id.to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    row.map(to_job).transpose()
}

async fn save_job(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    expected_version: i64,
) -> Result<(), ProcessingRepositoryError> {
    let lease = job.lease_snapshot();
    let (lease_owner, lease_token, lease_expires_at) =
        lease.map_or((None, None, None), |(owner, token, expires_at, _fence)| {
            (Some(owner), Some(token), Some(expires_at.to_rfc3339()))
        });
    let result = sqlx::query(
        "UPDATE document_processing_jobs SET status = ?1, current_step = ?2, attempt_count = ?3, max_attempts = ?4, next_attempt_at = ?5, cancel_requested_at = ?6, failure_code = ?7, failure_message = ?8, lease_owner = ?9, lease_token = ?10, lease_expires_at = ?11, fence_version = ?12, version = ?13, updated_at = ?14 WHERE tenant_id = ?15 AND id = ?16 AND version = ?17",
    )
    .bind(job.status().as_str())
    .bind(job.current_step().as_str())
    .bind(job.attempt_count())
    .bind(job.max_attempts())
    .bind(job.next_attempt_at().to_rfc3339())
    .bind(job.cancel_requested_at().map(|value| value.to_rfc3339()))
    .bind(job.failure_code())
    .bind(job.failure_message())
    .bind(lease_owner)
    .bind(lease_token)
    .bind(lease_expires_at)
    .bind(job.fence_version())
    .bind(job.aggregate_version().value())
    .bind(job.updated_at().to_rfc3339())
    .bind(job.tenant_id().to_string())
    .bind(job.id().to_string())
    .bind(expected_version)
    .execute(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    if result.rows_affected() != 1 {
        return Err(ProcessingRepositoryError::Conflict);
    }
    Ok(())
}

async fn insert_steps(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
) -> Result<(), ProcessingRepositoryError> {
    for step in ProcessingStepKind::FIXED {
        sqlx::query(
            "INSERT INTO document_processing_steps (job_id, tenant_id, step_kind, status, attempt_number, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
        )
        .bind(job.id().to_string())
        .bind(job.tenant_id().to_string())
        .bind(step.as_str())
        .bind(ProcessingStepStatus::Pending.as_str())
        .bind(job.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    }
    Ok(())
}

#[async_trait]
impl ProcessingJobCommandPort for SqliteProcessingStore {
    async fn create(
        &self,
        job: &ProcessingJob,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let existing = sqlx::query_as::<_, JobRow>(
            "SELECT id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE tenant_id = ?1 AND document_id = ?2 AND request_key = ?3",
        )
        .bind(job.tenant_id().to_string())
        .bind(job.document_id().to_string())
        .bind(job.request_key())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        if let Some(existing) = existing {
            let existing = to_job(existing)?;
            if existing.document_content_revision() != job.document_content_revision()
                || existing.document_revision_id() != job.document_revision_id()
            {
                rollback(&mut connection).await?;
                return Err(ProcessingRepositoryError::IdempotencyConflict);
            }
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            return Ok(existing);
        }
        let result = sqlx::query(
            "INSERT INTO document_processing_jobs (id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, NULL, NULL, NULL, NULL, 0, ?12, ?13, ?14, ?15)",
        )
        .bind(job.id().to_string())
        .bind(job.tenant_id().to_string())
        .bind(job.document_id().to_string())
        .bind(job.document_revision_id().map(|id| id.to_string()))
        .bind(job.document_content_revision())
        .bind(job.request_key())
        .bind(job.status().as_str())
        .bind(job.current_step().as_str())
        .bind(job.attempt_count())
        .bind(job.max_attempts())
        .bind(job.next_attempt_at().to_rfc3339())
        .bind(job.aggregate_version().value())
        .bind(job.created_by().to_string())
        .bind(job.created_at().to_rfc3339())
        .bind(job.updated_at().to_rfc3339())
        .execute(&mut *connection)
        .await;
        if let Err(error) = result {
            let mapped_error = map_sql_error(error);
            rollback(&mut connection).await?;
            return Err(mapped_error);
        }
        insert_steps(&mut connection, job).await?;
        sqlx::query(
            "INSERT INTO outbox_events (event_id, event_type, tenant_id, aggregate_id, aggregate_type, payload, schema_version, occurred_at, published) VALUES (?1, 'document.processing.requested.v1', ?2, ?3, 'document_processing_job', ?4, 'v1', ?5, 0)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(job.tenant_id().to_string())
        .bind(job.id().to_string())
        .bind(
            serde_json::json!({
                "job_id": job.id(),
                "document_id": job.document_id(),
                "document_revision_id": job.document_revision_id(),
                "content_revision": job.document_content_revision()
            })
            .to_string(),
        )
        .bind(job.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        Ok(job.clone())
    }

    async fn load(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        load_job(&mut connection, tenant_id, job_id).await
    }

    async fn save(
        &self,
        job: &ProcessingJob,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let result = save_job(&mut connection, job, expected_version).await;
        match result {
            Ok(()) => {
                sqlx::query("COMMIT")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sql_error)?;
                Ok(())
            }
            Err(error) => {
                rollback(&mut connection).await?;
                Err(error)
            }
        }
    }

    async fn request_cancel(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let Some(mut job) = load_job(&mut connection, tenant_id, job_id).await? else {
            rollback(&mut connection).await?;
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        if job.request_cancel(Utc::now()).is_err() {
            rollback(&mut connection).await?;
            return Err(ProcessingRepositoryError::Failed);
        }
        save_job(&mut connection, &job, expected).await?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        Ok(job)
    }
}

#[async_trait]
impl ProcessingJobClaimPort for SqliteProcessingStore {
    async fn claim_next(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError> {
        if worker_id.trim().is_empty() || lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let row = sqlx::query_as::<_, JobRow>(
            "SELECT id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE status = 'queued' AND cancel_requested_at IS NULL AND next_attempt_at <= ?1 AND (lease_expires_at IS NULL OR lease_expires_at <= ?1) ORDER BY created_at, id LIMIT 1",
        )
        .bind(now.to_rfc3339())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        let Some(row) = row else {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            return Ok(None);
        };
        let mut job = to_job(row)?;
        let expected = job.aggregate_version().value();
        let token = Uuid::now_v7().to_string();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let fence_version = job
            .claim(worker_id.to_string(), token.clone(), expires_at, now)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        save_job(&mut connection, &job, expected).await?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        Ok(Some(ClaimedProcessingJob {
            job,
            lease_token: token,
            fence_version,
            lease_expires_at: expires_at,
        }))
    }

    async fn heartbeat(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(mut job) = load_job_by_id(&mut connection, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        job.heartbeat(worker_id, lease_token, fence_version, expires_at, now)
            .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job(&mut connection, &job, expected).await?;
        Ok(expires_at)
    }

    async fn release(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(mut job) = load_job_by_id(&mut connection, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        job.release(worker_id, lease_token, fence_version, now)
            .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job(&mut connection, &job, expected).await
    }

    async fn reclaim_expired(&self, now: DateTime<Utc>) -> Result<u64, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let rows = sqlx::query_as::<_, JobRow>(
            "SELECT id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at FROM document_processing_jobs WHERE lease_expires_at IS NOT NULL AND lease_expires_at <= ?1",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        let mut reclaimed = 0_u64;
        for row in rows {
            let mut job = to_job(row)?;
            let expected = job.aggregate_version().value();
            if job
                .reclaim_expired(now)
                .map_err(|_| ProcessingRepositoryError::Failed)?
            {
                save_job(&mut connection, &job, expected).await?;
                reclaimed = reclaimed.saturating_add(1);
            }
        }
        Ok(reclaimed)
    }
}

#[async_trait]
impl ProcessingJobQuery for SqliteProcessingStore {
    async fn status_counts(
        &self,
        tenant_id: Uuid,
    ) -> Result<ProcessingJobStatusCounts, ProcessingRepositoryError> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT status, COUNT(*) FROM document_processing_jobs WHERE tenant_id = ?1 GROUP BY status",
        )
        .bind(tenant_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sql_error)?;
        let mut counts = ProcessingJobStatusCounts::default();
        for (status, count) in rows {
            let count = u64::try_from(count).map_err(|_| ProcessingRepositoryError::Failed)?;
            match status.as_str() {
                "queued" => counts.queued = count,
                "running" => counts.running = count,
                "waiting_for_ai" => counts.waiting_for_ai = count,
                "waiting_for_review" => counts.waiting_for_review = count,
                "succeeded" => counts.succeeded = count,
                "failed" => counts.failed = count,
                "cancelled" => counts.cancelled = count,
                "rejected" => counts.rejected = count,
                _ => return Err(ProcessingRepositoryError::Failed),
            }
        }
        Ok(counts)
    }
    async fn list(
        &self,
        request: ProcessingJobListRequest,
    ) -> Result<ProcessingJobPage, ProcessingRepositoryError> {
        let limit = request.limit.clamp(1, 100);
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT id, created_at FROM document_processing_jobs WHERE tenant_id = ?1 AND (?2 IS NULL OR document_id = ?2) AND (?3 IS NULL OR (created_at < ?3 OR (created_at = ?3 AND id < ?4))) ORDER BY created_at DESC, id DESC LIMIT ?5",
        )
        .bind(request.tenant_id.to_string())
        .bind(request.document_id.map(|id| id.to_string()))
        .bind(request.cursor.map(|cursor| cursor.created_at.to_rfc3339()))
        .bind(request.cursor.map(|cursor| cursor.id.to_string()))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sql_error)?;
        let take = usize::try_from(limit).unwrap_or(100);
        let next_cursor = if rows.len() > take {
            let row = &rows[take - 1];
            Some(ProcessingJobCursor {
                created_at: DateTime::parse_from_rfc3339(&row.1)
                    .map_err(|_| ProcessingRepositoryError::Failed)?
                    .with_timezone(&Utc),
                id: Uuid::parse_str(&row.0).map_err(|_| ProcessingRepositoryError::Failed)?,
            })
        } else {
            None
        };
        let mut items = Vec::with_capacity(rows.len().min(take));
        for (id, _) in rows.into_iter().take(take) {
            let id = Uuid::parse_str(&id).map_err(|_| ProcessingRepositoryError::Failed)?;
            if let Some(detail) = self.detail(request.tenant_id, id).await? {
                items.push(detail);
            }
        }
        Ok(ProcessingJobPage { items, next_cursor })
    }

    async fn detail(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingJobDetail>, ProcessingRepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        let Some(job) = load_job(&mut connection, tenant_id, job_id).await? else {
            return Ok(None);
        };
        let candidate: Option<ExtractionCandidate> = sqlx::query_as::<_, CandidateRow>(
            "SELECT payload FROM document_extraction_candidates WHERE tenant_id = ?1 AND job_id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?
        .map(|row| serde_json::from_str(&row.payload).map_err(|_| ProcessingRepositoryError::Failed))
        .transpose()?;
        let review = if let Some(candidate) = candidate.as_ref() {
            sqlx::query_as::<_, ReviewRow>(
                "SELECT id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at, idempotency_key, request_fingerprint FROM document_extraction_reviews WHERE tenant_id = ?1 AND candidate_id = ?2",
            )
            .bind(tenant_id.to_string())
            .bind(candidate.id().to_string())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?
            .map(to_review)
            .transpose()?
        } else {
            None
        };
        Ok(Some(ProcessingJobDetail {
            job,
            candidate,
            review,
        }))
    }

    async fn list_for_document(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<ProcessingJobDetail>, ProcessingRepositoryError> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM document_processing_jobs WHERE tenant_id = ?1 AND document_id = ?2 ORDER BY created_at DESC",
        )
        .bind(tenant_id.to_string())
        .bind(document_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sql_error)?;
        let mut details = Vec::with_capacity(ids.len());
        for id in ids {
            let id = parse_uuid(&id)?;
            if let Some(detail) = self.detail(tenant_id, id).await? {
                details.push(detail);
            }
        }
        Ok(details)
    }
}

fn to_review(row: ReviewRow) -> Result<CandidateReview, ProcessingRepositoryError> {
    Ok(CandidateReview {
        id: parse_uuid(&row.id)?,
        tenant_id: parse_uuid(&row.tenant_id)?,
        candidate_id: parse_uuid(&row.candidate_id)?,
        reviewer_id: parse_uuid(&row.reviewer_id)?,
        decision: match row.decision.as_str() {
            "accepted" => document_processing::ReviewDecision::Accepted,
            "edited" => document_processing::ReviewDecision::Edited,
            "rejected" => document_processing::ReviewDecision::Rejected,
            _ => return Err(ProcessingRepositoryError::Failed),
        },
        patch: row
            .patch
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| ProcessingRepositoryError::Failed)
            })
            .transpose()?,
        comment: row.comment,
        candidate_version: row.candidate_version,
        created_at: parse_time(&row.created_at)?,
    })
}

#[async_trait]
impl ProcessingStepStore for SqliteProcessingStore {
    async fn start(
        &self,
        checkpoint: &StepCheckpoint,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'running', started_at = ?1, checkpoint_json = ?2, updated_at = ?1 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?3 AND id = ?4 AND version = ?7)")
            .bind(checkpoint.updated_at.to_rfc3339())
            .bind(checkpoint.checkpoint_json.to_string())
            .bind(checkpoint.tenant_id.to_string())
            .bind(checkpoint.job_id.to_string())
            .bind(checkpoint.step_kind.as_str())
            .bind(checkpoint.attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(ProcessingRepositoryError::Conflict)
        }
    }

    async fn checkpoint(
        &self,
        checkpoint: &StepCheckpoint,
        expected_version: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET checkpoint_json = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?3 AND id = ?4 AND version = ?7)")
            .bind(checkpoint.checkpoint_json.to_string())
            .bind(checkpoint.updated_at.to_rfc3339())
            .bind(checkpoint.tenant_id.to_string())
            .bind(checkpoint.job_id.to_string())
            .bind(checkpoint.step_kind.as_str())
            .bind(checkpoint.attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::NotFound);
        }
        Ok(())
    }

    async fn complete(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
        step_kind: ProcessingStepKind,
        attempt_number: i32,
        expected_version: i64,
        finished_at: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'succeeded', finished_at = ?1, updated_at = ?1 WHERE tenant_id = ?2 AND job_id = ?3 AND step_kind = ?4 AND attempt_number = ?5 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?2 AND id = ?3 AND version = ?6)")
            .bind(finished_at.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(job_id.to_string())
            .bind(step_kind.as_str())
            .bind(attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::NotFound);
        }
        Ok(())
    }

    async fn fail(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
        step_kind: ProcessingStepKind,
        attempt_number: i32,
        failure_code: &str,
        expected_version: i64,
        finished_at: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_processing_steps SET status = 'failed', failure_code = ?1, finished_at = ?2, updated_at = ?2 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6 AND EXISTS (SELECT 1 FROM document_processing_jobs WHERE tenant_id = ?3 AND id = ?4 AND version = ?7)")
            .bind(failure_code)
            .bind(finished_at.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(job_id.to_string())
            .bind(step_kind.as_str())
            .bind(attempt_number)
            .bind(expected_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::NotFound);
        }
        Ok(())
    }
}

#[async_trait]
impl CandidateStore for SqliteProcessingStore {
    async fn save_candidate(
        &self,
        candidate: &ExtractionCandidate,
    ) -> Result<(), ProcessingRepositoryError> {
        let payload =
            serde_json::to_string(candidate).map_err(|_| ProcessingRepositoryError::Failed)?;
        let evidence = serde_json::to_string(&candidate.evidence)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        sqlx::query("INSERT INTO document_extraction_candidates (id, tenant_id, job_id, schema_version, payload, evidence, provider, model, prompt_version, version, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT(tenant_id, job_id) DO UPDATE SET payload = excluded.payload, evidence = excluded.evidence")
            .bind(candidate.id().to_string())
            .bind(candidate.tenant_id().to_string())
            .bind(candidate.job_id().to_string())
            .bind(&candidate.schema_version)
            .bind(payload)
            .bind(evidence)
            .bind(&candidate.provider)
            .bind(&candidate.model)
            .bind(&candidate.prompt_version)
            .bind(candidate.version())
            .bind(candidate.created_at().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        Ok(())
    }

    async fn get_candidate(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ExtractionCandidate>, ProcessingRepositoryError> {
        let row = sqlx::query_as::<_, CandidateRow>(
            "SELECT payload FROM document_extraction_candidates WHERE tenant_id = ?1 AND job_id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sql_error)?;
        row.map(|row| {
            serde_json::from_str(&row.payload).map_err(|_| ProcessingRepositoryError::Failed)
        })
        .transpose()
    }

    async fn save_review(&self, review: &CandidateReview) -> Result<(), ProcessingRepositoryError> {
        let idempotency_key = format!("legacy-review-{}", review.id);
        let request_fingerprint = state_hash(review.id.to_string());
        let result = sqlx::query("INSERT INTO document_extraction_reviews (id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at, idempotency_key, request_fingerprint) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT(tenant_id, candidate_id) DO NOTHING")
            .bind(review.id.to_string())
            .bind(review.tenant_id.to_string())
            .bind(review.candidate_id.to_string())
            .bind(review.reviewer_id.to_string())
            .bind(review.decision.as_str())
            .bind(review.patch.as_ref().map(ToString::to_string))
            .bind(&review.comment)
            .bind(review.candidate_version)
            .bind(review.created_at.to_rfc3339())
            .bind(idempotency_key)
            .bind(request_fingerprint)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::Conflict);
        }
        Ok(())
    }
}

#[async_trait]
impl CandidateQuery for SqliteProcessingStore {
    async fn get_candidate(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ExtractionCandidate>, ProcessingRepositoryError> {
        CandidateStore::get_candidate(self, tenant_id, job_id).await
    }
}

#[async_trait]
impl ProcessingStepQuery for SqliteProcessingStore {
    async fn list_steps(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Vec<StoredStep>, ProcessingRepositoryError> {
        let rows = sqlx::query_as::<_, StepRow>(
            "SELECT step_kind, status, attempt_number, checkpoint_json, failure_code FROM document_processing_steps WHERE tenant_id = ?1 AND job_id = ?2 ORDER BY step_kind, attempt_number",
        )
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sql_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredStep {
                    step_kind: ProcessingStepKind::try_from(row.step_kind.as_str())
                        .map_err(|_| ProcessingRepositoryError::Failed)?,
                    status: match row.status.as_str() {
                        "pending" => ProcessingStepStatus::Pending,
                        "running" => ProcessingStepStatus::Running,
                        "succeeded" => ProcessingStepStatus::Succeeded,
                        "failed" => ProcessingStepStatus::Failed,
                        "skipped" => ProcessingStepStatus::Skipped,
                        _ => return Err(ProcessingRepositoryError::Failed),
                    },
                    attempt_number: row.attempt_number,
                    checkpoint_json: row
                        .checkpoint_json
                        .map(|value| serde_json::from_str(&value))
                        .transpose()
                        .map_err(|_| ProcessingRepositoryError::Failed)?,
                    failure_code: row.failure_code,
                })
            })
            .collect()
    }
}

fn to_ai_task(row: AiTaskRow) -> Result<AiTask, ProcessingRepositoryError> {
    Ok(AiTask {
        id: parse_uuid(&row.id)?,
        tenant_id: parse_uuid(&row.tenant_id)?,
        job_id: parse_uuid(&row.job_id)?,
        step_kind: ProcessingStepKind::try_from(row.step_kind.as_str())
            .map_err(|_| ProcessingRepositoryError::Failed)?,
        status: row.status,
        input_artifact_id: row.input_artifact_id,
        attempt_count: row.attempt_count,
        max_attempts: row.max_attempts,
        next_attempt_at: parse_time(&row.next_attempt_at)?,
        cancel_requested_at: parse_optional_time(row.cancel_requested_at)?,
        lease_owner: row.lease_owner,
        lease_token: row.lease_token,
        fence_version: row.fence_version,
        lease_expires_at: row
            .lease_expires_at
            .map(|value| parse_time(&value))
            .transpose()?,
        output_candidate_id: row
            .output_candidate_id
            .map(|value| parse_uuid(&value))
            .transpose()?,
    })
}

#[async_trait]
impl AiTaskPort for SqliteProcessingStore {
    async fn enqueue(&self, task: &AiTask) -> Result<(), ProcessingRepositoryError> {
        sqlx::query("INSERT INTO document_ai_tasks (id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, next_attempt_at, fence_version, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6, ?7, ?8, 0, ?9, ?9) ON CONFLICT(tenant_id, job_id, step_kind, attempt_count) DO NOTHING")
            .bind(task.id.to_string())
            .bind(task.tenant_id.to_string())
            .bind(task.job_id.to_string())
            .bind(task.step_kind.as_str())
            .bind(&task.input_artifact_id)
            .bind(task.attempt_count)
            .bind(task.max_attempts)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        Ok(())
    }

    async fn claim_next(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<AiTask>, ProcessingRepositoryError> {
        if worker_id.trim().is_empty() || lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let mut connection = self.pool.acquire().await.map_err(map_sql_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let query = format!("SELECT {AI_TASK_COLUMNS} FROM document_ai_tasks WHERE status = 'queued' AND attempt_count < max_attempts AND next_attempt_at <= ?1 ORDER BY created_at, id LIMIT 1");
        let row = sqlx::query_as::<_, AiTaskRow>(&query)
            .bind(now.to_rfc3339())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let Some(row) = row else {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            return Ok(None);
        };
        let id = row.id.clone();
        let fence = row
            .fence_version
            .checked_add(1)
            .ok_or(ProcessingRepositoryError::Failed)?;
        let token = Uuid::now_v7().to_string();
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE document_ai_tasks SET status = 'running', lease_owner = ?1, lease_token = ?2, lease_expires_at = ?3, fence_version = ?4, attempt_count = attempt_count + 1, updated_at = ?5 WHERE id = ?6 AND status = 'queued' AND attempt_count < max_attempts AND fence_version = ?7")
            .bind(worker_id)
            .bind(&token)
            .bind(expires_at.to_rfc3339())
            .bind(fence)
            .bind(now.to_rfc3339())
            .bind(&id)
            .bind(row.fence_version)
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::Conflict);
        }
        let query = format!("SELECT {AI_TASK_COLUMNS} FROM document_ai_tasks WHERE id = ?1");
        let updated = sqlx::query_as::<_, AiTaskRow>(&query)
            .bind(id)
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        to_ai_task(updated).map(Some)
    }

    async fn heartbeat(
        &self,
        task_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<(), ProcessingRepositoryError> {
        if lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE document_ai_tasks SET lease_expires_at = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_token = ?5 AND fence_version = ?6 AND lease_expires_at > ?2")
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(task_id.to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(())
    }

    async fn complete(
        &self,
        task_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        candidate_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_ai_tasks SET status = 'succeeded', output_candidate_id = ?1, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_token = ?5 AND fence_version = ?6")
            .bind(candidate_id.to_string())
            .bind(now.to_rfc3339())
            .bind(task_id.to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(())
    }

    async fn fail(
        &self,
        task_id: Uuid,
        worker_id: &str,
        lease_token: &str,
        fence_version: i64,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let result = sqlx::query("UPDATE document_ai_tasks SET status = CASE WHEN attempt_count < max_attempts THEN 'queued' ELSE 'failed' END, failure_code = ?1, next_attempt_at = ?2, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_token = ?5 AND fence_version = ?6")
            .bind(failure_code)
            .bind(now.to_rfc3339())
            .bind(task_id.to_string())
            .bind(worker_id)
            .bind(lease_token)
            .bind(fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(())
    }
}

const JOB_COLUMNS: &str = "id, tenant_id, document_id, document_revision_id, content_revision, request_key, status, current_step, attempt_count, max_attempts, next_attempt_at, cancel_requested_at, failure_code, failure_message, lease_owner, lease_token, lease_expires_at, fence_version, version, created_by, created_at, updated_at";

struct ImmediateSqliteConnection {
    inner: PoolConnection<Sqlite>,
}

impl Deref for ImmediateSqliteConnection {
    type Target = SqliteConnection;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ImmediateSqliteConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Drop for ImmediateSqliteConnection {
    fn drop(&mut self) {
        SqliteTransactionManager::start_rollback(&mut self.inner);
    }
}

async fn begin_immediate(
    pool: &SqlitePool,
) -> Result<ImmediateSqliteConnection, ProcessingRepositoryError> {
    let mut connection = pool.acquire().await.map_err(map_sql_error)?;
    SqliteTransactionManager::begin(&mut connection, Some(Cow::Borrowed("BEGIN IMMEDIATE")))
        .await
        .map_err(map_sql_error)?;
    Ok(ImmediateSqliteConnection { inner: connection })
}

async fn commit_immediate(
    connection: &mut ImmediateSqliteConnection,
) -> Result<(), ProcessingRepositoryError> {
    SqliteTransactionManager::commit(&mut **connection)
        .await
        .map_err(map_sql_error)
}

async fn load_job_for_update(
    connection: &mut SqliteConnection,
    tenant_id: Uuid,
    job_id: Uuid,
) -> Result<Option<ProcessingJob>, ProcessingRepositoryError> {
    let query = format!(
        "SELECT {JOB_COLUMNS} FROM document_processing_jobs WHERE tenant_id = ?1 AND id = ?2"
    );
    sqlx::query_as::<_, JobRow>(&query)
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?
        .map(to_job)
        .transpose()
}

async fn save_job_fenced(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    expected_version: i64,
    fence: &ExecutionFence,
    now: DateTime<Utc>,
) -> Result<(), ProcessingRepositoryError> {
    let lease = job.lease_snapshot();
    let (owner, token, expires_at) = lease
        .map_or((None, None, None), |(owner, token, expires_at, _)| {
            (Some(owner), Some(token), Some(expires_at.to_rfc3339()))
        });
    let result = sqlx::query("UPDATE document_processing_jobs SET status = ?1, current_step = ?2, attempt_count = ?3, max_attempts = ?4, next_attempt_at = ?5, cancel_requested_at = ?6, failure_code = ?7, failure_message = ?8, lease_owner = ?9, lease_token = ?10, lease_expires_at = ?11, fence_version = ?12, version = ?13, updated_at = ?14 WHERE tenant_id = ?15 AND id = ?16 AND version = ?17 AND lease_owner = ?18 AND lease_token = ?19 AND fence_version = ?20 AND lease_expires_at > ?21")
        .bind(job.status().as_str())
        .bind(job.current_step().as_str())
        .bind(job.attempt_count())
        .bind(job.max_attempts())
        .bind(job.next_attempt_at().to_rfc3339())
        .bind(job.cancel_requested_at().map(|value| value.to_rfc3339()))
        .bind(job.failure_code())
        .bind(job.failure_message())
        .bind(owner)
        .bind(token)
        .bind(expires_at)
        .bind(job.fence_version())
        .bind(job.aggregate_version().value())
        .bind(job.updated_at().to_rfc3339())
        .bind(job.tenant_id().to_string())
        .bind(job.id().to_string())
        .bind(expected_version)
        .bind(Some(fence.worker_id.as_str()))
        .bind(Some(fence.lease_token.as_str()))
        .bind(fence.fence_version)
        .bind(now.to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    if result.rows_affected() != 1 {
        return Err(ProcessingRepositoryError::LeaseLost);
    }
    Ok(())
}

async fn save_job_without_fence(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    expected_version: i64,
) -> Result<(), ProcessingRepositoryError> {
    save_job(connection, job, expected_version).await
}

async fn insert_processing_outbox(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    event_type: &str,
    payload: serde_json::Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), ProcessingRepositoryError> {
    sqlx::query("INSERT INTO outbox_events (event_id, event_type, tenant_id, aggregate_id, aggregate_type, payload, schema_version, occurred_at) VALUES (?1, ?2, ?3, ?4, 'document_processing_job', ?5, 'v1', ?6)")
        .bind(Uuid::now_v7().to_string())
        .bind(event_type)
        .bind(job.tenant_id().to_string())
        .bind(job.id().to_string())
        .bind(payload.to_string())
        .bind(occurred_at.to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    Ok(())
}

async fn insert_processing_audit(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    action: &str,
    actor_id: Option<Uuid>,
    details: serde_json::Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), ProcessingRepositoryError> {
    sqlx::query("INSERT INTO document_processing_audit_events (id, tenant_id, job_id, action, actor_id, details, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
        .bind(Uuid::now_v7().to_string())
        .bind(job.tenant_id().to_string())
        .bind(job.id().to_string())
        .bind(action)
        .bind(actor_id.map(|value| value.to_string()))
        .bind(details.to_string())
        .bind(occurred_at.to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    insert_unified_audit(connection, job, action, actor_id, details, occurred_at).await?;
    Ok(())
}

async fn insert_unified_audit(
    connection: &mut SqliteConnection,
    job: &ProcessingJob,
    action: &str,
    actor_id: Option<Uuid>,
    details: serde_json::Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), ProcessingRepositoryError> {
    let actor_id = actor_id.unwrap_or_else(|| job.created_by());
    let result = if action.contains("failed") || action.contains("retry") {
        AuditResult::Failed
    } else if action.contains("cancel") {
        AuditResult::Cancelled
    } else {
        AuditResult::Succeeded
    };
    let action = AuditAction::new(format!("document_processing.{action}"))
        .map_err(|_| ProcessingRepositoryError::Failed)?;
    let resource = AuditResource::new("processing_job", job.id().to_string())
        .map_err(|_| ProcessingRepositoryError::Failed)?;
    let event = AuditEvent::new(
        Uuid::now_v7(),
        job.tenant_id(),
        AuditActor {
            actor_type: if actor_id == job.created_by() {
                AuditActorType::User
            } else {
                AuditActorType::Worker
            },
            actor_id,
        },
        action,
        resource,
        Uuid::now_v7(),
        None,
        None,
        None,
        None,
        result,
        None,
        None,
        None,
        Vec::new(),
        details,
        "audit.v1",
        occurred_at,
    )
    .map_err(|_| ProcessingRepositoryError::Failed)?;
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT record_hash FROM audit_events WHERE tenant_id=?1 AND chain_version=1 ORDER BY stream_sequence DESC LIMIT 1",
    )
    .bind(job.tenant_id().to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(map_sql_error)?
    .flatten()
    .unwrap_or_else(|| audit_chain_genesis(job.tenant_id()));
    let sequence = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(stream_sequence),0)+1 FROM audit_events WHERE tenant_id=?1",
    )
    .bind(job.tenant_id().to_string())
    .fetch_one(&mut *connection)
    .await
    .map_err(map_sql_error)?;
    let event = event
        .with_chain_metadata(sequence, Utc::now(), 1, Some(previous))
        .map_err(|_| ProcessingRepositoryError::Failed)?;
    sqlx::query("INSERT INTO audit_events (id,tenant_id,action,resource_type,resource_id,details,trace_id,created_at,occurred_at,recorded_at,stream_sequence,chain_version,operation_id,actor_type,actor_id,correlation_id,causation_id,reason,result,failure_code,before_hash,after_hash,changed_fields,schema_version,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)")
        .bind(event.id().to_string())
        .bind(event.tenant_id().to_string())
        .bind(event.action().as_str())
        .bind(&event.resource().resource_type)
        .bind(&event.resource().resource_id)
        .bind(event.details().to_string())
        .bind(event.trace_id())
        .bind(event.occurred_at().to_rfc3339())
        .bind(event.recorded_at().to_rfc3339())
        .bind(event.stream_sequence())
        .bind(event.chain_version())
        .bind(event.operation_id().to_string())
        .bind(format!("{:?}", event.actor().actor_type).to_lowercase())
        .bind(event.actor().actor_id.to_string())
        .bind(event.correlation_id().map(|value| value.to_string()))
        .bind(event.causation_id().map(|value| value.to_string()))
        .bind(event.reason())
        .bind(format!("{:?}", event.result()).to_lowercase())
        .bind(event.failure_code())
        .bind(event.before_hash())
        .bind(event.after_hash())
        .bind(serde_json::to_string(event.changed_fields()).map_err(|_| ProcessingRepositoryError::Failed)?)
        .bind(event.schema_version())
        .bind(event.previous_hash())
        .bind(event.record_hash().ok_or(ProcessingRepositoryError::Failed)?)
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    Ok(())
}

async fn write_step_started(
    connection: &mut SqliteConnection,
    checkpoint: &StepCheckpoint,
) -> Result<(), ProcessingRepositoryError> {
    sqlx::query("INSERT INTO document_processing_steps (job_id, tenant_id, step_kind, status, attempt_number, started_at, checkpoint_json, created_at, updated_at) VALUES (?1, ?2, ?3, 'running', ?4, ?5, ?6, ?5, ?5) ON CONFLICT(job_id, step_kind, attempt_number) DO UPDATE SET status = 'running', started_at = excluded.started_at, checkpoint_json = excluded.checkpoint_json, updated_at = excluded.updated_at")
        .bind(checkpoint.job_id.to_string())
        .bind(checkpoint.tenant_id.to_string())
        .bind(checkpoint.step_kind.as_str())
        .bind(checkpoint.attempt_number)
        .bind(checkpoint.updated_at.to_rfc3339())
        .bind(checkpoint.checkpoint_json.to_string())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    Ok(())
}

async fn write_step_completed(
    connection: &mut SqliteConnection,
    tenant_id: Uuid,
    job_id: Uuid,
    step: ProcessingStepKind,
    attempt_number: i32,
    checkpoint: Option<&StepCheckpoint>,
    now: DateTime<Utc>,
) -> Result<(), ProcessingRepositoryError> {
    let checkpoint_json = checkpoint.map(|value| value.checkpoint_json.to_string());
    let result = sqlx::query("UPDATE document_processing_steps SET status = 'succeeded', finished_at = ?1, checkpoint_json = COALESCE(?2, checkpoint_json), updated_at = ?1 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6")
        .bind(now.to_rfc3339())
        .bind(checkpoint_json.clone())
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .bind(step.as_str())
        .bind(attempt_number)
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    if result.rows_affected() == 0 {
        sqlx::query("INSERT INTO document_processing_steps (job_id, tenant_id, step_kind, status, attempt_number, finished_at, checkpoint_json, created_at, updated_at) VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5, ?6, ?5, ?5) ON CONFLICT(job_id, step_kind, attempt_number) DO UPDATE SET status = 'succeeded', finished_at = excluded.finished_at, checkpoint_json = COALESCE(excluded.checkpoint_json, document_processing_steps.checkpoint_json), updated_at = excluded.updated_at")
            .bind(job_id.to_string())
            .bind(tenant_id.to_string())
            .bind(step.as_str())
            .bind(attempt_number)
            .bind(now.to_rfc3339())
            .bind(checkpoint_json)
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
    }
    Ok(())
}

async fn write_step_failed(
    connection: &mut SqliteConnection,
    tenant_id: Uuid,
    job_id: Uuid,
    step: ProcessingStepKind,
    attempt_number: i32,
    failure_code: &str,
    now: DateTime<Utc>,
) -> Result<(), ProcessingRepositoryError> {
    let result = sqlx::query("UPDATE document_processing_steps SET status = 'failed', failure_code = ?1, finished_at = ?2, updated_at = ?2 WHERE tenant_id = ?3 AND job_id = ?4 AND step_kind = ?5 AND attempt_number = ?6")
        .bind(failure_code)
        .bind(now.to_rfc3339())
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .bind(step.as_str())
        .bind(attempt_number)
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    if result.rows_affected() == 0 {
        sqlx::query("INSERT INTO document_processing_steps (job_id, tenant_id, step_kind, status, attempt_number, failure_code, finished_at, created_at, updated_at) VALUES (?1, ?2, ?3, 'failed', ?4, ?5, ?6, ?6, ?6) ON CONFLICT(job_id, step_kind, attempt_number) DO UPDATE SET status = 'failed', failure_code = excluded.failure_code, finished_at = excluded.finished_at, updated_at = excluded.updated_at")
            .bind(job_id.to_string())
            .bind(tenant_id.to_string())
            .bind(step.as_str())
            .bind(attempt_number)
            .bind(failure_code)
            .bind(now.to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
    }
    Ok(())
}

async fn insert_candidate_tx(
    connection: &mut SqliteConnection,
    candidate: &ExtractionCandidate,
) -> Result<Uuid, ProcessingRepositoryError> {
    let payload =
        serde_json::to_string(candidate).map_err(|_| ProcessingRepositoryError::Failed)?;
    let evidence = serde_json::to_string(&candidate.evidence)
        .map_err(|_| ProcessingRepositoryError::Failed)?;
    sqlx::query("INSERT INTO document_extraction_candidates (id, tenant_id, job_id, schema_version, payload, evidence, provider, model, prompt_version, version, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT(tenant_id, job_id) DO UPDATE SET payload = excluded.payload, evidence = excluded.evidence, provider = excluded.provider, model = excluded.model, prompt_version = excluded.prompt_version")
        .bind(candidate.id().to_string())
        .bind(candidate.tenant_id().to_string())
        .bind(candidate.job_id().to_string())
        .bind(&candidate.schema_version)
        .bind(payload)
        .bind(evidence)
        .bind(&candidate.provider)
        .bind(&candidate.model)
        .bind(&candidate.prompt_version)
        .bind(candidate.version())
        .bind(candidate.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(map_sql_error)?;
    Ok(candidate.id())
}

async fn load_ai_task_for_update(
    connection: &mut SqliteConnection,
    tenant_id: Uuid,
    task_id: Uuid,
) -> Result<Option<AiTask>, ProcessingRepositoryError> {
    let query =
        format!("SELECT {AI_TASK_COLUMNS} FROM document_ai_tasks WHERE tenant_id = ?1 AND id = ?2");
    sqlx::query_as::<_, AiTaskRow>(&query)
        .bind(tenant_id.to_string())
        .bind(task_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sql_error)?
        .map(to_ai_task)
        .transpose()
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl ProcessingExecutionUnitOfWork for SqliteProcessingStore {
    async fn create_job(
        &self,
        job: &ProcessingJob,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        ProcessingJobCommandPort::create(self, job).await
    }

    async fn claim_next_job(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError> {
        ProcessingJobClaimPort::claim_next(self, worker_id, now, lease_duration_secs).await
    }

    async fn claim_next_ai_task(
        &self,
        worker_id: &str,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<Option<AiTask>, ProcessingRepositoryError> {
        legacy::AiTaskPort::claim_next(self, worker_id, now, lease_duration_secs).await
    }

    async fn start_step(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        expected_step: ProcessingStepKind,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        job.start_step(
            &fence.worker_id,
            &fence.lease_token,
            fence.fence_version,
            expected_step,
            now,
        )
        .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job_fenced(&mut connection, &job, expected, fence, now).await?;
        write_step_started(
            &mut connection,
            &StepCheckpoint {
                job_id,
                tenant_id,
                step_kind: expected_step,
                attempt_number: job.attempt_count(),
                checkpoint_json: serde_json::json!({}),
                updated_at: now,
            },
        )
        .await?;
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.started.v1",
            serde_json::json!({"step": expected_step.as_str()}),
            now,
        )
        .await?;
        insert_processing_audit(
            &mut connection,
            &job,
            "step_started",
            None,
            serde_json::json!({"step": expected_step.as_str()}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(job)
    }

    async fn complete_step(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        completed_step: ProcessingStepKind,
        checkpoint: Option<StepCheckpoint>,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        let attempt_number = job.attempt_count();
        if job.cancel_requested_at().is_some() {
            job.cancel(
                Some((&fence.worker_id, &fence.lease_token, fence.fence_version)),
                now,
            )
            .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
            save_job_fenced(&mut connection, &job, expected, fence, now).await?;
            insert_processing_outbox(
                &mut connection,
                &job,
                "document.processing.cancelled.v1",
                serde_json::json!({"step": completed_step.as_str()}),
                now,
            )
            .await?;
            commit_immediate(&mut connection).await?;
            return Ok(job);
        }
        job.complete_step(
            &fence.worker_id,
            &fence.lease_token,
            fence.fence_version,
            completed_step,
            now,
        )
        .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job_fenced(&mut connection, &job, expected, fence, now).await?;
        write_step_completed(
            &mut connection,
            tenant_id,
            job_id,
            completed_step,
            attempt_number,
            checkpoint.as_ref(),
            now,
        )
        .await?;
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.step-completed.v1",
            serde_json::json!({"step": completed_step.as_str()}),
            now,
        )
        .await?;
        if job.status() == ProcessingJobStatus::WaitingForReview {
            insert_processing_outbox(
                &mut connection,
                &job,
                "document.processing.waiting-for-review.v1",
                serde_json::json!({}),
                now,
            )
            .await?;
        }
        insert_processing_audit(
            &mut connection,
            &job,
            "step_completed",
            None,
            serde_json::json!({"step": completed_step.as_str()}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(job)
    }

    async fn retry_or_fail_step(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        step: ProcessingStepKind,
        failure: ClassifiedProcessingFailure,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        let attempt_number = job.attempt_count();
        let disposition = failure.disposition.clone();
        match failure.disposition {
            ProcessingFailureDisposition::Retry { backoff } => {
                job.fail_transient(
                    &fence.worker_id,
                    &fence.lease_token,
                    fence.fence_version,
                    failure.code.clone(),
                    failure.message.clone(),
                    now,
                    backoff,
                )
                .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
            }
            ProcessingFailureDisposition::Permanent => {
                job.fail_permanent(
                    &fence.worker_id,
                    &fence.lease_token,
                    fence.fence_version,
                    failure.code.clone(),
                    failure.message.clone(),
                    now,
                )
                .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
            }
            ProcessingFailureDisposition::Cancelled => {
                job.cancel(
                    Some((&fence.worker_id, &fence.lease_token, fence.fence_version)),
                    now,
                )
                .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
            }
            ProcessingFailureDisposition::LeaseLost => {
                return Err(ProcessingRepositoryError::LeaseLost)
            }
        }
        save_job_fenced(&mut connection, &job, expected, fence, now).await?;
        write_step_failed(
            &mut connection,
            tenant_id,
            job_id,
            step,
            attempt_number,
            &failure.code,
            now,
        )
        .await?;
        insert_processing_outbox(
            &mut connection,
            &job,
            if matches!(&disposition, ProcessingFailureDisposition::Retry { .. }) {
                "document.processing.retry-scheduled.v1"
            } else if job.status() == ProcessingJobStatus::Cancelled {
                "document.processing.cancelled.v1"
            } else if job.status() == ProcessingJobStatus::Failed {
                "document.processing.failed.v1"
            } else {
                "document.processing.step-completed.v1"
            },
            serde_json::json!({"step": step.as_str(), "failure_code": failure.code, "disposition": format!("{disposition:?}")}),
            now,
        )
        .await?;
        insert_processing_audit(
            &mut connection,
            &job,
            "step_failed",
            None,
            serde_json::json!({"step": step.as_str()}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(job)
    }

    async fn enqueue_ai_and_wait(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        text_artifact: TextArtifactReference,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<AiTask, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if job.current_step() != ProcessingStepKind::ExtractText {
            return Err(ProcessingRepositoryError::Conflict);
        }
        let expected = job.aggregate_version().value();
        job.complete_step(
            &fence.worker_id,
            &fence.lease_token,
            fence.fence_version,
            ProcessingStepKind::ExtractText,
            now,
        )
        .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        job.wait_for_ai(
            &fence.worker_id,
            &fence.lease_token,
            fence.fence_version,
            now,
        )
        .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        let task = AiTask {
            id: Uuid::now_v7(),
            tenant_id,
            job_id,
            step_kind: ProcessingStepKind::ExtractFields,
            status: "queued".to_string(),
            input_artifact_id: Some(text_artifact.key.clone()),
            attempt_count: 0,
            max_attempts: job.max_attempts(),
            next_attempt_at: now,
            cancel_requested_at: None,
            lease_owner: None,
            lease_token: None,
            fence_version: 0,
            lease_expires_at: None,
            output_candidate_id: None,
        };
        save_job_fenced(&mut connection, &job, expected, fence, now).await?;
        write_step_completed(
            &mut connection,
            tenant_id,
            job_id,
            ProcessingStepKind::ExtractText,
            job.attempt_count(),
            Some(&StepCheckpoint {
                job_id,
                tenant_id,
                step_kind: ProcessingStepKind::ExtractText,
                attempt_number: job.attempt_count(),
                checkpoint_json: serde_json::json!({
                    "content_hash": text_artifact.content_hash,
                    "content_revision": text_artifact.content_revision,
                    "byte_count": text_artifact.byte_count,
                    "line_count": text_artifact.line_count,
                    "character_count": text_artifact.character_count,
                    "text_artifact_reference": text_artifact.key,
                }),
                updated_at: now,
            }),
            now,
        )
        .await?;
        sqlx::query("INSERT INTO document_ai_tasks (id, tenant_id, job_id, step_kind, status, input_artifact_id, attempt_count, max_attempts, next_attempt_at, fence_version, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, 0, ?6, ?7, 0, ?7, ?7) ON CONFLICT(tenant_id, job_id, step_kind, attempt_count) DO NOTHING")
            .bind(task.id.to_string())
            .bind(task.tenant_id.to_string())
            .bind(task.job_id.to_string())
            .bind(task.step_kind.as_str())
            .bind(&task.input_artifact_id)
            .bind(task.max_attempts)
            .bind(now.to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.waiting-for-ai.v1",
            serde_json::json!({"task_id": task.id, "step": "extract_fields"}),
            now,
        )
        .await?;
        insert_processing_audit(
            &mut connection,
            &job,
            "ai_task_enqueued",
            None,
            serde_json::json!({"task_id": task.id}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(task)
    }

    async fn complete_ai_and_resume(
        &self,
        completion: CompleteAiTaskCommand,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(task) =
            load_ai_task_for_update(&mut connection, completion.tenant_id, completion.task_id)
                .await?
        else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if task.job_id != completion.job_id
            || task.status != "running"
            || task.lease_owner.as_deref() != Some(&completion.fence.worker_id)
            || task.lease_token.as_deref() != Some(&completion.fence.lease_token)
            || task.fence_version != completion.fence.fence_version
            || task.cancel_requested_at.is_some()
            || task.lease_expires_at.is_none_or(|expires| expires <= now)
        {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        let Some(mut job) =
            load_job_for_update(&mut connection, completion.tenant_id, completion.job_id).await?
        else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if job.status() != ProcessingJobStatus::WaitingForAi
            || job.current_step() != ProcessingStepKind::ExtractFields
            || job.cancel_requested_at().is_some()
        {
            return Err(ProcessingRepositoryError::Conflict);
        }
        if completion.candidate.tenant_id() != completion.tenant_id
            || completion.candidate.job_id() != completion.job_id
        {
            return Err(ProcessingRepositoryError::TenantMismatch);
        }
        let candidate_id = insert_candidate_tx(&mut connection, &completion.candidate).await?;
        let expected = job.aggregate_version().value();
        job.resume_after_ai(now)
            .map_err(|_| ProcessingRepositoryError::Conflict)?;
        save_job_without_fence(&mut connection, &job, expected).await?;
        write_step_completed(
            &mut connection,
            completion.tenant_id,
            completion.job_id,
            ProcessingStepKind::ExtractFields,
            task.attempt_count,
            None,
            now,
        )
        .await?;
        let result = sqlx::query("UPDATE document_ai_tasks SET status = 'succeeded', output_candidate_id = ?1, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND lease_owner = ?5 AND lease_token = ?6 AND fence_version = ?7 AND lease_expires_at > ?2")
            .bind(candidate_id.to_string())
            .bind(now.to_rfc3339())
            .bind(completion.tenant_id.to_string())
            .bind(completion.task_id.to_string())
            .bind(&completion.fence.worker_id)
            .bind(&completion.fence.lease_token)
            .bind(completion.fence.fence_version)
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.step-completed.v1",
            serde_json::json!({"step": "extract_fields", "candidate_id": candidate_id}),
            now,
        )
        .await?;
        insert_processing_audit(
            &mut connection,
            &job,
            "ai_task_completed",
            None,
            serde_json::json!({"task_id": completion.task_id, "candidate_id": candidate_id}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(job)
    }

    async fn fail_ai_task(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        task_id: Uuid,
        failure: ClassifiedProcessingFailure,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<AiTask, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(task) = load_ai_task_for_update(&mut connection, tenant_id, task_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if task.job_id != job_id
            || task.status != "running"
            || task.lease_owner.as_deref() != Some(&fence.worker_id)
            || task.lease_token.as_deref() != Some(&fence.lease_token)
            || task.fence_version != fence.fence_version
            || task.cancel_requested_at.is_some()
            || task.lease_expires_at.is_none_or(|expires| expires <= now)
        {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        let (status, next_attempt_at) = match failure.disposition {
            ProcessingFailureDisposition::Retry { backoff }
                if task.attempt_count < task.max_attempts =>
            {
                ("queued", now + backoff)
            }
            ProcessingFailureDisposition::Retry { .. }
            | ProcessingFailureDisposition::Permanent => ("failed", now),
            ProcessingFailureDisposition::Cancelled => ("cancelled", now),
            ProcessingFailureDisposition::LeaseLost => {
                return Err(ProcessingRepositoryError::LeaseLost)
            }
        };
        let result = sqlx::query("UPDATE document_ai_tasks SET status = ?1, failure_code = ?2, cancel_requested_at = CASE WHEN ?1 = 'cancelled' THEN COALESCE(cancel_requested_at, ?3) ELSE cancel_requested_at END, next_attempt_at = ?3, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND status = 'running' AND lease_owner = ?6 AND lease_token = ?7 AND fence_version = ?8 AND lease_expires_at > ?3")
            .bind(status)
            .bind(&failure.code)
            .bind(next_attempt_at.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(task_id.to_string())
            .bind(&fence.worker_id)
            .bind(&fence.lease_token)
            .bind(fence.fence_version)
            .execute(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if status == "failed" || status == "cancelled" {
            sqlx::query("UPDATE document_processing_jobs SET status = ?1, cancel_requested_at = CASE WHEN ?1 = 'cancelled' THEN COALESCE(cancel_requested_at, ?2) ELSE cancel_requested_at END, failure_code = ?3, failure_message = ?4, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, version = version + 1, updated_at = ?2 WHERE tenant_id = ?5 AND id = ?6 AND status = 'waiting_for_ai'")
                .bind(status)
                .bind(now.to_rfc3339())
                .bind(&failure.code)
                .bind(failure.message.as_deref())
                .bind(tenant_id.to_string())
                .bind(job_id.to_string())
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            job = load_job_for_update(&mut connection, tenant_id, job_id)
                .await?
                .ok_or(ProcessingRepositoryError::NotFound)?;
            insert_processing_outbox(
                &mut connection,
                &job,
                if status == "cancelled" {
                    "document.processing.cancelled.v1"
                } else {
                    "document.processing.failed.v1"
                },
                serde_json::json!({"task_id": task_id, "failure_code": failure.code}),
                now,
            )
            .await?;
        } else {
            insert_processing_outbox(
                &mut connection,
                &job,
                "document.processing.retry-scheduled.v1",
                serde_json::json!({"task_id": task_id, "failure_code": failure.code, "next_attempt_at": next_attempt_at}),
                now,
            )
            .await?;
        }
        insert_processing_audit(
            &mut connection,
            &job,
            "ai_task_failed",
            None,
            serde_json::json!({"task_id": task_id, "failure_code": failure.code, "status": status}),
            now,
        )
        .await?;
        let updated = load_ai_task_for_update(&mut connection, tenant_id, task_id)
            .await?
            .ok_or(ProcessingRepositoryError::NotFound)?;
        commit_immediate(&mut connection).await?;
        Ok(updated)
    }

    async fn save_candidate_and_wait_for_review(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        candidate: &ExtractionCandidate,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        if candidate.tenant_id() != tenant_id || candidate.job_id() != job_id {
            return Err(ProcessingRepositoryError::TenantMismatch);
        }
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if job.current_step() != ProcessingStepKind::ValidateCandidate {
            return Err(ProcessingRepositoryError::Conflict);
        }
        let expected = job.aggregate_version().value();
        let attempt_number = job.attempt_count();
        insert_candidate_tx(&mut connection, candidate).await?;
        job.wait_for_review(
            &fence.worker_id,
            &fence.lease_token,
            fence.fence_version,
            now,
        )
        .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job_fenced(&mut connection, &job, expected, fence, now).await?;
        write_step_completed(
            &mut connection,
            tenant_id,
            job_id,
            ProcessingStepKind::ValidateCandidate,
            attempt_number,
            None,
            now,
        )
        .await?;
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.waiting-for-review.v1",
            serde_json::json!({"candidate_id": candidate.id()}),
            now,
        )
        .await?;
        insert_processing_audit(
            &mut connection,
            &job,
            "candidate_waiting_for_review",
            None,
            serde_json::json!({"candidate_id": candidate.id()}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(job)
    }

    async fn finalize_review(
        &self,
        command: FinalizeReviewCommand,
        now: DateTime<Utc>,
    ) -> Result<FinalizeReviewResult, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let result = async {
            let Some(mut job) =
                load_job_for_update(&mut connection, command.tenant_id, command.job_id).await?
            else {
                return Err(ProcessingRepositoryError::NotFound);
            };
            if command.idempotency_key.trim().is_empty()
                || command.idempotency_key.len() > 255
                || command.request_fingerprint.len() != 64
                || !command
                    .request_fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ProcessingRepositoryError::IdempotencyConflict);
            }
            if let Some(existing) = sqlx::query_as::<_, ReviewRow>("SELECT id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at, idempotency_key, request_fingerprint FROM document_extraction_reviews WHERE tenant_id = ?1 AND idempotency_key = ?2")
                .bind(command.tenant_id.to_string())
                .bind(&command.idempotency_key)
                .fetch_optional(&mut *connection)
                .await
                .map_err(map_sql_error)?
            {
                if existing.request_fingerprint != command.request_fingerprint {
                    return Err(ProcessingRepositoryError::IdempotencyConflict);
                }
                let existing = to_review(existing)?;
                return Ok(FinalizeReviewResult {
                    job,
                    review: existing,
                    replayed: true,
                });
            }
            let candidate_row = sqlx::query_as::<_, CandidateRow>(
                "SELECT payload FROM document_extraction_candidates WHERE tenant_id = ?1 AND id = ?2",
            )
            .bind(command.tenant_id.to_string())
            .bind(command.review.candidate_id.to_string())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?
            .ok_or(ProcessingRepositoryError::NotFound)?;
            let candidate: ExtractionCandidate = serde_json::from_str(&candidate_row.payload)
                .map_err(|_| ProcessingRepositoryError::Failed)?;
            if candidate.job_id() != command.job_id
                || command.review.tenant_id != command.tenant_id
            {
                return Err(ProcessingRepositoryError::Conflict);
            }
            if let Some(existing) = sqlx::query_as::<_, ReviewRow>("SELECT id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at, idempotency_key, request_fingerprint FROM document_extraction_reviews WHERE tenant_id = ?1 AND candidate_id = ?2")
                .bind(command.tenant_id.to_string())
                .bind(command.review.candidate_id.to_string())
                .fetch_optional(&mut *connection)
                .await
                .map_err(map_sql_error)?
            {
                let existing = to_review(existing)?;
                if existing.reviewer_id == command.review.reviewer_id
                    && existing.decision == command.review.decision
                    && existing.candidate_version == command.review.candidate_version
                    && existing.patch == command.review.patch
                    && existing.comment == command.review.comment
                {
                    return Ok(FinalizeReviewResult {
                        job,
                        review: existing,
                        replayed: true,
                    });
                }
                return Err(ProcessingRepositoryError::Conflict);
            }
            if job.status() != ProcessingJobStatus::WaitingForReview {
                return Err(ProcessingRepositoryError::Conflict);
            }
            command
                .review
                .validate(&candidate)
                .map_err(|_| ProcessingRepositoryError::Conflict)?;
            let expected = job.aggregate_version().value();
            match command.review.decision {
                document_processing::ReviewDecision::Rejected => job
                    .reject_review(now)
                    .map_err(|_| ProcessingRepositoryError::Conflict)?,
                document_processing::ReviewDecision::Accepted
                | document_processing::ReviewDecision::Edited => job
                    .confirm_review(now)
                    .map_err(|_| ProcessingRepositoryError::Conflict)?,
            }
            save_job_without_fence(&mut connection, &job, expected).await?;
            sqlx::query("INSERT INTO document_extraction_reviews (id, tenant_id, candidate_id, reviewer_id, decision, patch, comment, candidate_version, created_at, idempotency_key, request_fingerprint) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)")
                .bind(command.review.id.to_string())
                .bind(command.review.tenant_id.to_string())
                .bind(command.review.candidate_id.to_string())
                .bind(command.review.reviewer_id.to_string())
                .bind(command.review.decision.as_str())
                .bind(command.review.patch.as_ref().map(ToString::to_string))
                .bind(&command.review.comment)
                .bind(command.review.candidate_version)
                .bind(command.review.created_at.to_rfc3339())
                .bind(&command.idempotency_key)
                .bind(&command.request_fingerprint)
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
            insert_processing_outbox(
                &mut connection,
                &job,
                if command.review.decision == document_processing::ReviewDecision::Rejected {
                    "document.processing.failed.v1"
                } else {
                    "document.processing.succeeded.v1"
                },
                serde_json::json!({"review_id": command.review.id, "decision": command.review.decision.as_str()}),
                now,
            )
            .await?;
            insert_processing_audit(
                &mut connection,
                &job,
                "review_finalized",
                Some(command.review.reviewer_id),
                serde_json::json!({"review_id": command.review.id, "decision": command.review.decision.as_str()}),
                now,
            )
            .await?;
            Ok(FinalizeReviewResult {
                job,
                review: command.review,
                replayed: false,
            })
        }
        .await;
        match result {
            Ok(result) => {
                commit_immediate(&mut connection).await?;
                Ok(result)
            }
            Err(error) => match SqliteTransactionManager::rollback(&mut *connection).await {
                Ok(()) => Err(error),
                Err(_) => Err(ProcessingRepositoryError::Failed),
            },
        }
    }

    async fn cancel_processing(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        requested_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<ProcessingJob, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        if job.status().is_terminal() {
            commit_immediate(&mut connection).await?;
            return Ok(job);
        }
        if job.cancel_requested_at().is_some() {
            commit_immediate(&mut connection).await?;
            return Ok(job);
        }
        let expected = job.aggregate_version().value();
        job.request_cancel(now)
            .map_err(|_| ProcessingRepositoryError::Failed)?;
        if job.status() == ProcessingJobStatus::WaitingForAi {
            job.cancel(None, now)
                .map_err(|_| ProcessingRepositoryError::Failed)?;
            sqlx::query("UPDATE document_ai_tasks SET status = 'cancelled', cancel_requested_at = COALESCE(cancel_requested_at, ?1), lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND job_id = ?3 AND status IN ('queued', 'running')")
                .bind(now.to_rfc3339())
                .bind(tenant_id.to_string())
                .bind(job_id.to_string())
                .execute(&mut *connection)
                .await
                .map_err(map_sql_error)?;
        }
        save_job_without_fence(&mut connection, &job, expected).await?;
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.cancelled.v1",
            serde_json::json!({"requested_by": requested_by}),
            now,
        )
        .await?;
        insert_processing_audit(
            &mut connection,
            &job,
            "processing_cancelled",
            Some(requested_by),
            serde_json::json!({}),
            now,
        )
        .await?;
        commit_immediate(&mut connection).await?;
        Ok(job)
    }

    async fn heartbeat_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
        if lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE document_processing_jobs SET lease_expires_at = ?1, updated_at = ?2, version = version + 1 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND cancel_requested_at IS NULL AND lease_owner = ?5 AND lease_token = ?6 AND fence_version = ?7 AND lease_expires_at > ?2")
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(job_id.to_string())
            .bind(&fence.worker_id)
            .bind(&fence.lease_token)
            .bind(fence.fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(expires_at)
    }

    async fn release_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
    ) -> Result<(), ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let Some(mut job) = load_job_for_update(&mut connection, tenant_id, job_id).await? else {
            return Err(ProcessingRepositoryError::NotFound);
        };
        let expected = job.aggregate_version().value();
        job.release(
            &fence.worker_id,
            &fence.lease_token,
            fence.fence_version,
            now,
        )
        .map_err(|_| ProcessingRepositoryError::LeaseLost)?;
        save_job_fenced(&mut connection, &job, expected, fence, now).await?;
        let cancelled = job.status() == ProcessingJobStatus::Cancelled;
        insert_processing_audit(
            &mut connection,
            &job,
            if cancelled {
                "processing_cancelled"
            } else {
                "job_released"
            },
            None,
            serde_json::json!({}),
            now,
        )
        .await?;
        if cancelled {
            insert_processing_outbox(
                &mut connection,
                &job,
                "document.processing.cancelled.v1",
                serde_json::json!({}),
                now,
            )
            .await?;
        }
        commit_immediate(&mut connection).await?;
        Ok(())
    }

    async fn reclaim_expired_jobs(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let rows = sqlx::query_as::<_, JobRow>(&format!(
            "SELECT {JOB_COLUMNS} FROM document_processing_jobs WHERE lease_expires_at IS NOT NULL AND lease_expires_at <= ?1"
        ))
        .bind(now.to_rfc3339())
        .fetch_all(&mut *connection)
        .await
        .map_err(map_sql_error)?;
        let mut reclaimed = 0_u64;
        for row in rows {
            let mut job = to_job(row)?;
            let expected = job.aggregate_version().value();
            if job
                .reclaim_expired(now)
                .map_err(|_| ProcessingRepositoryError::Failed)?
            {
                save_job_without_fence(&mut connection, &job, expected).await?;
                insert_processing_audit(
                    &mut connection,
                    &job,
                    "job_reclaimed",
                    None,
                    serde_json::json!({}),
                    now,
                )
                .await?;
                reclaimed = reclaimed.saturating_add(1);
            }
        }
        commit_immediate(&mut connection).await?;
        Ok(reclaimed)
    }

    async fn heartbeat_ai_task(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
        fence: &ExecutionFence,
        now: DateTime<Utc>,
        lease_duration_secs: i64,
    ) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
        if lease_duration_secs <= 0 {
            return Err(ProcessingRepositoryError::Failed);
        }
        let expires_at = now + Duration::seconds(lease_duration_secs);
        let result = sqlx::query("UPDATE document_ai_tasks SET lease_expires_at = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND lease_owner = ?5 AND lease_token = ?6 AND fence_version = ?7 AND lease_expires_at > ?2 AND EXISTS (SELECT 1 FROM document_processing_jobs j WHERE j.id = document_ai_tasks.job_id AND j.tenant_id = document_ai_tasks.tenant_id AND j.status = 'waiting_for_ai' AND j.cancel_requested_at IS NULL)")
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(task_id.to_string())
            .bind(&fence.worker_id)
            .bind(&fence.lease_token)
            .bind(fence.fence_version)
            .execute(&self.pool)
            .await
            .map_err(map_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(ProcessingRepositoryError::LeaseLost);
        }
        Ok(expires_at)
    }

    async fn reclaim_expired_ai_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, ProcessingRepositoryError> {
        let mut connection = begin_immediate(&self.pool).await?;
        let query = format!("SELECT {AI_TASK_COLUMNS} FROM document_ai_tasks WHERE status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?1");
        let rows = sqlx::query_as::<_, AiTaskRow>(&query)
            .bind(now.to_rfc3339())
            .fetch_all(&mut *connection)
            .await
            .map_err(map_sql_error)?;
        let mut reclaimed = 0_u64;
        for row in rows {
            let task = to_ai_task(row)?;
            let (job_status, job_cancelled) = sqlx::query_as::<_, (String, i64)>(
                "SELECT status, CASE WHEN status = 'cancelled' OR cancel_requested_at IS NOT NULL THEN 1 ELSE 0 END FROM document_processing_jobs WHERE tenant_id = ?1 AND id = ?2",
            )
            .bind(task.tenant_id.to_string())
            .bind(task.job_id.to_string())
            .fetch_optional(&mut *connection)
            .await
            .map_err(map_sql_error)?
            .ok_or(ProcessingRepositoryError::NotFound)?;
            let cancelled = task.cancel_requested_at.is_some() || job_cancelled != 0;
            if cancelled {
                let updated = sqlx::query("UPDATE document_ai_tasks SET status = 'cancelled', cancel_requested_at = COALESCE(cancel_requested_at, ?1), lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, next_attempt_at = ?1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'running' AND fence_version = ?4")
                    .bind(now.to_rfc3339())
                    .bind(task.tenant_id.to_string())
                    .bind(task.id.to_string())
                    .bind(task.fence_version)
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sql_error)?;
                if updated.rows_affected() != 1 {
                    return Err(ProcessingRepositoryError::LeaseLost);
                }
                // The cancellation command owns the Job + AI Task atomic
                // transition. Reclaim only fences the stale task here so it
                // cannot acquire the Job row in the opposite lock order.
            } else if job_status != "waiting_for_ai" {
                return Err(ProcessingRepositoryError::Conflict);
            } else if task.attempt_count < task.max_attempts {
                sqlx::query("UPDATE document_ai_tasks SET status = 'queued', lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, next_attempt_at = ?1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'running' AND fence_version = ?4")
                    .bind(now.to_rfc3339())
                    .bind(task.tenant_id.to_string())
                    .bind(task.id.to_string())
                    .bind(task.fence_version)
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sql_error)?;
            } else {
                sqlx::query("UPDATE document_ai_tasks SET status = 'failed', failure_code = 'ai_provider_unavailable', lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'running' AND fence_version = ?4")
                    .bind(now.to_rfc3339())
                    .bind(task.tenant_id.to_string())
                    .bind(task.id.to_string())
                    .bind(task.fence_version)
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sql_error)?;
                sqlx::query("UPDATE document_processing_jobs SET status = 'failed', failure_code = 'ai_provider_unavailable', version = version + 1, updated_at = ?1, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL WHERE tenant_id = ?2 AND id = ?3 AND status = 'waiting_for_ai' AND cancel_requested_at IS NULL")
                    .bind(now.to_rfc3339())
                    .bind(task.tenant_id.to_string())
                    .bind(task.job_id.to_string())
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sql_error)?;
            }
            reclaimed = reclaimed
                .checked_add(1)
                .ok_or(ProcessingRepositoryError::Failed)?;
        }
        commit_immediate(&mut connection).await?;
        Ok(reclaimed)
    }
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl ProcessingRepairPort for SqliteProcessingStore {
    async fn preview_reconcile_processing_job(
        &self,
        command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        self.preview_processing_repair(command, "reconcile processing review", |job| {
            job.status() == ProcessingJobStatus::WaitingForReview
                && job.current_step() == ProcessingStepKind::AwaitReview
        })
        .await
    }

    async fn preview_requeue_missing_ai_task(
        &self,
        command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        self.preview_processing_repair(command, "requeue missing extract-fields AI task", |job| {
            job.status() == ProcessingJobStatus::WaitingForAi
                && job.current_step() == ProcessingStepKind::ExtractFields
        })
        .await
    }

    async fn preview_clear_terminal_job_lease(
        &self,
        command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        self.preview_processing_repair(command, "clear terminal job lease", |job| {
            matches!(
                job.status(),
                ProcessingJobStatus::Succeeded
                    | ProcessingJobStatus::Rejected
                    | ProcessingJobStatus::Failed
                    | ProcessingJobStatus::Cancelled
            ) && job.lease_snapshot().is_some()
        })
        .await
    }

    async fn preview_rebuild_processing_step_projection(
        &self,
        command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        self.preview_processing_repair(command, "rebuild await-review step projection", |job| {
            job.status() == ProcessingJobStatus::WaitingForReview
                && job.current_step() == ProcessingStepKind::AwaitReview
        })
        .await
    }

    async fn preview_reconcile_ai_completion(
        &self,
        command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        self.preview_processing_repair(command, "reconcile completed AI task", |job| {
            job.status() == ProcessingJobStatus::WaitingForAi
                && job.current_step() == ProcessingStepKind::ExtractFields
        })
        .await
    }

    async fn verify_repair(
        &self,
        command: &RepairCommand,
        result: &RepairResult,
    ) -> Result<RepairVerification, RepairError> {
        if !matches!(
            result.outcome,
            RepairOutcome::Succeeded | RepairOutcome::Noop
        ) {
            return Ok(RepairVerification {
                valid: false,
                message: "owner reported a non-success outcome".to_string(),
            });
        }
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|_| RepairError::Unavailable)?;
        let Some(job) = load_job(&mut connection, command.tenant_id, command.target.uuid()?)
            .await
            .map_err(|_| RepairError::Persistence)?
        else {
            return Ok(RepairVerification {
                valid: false,
                message: "owner resource no longer exists".to_string(),
            });
        };
        let valid = match command.repair_type.as_str() {
            "requeue_missing_ai_task.v1" => {
                let active: Option<String> = sqlx::query_scalar("SELECT id FROM document_ai_tasks WHERE tenant_id=?1 AND job_id=?2 AND status IN ('queued','running','retry_scheduled') LIMIT 1")
                    .bind(command.tenant_id.to_string())
                    .bind(job.id().to_string())
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                active.is_some()
            }
            "clear_terminal_job_lease.v1" => job.lease_snapshot().is_none(),
            "reconcile_processing_job.v1" => {
                job.current_step() == ProcessingStepKind::AwaitReview
                    && !matches!(job.status(), ProcessingJobStatus::WaitingForReview)
            }
            "rebuild_processing_step_projection.v1" => {
                let validate_candidate: Option<i64> = sqlx::query_scalar(
                    "SELECT (EXISTS (SELECT 1 FROM document_processing_steps WHERE tenant_id=?1 AND job_id=?2 AND step_kind='validate_candidate' AND status='succeeded')) AND (EXISTS (SELECT 1 FROM document_processing_steps WHERE tenant_id=?1 AND job_id=?2 AND step_kind='await_review' AND status='pending'))",
                )
                .bind(command.tenant_id.to_string())
                .bind(job.id().to_string())
                .fetch_one(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
                matches!(job.status(), ProcessingJobStatus::WaitingForReview)
                    && job.current_step() == ProcessingStepKind::AwaitReview
                    && validate_candidate.unwrap_or(0) != 0
            }
            "reconcile_ai_completion.v1" => {
                job.current_step() == ProcessingStepKind::ValidateCandidate
                    && matches!(job.status(), ProcessingJobStatus::Queued)
            }
            _ => false,
        };
        Ok(RepairVerification {
            valid,
            message: if valid {
                "processing integrity rule passed after re-read".to_string()
            } else {
                "processing integrity rule still reports the finding".to_string()
            },
        })
    }

    async fn reconcile_processing_job(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        ensure_live_repair_context(context)?;
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let Some(job) =
            load_job_for_update(&mut connection, command.tenant_id, command.target.uuid()?)
                .await
                .map_err(|_| RepairError::Persistence)?
        else {
            return Err(RepairError::Conflict);
        };
        let before = job.aggregate_version().value();
        if command
            .target
            .expected_resource_version
            .is_some_and(|expected| expected != before)
        {
            return Err(RepairError::Conflict);
        }
        let decision = sqlx::query_scalar::<_, String>(
            "SELECT r.decision FROM document_extraction_reviews r JOIN document_extraction_candidates c ON c.id=r.candidate_id AND c.tenant_id=r.tenant_id WHERE r.tenant_id=?1 AND c.job_id=?2 ORDER BY r.created_at DESC LIMIT 1",
        )
        .bind(command.tenant_id.to_string())
        .bind(command.target.uuid()?.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?
        .ok_or(RepairError::Conflict)?;
        let status = if decision == "rejected" {
            "rejected"
        } else {
            "succeeded"
        };
        let updated = sqlx::query("UPDATE document_processing_jobs SET status=?1,current_step='await_review',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,version=version+1,updated_at=?2 WHERE tenant_id=?3 AND id=?4 AND status='waiting_for_review' AND version=?5")
            .bind(status)
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .bind(before)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            return Err(RepairError::Conflict);
        }
        insert_processing_outbox(
            &mut connection,
            &job,
            "processing.repair_review_reconciled",
            serde_json::json!({ "job_id": job.id(), "decision": decision }),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        insert_processing_audit(
            &mut connection,
            &job,
            "repair_review_reconciled",
            Some(command.requested_by),
            serde_json::json!({ "decision": decision }),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(RepairResult {
            command_id: Uuid::now_v7(),
            resource_version_before: Some(before),
            resource_version_after: Some(before.saturating_add(1)),
            before_hash: state_hash(format!("processing-job:{before}")),
            after_hash: state_hash(format!("processing-job:{}", before.saturating_add(1))),
            rows_affected: 1,
            outcome: RepairOutcome::Succeeded,
        })
    }

    async fn requeue_missing_ai_task(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        ensure_live_repair_context(context)?;
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let Some(job) =
            load_job_for_update(&mut connection, command.tenant_id, command.target.uuid()?)
                .await
                .map_err(|_| RepairError::Persistence)?
        else {
            return Err(RepairError::Conflict);
        };
        let before = job.aggregate_version().value();
        if command
            .target
            .expected_resource_version
            .is_some_and(|expected| expected != before)
            || job.status() != ProcessingJobStatus::WaitingForAi
            || job.current_step() != ProcessingStepKind::ExtractFields
        {
            return Err(RepairError::Conflict);
        }
        let active = sqlx::query_scalar::<_, String>(
            "SELECT id FROM document_ai_tasks WHERE tenant_id=?1 AND job_id=?2 AND status IN ('queued','running','retry_scheduled') LIMIT 1",
        )
        .bind(command.tenant_id.to_string())
        .bind(command.target.uuid()?.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?;
        if active.is_some() {
            commit_immediate(&mut connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            return Ok(repair_result(before, before, 0, RepairOutcome::Noop));
        }
        let checkpoint = sqlx::query_scalar::<_, Option<String>>(
            "SELECT checkpoint_json FROM document_processing_steps WHERE tenant_id=?1 AND job_id=?2 AND step_kind='extract_text' AND status='succeeded' ORDER BY attempt_number DESC LIMIT 1",
        )
        .bind(command.tenant_id.to_string())
        .bind(command.target.uuid()?.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?
        .flatten()
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|_| RepairError::Persistence)?;
        let artifact =
            artifact_from_checkpoint(checkpoint.as_ref(), job.document_content_revision())?;
        let next_attempt = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(attempt_count) FROM document_ai_tasks WHERE tenant_id=?1 AND job_id=?2 AND step_kind='extract_fields'",
        )
        .bind(command.tenant_id.to_string())
        .bind(command.target.uuid()?.to_string())
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?
        .unwrap_or(-1)
        .saturating_add(1);
        if next_attempt >= i64::from(job.max_attempts()) {
            return Err(RepairError::Conflict);
        }
        let task_id = Uuid::now_v7();
        let inserted = sqlx::query("INSERT INTO document_ai_tasks (id,tenant_id,job_id,step_kind,status,input_artifact_id,attempt_count,max_attempts,next_attempt_at,fence_version,created_at,updated_at) VALUES (?1,?2,?3,'extract_fields','queued',?4,?5,?6,?7,0,?7,?7) ON CONFLICT(tenant_id,job_id,step_kind,attempt_count) DO NOTHING")
            .bind(task_id.to_string())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .bind(&artifact.key)
            .bind(next_attempt)
            .bind(job.max_attempts())
            .bind(context.now.to_rfc3339())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if inserted.rows_affected() != 1 {
            return Err(RepairError::Conflict);
        }
        let updated = sqlx::query("UPDATE document_processing_jobs SET version=version+1,updated_at=?1 WHERE tenant_id=?2 AND id=?3 AND status='waiting_for_ai' AND current_step='extract_fields' AND version=?4")
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .bind(before)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            return Err(RepairError::Conflict);
        }
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.waiting-for-ai.v1",
            serde_json::json!({"task_id": task_id, "step": "extract_fields"}),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        insert_processing_audit(
            &mut connection,
            &job,
            "ai_task_requeued",
            Some(command.requested_by),
            serde_json::json!({"task_id": task_id}),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(repair_result(
            before,
            before.saturating_add(1),
            1,
            RepairOutcome::Succeeded,
        ))
    }

    async fn clear_terminal_job_lease(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        ensure_live_repair_context(context)?;
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let before = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM document_processing_jobs WHERE tenant_id=?1 AND id=?2 AND status IN ('succeeded','rejected','failed','cancelled') AND (lease_owner IS NOT NULL OR lease_token IS NOT NULL)",
        )
        .bind(command.tenant_id.to_string())
        .bind(command.target.uuid()?.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?
        .ok_or(RepairError::Conflict)?;
        if command
            .target
            .expected_resource_version
            .is_some_and(|expected| expected != before)
        {
            return Err(RepairError::Conflict);
        }
        let updated = sqlx::query("UPDATE document_processing_jobs SET lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,version=version+1,updated_at=?1 WHERE tenant_id=?2 AND id=?3 AND version=?4")
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .bind(before)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            return Err(RepairError::Conflict);
        }
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(RepairResult {
            command_id: Uuid::now_v7(),
            resource_version_before: Some(before),
            resource_version_after: Some(before.saturating_add(1)),
            before_hash: state_hash(format!("processing-job:{before}")),
            after_hash: state_hash(format!("processing-job:{}", before.saturating_add(1))),
            rows_affected: 1,
            outcome: RepairOutcome::Succeeded,
        })
    }

    async fn rebuild_processing_step_projection(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        ensure_live_repair_context(context)?;
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let Some(job) =
            load_job_for_update(&mut connection, command.tenant_id, command.target.uuid()?)
                .await
                .map_err(|_| RepairError::Persistence)?
        else {
            return Err(RepairError::Conflict);
        };
        let before = job.aggregate_version().value();
        if command
            .target
            .expected_resource_version
            .is_some_and(|expected| expected != before)
            || job.status() != ProcessingJobStatus::WaitingForReview
            || job.current_step() != ProcessingStepKind::AwaitReview
        {
            return Err(RepairError::Conflict);
        }
        let checkpoint = serde_json::json!({
            "repaired_from": "processing_job",
            "status": "waiting_for_review"
        })
        .to_string();
        let updated = sqlx::query("UPDATE document_processing_steps SET status='pending',finished_at=NULL,checkpoint_json=COALESCE(checkpoint_json,?1),updated_at=?2 WHERE tenant_id=?3 AND job_id=?4 AND step_kind='await_review' AND status <> 'pending'")
            .bind(&checkpoint)
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() == 0 {
            let inserted = sqlx::query("INSERT INTO document_processing_steps (job_id,tenant_id,step_kind,status,attempt_number,finished_at,checkpoint_json,created_at,updated_at) VALUES (?1,?2,'await_review','pending',?3,NULL,?4,?5,?5) ON CONFLICT(job_id,step_kind,attempt_number) DO NOTHING")
                .bind(command.target.uuid()?.to_string())
                .bind(command.tenant_id.to_string())
                .bind(job.attempt_count())
                .bind(&checkpoint)
                .bind(context.now.to_rfc3339())
                .execute(&mut *connection)
                .await
                .map_err(|_| RepairError::Persistence)?;
            if inserted.rows_affected() == 0 {
                commit_immediate(&mut connection)
                    .await
                    .map_err(|_| RepairError::Persistence)?;
                return Ok(repair_result(before, before, 0, RepairOutcome::Noop));
            }
        }
        let updated_job = sqlx::query("UPDATE document_processing_jobs SET version=version+1,updated_at=?1 WHERE tenant_id=?2 AND id=?3 AND version=?4")
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .bind(before)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated_job.rows_affected() != 1 {
            return Err(RepairError::Conflict);
        }
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.step-projection-rebuilt.v1",
            serde_json::json!({"step":"await_review"}),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        insert_processing_audit(
            &mut connection,
            &job,
            "processing_step_projection_rebuilt",
            Some(command.requested_by),
            serde_json::json!({"step":"await_review"}),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(repair_result(
            before,
            before.saturating_add(1),
            1,
            RepairOutcome::Succeeded,
        ))
    }

    async fn reconcile_ai_completion(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        ensure_live_repair_context(context)?;
        let mut connection = begin_immediate(&self.pool)
            .await
            .map_err(|_| RepairError::Persistence)?;
        let Some(job) =
            load_job_for_update(&mut connection, command.tenant_id, command.target.uuid()?)
                .await
                .map_err(|_| RepairError::Persistence)?
        else {
            return Err(RepairError::Conflict);
        };
        let before = job.aggregate_version().value();
        if command
            .target
            .expected_resource_version
            .is_some_and(|expected| expected != before)
            || job.status() != ProcessingJobStatus::WaitingForAi
            || job.current_step() != ProcessingStepKind::ExtractFields
        {
            return Err(RepairError::Conflict);
        }
        let candidate = sqlx::query_scalar::<_, String>(
            "SELECT c.payload FROM document_ai_tasks a JOIN document_extraction_candidates c ON c.id=a.output_candidate_id AND c.tenant_id=a.tenant_id WHERE a.tenant_id=?1 AND a.job_id=?2 AND a.status='succeeded' ORDER BY a.updated_at DESC LIMIT 1",
        )
        .bind(command.tenant_id.to_string())
        .bind(command.target.uuid()?.to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| RepairError::Persistence)?
        .ok_or(RepairError::Conflict)?;
        let candidate: Value =
            serde_json::from_str(&candidate).map_err(|_| RepairError::Conflict)?;
        let candidate_revision = candidate
            .get("content_revision")
            .and_then(Value::as_i64)
            .ok_or(RepairError::Conflict)?;
        if candidate_revision != job.document_content_revision() {
            return Err(RepairError::Conflict);
        }
        let updated = sqlx::query("UPDATE document_processing_jobs SET status='queued',current_step='validate_candidate',next_attempt_at=?1,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,version=version+1,updated_at=?1 WHERE tenant_id=?2 AND id=?3 AND status='waiting_for_ai' AND current_step='extract_fields' AND version=?4")
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .bind(before)
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        if updated.rows_affected() != 1 {
            return Err(RepairError::Conflict);
        }
        sqlx::query("UPDATE document_processing_steps SET status='succeeded',finished_at=COALESCE(finished_at,?1),updated_at=?1 WHERE tenant_id=?2 AND job_id=?3 AND step_kind='extract_fields' AND status <> 'succeeded'")
            .bind(context.now.to_rfc3339())
            .bind(command.tenant_id.to_string())
            .bind(command.target.uuid()?.to_string())
            .execute(&mut *connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        insert_processing_outbox(
            &mut connection,
            &job,
            "document.processing.ai-completion-reconciled.v1",
            serde_json::json!({"step":"validate_candidate"}),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        insert_processing_audit(
            &mut connection,
            &job,
            "ai_completion_reconciled",
            Some(command.requested_by),
            serde_json::json!({"step":"validate_candidate"}),
            context.now,
        )
        .await
        .map_err(|_| RepairError::Persistence)?;
        commit_immediate(&mut connection)
            .await
            .map_err(|_| RepairError::Persistence)?;
        Ok(repair_result(
            before,
            before.saturating_add(1),
            1,
            RepairOutcome::Succeeded,
        ))
    }
}

fn repair_result(
    before: i64,
    after: i64,
    rows_affected: u32,
    outcome: RepairOutcome,
) -> RepairResult {
    RepairResult {
        command_id: Uuid::now_v7(),
        resource_version_before: Some(before),
        resource_version_after: Some(after),
        before_hash: state_hash(format!("processing-job:{before}")),
        after_hash: state_hash(format!("processing-job:{after}")),
        rows_affected,
        outcome,
    }
}

fn artifact_from_checkpoint(
    checkpoint: Option<&Value>,
    expected_revision: i64,
) -> Result<TextArtifactReference, RepairError> {
    let object = checkpoint
        .and_then(Value::as_object)
        .ok_or(RepairError::Unavailable)?;
    let key = object
        .get("text_artifact_reference")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(RepairError::Unavailable)?;
    let content_revision = object
        .get("content_revision")
        .and_then(Value::as_i64)
        .ok_or(RepairError::Unavailable)?;
    let content_hash = object
        .get("content_hash")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(RepairError::Unavailable)?;
    if content_revision != expected_revision {
        return Err(RepairError::Conflict);
    }
    Ok(TextArtifactReference {
        key: key.to_string(),
        content_hash: content_hash.to_string(),
        content_revision,
        byte_count: object
            .get("byte_count")
            .and_then(Value::as_u64)
            .ok_or(RepairError::Unavailable)?,
        line_count: object
            .get("line_count")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(RepairError::Unavailable)?,
        character_count: object
            .get("character_count")
            .and_then(Value::as_u64)
            .ok_or(RepairError::Unavailable)?,
    })
}
