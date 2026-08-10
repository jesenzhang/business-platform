//! Stage 2 deterministic mapping and isolated target rehearsal.
//!
//! This module is an infrastructure adapter for PLAN-0009.  It consumes only
//! the frozen Stage 1 manifest and re-checks the read-only source snapshot
//! before producing target-side evidence.  It deliberately does not expose a
//! source write path or a production mode.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, LocalResult, TimeZone, Utc};
use document::application::{CreateDocumentCommand, CreateDocumentMetadata};
use document::domain::{DocumentLink, DocumentLinkRole, DocumentMetadata, DocumentResourceKind};
use document_processing::{ArtifactKind, Evidence, ProcessingArtifact, ProcessingRun};
use document_sqlite::SqliteCreateDocumentUnitOfWork;
use legacy_migration_rehearsal::{BoundaryError, ExecutionMode, RehearsalBoundary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::migrate::MigrateDatabase;
use sqlx::{Sqlite, SqlitePool};
use thiserror::Error;
use uuid::Uuid;

use super::{
    canonical_directory, canonical_file, connect_read_only, file_digest, hash_text,
    manifest_digest, relative_path, scan_root, sha256_file, source_fingerprint, FrozenManifest,
    InventoryConfig, InventoryError, InventoryRecord, PhysicalFile, PhysicalRootConfig,
    ScannedRoot,
};

const STAGE1_DIRECTORY: &str = "stage-1-inventory-v8";
const STAGE2_DIRECTORY: &str = "stage-2-rehearsal-v1";
const MANIFEST_FILE_NAME: &str = "manifest-v1.json";
const MANIFEST_DIGEST_FILE_NAME: &str = "manifest-v1-digests.json";
const MAPPING_PLAN_FILE_NAME: &str = "mapping-plan-v1.json";
const MAPPING_DIGEST_FILE_NAME: &str = "mapping-plan-v1-digests.json";
const AUDIT_FILE_NAME: &str = "rehearsal-audit-v1.json";
const MAPPING_SCHEMA: &str = "plan-0009.stage-2.mapping.v1";
const AUDIT_SCHEMA: &str = "plan-0009.stage-2.rehearsal-audit.v1";
const TARGET_DB_DIRECTORY: &str = "db";
const TARGET_DB_FILE: &str = "document-management.sqlite";
const TARGET_OBJECT_DIRECTORY: &str = "objects";
const REHEARSAL_TENANT: &str = "00000000-0000-4000-8000-000000000009";
const REHEARSAL_ACTOR: &str = "00000000-0000-4000-8000-000000000010";

#[derive(Debug, Error)]
pub enum Stage2Error {
    #[error("invalid Stage 2 configuration")]
    InvalidConfiguration,
    #[error("frozen Stage 1 manifest could not be read")]
    ManifestRead,
    #[error("frozen Stage 1 manifest is inconsistent")]
    ManifestDigestMismatch,
    #[error("target mapping already contains a different frozen result")]
    ManifestConflict,
    #[error("source snapshot changed after Stage 1 freeze")]
    SourceChanged,
    #[error("source read failed")]
    SourceRead,
    #[error("source object does not match frozen evidence")]
    SourceObjectMismatch,
    #[error("rehearsal boundary rejected the operation")]
    Boundary(#[from] BoundaryError),
    #[error("target database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("target database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("target artifact write failed")]
    TargetWrite,
    #[error("target contains a conflicting row")]
    TargetConflict,
    #[error("mapping serialization failed")]
    Serialization(#[source] serde_json::Error),
    #[error("domain validation rejected the mapped entity")]
    DomainValidation,
    #[error("frozen classification is not eligible for automatic materialization")]
    ClassificationInvariant,
}

impl Stage2Error {
    /// Stable, non-sensitive CLI/audit code.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::ManifestRead => "manifest_read_failed",
            Self::ManifestDigestMismatch => "manifest_digest_mismatch",
            Self::ManifestConflict => "manifest_conflict",
            Self::SourceChanged => "source_changed",
            Self::SourceRead => "source_read_failed",
            Self::SourceObjectMismatch => "source_object_mismatch",
            Self::Boundary(_) => "boundary_rejected",
            Self::Database(_) => "target_database_failed",
            Self::Migration(_) => "target_database_failed",
            Self::TargetWrite => "target_write_failed",
            Self::TargetConflict => "target_conflict",
            Self::Serialization(_) => "serialization_failed",
            Self::DomainValidation => "domain_validation_failed",
            Self::ClassificationInvariant => "classification_invariant_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage2Summary {
    pub selected_contracts: usize,
    pub exact_eligible: usize,
    pub exact_materialized: usize,
    pub review_count: usize,
    pub quarantine_count: usize,
    pub mapping_plan_sha256: String,
    pub mapping_file_bytes_sha256: String,
    pub replayed: bool,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappingPlan {
    mapping_schema: String,
    manifest_schema: String,
    manifest_canonical_sha256: String,
    records: Vec<MappingPlanRecord>,
    mapping_plan_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappingPlanRecord {
    selection_rank: usize,
    source_contract_id: i64,
    source_business_key_sha256: Option<String>,
    classification: String,
    reason_code: String,
    disposition: String,
    auto_write_allowed: bool,
    lineage: super::LineageCount,
    evidence_count: usize,
    evidence_path_sha256: Vec<String>,
    observed_sha256: Vec<String>,
    candidate_document_id: String,
    candidate_revision_id: String,
    candidate_link_id: String,
    candidate_processing_run_id: String,
    candidate_artifact_id: String,
    candidate_evidence_id: String,
    target_object_ref_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappingDigestSidecar {
    mapping_schema: String,
    mapping_plan_sha256: String,
    file_bytes_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RehearsalAudit {
    audit_schema: String,
    mapping_schema: String,
    manifest_canonical_sha256: String,
    mapping_plan_sha256: String,
    selected_contracts: usize,
    exact_eligible: usize,
    exact_materialized: usize,
    review_count: usize,
    quarantine_count: usize,
    first_run: AuditRun,
    replay_count: u64,
    last_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditRun {
    status: String,
    selected_contracts: usize,
    exact_eligible: usize,
    exact_materialized: usize,
    review_count: usize,
    quarantine_count: usize,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExistingMappingRow {
    mapping_record_sha256: String,
}

/// Run Stage 2 against the existing frozen Stage 1 manifest.
pub async fn run_stage2(config: &InventoryConfig) -> Result<Stage2Summary, Stage2Error> {
    run_stage2_at(config, STAGE2_DIRECTORY).await
}

/// Run the reviewed Stage 2 mapping engine against a bounded target directory.
///
/// Stage 3 uses this seam to preserve the exact mapping/materialization rules
/// while keeping its target isolated from the original Stage 2 rehearsal.
pub(crate) async fn run_stage2_at(
    config: &InventoryConfig,
    target_directory: &str,
) -> Result<Stage2Summary, Stage2Error> {
    validate_target_shape(config, target_directory)?;
    let stage1_root = config.isolation_root.join(STAGE1_DIRECTORY);
    let manifest = load_frozen_manifest(&stage1_root)?;
    let (boundary, scanned_roots) = verify_source_snapshot(config, &manifest).await?;
    let mut plan = build_mapping_plan(&manifest)?;
    let mapping_digest = mapping_plan_digest(&plan)?;
    plan.mapping_plan_sha256.clone_from(&mapping_digest);
    let (replayed, mapping_file_bytes_sha256) =
        write_or_verify_mapping_plan(config.target_root.as_path(), &plan)?;

    let target_objects = config.target_root.join(TARGET_OBJECT_DIRECTORY);
    fs::create_dir_all(&target_objects).map_err(|_| Stage2Error::TargetWrite)?;
    let pool = open_target_database(config.target_root.as_path()).await?;
    create_rehearsal_tables(&pool).await?;
    persist_mapping_records(&pool, &plan).await?;

    let mut exact_materialized = 0_usize;
    for record in &plan.records {
        if !record.auto_write_allowed {
            continue;
        }
        materialize_exact(
            &pool,
            &target_objects,
            config,
            &boundary,
            &scanned_roots,
            &manifest,
            record,
        )
        .await?;
        exact_materialized += 1;
    }

    let exact_eligible = plan
        .records
        .iter()
        .filter(|record| record.auto_write_allowed)
        .count();
    let review_count = plan
        .records
        .iter()
        .filter(|record| record.disposition == "manual_review")
        .count();
    let quarantine_count = plan
        .records
        .iter()
        .filter(|record| record.disposition == "quarantine")
        .count();
    write_or_verify_audit(
        config.target_root.as_path(),
        &plan,
        exact_eligible,
        exact_materialized,
        review_count,
        quarantine_count,
        replayed,
    )?;
    pool.close().await;

    Ok(Stage2Summary {
        selected_contracts: plan.records.len(),
        exact_eligible,
        exact_materialized,
        review_count,
        quarantine_count,
        mapping_plan_sha256: mapping_digest,
        mapping_file_bytes_sha256,
        replayed,
    })
}

fn validate_target_shape(
    config: &InventoryConfig,
    target_directory: &str,
) -> Result<(), Stage2Error> {
    if config.target_root != config.isolation_root.join(target_directory)
        || !config.isolation_root.is_dir()
        || !config.target_root.is_dir()
    {
        return Err(Stage2Error::InvalidConfiguration);
    }
    Ok(())
}

fn load_frozen_manifest(stage1_root: &Path) -> Result<FrozenManifest, Stage2Error> {
    let manifest_path = stage1_root.join(MANIFEST_FILE_NAME);
    let digest_path = stage1_root.join(MANIFEST_DIGEST_FILE_NAME);
    let bytes = fs::read(manifest_path).map_err(|_| Stage2Error::ManifestRead)?;
    let manifest: FrozenManifest =
        serde_json::from_slice(&bytes).map_err(|_| Stage2Error::ManifestRead)?;
    if manifest.manifest_schema != super::MANIFEST_SCHEMA {
        return Err(Stage2Error::ManifestDigestMismatch);
    }
    let recomputed = manifest_digest(&manifest).map_err(|_| Stage2Error::ManifestDigestMismatch)?;
    if recomputed != manifest.canonical_manifest_sha256 {
        return Err(Stage2Error::ManifestDigestMismatch);
    }
    let sidecar_bytes = fs::read(digest_path).map_err(|_| Stage2Error::ManifestRead)?;
    let sidecar: super::ManifestDigestSidecar =
        serde_json::from_slice(&sidecar_bytes).map_err(|_| Stage2Error::ManifestRead)?;
    if sidecar.manifest_schema != super::MANIFEST_SCHEMA
        || sidecar.canonical_manifest_sha256 != manifest.canonical_manifest_sha256
        || sidecar.file_bytes_sha256 != sha256_bytes(&bytes)
    {
        return Err(Stage2Error::ManifestDigestMismatch);
    }
    Ok(manifest)
}

async fn verify_source_snapshot(
    config: &InventoryConfig,
    manifest: &FrozenManifest,
) -> Result<(RehearsalBoundary, Vec<ScannedRoot>), Stage2Error> {
    let legacy_root = canonical_directory(&config.legacy_root).map_err(map_source_error)?;
    let data_root = canonical_directory(&config.data_root).map_err(map_source_error)?;
    let database_path = canonical_file(&config.database_path).map_err(map_source_error)?;
    if !database_path.starts_with(&data_root) {
        return Err(Stage2Error::SourceRead);
    }
    let env_file = canonical_file(&config.env_file).map_err(map_source_error)?;
    if !env_file.starts_with(&legacy_root) {
        return Err(Stage2Error::SourceRead);
    }
    let physical_roots = config
        .physical_roots
        .iter()
        .map(|root| {
            let path = canonical_directory(&root.path).map_err(map_source_error)?;
            if !path.starts_with(&data_root) {
                return Err(Stage2Error::SourceRead);
            }
            Ok(PhysicalRootConfig {
                label: root.label,
                path,
            })
        })
        .collect::<Result<Vec<_>, Stage2Error>>()?;
    let boundary = RehearsalBoundary::validate_sources(
        [&legacy_root, &data_root],
        &config.isolation_root,
        &config.target_root,
        ExecutionMode::Rehearsal,
    )?;
    let env_relative = relative_path(&legacy_root, &env_file).map_err(map_source_error)?;
    boundary
        .read_only_source_at(0)
        .ok_or(Stage2Error::InvalidConfiguration)?
        .open(env_relative)
        .map_err(|_| Stage2Error::SourceRead)?;
    let database_relative = relative_path(&data_root, &database_path).map_err(map_source_error)?;
    boundary
        .read_only_source_at(1)
        .ok_or(Stage2Error::InvalidConfiguration)?
        .open(database_relative)
        .map_err(|_| Stage2Error::SourceRead)?;

    let scanned_roots = physical_roots
        .iter()
        .map(scan_root)
        .collect::<Result<Vec<_>, InventoryError>>()
        .map_err(map_source_error)?;
    let observed_roots = scanned_roots
        .iter()
        .map(|root| root.fingerprint.clone())
        .collect::<Vec<_>>();
    if observed_roots != manifest.physical_roots {
        return Err(Stage2Error::SourceChanged);
    }

    let source_pool = connect_read_only(&database_path)
        .await
        .map_err(map_source_error)?;
    let observed_source = source_fingerprint(&source_pool, &env_file, &database_path)
        .await
        .map_err(map_source_error)?;
    source_pool.close().await;
    if observed_source != manifest.source {
        return Err(Stage2Error::SourceChanged);
    }
    Ok((boundary, scanned_roots))
}

#[allow(clippy::needless_pass_by_value)]
fn map_source_error(_error: InventoryError) -> Stage2Error {
    Stage2Error::SourceRead
}

fn build_mapping_plan(manifest: &FrozenManifest) -> Result<MappingPlan, Stage2Error> {
    let records = manifest
        .records
        .iter()
        .map(|record| build_mapping_record(manifest, record))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MappingPlan {
        mapping_schema: MAPPING_SCHEMA.to_string(),
        manifest_schema: manifest.manifest_schema.clone(),
        manifest_canonical_sha256: manifest.canonical_manifest_sha256.clone(),
        records,
        mapping_plan_sha256: String::new(),
    })
}

fn build_mapping_record(
    manifest: &FrozenManifest,
    record: &InventoryRecord,
) -> Result<MappingPlanRecord, Stage2Error> {
    let classification = record.classification.clone();
    let disposition = if classification == "Exact" {
        "auto_materialize"
    } else if classification == "Probable" {
        "manual_review"
    } else {
        "quarantine"
    };
    let observed_sha256 = record
        .evidence
        .iter()
        .filter_map(|evidence| evidence.observed_sha256.clone())
        .collect::<Vec<_>>();
    let auto_write_allowed =
        classification == "Exact" && record.evidence.len() == 1 && observed_sha256.len() == 1;
    if classification == "Exact" && !auto_write_allowed {
        return Err(Stage2Error::ClassificationInvariant);
    }
    let document_id = deterministic_uuid(
        "document",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let revision_id = deterministic_uuid(
        "revision",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let link_id = deterministic_uuid(
        "link",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let run_id = deterministic_uuid(
        "processing-run",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let artifact_id = deterministic_uuid(
        "processing-artifact",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let evidence_id = deterministic_uuid(
        "evidence",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let target_object_ref = revision_object_key(
        parse_fixed_uuid(REHEARSAL_TENANT)?,
        document_id,
        revision_id,
    );
    Ok(MappingPlanRecord {
        selection_rank: record.selection_rank,
        source_contract_id: record.source_contract_id,
        source_business_key_sha256: record.source_business_key_sha256.clone(),
        classification,
        reason_code: record.reason_code.clone(),
        disposition: disposition.to_string(),
        auto_write_allowed,
        lineage: record.lineage.clone(),
        evidence_count: record.evidence.len(),
        evidence_path_sha256: record
            .evidence
            .iter()
            .map(|evidence| evidence.relative_path_sha256.clone())
            .collect(),
        observed_sha256,
        candidate_document_id: document_id.to_string(),
        candidate_revision_id: revision_id.to_string(),
        candidate_link_id: link_id.to_string(),
        candidate_processing_run_id: run_id.to_string(),
        candidate_artifact_id: artifact_id.to_string(),
        candidate_evidence_id: evidence_id.to_string(),
        target_object_ref_sha256: hash_text(&target_object_ref),
    })
}

fn deterministic_uuid(
    scope: &str,
    manifest_digest_value: &str,
    source_id: i64,
    ordinal: u8,
) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"plan-0009.stage-2.deterministic-id:v1\0");
    hasher.update(scope.as_bytes());
    hasher.update([0]);
    hasher.update(manifest_digest_value.as_bytes());
    hasher.update(source_id.to_be_bytes());
    hasher.update([ordinal]);
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn revision_object_key(tenant_id: Uuid, document_id: Uuid, revision_id: Uuid) -> String {
    format!("tenants/{tenant_id}/documents/{document_id}/revisions/{revision_id}/source")
}

fn deterministic_rehearsal_time(source_contract_id: i64) -> DateTime<Utc> {
    let offset = source_contract_id.rem_euclid(86_400);
    let seconds = 1_735_689_600_i64 + offset;
    match Utc.timestamp_opt(seconds, 0) {
        LocalResult::Single(value) => value,
        _ => DateTime::<Utc>::UNIX_EPOCH,
    }
}

fn mapping_plan_digest(plan: &MappingPlan) -> Result<String, Stage2Error> {
    let mut unsigned = plan.clone();
    unsigned.mapping_plan_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(Stage2Error::Serialization)?;
    Ok(sha256_bytes(&bytes))
}

fn write_or_verify_mapping_plan(
    target_root: &Path,
    plan: &MappingPlan,
) -> Result<(bool, String), Stage2Error> {
    let plan_path = target_root.join(MAPPING_PLAN_FILE_NAME);
    let digest_path = target_root.join(MAPPING_DIGEST_FILE_NAME);
    let bytes = serde_json::to_vec_pretty(plan).map_err(Stage2Error::Serialization)?;
    let generated_file_digest = sha256_bytes(&bytes);
    if plan_path.exists() {
        let existing_bytes = fs::read(&plan_path).map_err(|_| Stage2Error::TargetWrite)?;
        let existing: MappingPlan =
            serde_json::from_slice(&existing_bytes).map_err(|_| Stage2Error::ManifestRead)?;
        let existing_digest = mapping_plan_digest(&existing)?;
        if existing.mapping_schema != MAPPING_SCHEMA
            || existing.mapping_plan_sha256 != existing_digest
            || existing.mapping_plan_sha256 != plan.mapping_plan_sha256
        {
            return Err(Stage2Error::ManifestConflict);
        }
        let sidecar_bytes = fs::read(&digest_path).map_err(|_| Stage2Error::ManifestConflict)?;
        let sidecar: MappingDigestSidecar =
            serde_json::from_slice(&sidecar_bytes).map_err(|_| Stage2Error::ManifestRead)?;
        if sidecar.mapping_schema != MAPPING_SCHEMA
            || sidecar.mapping_plan_sha256 != existing.mapping_plan_sha256
            || sidecar.file_bytes_sha256 != sha256_bytes(&existing_bytes)
        {
            return Err(Stage2Error::ManifestDigestMismatch);
        }
        return Ok((true, sidecar.file_bytes_sha256));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&plan_path)
        .map_err(|_| Stage2Error::TargetWrite)?;
    file.write_all(&bytes)
        .map_err(|_| Stage2Error::TargetWrite)?;
    file.flush().map_err(|_| Stage2Error::TargetWrite)?;
    let sidecar = MappingDigestSidecar {
        mapping_schema: MAPPING_SCHEMA.to_string(),
        mapping_plan_sha256: plan.mapping_plan_sha256.clone(),
        file_bytes_sha256: generated_file_digest.clone(),
    };
    let sidecar_bytes = serde_json::to_vec_pretty(&sidecar).map_err(Stage2Error::Serialization)?;
    let mut digest_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&digest_path)
        .map_err(|_| Stage2Error::TargetWrite)?;
    digest_file
        .write_all(&sidecar_bytes)
        .map_err(|_| Stage2Error::TargetWrite)?;
    digest_file.flush().map_err(|_| Stage2Error::TargetWrite)?;
    Ok((false, generated_file_digest))
}

async fn open_target_database(target_root: &Path) -> Result<SqlitePool, Stage2Error> {
    let database_directory = target_root.join(TARGET_DB_DIRECTORY);
    fs::create_dir_all(&database_directory).map_err(|_| Stage2Error::TargetWrite)?;
    let database_path = database_directory.join(TARGET_DB_FILE);
    let database_url = format!(
        "sqlite://{}",
        database_path.to_string_lossy().replace('\\', "/")
    );
    if !database_path.exists() {
        sqlx::Sqlite::create_database(&database_url)
            .await
            .map_err(Stage2Error::Database)?;
    }
    let pool = document_sqlite::connect(&database_url, 1)
        .await
        .map_err(Stage2Error::Database)?;
    document_sqlite::MIGRATOR
        .run(&pool)
        .await
        .map_err(Stage2Error::Migration)?;
    document_processing_sqlite::run_migrations(&pool)
        .await
        .map_err(Stage2Error::Database)?;
    Ok(pool)
}

async fn create_rehearsal_tables(pool: &SqlitePool) -> Result<(), Stage2Error> {
    // The Document SQLite evidence migration uses tenant-scoped composite
    // foreign keys.  The single-column primary keys are not sufficient for
    // SQLite's composite-parent validation, so the isolated rehearsal target
    // installs the adapter-owned unique indexes before any mapping rows exist.
    for statement in [
        "CREATE UNIQUE INDEX IF NOT EXISTS plan_0009_processing_runs_tenant_id_key \
         ON document_processing_runs (tenant_id, id)",
        "CREATE UNIQUE INDEX IF NOT EXISTS plan_0009_processing_artifacts_tenant_id_key \
         ON document_processing_artifacts (tenant_id, id)",
        "CREATE UNIQUE INDEX IF NOT EXISTS plan_0009_processing_evidence_tenant_id_key \
         ON document_processing_evidence (tenant_id, id)",
    ] {
        sqlx::query(statement)
            .execute(pool)
            .await
            .map_err(Stage2Error::Database)?;
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS plan_0009_mapping_records (\
            manifest_sha256 TEXT NOT NULL,\
            source_contract_id INTEGER NOT NULL,\
            selection_rank INTEGER NOT NULL,\
            source_business_key_sha256 TEXT,\
            classification TEXT NOT NULL,\
            reason_code TEXT NOT NULL,\
            disposition TEXT NOT NULL,\
            mapping_record_sha256 TEXT NOT NULL,\
            candidate_document_id TEXT NOT NULL,\
            candidate_revision_id TEXT NOT NULL,\
            candidate_link_id TEXT NOT NULL,\
            candidate_processing_run_id TEXT NOT NULL,\
            candidate_artifact_id TEXT NOT NULL,\
            candidate_evidence_id TEXT NOT NULL,\
            materialized INTEGER NOT NULL DEFAULT 0 CHECK(materialized IN (0, 1)),\
            PRIMARY KEY (manifest_sha256, source_contract_id)\
        )",
    )
    .execute(pool)
    .await
    .map_err(Stage2Error::Database)?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_plan_0009_mapping_disposition \
         ON plan_0009_mapping_records (manifest_sha256, disposition, source_contract_id)",
    )
    .execute(pool)
    .await
    .map_err(Stage2Error::Database)?;
    Ok(())
}

async fn persist_mapping_records(pool: &SqlitePool, plan: &MappingPlan) -> Result<(), Stage2Error> {
    let mut connection = pool.acquire().await.map_err(Stage2Error::Database)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
    let result = persist_mapping_records_in_transaction(&mut connection, plan).await;
    match result {
        Ok(()) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(Stage2Error::Database)?;
            Ok(())
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(error)
        }
    }
}

async fn persist_mapping_records_in_transaction(
    connection: &mut sqlx::pool::PoolConnection<Sqlite>,
    plan: &MappingPlan,
) -> Result<(), Stage2Error> {
    for record in &plan.records {
        let mapping_record_sha256 = mapping_record_digest(record)?;
        let existing = sqlx::query_as::<_, ExistingMappingRow>(
            "SELECT mapping_record_sha256 \
             FROM plan_0009_mapping_records \
             WHERE manifest_sha256 = ?1 AND source_contract_id = ?2",
        )
        .bind(&plan.manifest_canonical_sha256)
        .bind(record.source_contract_id)
        .fetch_optional(&mut **connection)
        .await
        .map_err(Stage2Error::Database)?;
        if let Some(existing) = existing {
            if existing.mapping_record_sha256 != mapping_record_sha256 {
                return Err(Stage2Error::TargetConflict);
            }
            continue;
        }
        sqlx::query(
            "INSERT INTO plan_0009_mapping_records\
             (manifest_sha256, source_contract_id, selection_rank, source_business_key_sha256,\
              classification, reason_code, disposition, mapping_record_sha256,\
             candidate_document_id, candidate_revision_id, candidate_link_id,\
              candidate_processing_run_id, candidate_artifact_id, candidate_evidence_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(&plan.manifest_canonical_sha256)
        .bind(record.source_contract_id)
        .bind(i64::try_from(record.selection_rank).map_err(|_| Stage2Error::TargetWrite)?)
        .bind(&record.source_business_key_sha256)
        .bind(&record.classification)
        .bind(&record.reason_code)
        .bind(&record.disposition)
        .bind(&mapping_record_sha256)
        .bind(&record.candidate_document_id)
        .bind(&record.candidate_revision_id)
        .bind(&record.candidate_link_id)
        .bind(&record.candidate_processing_run_id)
        .bind(&record.candidate_artifact_id)
        .bind(&record.candidate_evidence_id)
        .execute(&mut **connection)
        .await
        .map_err(Stage2Error::Database)?;
    }
    Ok(())
}

fn mapping_record_digest(record: &MappingPlanRecord) -> Result<String, Stage2Error> {
    serde_json::to_vec(record)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(Stage2Error::Serialization)
}

#[allow(clippy::too_many_lines)]
async fn materialize_exact(
    pool: &SqlitePool,
    target_objects: &Path,
    config: &InventoryConfig,
    boundary: &RehearsalBoundary,
    scanned_roots: &[ScannedRoot],
    manifest: &FrozenManifest,
    record: &MappingPlanRecord,
) -> Result<(), Stage2Error> {
    let source_record = manifest
        .records
        .iter()
        .find(|candidate| candidate.source_contract_id == record.source_contract_id)
        .ok_or(Stage2Error::TargetConflict)?;
    let evidence = source_record
        .evidence
        .first()
        .ok_or(Stage2Error::ClassificationInvariant)?;
    let checksum = evidence
        .observed_sha256
        .as_deref()
        .ok_or(Stage2Error::ClassificationInvariant)?;
    let tenant_id = parse_fixed_uuid(REHEARSAL_TENANT)?;
    let actor_id = parse_fixed_uuid(REHEARSAL_ACTOR)?;
    let document_id =
        Uuid::parse_str(&record.candidate_document_id).map_err(|_| Stage2Error::TargetConflict)?;
    let revision_id =
        Uuid::parse_str(&record.candidate_revision_id).map_err(|_| Stage2Error::TargetConflict)?;
    let link_id =
        Uuid::parse_str(&record.candidate_link_id).map_err(|_| Stage2Error::TargetConflict)?;
    let evidence_id =
        Uuid::parse_str(&record.candidate_evidence_id).map_err(|_| Stage2Error::TargetConflict)?;
    let object_key = revision_object_key(tenant_id, document_id, revision_id);
    let object_path = target_objects.join(&object_key);
    let source_size =
        i64::try_from(evidence.size_bytes).map_err(|_| Stage2Error::DomainValidation)?;
    let extension = evidence.extension.as_deref().unwrap_or("bin");
    let filename = format!("legacy-contract-{}.{extension}", record.source_contract_id);
    let content_type = content_type_for_extension(extension);
    let created_at = deterministic_rehearsal_time(record.source_contract_id);
    let materialized = sqlx::query_scalar::<_, i64>(
        "SELECT materialized FROM plan_0009_mapping_records \
         WHERE manifest_sha256 = ?1 AND source_contract_id = ?2",
    )
    .bind(&manifest.canonical_manifest_sha256)
    .bind(record.source_contract_id)
    .fetch_optional(pool)
    .await
    .map_err(Stage2Error::Database)?
    .ok_or(Stage2Error::TargetConflict)?;
    if materialized == 1 {
        return verify_exact_materialization(
            pool,
            target_objects,
            manifest,
            record,
            source_record,
            evidence,
            tenant_id,
            actor_id,
            document_id,
            revision_id,
            link_id,
            evidence_id,
            object_key.as_str(),
            checksum,
            source_size,
            filename.as_str(),
            content_type,
            created_at,
        )
        .await;
    }
    let (source_file, root_index) = find_exact_source(source_record, scanned_roots)?;
    copy_exact_source(
        boundary,
        config,
        root_index,
        &source_file,
        &object_path,
        checksum,
    )?;
    let document_app = CreateDocumentMetadata::new(Arc::new(
        SqliteCreateDocumentUnitOfWork::new_for_rehearsal(pool.clone(), created_at),
    ));
    let document_result = document_app
        .execute_with_id_at(
            Some(document_id),
            CreateDocumentCommand {
                tenant_id,
                user_id: actor_id,
                original_filename: filename,
                content_type: content_type.to_string(),
                object_key: format!("legacy-contract/{}", record.source_contract_id),
                size_bytes: Some(source_size),
                sha256: Some(checksum.to_string()),
                revision_id: Some(revision_id),
                idempotency_key: format!(
                    "plan-0009:{}:{}",
                    manifest.canonical_manifest_sha256, record.source_contract_id
                ),
            },
            created_at,
        )
        .await
        .map_err(|_| Stage2Error::DomainValidation)?;
    if document_result.document.id() != document_id
        || document_result.document.current_revision_id() != revision_id
        || document_result.document.object_key() != object_key
        || document_result.document.created_at() != created_at
        || document_result.document.updated_at() != created_at
    {
        return Err(Stage2Error::TargetConflict);
    }

    let resource_id = deterministic_uuid(
        "contract-resource",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let link = DocumentLink::new_with_id(
        link_id,
        tenant_id,
        document_id,
        DocumentResourceKind::Contract,
        resource_id,
        DocumentLinkRole::MainContract,
        actor_id,
        created_at,
    )
    .map_err(|_| Stage2Error::DomainValidation)?;
    persist_document_link(pool, &link).await?;

    if has_processing_lineage(record) {
        let run_id = Uuid::parse_str(&record.candidate_processing_run_id)
            .map_err(|_| Stage2Error::TargetConflict)?;
        let artifact_id = Uuid::parse_str(&record.candidate_artifact_id)
            .map_err(|_| Stage2Error::TargetConflict)?;
        let (run, artifact, evidence_entity) = build_processing_entities(
            record,
            evidence,
            tenant_id,
            actor_id,
            revision_id,
            run_id,
            artifact_id,
            evidence_id,
            &object_key,
            checksum,
            created_at,
        )?;
        persist_processing_entities(pool, &run, &artifact, &evidence_entity).await?;
    }

    let mapping_record_sha256 = mapping_record_digest(record)?;
    let updated = sqlx::query(
        "UPDATE plan_0009_mapping_records SET materialized = 1\
         WHERE manifest_sha256 = ?1 AND source_contract_id = ?2\
           AND mapping_record_sha256 = ?3",
    )
    .bind(&manifest.canonical_manifest_sha256)
    .bind(record.source_contract_id)
    .bind(mapping_record_sha256)
    .execute(pool)
    .await
    .map_err(Stage2Error::Database)?;
    if updated.rows_affected() != 1 {
        return Err(Stage2Error::TargetConflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_processing_entities(
    record: &MappingPlanRecord,
    evidence: &super::EvidenceReference,
    tenant_id: Uuid,
    actor_id: Uuid,
    revision_id: Uuid,
    run_id: Uuid,
    artifact_id: Uuid,
    evidence_id: Uuid,
    object_key: &str,
    checksum: &str,
    created_at: DateTime<Utc>,
) -> Result<(ProcessingRun, ProcessingArtifact, Evidence), Stage2Error> {
    let mut run = ProcessingRun::start_with_id(
        run_id,
        tenant_id,
        revision_id,
        "legacy-rehearsal.v1".to_string(),
        "legacy-import".to_string(),
        "1".to_string(),
        None,
        None,
        actor_id,
        created_at,
    )
    .map_err(|_| Stage2Error::DomainValidation)?;
    run.finish_succeeded(created_at)
        .map_err(|_| Stage2Error::DomainValidation)?;
    let artifact = ProcessingArtifact::new_with_id(
        artifact_id,
        tenant_id,
        run_id,
        artifact_kind_for(record),
        object_key.to_string(),
        checksum.to_string(),
        "legacy-rehearsal.v1".to_string(),
        created_at,
    )
    .map_err(|_| Stage2Error::DomainValidation)?;
    let evidence_entity = Evidence::new_with_id(
        evidence_id,
        tenant_id,
        revision_id,
        &run,
        &artifact,
        serde_json::json!({
            "source_contract_id": record.source_contract_id,
            "selection_rank": record.selection_rank,
            "path_sha256": evidence.relative_path_sha256,
        }),
        checksum.to_string(),
        created_at,
    )
    .map_err(|_| Stage2Error::DomainValidation)?;
    Ok((run, artifact, evidence_entity))
}

#[allow(clippy::too_many_arguments)]
async fn verify_exact_materialization(
    pool: &SqlitePool,
    target_objects: &Path,
    manifest: &FrozenManifest,
    record: &MappingPlanRecord,
    _source_record: &InventoryRecord,
    evidence: &super::EvidenceReference,
    tenant_id: Uuid,
    actor_id: Uuid,
    document_id: Uuid,
    revision_id: Uuid,
    link_id: Uuid,
    evidence_id: Uuid,
    object_key: &str,
    checksum: &str,
    source_size: i64,
    filename: &str,
    content_type: &str,
    created_at: DateTime<Utc>,
) -> Result<(), Stage2Error> {
    let object_path = target_objects.join(object_key);
    let object_metadata =
        fs::symlink_metadata(&object_path).map_err(|_| Stage2Error::TargetConflict)?;
    if !object_metadata.is_file()
        || file_digest(&object_path).map_err(map_source_error)?.1 != checksum
    {
        return Err(Stage2Error::TargetConflict);
    }

    let expected_object = DocumentMetadata::create_with_revision_id_at(
        document_id,
        tenant_id,
        filename.to_string(),
        content_type.to_string(),
        format!("legacy-contract/{}", record.source_contract_id),
        actor_id,
        Some(source_size),
        revision_id,
        created_at,
    )
    .map_err(|_| Stage2Error::DomainValidation)?;
    if expected_object.object_key() != object_key {
        return Err(Stage2Error::TargetConflict);
    }

    let document = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            String,
            String,
            String,
            Option<i64>,
            String,
            String,
            String,
        ),
    >(
        "SELECT tenant_id, original_filename, content_type, object_key, status,
                version, content_revision, current_revision_id, deletion_state,
                pre_trash_lifecycle, size_bytes, created_by, created_at, updated_at
         FROM documents WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id.to_string())
    .bind(document_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(Stage2Error::Database)?
    .ok_or(Stage2Error::TargetConflict)?;
    if document.0 != tenant_id.to_string()
        || document.1 != expected_object.original_filename()
        || document.2 != expected_object.content_type()
        || document.3 != object_key
        || document.4 != "active"
        || document.5 != 1
        || document.6 != 1
        || document.7 != revision_id.to_string()
        || document.8 != "present"
        || document.9 != "active"
        || document.10 != Some(source_size)
        || document.11 != actor_id.to_string()
        || document.12 != created_at.to_rfc3339()
        || document.13 != created_at.to_rfc3339()
    {
        return Err(Stage2Error::TargetConflict);
    }

    let revision = sqlx::query_as::<
        _,
        (
            String,
            String,
            i64,
            Option<String>,
            String,
            Option<String>,
            String,
            Option<i64>,
            String,
            String,
            String,
            Option<String>,
        ),
    >(
        "SELECT tenant_id, document_id, revision_no, parent_revision_id,
                source_object_ref, sha256, content_type, size_bytes,
                original_filename, created_by, created_at, change_reason
         FROM document_revisions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id.to_string())
    .bind(revision_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(Stage2Error::Database)?
    .ok_or(Stage2Error::TargetConflict)?;
    if revision.0 != tenant_id.to_string()
        || revision.1 != document_id.to_string()
        || revision.2 != 1
        || revision.3.is_some()
        || revision.4 != object_key
        || revision.5.as_deref() != Some(checksum)
        || revision.6 != content_type
        || revision.7 != Some(source_size)
        || revision.8 != filename
        || revision.9 != actor_id.to_string()
        || revision.10 != created_at.to_rfc3339()
        || revision.11.as_deref() != Some("initial upload")
    {
        return Err(Stage2Error::TargetConflict);
    }

    let resource_id = deterministic_uuid(
        "contract-resource",
        &manifest.canonical_manifest_sha256,
        record.source_contract_id,
        0,
    );
    let link = DocumentLink::new_with_id(
        link_id,
        tenant_id,
        document_id,
        DocumentResourceKind::Contract,
        resource_id,
        DocumentLinkRole::MainContract,
        actor_id,
        created_at,
    )
    .map_err(|_| Stage2Error::DomainValidation)?;
    let stored_link =
        sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT tenant_id, document_id, resource_kind, resource_id, role,
                created_at, created_by FROM document_links WHERE id = ?1",
        )
        .bind(link_id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
    if stored_link.0 != link.tenant_id().to_string()
        || stored_link.1 != link.document_id().to_string()
        || stored_link.2 != link.resource_kind().as_str()
        || stored_link.3 != link.resource_id().to_string()
        || stored_link.4 != link.role().as_str()
        || stored_link.5 != link.created_at().to_rfc3339()
        || stored_link.6 != link.created_by().to_string()
    {
        return Err(Stage2Error::TargetConflict);
    }

    if has_processing_lineage(record) {
        let run_id = Uuid::parse_str(&record.candidate_processing_run_id)
            .map_err(|_| Stage2Error::TargetConflict)?;
        let artifact_id = Uuid::parse_str(&record.candidate_artifact_id)
            .map_err(|_| Stage2Error::TargetConflict)?;
        let (run, artifact, evidence_entity) = build_processing_entities(
            record,
            evidence,
            tenant_id,
            actor_id,
            revision_id,
            run_id,
            artifact_id,
            evidence_id,
            object_key,
            checksum,
            created_at,
        )?;
        verify_processing_entities(pool, &run, &artifact, &evidence_entity).await?;
    }

    let document_uow = SqliteCreateDocumentUnitOfWork::new_for_rehearsal(pool.clone(), created_at);
    document_uow
        .verify_rehearsal_events(&expected_object)
        .await
        .map_err(|_| Stage2Error::TargetConflict)
}

fn has_processing_lineage(record: &MappingPlanRecord) -> bool {
    record.lineage.ocr_artifacts > 0
        || record.lineage.structured_artifacts > 0
        || record.lineage.parse_jobs > 0
        || record.lineage.extraction_results > 0
}

fn artifact_kind_for(record: &MappingPlanRecord) -> ArtifactKind {
    if record.lineage.ocr_artifacts > 0 {
        ArtifactKind::OcrText
    } else if record.lineage.structured_artifacts > 0 || record.lineage.extraction_results > 0 {
        ArtifactKind::FieldExtraction
    } else {
        ArtifactKind::NormalizedPdf
    }
}

fn find_exact_source(
    record: &InventoryRecord,
    scanned_roots: &[ScannedRoot],
) -> Result<(PhysicalFile, usize), Stage2Error> {
    let evidence = record
        .evidence
        .first()
        .ok_or(Stage2Error::ClassificationInvariant)?;
    let expected_checksum = evidence
        .observed_sha256
        .as_deref()
        .ok_or(Stage2Error::ClassificationInvariant)?;
    let mut matches = Vec::new();
    for (root_index, root) in scanned_roots.iter().enumerate() {
        if root.fingerprint.label != evidence.root {
            continue;
        }
        for files in root.index.by_relative.values() {
            for file in files {
                if hash_text(&file.relative_path) == evidence.relative_path_sha256
                    && file.size_bytes == evidence.size_bytes
                    && sha256_file(&file.absolute_path).map_err(map_source_error)?
                        == expected_checksum
                {
                    matches.push((file.clone(), root_index));
                }
            }
        }
    }
    if matches.len() != 1 {
        return Err(Stage2Error::SourceObjectMismatch);
    }
    matches.pop().ok_or(Stage2Error::SourceObjectMismatch)
}

fn copy_exact_source(
    boundary: &RehearsalBoundary,
    config: &InventoryConfig,
    root_index: usize,
    source_file: &PhysicalFile,
    target_path: &Path,
    expected_checksum: &str,
) -> Result<(), Stage2Error> {
    let target_root = config.target_root.join(TARGET_OBJECT_DIRECTORY);
    if !target_path.starts_with(&target_root) {
        return Err(Stage2Error::TargetWrite);
    }
    let data_root = canonical_directory(&config.data_root).map_err(map_source_error)?;
    let source_relative =
        relative_path(&data_root, &source_file.absolute_path).map_err(map_source_error)?;
    let source = boundary
        .read_only_source_at(1)
        .ok_or(Stage2Error::InvalidConfiguration)?
        .open(source_relative)
        .map_err(|_| Stage2Error::SourceRead)?;
    if target_path.exists() {
        let metadata = fs::symlink_metadata(target_path).map_err(|_| Stage2Error::TargetWrite)?;
        if !metadata.is_file() {
            return Err(Stage2Error::TargetConflict);
        }
        let (_, digest) = file_digest(target_path).map_err(map_source_error)?;
        if digest != expected_checksum {
            return Err(Stage2Error::TargetConflict);
        }
        return Ok(());
    }
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|_| Stage2Error::TargetWrite)?;
    }
    let mut source = source;
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target_path)
        .map_err(|_| Stage2Error::TargetWrite)?;
    std::io::copy(&mut source, &mut target).map_err(|_| Stage2Error::TargetWrite)?;
    target.flush().map_err(|_| Stage2Error::TargetWrite)?;
    let (_, digest) = file_digest(target_path).map_err(map_source_error)?;
    if digest != expected_checksum {
        return Err(Stage2Error::TargetConflict);
    }
    let _ = root_index;
    Ok(())
}

async fn persist_document_link(pool: &SqlitePool, link: &DocumentLink) -> Result<(), Stage2Error> {
    let mut connection = pool.acquire().await.map_err(Stage2Error::Database)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
    let result = async {
        sqlx::query(
            "INSERT OR IGNORE INTO document_links \
             (id, tenant_id, document_id, resource_kind, resource_id, role, created_at, created_by) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(link.id().to_string())
        .bind(link.tenant_id().to_string())
        .bind(link.document_id().to_string())
        .bind(link.resource_kind().as_str())
        .bind(link.resource_id().to_string())
        .bind(link.role().as_str())
        .bind(link.created_at().to_rfc3339())
        .bind(link.created_by().to_string())
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
        let existing = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT tenant_id, document_id, resource_kind, resource_id, role, created_at, created_by \
             FROM document_links WHERE id = ?1",
        )
        .bind(link.id().to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
        if existing.0 != link.tenant_id().to_string()
            || existing.1 != link.document_id().to_string()
            || existing.2 != link.resource_kind().as_str()
            || existing.3 != link.resource_id().to_string()
            || existing.4 != link.role().as_str()
            || existing.5 != link.created_at().to_rfc3339()
            || existing.6 != link.created_by().to_string()
        {
            return Err(Stage2Error::TargetConflict);
        }
        Ok::<(), Stage2Error>(())
    }
    .await;
    match result {
        Ok(()) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(Stage2Error::Database)?;
            Ok(())
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn persist_processing_entities(
    pool: &SqlitePool,
    run: &ProcessingRun,
    artifact: &ProcessingArtifact,
    evidence: &Evidence,
) -> Result<(), Stage2Error> {
    let mut connection = pool.acquire().await.map_err(Stage2Error::Database)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
    let result = async {
        sqlx::query(
            "INSERT OR IGNORE INTO document_processing_runs \
             (id, tenant_id, document_revision_id, pipeline_version, parser_name, parser_version,\
              model_provider, model_name, status, started_at, finished_at, failure_code,\
              created_by, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(run.id().to_string())
        .bind(run.tenant_id().to_string())
        .bind(run.document_revision_id().to_string())
        .bind(run.pipeline_version())
        .bind(run.parser_name())
        .bind(run.parser_version())
        .bind(run.model_provider())
        .bind(run.model_name())
        .bind(processing_run_status(run.status()))
        .bind(run.started_at().map(|value| value.to_rfc3339()))
        .bind(run.finished_at().map(|value| value.to_rfc3339()))
        .bind(run.failure_code())
        .bind(run.created_by().to_string())
        .bind(run.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
        let existing_run = sqlx::query_as::<_, (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
        )>(
            "SELECT tenant_id, document_revision_id, pipeline_version, parser_name,
                    parser_version, model_provider, model_name, status, started_at,
                    finished_at, failure_code, created_by, created_at
             FROM document_processing_runs WHERE id = ?1",
        )
        .bind(run.id().to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
        if existing_run.0 != run.tenant_id().to_string()
            || existing_run.1 != run.document_revision_id().to_string()
            || existing_run.2 != run.pipeline_version()
            || existing_run.3 != run.parser_name()
            || existing_run.4 != run.parser_version()
            || existing_run.5.as_deref() != run.model_provider()
            || existing_run.6.as_deref() != run.model_name()
            || existing_run.7 != processing_run_status(run.status())
            || existing_run.8 != run.started_at().map(|value| value.to_rfc3339())
            || existing_run.9 != run.finished_at().map(|value| value.to_rfc3339())
            || existing_run.10.as_deref() != run.failure_code()
            || existing_run.11 != run.created_by().to_string()
            || existing_run.12 != run.created_at().to_rfc3339()
        {
            return Err(Stage2Error::TargetConflict);
        }
        sqlx::query(
            "INSERT OR IGNORE INTO document_processing_artifacts \
             (id, tenant_id, processing_run_id, kind, storage_ref, checksum, schema_version, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(artifact.id().to_string())
        .bind(artifact.tenant_id().to_string())
        .bind(artifact.processing_run_id().to_string())
        .bind(artifact.kind().as_str())
        .bind(artifact.storage_ref())
        .bind(artifact.checksum())
        .bind(artifact.schema_version())
        .bind(artifact.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
        let existing_artifact = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT tenant_id, processing_run_id, kind, storage_ref, checksum,
                    schema_version, created_at
             FROM document_processing_artifacts WHERE id = ?1",
        )
        .bind(artifact.id().to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
        if existing_artifact.0 != artifact.tenant_id().to_string()
            || existing_artifact.1 != artifact.processing_run_id().to_string()
            || existing_artifact.2 != artifact.kind().as_str()
            || existing_artifact.3 != artifact.storage_ref()
            || existing_artifact.4 != artifact.checksum()
            || existing_artifact.5 != artifact.schema_version()
            || existing_artifact.6 != artifact.created_at().to_rfc3339()
        {
            return Err(Stage2Error::TargetConflict);
        }
        let location_json =
            serde_json::to_string(evidence.location()).map_err(Stage2Error::Serialization)?;
        sqlx::query(
            "INSERT OR IGNORE INTO document_processing_evidence \
             (id, tenant_id, document_revision_id, processing_run_id, artifact_id, location_json,\
              source_checksum, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(evidence.id().to_string())
        .bind(evidence.tenant_id().to_string())
        .bind(evidence.document_revision_id().to_string())
        .bind(evidence.processing_run_id().to_string())
        .bind(evidence.artifact_id().to_string())
        .bind(&location_json)
        .bind(evidence.source_checksum())
        .bind(evidence.created_at().to_rfc3339())
        .execute(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?;
        let existing_evidence = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT tenant_id, document_revision_id, processing_run_id, artifact_id,
                    location_json, source_checksum, created_at
             FROM document_processing_evidence WHERE id = ?1",
        )
        .bind(evidence.id().to_string())
        .fetch_optional(&mut *connection)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
        if existing_evidence.0 != evidence.tenant_id().to_string()
            || existing_evidence.1 != evidence.document_revision_id().to_string()
            || existing_evidence.2 != evidence.processing_run_id().to_string()
            || existing_evidence.3 != evidence.artifact_id().to_string()
            || existing_evidence.4 != location_json
            || existing_evidence.5 != evidence.source_checksum()
            || existing_evidence.6 != evidence.created_at().to_rfc3339()
        {
            return Err(Stage2Error::TargetConflict);
        }
        Ok::<(), Stage2Error>(())
    }
    .await;
    match result {
        Ok(()) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(Stage2Error::Database)?;
            Ok(())
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(error)
        }
    }
}

async fn verify_processing_entities(
    pool: &SqlitePool,
    run: &ProcessingRun,
    artifact: &ProcessingArtifact,
    evidence: &Evidence,
) -> Result<(), Stage2Error> {
    let existing_run = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
        ),
    >(
        "SELECT tenant_id, document_revision_id, pipeline_version, parser_name,
                parser_version, model_provider, model_name, status, started_at,
                finished_at, failure_code, created_by, created_at
         FROM document_processing_runs WHERE id = ?1",
    )
    .bind(run.id().to_string())
    .fetch_optional(pool)
    .await
    .map_err(Stage2Error::Database)?
    .ok_or(Stage2Error::TargetConflict)?;
    if existing_run.0 != run.tenant_id().to_string()
        || existing_run.1 != run.document_revision_id().to_string()
        || existing_run.2 != run.pipeline_version()
        || existing_run.3 != run.parser_name()
        || existing_run.4 != run.parser_version()
        || existing_run.5.as_deref() != run.model_provider()
        || existing_run.6.as_deref() != run.model_name()
        || existing_run.7 != processing_run_status(run.status())
        || existing_run.8 != run.started_at().map(|value| value.to_rfc3339())
        || existing_run.9 != run.finished_at().map(|value| value.to_rfc3339())
        || existing_run.10.as_deref() != run.failure_code()
        || existing_run.11 != run.created_by().to_string()
        || existing_run.12 != run.created_at().to_rfc3339()
    {
        return Err(Stage2Error::TargetConflict);
    }

    let existing_artifact =
        sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT tenant_id, processing_run_id, kind, storage_ref, checksum,
                schema_version, created_at
         FROM document_processing_artifacts WHERE id = ?1",
        )
        .bind(artifact.id().to_string())
        .fetch_optional(pool)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
    if existing_artifact.0 != artifact.tenant_id().to_string()
        || existing_artifact.1 != artifact.processing_run_id().to_string()
        || existing_artifact.2 != artifact.kind().as_str()
        || existing_artifact.3 != artifact.storage_ref()
        || existing_artifact.4 != artifact.checksum()
        || existing_artifact.5 != artifact.schema_version()
        || existing_artifact.6 != artifact.created_at().to_rfc3339()
    {
        return Err(Stage2Error::TargetConflict);
    }

    let location_json =
        serde_json::to_string(evidence.location()).map_err(Stage2Error::Serialization)?;
    let existing_evidence =
        sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT tenant_id, document_revision_id, processing_run_id, artifact_id,
                location_json, source_checksum, created_at
         FROM document_processing_evidence WHERE id = ?1",
        )
        .bind(evidence.id().to_string())
        .fetch_optional(pool)
        .await
        .map_err(Stage2Error::Database)?
        .ok_or(Stage2Error::TargetConflict)?;
    if existing_evidence.0 != evidence.tenant_id().to_string()
        || existing_evidence.1 != evidence.document_revision_id().to_string()
        || existing_evidence.2 != evidence.processing_run_id().to_string()
        || existing_evidence.3 != evidence.artifact_id().to_string()
        || existing_evidence.4 != location_json
        || existing_evidence.5 != evidence.source_checksum()
        || existing_evidence.6 != evidence.created_at().to_rfc3339()
    {
        return Err(Stage2Error::TargetConflict);
    }
    Ok(())
}

fn processing_run_status(status: document_processing::ProcessingRunStatus) -> &'static str {
    match status {
        document_processing::ProcessingRunStatus::Queued => "queued",
        document_processing::ProcessingRunStatus::Running => "running",
        document_processing::ProcessingRunStatus::Succeeded => "succeeded",
        document_processing::ProcessingRunStatus::Failed => "failed",
    }
}

fn content_type_for_extension(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

fn parse_fixed_uuid(value: &str) -> Result<Uuid, Stage2Error> {
    Uuid::parse_str(value).map_err(|_| Stage2Error::InvalidConfiguration)
}

fn write_or_verify_audit(
    target_root: &Path,
    plan: &MappingPlan,
    exact_eligible: usize,
    exact_materialized: usize,
    review_count: usize,
    quarantine_count: usize,
    replayed: bool,
) -> Result<(), Stage2Error> {
    let audit_path = target_root.join(AUDIT_FILE_NAME);
    if audit_path.exists() {
        if !replayed {
            return Err(Stage2Error::ManifestConflict);
        }
        let bytes = fs::read(&audit_path).map_err(|_| Stage2Error::TargetWrite)?;
        let mut audit: RehearsalAudit =
            serde_json::from_slice(&bytes).map_err(|_| Stage2Error::ManifestRead)?;
        if audit.audit_schema != AUDIT_SCHEMA
            || audit.mapping_schema != MAPPING_SCHEMA
            || audit.manifest_canonical_sha256 != plan.manifest_canonical_sha256
            || audit.mapping_plan_sha256 != plan.mapping_plan_sha256
            || audit.selected_contracts != plan.records.len()
            || audit.exact_eligible != exact_eligible
            || audit.exact_materialized != exact_materialized
            || audit.review_count != review_count
            || audit.quarantine_count != quarantine_count
            || audit.first_run.status != "frozen"
        {
            return Err(Stage2Error::ManifestDigestMismatch);
        }
        audit.replay_count = audit.replay_count.saturating_add(1);
        audit.last_status = "replayed".to_string();
        write_audit_file(&audit_path, &audit, false)
    } else {
        let audit = RehearsalAudit {
            audit_schema: AUDIT_SCHEMA.to_string(),
            mapping_schema: MAPPING_SCHEMA.to_string(),
            manifest_canonical_sha256: plan.manifest_canonical_sha256.clone(),
            mapping_plan_sha256: plan.mapping_plan_sha256.clone(),
            selected_contracts: plan.records.len(),
            exact_eligible,
            exact_materialized,
            review_count,
            quarantine_count,
            first_run: AuditRun {
                status: "frozen".to_string(),
                selected_contracts: plan.records.len(),
                exact_eligible,
                exact_materialized,
                review_count,
                quarantine_count,
            },
            replay_count: u64::from(replayed),
            last_status: if replayed {
                "replayed_recovered".to_string()
            } else {
                "frozen".to_string()
            },
        };
        write_audit_file(&audit_path, &audit, true)
    }
}

fn write_audit_file(
    path: &Path,
    audit: &RehearsalAudit,
    create_new: bool,
) -> Result<(), Stage2Error> {
    let bytes = serde_json::to_vec_pretty(audit).map_err(Stage2Error::Serialization)?;
    let mut options = OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.truncate(true);
    }
    let mut file = options.open(path).map_err(|_| Stage2Error::TargetWrite)?;
    file.write_all(&bytes)
        .map_err(|_| Stage2Error::TargetWrite)?;
    file.flush().map_err(|_| Stage2Error::TargetWrite)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{
        build_mapping_record, deterministic_uuid, mapping_plan_digest, MappingPlan,
        MappingPlanRecord, MAPPING_SCHEMA,
    };
    use crate::{EvidenceReference, FrozenManifest, InventoryRecord, LineageCount};

    fn record(classification: &str) -> InventoryRecord {
        InventoryRecord {
            selection_rank: 1,
            source_contract_id: 7,
            source_table: "contracts".to_string(),
            source_record_id: 7,
            source_business_key_sha256: Some("a".repeat(64)),
            positive_source_contract_flag: true,
            source_tables: vec!["contracts".to_string()],
            artifact_kinds: Vec::new(),
            classification: classification.to_string(),
            reason_code: "test".to_string(),
            lineage: LineageCount {
                versions: 1,
                attachments: 0,
                artifacts: 0,
                ingestions: 0,
                ingestion_tasks: 0,
                task_files: 0,
                parse_jobs: 0,
                extraction_results: 0,
                legacy_fingerprint_count: 0,
                ingestion_task_results: 0,
                raw_result_links: 0,
                parsed_result_links: 0,
                extracted_result_links: 0,
                ocr_artifacts: 0,
                structured_artifacts: 0,
            },
            evidence: vec![EvidenceReference {
                root: "datasets".to_string(),
                source_table: "contract_versions".to_string(),
                source_record_id: 11,
                relative_path_sha256: "b".repeat(64),
                path_depth: 2,
                extension: Some("pdf".to_string()),
                source_kind: "test".to_string(),
                size_bytes: 10,
                expected_sha256: Some("c".repeat(64)),
                observed_sha256: Some("d".repeat(64)),
            }],
        }
    }

    fn manifest() -> FrozenManifest {
        FrozenManifest {
            manifest_schema: "plan-0009.stage-1.inventory.v8".to_string(),
            selection_rule: "test".to_string(),
            selection_limit: 120,
            source: serde_json::from_value(serde_json::json!({
                "env_file_sha256": "0".repeat(64),
                "database": {"bytes":0,"sha256":"0".repeat(64),"alembic_revision":"test","journal_mode":"wal","integrity_check":"ok","foreign_key_violation_count":0},
                "table_counts": []
            })).unwrap_or_else(|_| unreachable!()),
            physical_roots: Vec::new(),
            classification_counts: Vec::new(),
            source_classification_counts: Vec::new(),
            records: Vec::new(),
            canonical_manifest_sha256: "e".repeat(64),
        }
    }

    #[test]
    fn candidate_ids_are_stable_and_classification_is_fail_closed() {
        let first = deterministic_uuid("document", "e".repeat(64).as_str(), 7, 0);
        let second = deterministic_uuid("document", "e".repeat(64).as_str(), 7, 0);
        assert_eq!(first, second);
        let manifest = manifest();
        let probable =
            build_mapping_record(&manifest, &record("Probable")).unwrap_or_else(|_| unreachable!());
        assert!(!probable.auto_write_allowed);
        assert_eq!(probable.disposition, "manual_review");
        let rejected =
            build_mapping_record(&manifest, &record("Rejected")).unwrap_or_else(|_| unreachable!());
        assert_eq!(rejected.disposition, "quarantine");
    }

    #[test]
    fn mapping_digest_excludes_only_its_embedded_digest() {
        let plan = MappingPlan {
            mapping_schema: MAPPING_SCHEMA.to_string(),
            manifest_schema: "manifest".to_string(),
            manifest_canonical_sha256: "a".repeat(64),
            records: Vec::<MappingPlanRecord>::new(),
            mapping_plan_sha256: String::new(),
        };
        let first = mapping_plan_digest(&plan).unwrap_or_else(|_| unreachable!());
        let mut signed = plan;
        signed.mapping_plan_sha256 = first.clone();
        let second = mapping_plan_digest(&signed).unwrap_or_else(|_| unreachable!());
        assert_eq!(first, second);
    }
}
