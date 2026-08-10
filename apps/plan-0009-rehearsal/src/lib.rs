//! Read-only Stage 1 inventory for PLAN-0009.
//!
//! This crate is intentionally an infrastructure-facing adapter. The shared
//! rehearsal crate owns the production rejection, isolation boundary, and
//! stable classification vocabulary; this crate owns the coverage-first
//! selector and the explicit SQLite/filesystem reads needed to freeze an
//! inventory artifact.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use legacy_migration_rehearsal::{
    BoundaryError, ExecutionMode, InventoryClassification, RehearsalBoundary, SelectionError,
    REHEARSAL_SELECTION_LIMIT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;

mod stage2;

pub use stage2::{run_stage2, Stage2Summary};

const MANIFEST_SCHEMA: &str = "plan-0009.stage-1.inventory.v8";
const MAX_HASH_BYTES: u64 = 128 * 1024 * 1024;
const ENV_DATA_ROOT: &str = "DATA_ROOT";
const ENV_EXTERNAL_ROOT: &str = "CONTRACT_EXTERNAL_ROOT";
const ENV_REPAIR_ROOTS: &str = "CONTRACT_REPAIR_CANDIDATE_ROOTS";
const DATABASE_RELATIVE_PATH: &str = "db/contract_management.db";
const MANIFEST_FILE_NAME: &str = "manifest-v1.json";
const DIGEST_FILE_NAME: &str = "manifest-v1-digests.json";
const AUDIT_FILE_NAME: &str = "replay-audit-v1.json";
const AUDIT_SCHEMA: &str = "plan-0009.stage-1.replay-audit.v1";

const INVENTORY_TABLES: &[(&str, &str)] = &[
    ("contracts", "SELECT COUNT(*) FROM contracts"),
    (
        "contract_versions",
        "SELECT COUNT(*) FROM contract_versions",
    ),
    (
        "contract_attachments",
        "SELECT COUNT(*) FROM contract_attachments",
    ),
    (
        "contract_artifacts",
        "SELECT COUNT(*) FROM contract_artifacts",
    ),
    (
        "contract_ingestions",
        "SELECT COUNT(*) FROM contract_ingestions",
    ),
    (
        "contract_ingestion_tasks",
        "SELECT COUNT(*) FROM contract_ingestion_tasks",
    ),
    (
        "contract_ingestion_task_files",
        "SELECT COUNT(*) FROM contract_ingestion_task_files",
    ),
    (
        "contract_ingestion_task_results",
        "SELECT COUNT(*) FROM contract_ingestion_task_results",
    ),
    (
        "contract_parse_jobs",
        "SELECT COUNT(*) FROM contract_parse_jobs",
    ),
    (
        "extraction_results",
        "SELECT COUNT(*) FROM extraction_results",
    ),
];

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("invalid rehearsal configuration")]
    InvalidConfiguration,
    #[error("source read failed")]
    SourceRead,
    #[error("source path is outside its configured root")]
    SourcePathEscape,
    #[error("rehearsal boundary rejected the operation")]
    Boundary(#[from] BoundaryError),
    #[error("source database read failed")]
    Database(#[source] sqlx::Error),
    #[error("manifest serialization failed")]
    Serialization(#[source] serde_json::Error),
    #[error("manifest already contains a different frozen result")]
    ManifestConflict,
    #[error("manifest digest verification failed")]
    ManifestDigestMismatch,
    #[error("deterministic selection failed")]
    Selection(#[from] SelectionError),
    #[error("source value is unsafe for a relative evidence reference")]
    UnsafeSourceValue,
    #[error("physical source scan failed")]
    PhysicalScan,
    #[error("isolated manifest write failed")]
    TargetWrite,
}

impl InventoryError {
    /// Stable, non-sensitive error code for CLI/audit output.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::SourceRead => "source_read_failed",
            Self::SourcePathEscape => "source_path_escape",
            Self::Boundary(_) => "boundary_rejected",
            Self::Database(_) => "database_read_failed",
            Self::Serialization(_) => "manifest_serialization_failed",
            Self::ManifestConflict => "manifest_conflict",
            Self::ManifestDigestMismatch => "manifest_digest_mismatch",
            Self::Selection(_) => "selection_failed",
            Self::UnsafeSourceValue => "unsafe_source_value",
            Self::PhysicalScan => "physical_scan_failed",
            Self::TargetWrite => "target_write_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryConfig {
    pub legacy_root: PathBuf,
    pub env_file: PathBuf,
    pub data_root: PathBuf,
    pub database_path: PathBuf,
    pub physical_roots: Vec<PhysicalRootConfig>,
    pub isolation_root: PathBuf,
    pub target_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalRootConfig {
    pub label: &'static str,
    pub path: PathBuf,
}

impl InventoryConfig {
    /// Load only the allow-listed local-test variables from the C source env.
    /// Unknown keys are ignored and values are never returned in errors.
    pub fn from_env_file(
        legacy_root: impl Into<PathBuf>,
        env_file: impl Into<PathBuf>,
        isolation_root: impl Into<PathBuf>,
        target_root: impl Into<PathBuf>,
    ) -> Result<Self, InventoryError> {
        let legacy_root = legacy_root.into();
        let env_file = env_file.into();
        let legacy_canonical = canonical_directory(&legacy_root)?;
        let env_canonical = canonical_file(&env_file)?;
        if !env_canonical.starts_with(&legacy_canonical) {
            return Err(InventoryError::SourcePathEscape);
        }
        let text = fs::read_to_string(&env_canonical).map_err(|_| InventoryError::SourceRead)?;
        let values = parse_env(&text);
        let data_root = absolute_path(
            values
                .get(ENV_DATA_ROOT)
                .ok_or(InventoryError::InvalidConfiguration)?,
        )?;
        let external_root = absolute_path(
            values
                .get(ENV_EXTERNAL_ROOT)
                .ok_or(InventoryError::InvalidConfiguration)?,
        )?;
        let repair_value = values
            .get(ENV_REPAIR_ROOTS)
            .ok_or(InventoryError::InvalidConfiguration)?;
        let repair_root = repair_value
            .split([';', ','])
            .map(str::trim)
            .find(|value| !value.is_empty())
            .ok_or(InventoryError::InvalidConfiguration)
            .and_then(absolute_path)?;
        let database_path = data_root.join(DATABASE_RELATIVE_PATH.replace('/', "\\"));

        Ok(Self {
            legacy_root,
            env_file,
            data_root: data_root.clone(),
            database_path,
            physical_roots: vec![
                PhysicalRootConfig {
                    label: "datasets",
                    path: data_root.join("datasets"),
                },
                PhysicalRootConfig {
                    label: "external_contracts",
                    path: external_root,
                },
                PhysicalRootConfig {
                    label: "repair_candidates",
                    path: repair_root,
                },
            ],
            isolation_root: isolation_root.into(),
            target_root: target_root.into(),
        })
    }
}

/// Summary returned without revealing source paths or source business text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySummary {
    pub selected_contracts: usize,
    pub classification_counts: Vec<ClassificationCount>,
    pub canonical_manifest_sha256: String,
    pub file_bytes_sha256: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassificationCount {
    pub classification: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrozenManifest {
    manifest_schema: String,
    selection_rule: String,
    selection_limit: usize,
    source: SourceFingerprint,
    physical_roots: Vec<PhysicalRootFingerprint>,
    classification_counts: Vec<ClassificationCount>,
    source_classification_counts: Vec<ClassificationCount>,
    records: Vec<InventoryRecord>,
    canonical_manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestDigestSidecar {
    manifest_schema: String,
    canonical_manifest_sha256: String,
    file_bytes_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayAudit {
    audit_schema: String,
    canonical_manifest_sha256: String,
    file_bytes_sha256: String,
    selected_contracts: usize,
    classification_counts: Vec<ClassificationCount>,
    first_run: AuditRun,
    replay_count: u64,
    last_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditRun {
    status: String,
    selected_contracts: usize,
    classification_counts: Vec<ClassificationCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceFingerprint {
    env_file_sha256: String,
    database: DatabaseFingerprint,
    table_counts: Vec<TableCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DatabaseFingerprint {
    bytes: u64,
    sha256: String,
    alembic_revision: String,
    journal_mode: String,
    integrity_check: String,
    foreign_key_violation_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TableCount {
    table: String,
    count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PhysicalRootFingerprint {
    label: String,
    file_count: u64,
    total_bytes: u64,
    unsafe_entry_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InventoryRecord {
    selection_rank: usize,
    source_contract_id: i64,
    source_table: String,
    source_record_id: i64,
    source_business_key_sha256: Option<String>,
    positive_source_contract_flag: bool,
    source_tables: Vec<String>,
    artifact_kinds: Vec<String>,
    classification: String,
    reason_code: String,
    lineage: LineageCount,
    evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LineageCount {
    versions: usize,
    attachments: usize,
    artifacts: usize,
    ingestions: usize,
    ingestion_tasks: usize,
    task_files: usize,
    parse_jobs: usize,
    extraction_results: usize,
    legacy_fingerprint_count: usize,
    ingestion_task_results: usize,
    raw_result_links: usize,
    parsed_result_links: usize,
    extracted_result_links: usize,
    ocr_artifacts: usize,
    structured_artifacts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceReference {
    root: String,
    source_table: String,
    source_record_id: i64,
    relative_path_sha256: String,
    path_depth: usize,
    extension: Option<String>,
    source_kind: String,
    size_bytes: u64,
    expected_sha256: Option<String>,
    observed_sha256: Option<String>,
}

#[derive(Debug, Clone)]
struct ContractRow {
    id: i64,
    contract_no: Option<String>,
    file_name: Option<String>,
    has_contract: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct AttachmentRow {
    id: i64,
    contract_id: i64,
    relative_path: Option<String>,
    file_name: Option<String>,
    file_size: Option<i64>,
    sha256: Option<String>,
    storage_location: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct ArtifactRow {
    id: i64,
    contract_id: Option<i64>,
    artifact_kind: Option<String>,
    relative_path: Option<String>,
    file_name: Option<String>,
    file_size: Option<i64>,
    sha256: Option<String>,
    object_key: Option<String>,
    storage_location: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct IngestionRow {
    id: i64,
    contract_id: i64,
    storage_key: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct TaskRow {
    id: i64,
    target_contract_id: Option<i64>,
    completed_contract_id: Option<i64>,
    storage_key: Option<String>,
    source_filename: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct TaskFileRow {
    id: i64,
    task_id: i64,
    relative_path: Option<String>,
    object_key: Option<String>,
    file_name: Option<String>,
    file_size: Option<i64>,
    sha256: Option<String>,
    storage_location: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct TaskResultRow {
    task_id: i64,
    raw_result_file_id: Option<i64>,
    parsed_file_id: Option<i64>,
    extracted_file_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct VersionRow {
    id: i64,
    contract_id: i64,
    original_filename: Option<String>,
    file_size: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct ParseJobRow {
    contract_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct ExtractionRow {
    contract_id: Option<i64>,
    raw_result_file_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct RawCandidate {
    value: String,
    expected_size: Option<i64>,
    expected_sha256: Option<String>,
    source_kind: &'static str,
    source_table: &'static str,
    source_record_id: i64,
}

#[derive(Debug, Default)]
struct Lineage {
    business_key_sha256: Option<String>,
    has_contract: Option<bool>,
    versions: usize,
    attachments: usize,
    artifacts: usize,
    ingestions: usize,
    ingestion_tasks: usize,
    task_files: usize,
    parse_jobs: usize,
    extraction_results: usize,
    legacy_fingerprint_count: usize,
    ingestion_task_results: usize,
    raw_result_links: usize,
    parsed_result_links: usize,
    extracted_result_links: usize,
    ocr_artifacts: usize,
    structured_artifacts: usize,
    artifact_kinds: BTreeSet<String>,
    candidates: Vec<RawCandidate>,
    unsafe_value: bool,
}

#[derive(Debug, Clone)]
struct PhysicalFile {
    root: String,
    relative_path: String,
    absolute_path: PathBuf,
    size_bytes: u64,
}

#[derive(Debug, Default)]
struct PhysicalIndex {
    by_relative: BTreeMap<String, Vec<PhysicalFile>>,
    by_basename: BTreeMap<String, Vec<PhysicalFile>>,
}

#[derive(Debug)]
struct ScannedRoot {
    fingerprint: PhysicalRootFingerprint,
    index: PhysicalIndex,
}

/// Run the read-only inventory and create or verify the frozen manifest.
pub async fn run_inventory(config: &InventoryConfig) -> Result<InventorySummary, InventoryError> {
    validate_target_shape(config)?;
    let legacy_root = canonical_directory(&config.legacy_root)?;
    let data_root = canonical_directory(&config.data_root)?;
    let database_path = canonical_file(&config.database_path)?;
    if !database_path.starts_with(&data_root) {
        return Err(InventoryError::SourcePathEscape);
    }
    let env_file = canonical_file(&config.env_file)?;
    if !env_file.starts_with(&legacy_root) {
        return Err(InventoryError::SourcePathEscape);
    }

    let physical_roots = config
        .physical_roots
        .iter()
        .map(|root| {
            let path = canonical_directory(&root.path)?;
            if !path.starts_with(&data_root) {
                return Err(InventoryError::SourcePathEscape);
            }
            Ok(PhysicalRootConfig {
                label: root.label,
                path,
            })
        })
        .collect::<Result<Vec<_>, InventoryError>>()?;

    let boundary = RehearsalBoundary::validate_sources(
        [&legacy_root, &data_root],
        &config.isolation_root,
        &config.target_root,
        ExecutionMode::Rehearsal,
    )?;
    let legacy_relative = relative_path(&legacy_root, &env_file)?;
    boundary
        .read_only_source_at(0)
        .ok_or(InventoryError::InvalidConfiguration)?
        .open(legacy_relative)
        .map_err(|_| InventoryError::SourceRead)?;
    let database_relative = relative_path(&data_root, &database_path)?;
    boundary
        .read_only_source_at(1)
        .ok_or(InventoryError::InvalidConfiguration)?
        .open(database_relative)
        .map_err(|_| InventoryError::SourceRead)?;

    let scanned_roots = physical_roots
        .iter()
        .map(scan_root)
        .collect::<Result<Vec<_>, InventoryError>>()?;
    let pool = connect_read_only(&database_path).await?;
    let result = inventory_from_source(&pool, &env_file, &database_path, &scanned_roots).await;
    pool.close().await;
    let mut manifest = result?;
    let canonical_digest = manifest_digest(&manifest)?;
    manifest.canonical_manifest_sha256 = canonical_digest.clone();
    let (replayed, file_bytes_digest) = write_or_verify_manifest(&config.target_root, &manifest)?;
    write_or_verify_audit(
        &config.target_root,
        &manifest,
        &canonical_digest,
        &file_bytes_digest,
        replayed,
    )?;
    Ok(InventorySummary {
        selected_contracts: manifest.records.len(),
        classification_counts: manifest.classification_counts,
        canonical_manifest_sha256: canonical_digest,
        file_bytes_sha256: file_bytes_digest,
        replayed,
    })
}

fn validate_target_shape(config: &InventoryConfig) -> Result<(), InventoryError> {
    if config.target_root != config.isolation_root.join("stage-1-inventory-v8") {
        return Err(InventoryError::InvalidConfiguration);
    }
    if !config.isolation_root.is_dir() || !config.target_root.is_dir() {
        return Err(InventoryError::InvalidConfiguration);
    }
    Ok(())
}

async fn connect_read_only(path: &Path) -> Result<SqlitePool, InventoryError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(true)
        .foreign_keys(false);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(InventoryError::Database)
}

async fn inventory_from_source(
    pool: &SqlitePool,
    env_file: &Path,
    database_path: &Path,
    scanned_roots: &[ScannedRoot],
) -> Result<FrozenManifest, InventoryError> {
    let contracts = sqlx::query_as::<_, (i64, Option<String>, Option<String>, Option<String>)>(
        "SELECT id, contract_no, file_name, has_contract FROM contracts ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?
    .into_iter()
    .map(|(id, contract_no, file_name, has_contract)| ContractRow {
        id,
        contract_no,
        file_name,
        has_contract,
    })
    .collect::<Vec<_>>();
    let all_ids = contracts.iter().map(|row| row.id).collect::<Vec<_>>();
    let all_set = all_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut lineages = all_ids
        .iter()
        .copied()
        .map(|id| (id, Lineage::default()))
        .collect::<BTreeMap<_, _>>();
    for row in &contracts {
        if let Some(lineage) = lineages.get_mut(&row.id) {
            lineage.business_key_sha256 = row
                .contract_no
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(hash_text);
            lineage.has_contract = parse_has_contract(row.has_contract.as_deref());
            if let Some(value) = &row.file_name {
                add_candidate(
                    lineage,
                    "contracts",
                    row.id,
                    Some(value.as_str()),
                    None,
                    None,
                    "contract_file_name",
                );
            }
        }
    }

    let versions = sqlx::query_as::<_, VersionRow>(
        "SELECT id, contract_id, original_filename, file_size FROM contract_versions ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in versions {
        if let Some(lineage) = lineages.get_mut(&row.contract_id) {
            lineage.versions += 1;
            if let Some(value) = row.original_filename {
                add_candidate(
                    lineage,
                    "contract_versions",
                    row.id,
                    Some(value.as_str()),
                    row.file_size,
                    None,
                    "contract_version",
                );
            }
        }
    }

    let attachments = sqlx::query_as::<_, AttachmentRow>(
        "SELECT id, contract_id, relative_path, file_name, file_size, sha256, storage_location FROM contract_attachments ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in attachments {
        if let Some(lineage) = lineages.get_mut(&row.contract_id) {
            lineage.attachments += 1;
            add_candidate(
                lineage,
                "contract_attachments",
                row.id,
                row.relative_path.as_deref(),
                row.file_size,
                row.sha256.as_deref(),
                "contract_attachment",
            );
            add_candidate(
                lineage,
                "contract_attachments",
                row.id,
                row.file_name.as_deref(),
                row.file_size,
                row.sha256.as_deref(),
                "contract_attachment_name",
            );
            add_candidate(
                lineage,
                "contract_attachments",
                row.id,
                row.storage_location.as_deref(),
                row.file_size,
                row.sha256.as_deref(),
                "contract_attachment_storage",
            );
        }
    }

    let artifacts = sqlx::query_as::<_, ArtifactRow>(
        "SELECT id, contract_id, artifact_kind, relative_path, file_name, file_size, sha256, object_key, storage_location FROM contract_artifacts ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in artifacts {
        if let Some(contract_id) = row.contract_id {
            if let Some(lineage) = lineages.get_mut(&contract_id) {
                lineage.artifacts += 1;
                if let Some(kind) = row.artifact_kind.as_deref() {
                    lineage.artifact_kinds.insert(kind.to_string());
                    if kind.eq_ignore_ascii_case("OCR_JSON") {
                        lineage.ocr_artifacts += 1;
                    }
                    if matches!(kind, "RAW_JSON" | "EXTRACTED_JSON" | "PARSED_JSON") {
                        lineage.structured_artifacts += 1;
                    }
                }
                add_candidate(
                    lineage,
                    "contract_artifacts",
                    row.id,
                    row.relative_path.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "contract_artifact",
                );
                add_candidate(
                    lineage,
                    "contract_artifacts",
                    row.id,
                    row.file_name.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "contract_artifact_name",
                );
                add_candidate(
                    lineage,
                    "contract_artifacts",
                    row.id,
                    row.object_key.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "contract_artifact_object",
                );
                add_candidate(
                    lineage,
                    "contract_artifacts",
                    row.id,
                    row.storage_location.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "contract_artifact_storage",
                );
            }
        }
    }

    let ingestions = sqlx::query_as::<_, IngestionRow>(
        "SELECT id, contract_id, storage_key FROM contract_ingestions ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in ingestions {
        if let Some(lineage) = lineages.get_mut(&row.contract_id) {
            lineage.ingestions += 1;
            add_candidate(
                lineage,
                "contract_ingestions",
                row.id,
                row.storage_key.as_deref(),
                None,
                None,
                "contract_ingestion",
            );
        }
    }

    let tasks = sqlx::query_as::<_, TaskRow>(
        "SELECT id, target_contract_id, completed_contract_id, storage_key, source_filename FROM contract_ingestion_tasks ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    let mut task_contracts = BTreeMap::new();
    for row in tasks {
        let contract_id = row
            .target_contract_id
            .filter(|id| all_set.contains(id))
            .or_else(|| row.completed_contract_id.filter(|id| all_set.contains(id)));
        if let Some(contract_id) = contract_id {
            if let Some(lineage) = lineages.get_mut(&contract_id) {
                lineage.ingestion_tasks += 1;
                add_candidate(
                    lineage,
                    "contract_ingestion_tasks",
                    row.id,
                    row.storage_key.as_deref(),
                    None,
                    None,
                    "ingestion_task",
                );
                add_candidate(
                    lineage,
                    "contract_ingestion_tasks",
                    row.id,
                    row.source_filename.as_deref(),
                    None,
                    None,
                    "ingestion_task_name",
                );
                task_contracts.insert(row.id, contract_id);
            }
        }
    }

    let task_files = sqlx::query_as::<_, TaskFileRow>(
        "SELECT id, task_id, relative_path, object_key, file_name, file_size, sha256, storage_location FROM contract_ingestion_task_files ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in task_files {
        if let Some(contract_id) = task_contracts.get(&row.task_id).copied() {
            if let Some(lineage) = lineages.get_mut(&contract_id) {
                lineage.task_files += 1;
                add_candidate(
                    lineage,
                    "contract_ingestion_task_files",
                    row.id,
                    row.relative_path.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "ingestion_task_file",
                );
                add_candidate(
                    lineage,
                    "contract_ingestion_task_files",
                    row.id,
                    row.object_key.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "ingestion_task_object",
                );
                add_candidate(
                    lineage,
                    "contract_ingestion_task_files",
                    row.id,
                    row.file_name.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "ingestion_task_name",
                );
                add_candidate(
                    lineage,
                    "contract_ingestion_task_files",
                    row.id,
                    row.storage_location.as_deref(),
                    row.file_size,
                    row.sha256.as_deref(),
                    "ingestion_task_storage",
                );
            }
        }
    }

    let task_results = sqlx::query_as::<_, TaskResultRow>(
        "SELECT task_id, raw_result_file_id, parsed_file_id, extracted_file_id FROM contract_ingestion_task_results ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in task_results {
        if let Some(contract_id) = task_contracts.get(&row.task_id).copied() {
            if let Some(lineage) = lineages.get_mut(&contract_id) {
                lineage.ingestion_task_results += 1;
                if row.raw_result_file_id.is_some() {
                    lineage.raw_result_links += 1;
                }
                if row.parsed_file_id.is_some() {
                    lineage.parsed_result_links += 1;
                }
                if row.extracted_file_id.is_some() {
                    lineage.extracted_result_links += 1;
                }
            }
        }
    }

    let parse_jobs = sqlx::query_as::<_, ParseJobRow>(
        "SELECT contract_id FROM contract_parse_jobs ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in parse_jobs {
        if let Some(contract_id) = row.contract_id {
            if let Some(lineage) = lineages.get_mut(&contract_id) {
                lineage.parse_jobs += 1;
            }
        }
    }

    let extraction_results = sqlx::query_as::<_, ExtractionRow>(
        "SELECT contract_id, raw_result_file_id FROM extraction_results ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(InventoryError::Database)?;
    for row in extraction_results {
        if let Some(contract_id) = row.contract_id {
            if let Some(lineage) = lineages.get_mut(&contract_id) {
                lineage.extraction_results += 1;
                if row.raw_result_file_id.is_some() {
                    lineage.raw_result_links += 1;
                }
            }
        }
    }

    let source = source_fingerprint(pool, env_file, database_path).await?;
    let physical_fingerprints = scanned_roots
        .iter()
        .map(|root| root.fingerprint.clone())
        .collect::<Vec<_>>();
    let mut all_records = BTreeMap::new();
    for (contract_id, lineage) in lineages {
        let record = resolve_record(0, contract_id, lineage, scanned_roots)?;
        all_records.insert(contract_id, record);
    }
    let source_classification_counts =
        classification_counts(&all_records.values().cloned().collect::<Vec<_>>());
    let selected_ids = select_representative_contract_ids(&all_records)?;
    let records = selected_ids
        .iter()
        .enumerate()
        .map(|(index, contract_id)| {
            let mut record = all_records
                .get(contract_id)
                .cloned()
                .ok_or(InventoryError::InvalidConfiguration)?;
            record.selection_rank = index + 1;
            Ok(record)
        })
        .collect::<Result<Vec<_>, InventoryError>>()?;
    let classification_counts = classification_counts(&records);
    Ok(FrozenManifest {
        manifest_schema: MANIFEST_SCHEMA.to_string(),
        selection_rule: "classification and lineage coverage first in fixed order, then positive source contract flag, then contracts.id ASC; manifest order preserves selection rank".to_string(),
        selection_limit: REHEARSAL_SELECTION_LIMIT,
        source,
        physical_roots: physical_fingerprints,
        classification_counts,
        source_classification_counts,
        records,
        canonical_manifest_sha256: String::new(),
    })
}

async fn source_fingerprint(
    pool: &SqlitePool,
    env_file: &Path,
    database_path: &Path,
) -> Result<SourceFingerprint, InventoryError> {
    let alembic_revision = sqlx::query_scalar::<_, String>(
        "SELECT version_num FROM alembic_version ORDER BY version_num LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .map_err(InventoryError::Database)?;
    let journal_mode = sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
        .fetch_one(pool)
        .await
        .map_err(InventoryError::Database)?;
    let integrity_check = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .map_err(InventoryError::Database)?;
    let foreign_key_violation_count = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .map_err(InventoryError::Database)?
        .len();
    let table_counts = futures_table_counts(pool).await?;
    let (database_bytes, database_sha256) = file_digest(database_path)?;
    let env_file_sha256 = file_digest(env_file)?.1;
    Ok(SourceFingerprint {
        env_file_sha256,
        database: DatabaseFingerprint {
            bytes: database_bytes,
            sha256: database_sha256,
            alembic_revision,
            journal_mode,
            integrity_check,
            foreign_key_violation_count,
        },
        table_counts,
    })
}

async fn futures_table_counts(pool: &SqlitePool) -> Result<Vec<TableCount>, InventoryError> {
    let mut counts = Vec::with_capacity(INVENTORY_TABLES.len());
    for (table, query) in INVENTORY_TABLES {
        let count = sqlx::query_scalar::<_, i64>(query)
            .fetch_one(pool)
            .await
            .map_err(InventoryError::Database)?;
        counts.push(TableCount {
            table: (*table).to_string(),
            count,
        });
    }
    Ok(counts)
}

fn parse_env(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() {
                return None;
            }
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| {
                    value
                        .strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                })
                .unwrap_or(value)
                .trim();
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn absolute_path(value: &str) -> Result<PathBuf, InventoryError> {
    let path = PathBuf::from(value.trim());
    if !path.is_absolute() {
        return Err(InventoryError::InvalidConfiguration);
    }
    Ok(path)
}

fn canonical_directory(path: &Path) -> Result<PathBuf, InventoryError> {
    let canonical = fs::canonicalize(path).map_err(|_| InventoryError::SourceRead)?;
    if !canonical.is_dir() {
        return Err(InventoryError::InvalidConfiguration);
    }
    Ok(canonical)
}

fn canonical_file(path: &Path) -> Result<PathBuf, InventoryError> {
    let canonical = fs::canonicalize(path).map_err(|_| InventoryError::SourceRead)?;
    if !canonical.is_file() {
        return Err(InventoryError::InvalidConfiguration);
    }
    Ok(canonical)
}

fn relative_path(root: &Path, path: &Path) -> Result<String, InventoryError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| InventoryError::SourcePathEscape)?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(InventoryError::SourcePathEscape)
            }
        }
    }
    if parts.is_empty() {
        return Err(InventoryError::InvalidConfiguration);
    }
    Ok(parts.join("/"))
}

fn hash_text(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn file_digest(path: &Path) -> Result<(u64, String), InventoryError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(path)
        .map_err(|_| InventoryError::SourceRead)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut bytes = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| InventoryError::SourceRead)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .ok_or(InventoryError::SourceRead)?;
        hasher.update(&buffer[..read]);
    }
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

fn normalize_hash(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(value)
}

fn parse_has_contract(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "0" | "false" | "no" | "n" | "否" | "无" => Some(false),
        "1" | "true" | "yes" | "y" | "是" | "有" => Some(true),
        _ => None,
    }
}

fn add_candidate(
    lineage: &mut Lineage,
    source_table: &'static str,
    source_record_id: i64,
    value: Option<&str>,
    expected_size: Option<i64>,
    expected_sha256: Option<&str>,
    source_kind: &'static str,
) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    if expected_size.is_some_and(|size| size < 0) {
        lineage.unsafe_value = true;
    }
    let expected_sha256 = expected_sha256
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let expected_sha256 = expected_sha256.and_then(|value| {
        let normalized = normalize_hash(value);
        if normalized.is_none() {
            lineage.legacy_fingerprint_count += 1;
        }
        normalized
    });
    lineage.candidates.push(RawCandidate {
        value: value.to_string(),
        expected_size,
        expected_sha256,
        source_kind,
        source_table,
        source_record_id,
    });
}

fn scan_root(root: &PhysicalRootConfig) -> Result<ScannedRoot, InventoryError> {
    let canonical_root = canonical_directory(&root.path)?;
    let mut stack = vec![canonical_root.clone()];
    let mut index = PhysicalIndex::default();
    let mut file_count = 0_u64;
    let mut total_bytes = 0_u64;
    let mut unsafe_entry_count = 0_u64;
    while let Some(directory) = stack.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|_| InventoryError::PhysicalScan)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| InventoryError::PhysicalScan)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let entry_path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|_| InventoryError::PhysicalScan)?;
            if file_type.is_symlink() {
                unsafe_entry_count = unsafe_entry_count.saturating_add(1);
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry_path);
                continue;
            }
            if !file_type.is_file() {
                unsafe_entry_count = unsafe_entry_count.saturating_add(1);
                continue;
            }
            let canonical_file_path =
                fs::canonicalize(&entry_path).map_err(|_| InventoryError::PhysicalScan)?;
            if !canonical_file_path.starts_with(&canonical_root) {
                unsafe_entry_count = unsafe_entry_count.saturating_add(1);
                continue;
            }
            let metadata =
                fs::metadata(&canonical_file_path).map_err(|_| InventoryError::PhysicalScan)?;
            let relative = relative_path(&canonical_root, &canonical_file_path)
                .map_err(|_| InventoryError::PhysicalScan)?;
            let observation = PhysicalFile {
                root: root.label.to_string(),
                relative_path: relative.clone(),
                absolute_path: canonical_file_path,
                size_bytes: metadata.len(),
            };
            file_count = file_count.saturating_add(1);
            total_bytes = total_bytes.saturating_add(metadata.len());
            let relative_key = relative.to_ascii_lowercase();
            index
                .by_relative
                .entry(relative_key)
                .or_default()
                .push(observation.clone());
            if let Some(name) = Path::new(&relative).file_name() {
                index
                    .by_basename
                    .entry(name.to_string_lossy().to_ascii_lowercase())
                    .or_default()
                    .push(observation);
            }
        }
    }
    Ok(ScannedRoot {
        fingerprint: PhysicalRootFingerprint {
            label: root.label.to_string(),
            file_count,
            total_bytes,
            unsafe_entry_count,
        },
        index,
    })
}

#[derive(Debug)]
struct ResolvedMatch {
    observation: PhysicalFile,
    source_kinds: BTreeSet<String>,
    source_records: BTreeSet<(String, i64)>,
    expected_sizes: BTreeSet<i64>,
    expected_hashes: BTreeSet<String>,
}

fn resolve_record(
    selection_rank: usize,
    source_contract_id: i64,
    lineage: Lineage,
    scanned_roots: &[ScannedRoot],
) -> Result<InventoryRecord, InventoryError> {
    let mut matches = BTreeMap::<String, ResolvedMatch>::new();
    let mut unsafe_value = lineage.unsafe_value;
    for candidate in &lineage.candidates {
        let normalized = match normalize_candidate(&candidate.value) {
            Ok(value) => value,
            Err(()) => {
                unsafe_value = true;
                continue;
            }
        };
        let Some(normalized) = normalized else {
            continue;
        };
        let key = normalized.to_ascii_lowercase();
        for root in scanned_roots {
            let observations = if normalized.contains('/') {
                root.index.by_relative.get(&key)
            } else {
                root.index.by_basename.get(&key)
            };
            let Some(observations) = observations else {
                continue;
            };
            for observation in observations {
                let match_key = format!("{}\u{1f}{}", observation.root, observation.relative_path);
                let resolved = matches.entry(match_key).or_insert_with(|| ResolvedMatch {
                    observation: observation.clone(),
                    source_kinds: BTreeSet::new(),
                    source_records: BTreeSet::new(),
                    expected_sizes: BTreeSet::new(),
                    expected_hashes: BTreeSet::new(),
                });
                resolved
                    .source_kinds
                    .insert(candidate.source_kind.to_string());
                resolved.source_records.insert((
                    candidate.source_table.to_string(),
                    candidate.source_record_id,
                ));
                if let Some(size) = candidate.expected_size {
                    resolved.expected_sizes.insert(size);
                }
                if let Some(hash) = &candidate.expected_sha256 {
                    resolved.expected_hashes.insert(hash.clone());
                }
            }
        }
    }

    let lineage_count = LineageCount {
        versions: lineage.versions,
        attachments: lineage.attachments,
        artifacts: lineage.artifacts,
        ingestions: lineage.ingestions,
        ingestion_tasks: lineage.ingestion_tasks,
        task_files: lineage.task_files,
        parse_jobs: lineage.parse_jobs,
        extraction_results: lineage.extraction_results,
        legacy_fingerprint_count: lineage.legacy_fingerprint_count,
        ingestion_task_results: lineage.ingestion_task_results,
        raw_result_links: lineage.raw_result_links,
        parsed_result_links: lineage.parsed_result_links,
        extracted_result_links: lineage.extracted_result_links,
        ocr_artifacts: lineage.ocr_artifacts,
        structured_artifacts: lineage.structured_artifacts,
    };
    let has_no_lineage = lineage_count.versions == 0
        && lineage_count.attachments == 0
        && lineage_count.artifacts == 0
        && lineage_count.ingestions == 0
        && lineage_count.ingestion_tasks == 0
        && lineage_count.task_files == 0
        && lineage_count.parse_jobs == 0
        && lineage_count.extraction_results == 0;

    let (classification, reason_code) = if lineage.has_contract == Some(false) {
        (InventoryClassification::Rejected, "has_contract_false")
    } else if unsafe_value {
        (
            InventoryClassification::Rejected,
            "unsafe_source_path_or_fingerprint",
        )
    } else if lineage.candidates.is_empty() && has_no_lineage {
        (InventoryClassification::Orphan, "no_source_lineage")
    } else if matches.is_empty() {
        (InventoryClassification::Missing, "no_physical_match")
    } else if matches.len() > 1 {
        (
            InventoryClassification::Ambiguous,
            "multiple_physical_matches",
        )
    } else {
        let resolved = matches
            .values()
            .next()
            .ok_or(InventoryError::PhysicalScan)?;
        if resolved.expected_sizes.len() > 1 || resolved.expected_hashes.len() > 1 {
            (
                InventoryClassification::Conflict,
                "conflicting_source_fingerprints",
            )
        } else if resolved
            .expected_sizes
            .first()
            .is_some_and(|size| *size < 0 || *size as u64 != resolved.observation.size_bytes)
        {
            (InventoryClassification::Conflict, "source_size_mismatch")
        } else if let Some(expected_hash) = resolved.expected_hashes.first() {
            if resolved.observation.size_bytes > MAX_HASH_BYTES {
                (
                    InventoryClassification::Probable,
                    "hash_deferred_for_large_file",
                )
            } else if sha256_file(&resolved.observation.absolute_path)? == *expected_hash {
                (InventoryClassification::Exact, "content_sha256_match")
            } else {
                (InventoryClassification::Conflict, "content_sha256_mismatch")
            }
        } else if resolved.expected_sizes.first().is_some() {
            (InventoryClassification::Probable, "path_and_size_match")
        } else {
            (
                InventoryClassification::Probable,
                "physical_path_match_without_fingerprint",
            )
        }
    };

    let match_count = matches.len();
    let mut evidence = matches
        .into_values()
        .map(|resolved| {
            let relative_path = resolved.observation.relative_path;
            let (path_sha256, path_depth, extension) = evidence_path_metadata(&relative_path);
            let (source_table, source_record_id) = resolved
                .source_records
                .into_iter()
                .next()
                .ok_or(InventoryError::PhysicalScan)?;
            let observed_sha256 =
                if match_count == 1 && resolved.observation.size_bytes <= MAX_HASH_BYTES {
                    Some(sha256_file(&resolved.observation.absolute_path)?)
                } else {
                    None
                };
            Ok(EvidenceReference {
                root: resolved.observation.root,
                source_table,
                source_record_id,
                relative_path_sha256: path_sha256,
                path_depth,
                extension,
                source_kind: if resolved.source_kinds.len() == 1 {
                    resolved
                        .source_kinds
                        .into_iter()
                        .next()
                        .ok_or(InventoryError::PhysicalScan)?
                } else {
                    "multiple".to_string()
                },
                size_bytes: resolved.observation.size_bytes,
                expected_sha256: resolved.expected_hashes.into_iter().next(),
                observed_sha256,
            })
        })
        .collect::<Result<Vec<_>, InventoryError>>()?;
    evidence.sort_by(|left, right| {
        left.root
            .cmp(&right.root)
            .then_with(|| left.relative_path_sha256.cmp(&right.relative_path_sha256))
    });
    let source_tables = source_tables_for_lineage(&lineage_count);
    let artifact_kinds = lineage.artifact_kinds.into_iter().collect();
    Ok(InventoryRecord {
        selection_rank,
        source_contract_id,
        source_table: "contracts".to_string(),
        source_record_id: source_contract_id,
        source_business_key_sha256: lineage.business_key_sha256,
        positive_source_contract_flag: lineage.has_contract == Some(true),
        source_tables,
        artifact_kinds,
        classification: classification.as_str().to_string(),
        reason_code: reason_code.to_string(),
        lineage: lineage_count,
        evidence,
    })
}

fn normalize_candidate(value: &str) -> Result<Option<String>, ()> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty() {
        return Ok(None);
    }
    if value.starts_with('/')
        || value.starts_with("//")
        || value.as_bytes().get(1).is_some_and(|byte| *byte == b':')
        || value.contains("://")
    {
        return Err(());
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(()),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Ok(None);
    }
    Ok(Some(parts.join("/")))
}

fn evidence_path_metadata(relative_path: &str) -> (String, usize, Option<String>) {
    let path = Path::new(relative_path);
    (
        hash_text(relative_path),
        path.components().count(),
        path.extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase()),
    )
}

fn source_tables_for_lineage(lineage: &LineageCount) -> Vec<String> {
    let mut tables = vec!["contracts".to_string()];
    if lineage.versions > 0 {
        tables.push("contract_versions".to_string());
    }
    if lineage.attachments > 0 {
        tables.push("contract_attachments".to_string());
    }
    if lineage.artifacts > 0 {
        tables.push("contract_artifacts".to_string());
    }
    if lineage.ingestions > 0 {
        tables.push("contract_ingestions".to_string());
    }
    if lineage.ingestion_tasks > 0 {
        tables.push("contract_ingestion_tasks".to_string());
    }
    if lineage.task_files > 0 {
        tables.push("contract_ingestion_task_files".to_string());
    }
    if lineage.ingestion_task_results > 0 {
        tables.push("contract_ingestion_task_results".to_string());
    }
    if lineage.parse_jobs > 0 {
        tables.push("contract_parse_jobs".to_string());
    }
    if lineage.extraction_results > 0 {
        tables.push("extraction_results".to_string());
    }
    tables
}

fn select_representative_contract_ids(
    records: &BTreeMap<i64, InventoryRecord>,
) -> Result<Vec<i64>, SelectionError> {
    if records.len() < REHEARSAL_SELECTION_LIMIT {
        return Err(SelectionError::TooFewContracts);
    }
    let mut selected = BTreeSet::new();
    let mut order = Vec::new();
    for classification in InventoryClassification::ALL {
        if let Some(record) = records
            .values()
            .find(|record| record.classification == classification.as_str())
        {
            add_selected(&mut selected, &mut order, record.source_contract_id);
        }
    }
    let feature_matches: [fn(&InventoryRecord) -> bool; 8] = [
        |record: &InventoryRecord| record.lineage.versions > 1,
        |record: &InventoryRecord| record.lineage.attachments > 1,
        |record: &InventoryRecord| record.lineage.parse_jobs > 0,
        |record: &InventoryRecord| record.lineage.extraction_results > 0,
        |record: &InventoryRecord| record.lineage.task_files > 0,
        |record: &InventoryRecord| record.evidence.len() > 1,
        |record: &InventoryRecord| record.lineage.ocr_artifacts > 0,
        |record: &InventoryRecord| {
            record
                .artifact_kinds
                .iter()
                .any(|kind| kind == "EXTRACTED_JSON")
        },
    ];
    for predicate in feature_matches {
        if let Some(record) = records.values().find(|record| predicate(record)) {
            add_selected(&mut selected, &mut order, record.source_contract_id);
        }
    }
    for record in records
        .values()
        .filter(|record| record.positive_source_contract_flag)
    {
        if order.len() == REHEARSAL_SELECTION_LIMIT {
            break;
        }
        add_selected(&mut selected, &mut order, record.source_contract_id);
    }
    for source_contract_id in records.keys() {
        if order.len() == REHEARSAL_SELECTION_LIMIT {
            break;
        }
        add_selected(&mut selected, &mut order, *source_contract_id);
    }
    Ok(order)
}

fn add_selected(selected: &mut BTreeSet<i64>, order: &mut Vec<i64>, source_contract_id: i64) {
    if selected.insert(source_contract_id) {
        order.push(source_contract_id);
    }
}

fn sha256_file(path: &Path) -> Result<String, InventoryError> {
    file_digest(path).map(|(_, digest)| digest)
}

fn classification_counts(records: &[InventoryRecord]) -> Vec<ClassificationCount> {
    InventoryClassification::ALL
        .into_iter()
        .map(|classification| ClassificationCount {
            classification: classification.as_str().to_string(),
            count: records
                .iter()
                .filter(|record| record.classification == classification.as_str())
                .count(),
        })
        .collect()
}

fn manifest_digest(manifest: &FrozenManifest) -> Result<String, InventoryError> {
    let mut unsigned = manifest.clone();
    unsigned.canonical_manifest_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(InventoryError::Serialization)?;
    Ok(sha256_bytes(&bytes))
}

fn write_or_verify_manifest(
    target_root: &Path,
    manifest: &FrozenManifest,
) -> Result<(bool, String), InventoryError> {
    let manifest_path = target_root.join(MANIFEST_FILE_NAME);
    let digest_path = target_root.join(DIGEST_FILE_NAME);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(InventoryError::Serialization)?;
    let generated_file_digest = sha256_bytes(&bytes);
    if manifest_path.exists() {
        let existing_bytes = fs::read(&manifest_path).map_err(|_| InventoryError::TargetWrite)?;
        let existing: FrozenManifest =
            serde_json::from_slice(&existing_bytes).map_err(InventoryError::Serialization)?;
        let existing_digest = manifest_digest(&existing)?;
        if existing.canonical_manifest_sha256 != existing_digest {
            return Err(InventoryError::ManifestDigestMismatch);
        }
        if existing.canonical_manifest_sha256 != manifest.canonical_manifest_sha256 {
            return Err(InventoryError::ManifestConflict);
        }
        let sidecar_bytes = fs::read(&digest_path).map_err(|_| InventoryError::ManifestConflict)?;
        let sidecar: ManifestDigestSidecar =
            serde_json::from_slice(&sidecar_bytes).map_err(InventoryError::Serialization)?;
        let existing_file_digest = sha256_bytes(&existing_bytes);
        if sidecar.manifest_schema != MANIFEST_SCHEMA
            || sidecar.canonical_manifest_sha256 != existing.canonical_manifest_sha256
            || sidecar.file_bytes_sha256 != existing_file_digest
        {
            return Err(InventoryError::ManifestDigestMismatch);
        }
        return Ok((true, existing_file_digest));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&manifest_path)
        .map_err(|_| InventoryError::TargetWrite)?;
    file.write_all(&bytes)
        .map_err(|_| InventoryError::TargetWrite)?;
    file.flush().map_err(|_| InventoryError::TargetWrite)?;
    let sidecar = ManifestDigestSidecar {
        manifest_schema: MANIFEST_SCHEMA.to_string(),
        canonical_manifest_sha256: manifest.canonical_manifest_sha256.clone(),
        file_bytes_sha256: generated_file_digest.clone(),
    };
    let sidecar_bytes =
        serde_json::to_vec_pretty(&sidecar).map_err(InventoryError::Serialization)?;
    let mut digest_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&digest_path)
        .map_err(|_| InventoryError::TargetWrite)?;
    digest_file
        .write_all(&sidecar_bytes)
        .map_err(|_| InventoryError::TargetWrite)?;
    digest_file
        .flush()
        .map_err(|_| InventoryError::TargetWrite)?;
    Ok((false, generated_file_digest))
}

fn write_or_verify_audit(
    target_root: &Path,
    manifest: &FrozenManifest,
    canonical_digest: &str,
    file_bytes_digest: &str,
    replayed: bool,
) -> Result<(), InventoryError> {
    let audit_path = target_root.join(AUDIT_FILE_NAME);
    let counts = manifest.classification_counts.clone();
    let selected_contracts = manifest.records.len();
    if audit_path.exists() {
        if !replayed {
            return Err(InventoryError::ManifestConflict);
        }
        let bytes = fs::read(&audit_path).map_err(|_| InventoryError::TargetWrite)?;
        let mut audit: ReplayAudit =
            serde_json::from_slice(&bytes).map_err(InventoryError::Serialization)?;
        if audit.audit_schema != AUDIT_SCHEMA
            || audit.canonical_manifest_sha256 != canonical_digest
            || audit.file_bytes_sha256 != file_bytes_digest
            || audit.selected_contracts != selected_contracts
            || audit.classification_counts != counts
            || audit.first_run.status != "frozen"
            || audit.first_run.selected_contracts != selected_contracts
            || audit.first_run.classification_counts != counts
        {
            return Err(InventoryError::ManifestDigestMismatch);
        }
        audit.replay_count = audit.replay_count.saturating_add(1);
        audit.last_status = "replayed".to_string();
        write_audit_file(&audit_path, &audit, false)
    } else {
        if replayed {
            return Err(InventoryError::ManifestConflict);
        }
        let audit = ReplayAudit {
            audit_schema: AUDIT_SCHEMA.to_string(),
            canonical_manifest_sha256: canonical_digest.to_string(),
            file_bytes_sha256: file_bytes_digest.to_string(),
            selected_contracts,
            classification_counts: counts.clone(),
            first_run: AuditRun {
                status: "frozen".to_string(),
                selected_contracts,
                classification_counts: counts,
            },
            replay_count: 0,
            last_status: "frozen".to_string(),
        };
        write_audit_file(&audit_path, &audit, true)
    }
}

fn write_audit_file(
    path: &Path,
    audit: &ReplayAudit,
    create_new: bool,
) -> Result<(), InventoryError> {
    let bytes = serde_json::to_vec_pretty(audit).map_err(InventoryError::Serialization)?;
    let mut options = OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(path)
        .map_err(|_| InventoryError::TargetWrite)?;
    file.write_all(&bytes)
        .map_err(|_| InventoryError::TargetWrite)?;
    file.flush().map_err(|_| InventoryError::TargetWrite)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        classification_counts, evidence_path_metadata, manifest_digest, normalize_candidate,
        parse_env, select_representative_contract_ids, write_or_verify_audit,
        write_or_verify_manifest, DatabaseFingerprint, EvidenceReference, FrozenManifest,
        InventoryRecord, LineageCount, SourceFingerprint, AUDIT_FILE_NAME, MANIFEST_SCHEMA,
    };
    use legacy_migration_rehearsal::{InventoryClassification, REHEARSAL_SELECTION_LIMIT};

    #[test]
    fn env_parser_reads_only_key_value_pairs_and_strips_quotes() {
        let values = parse_env("# comment\nDATA_ROOT=\"D:/data\"\nOTHER=ignored\n");
        assert_eq!(values.get("DATA_ROOT"), Some(&"D:/data".to_string()));
        assert_eq!(values.get("OTHER"), Some(&"ignored".to_string()));
    }

    #[test]
    fn candidate_paths_reject_absolute_and_parent_escape() {
        assert!(normalize_candidate("D:/outside/file.pdf").is_err());
        assert!(normalize_candidate("../outside/file.pdf").is_err());
        assert_eq!(
            normalize_candidate("./folder\\file.pdf"),
            Ok(Some("folder/file.pdf".to_string()))
        );
    }

    #[test]
    fn classification_count_order_is_the_manifest_order() {
        let records = Vec::new();
        let counts = classification_counts(&records);
        assert_eq!(counts.len(), InventoryClassification::ALL.len());
        assert_eq!(counts[0].classification, "Exact");
        assert_eq!(REHEARSAL_SELECTION_LIMIT, 120);
    }

    #[test]
    fn app_selector_prioritizes_coverage_before_id_order() {
        let mut records = BTreeMap::new();
        for id in 1_i64..=120 {
            records.insert(id, test_record(id, "Ambiguous"));
        }
        for (id, classification) in [(200, "Exact"), (201, "Rejected")] {
            records.insert(id, test_record(id, classification));
        }
        for id in 202_i64..=210 {
            records.insert(id, test_record(id, "Ambiguous"));
        }
        records
            .get_mut(&202)
            .expect("versions fixture")
            .lineage
            .versions = 2;
        records
            .get_mut(&203)
            .expect("attachments fixture")
            .lineage
            .attachments = 2;
        records
            .get_mut(&204)
            .expect("parse fixture")
            .lineage
            .parse_jobs = 1;
        records
            .get_mut(&205)
            .expect("extraction fixture")
            .lineage
            .extraction_results = 1;
        records
            .get_mut(&206)
            .expect("task file fixture")
            .lineage
            .task_files = 1;
        records.get_mut(&207).expect("evidence fixture").evidence =
            vec![test_evidence(), test_evidence()];
        records
            .get_mut(&208)
            .expect("ocr fixture")
            .lineage
            .ocr_artifacts = 1;
        records
            .get_mut(&209)
            .expect("structured fixture")
            .artifact_kinds
            .push("EXTRACTED_JSON".to_string());
        records
            .get_mut(&210)
            .expect("positive flag fixture")
            .positive_source_contract_flag = true;

        let selected = select_representative_contract_ids(&records).expect("120 records");

        assert_eq!(selected.len(), REHEARSAL_SELECTION_LIMIT);
        assert_eq!(
            &selected[..12],
            &[200, 1, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210]
        );
        assert_ne!(selected, (1_i64..=120).collect::<Vec<_>>());
    }

    #[test]
    fn evidence_path_metadata_does_not_return_raw_path() {
        let (digest, depth, extension) = evidence_path_metadata("root/Company Name/contract.doc");
        assert_eq!(digest.len(), 64);
        assert_eq!(depth, 3);
        assert_eq!(extension.as_deref(), Some("doc"));
        assert!(!digest.contains("Company"));
    }

    fn test_record(id: i64, classification: &str) -> InventoryRecord {
        InventoryRecord {
            selection_rank: 0,
            source_contract_id: id,
            source_table: "contracts".to_string(),
            source_record_id: id,
            source_business_key_sha256: None,
            positive_source_contract_flag: false,
            source_tables: vec!["contracts".to_string()],
            artifact_kinds: Vec::new(),
            classification: classification.to_string(),
            reason_code: "test".to_string(),
            lineage: LineageCount {
                versions: 0,
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
            evidence: Vec::new(),
        }
    }

    fn test_evidence() -> EvidenceReference {
        EvidenceReference {
            root: "datasets".to_string(),
            source_table: "contract_attachments".to_string(),
            source_record_id: 1,
            relative_path_sha256: "a".repeat(64),
            path_depth: 1,
            extension: Some("pdf".to_string()),
            source_kind: "test".to_string(),
            size_bytes: 1,
            expected_sha256: None,
            observed_sha256: None,
        }
    }

    #[test]
    fn manifest_and_replay_audit_fail_closed_on_corruption() {
        let target =
            std::env::temp_dir().join(format!("plan-0009-stage1-audit-{}", std::process::id()));
        std::fs::create_dir_all(&target).expect("create audit target");
        let mut manifest = FrozenManifest {
            manifest_schema: MANIFEST_SCHEMA.to_string(),
            selection_rule: "test".to_string(),
            selection_limit: 120,
            source: SourceFingerprint {
                env_file_sha256: "0".repeat(64),
                database: DatabaseFingerprint {
                    bytes: 0,
                    sha256: "0".repeat(64),
                    alembic_revision: "test".to_string(),
                    journal_mode: "wal".to_string(),
                    integrity_check: "ok".to_string(),
                    foreign_key_violation_count: 0,
                },
                table_counts: Vec::new(),
            },
            physical_roots: Vec::new(),
            classification_counts: Vec::new(),
            source_classification_counts: Vec::new(),
            records: Vec::new(),
            canonical_manifest_sha256: String::new(),
        };
        manifest.canonical_manifest_sha256 = manifest_digest(&manifest).expect("digest");
        let (replayed, file_digest) =
            write_or_verify_manifest(&target, &manifest).expect("freeze manifest");
        assert!(!replayed);
        write_or_verify_audit(
            &target,
            &manifest,
            &manifest.canonical_manifest_sha256,
            &file_digest,
            false,
        )
        .expect("freeze audit");
        let (replayed, replay_file_digest) =
            write_or_verify_manifest(&target, &manifest).expect("replay manifest");
        assert!(replayed);
        assert_eq!(file_digest, replay_file_digest);
        write_or_verify_audit(
            &target,
            &manifest,
            &manifest.canonical_manifest_sha256,
            &replay_file_digest,
            true,
        )
        .expect("replay audit");
        let audit_path = target.join(AUDIT_FILE_NAME);
        std::fs::write(&audit_path, b"corrupt").expect("corrupt test audit");
        assert!(write_or_verify_audit(
            &target,
            &manifest,
            &manifest.canonical_manifest_sha256,
            &replay_file_digest,
            true,
        )
        .is_err());
        std::fs::remove_dir_all(target).expect("remove audit target");
    }

    #[test]
    fn file_digest_handles_large_inputs_without_stack_allocation() {
        let path = std::env::temp_dir().join(format!(
            "plan-0009-stage1-digest-{}.bin",
            std::process::id()
        ));
        let bytes = vec![0_u8; 2 * 1024 * 1024];
        std::fs::write(&path, &bytes).expect("write test input");
        let (size, digest) = super::file_digest(&path).expect("read test input");
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(digest.len(), 64);
        std::fs::remove_file(path).expect("remove test input");
    }
}
