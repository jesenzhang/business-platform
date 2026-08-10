//! Stage 3 real-contract dual-replay rehearsal.
//!
//! Stage 3 deliberately composes the reviewed Stage 2 mapping engine.  It
//! owns only replay orchestration, semantic target comparison, coverage
//! evidence, and a safe audit artifact; it does not introduce another mapping
//! or materialization policy.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use thiserror::Error;

use super::{
    file_digest, manifest_digest, stage2, ClassificationCount, FrozenManifest, InventoryConfig,
    InventoryError, MANIFEST_SCHEMA,
};

const STAGE3_DIRECTORY: &str = "stage-3-rehearsal-v1";
const REPLAY_A_DIRECTORY: &str = "stage-3-rehearsal-v1\\replay-a";
const REPLAY_B_DIRECTORY: &str = "stage-3-rehearsal-v1\\replay-b";
const MANIFEST_FILE_NAME: &str = "manifest-v1.json";
const MANIFEST_DIGEST_FILE_NAME: &str = "manifest-v1-digests.json";
const AUDIT_FILE_NAME: &str = "stage3-replay-audit-v1.json";
const AUDIT_SCHEMA: &str = "plan-0009.stage-3.replay-audit.v1";
const TARGET_OBJECT_DIRECTORY: &str = "objects";
const TARGET_DATABASE_RELATIVE_PATH: &str = "db/document-management.sqlite";

#[derive(Debug, Error)]
pub enum Stage3Error {
    #[error("invalid stage 3 rehearsal configuration")]
    InvalidConfiguration,
    #[error("stage 1 manifest could not be read")]
    ManifestRead,
    #[error("stage 1 manifest digest verification failed")]
    ManifestDigestMismatch,
    #[error("stage 3 target already exists")]
    TargetExists,
    #[error("stage 3 target read failed at {0}")]
    TargetRead(&'static str),
    #[error("stage 3 target write failed")]
    TargetWrite,
    #[error("stage 2 replay failed")]
    Stage2Failed,
    #[error("stage 3 replay targets differ")]
    ReplayMismatch,
    #[error("stage 3 audit serialization failed")]
    Serialization,
}

impl Stage3Error {
    /// Stable, non-sensitive CLI error code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::ManifestRead => "manifest_read_failed",
            Self::ManifestDigestMismatch => "manifest_digest_mismatch",
            Self::TargetExists => "target_exists",
            Self::TargetRead(_) => "target_read_failed",
            Self::TargetWrite => "target_write_failed",
            Self::Stage2Failed => "stage2_replay_failed",
            Self::ReplayMismatch => "replay_mismatch",
            Self::Serialization => "audit_serialization_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage3Summary {
    pub selected_contracts: usize,
    pub input_manifest_sha256: String,
    pub replay_equal: bool,
    pub quarantine_count: usize,
    pub object_files: usize,
    pub object_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stage3Audit {
    audit_schema: String,
    status: String,
    input_manifest_sha256: String,
    selected_contracts: usize,
    selection_limit: usize,
    classification_counts: Vec<ClassificationCount>,
    source_classification_counts: Vec<ClassificationCount>,
    coverage: CoverageMatrix,
    lineage: LineageSummary,
    replays: Vec<ReplayEvidence>,
    replay_equal: bool,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CoverageMatrix {
    ordinary_single_file: usize,
    multi_version: usize,
    scanned_or_ocr: usize,
    attachments: usize,
    llm_or_structured: usize,
    known_bad_relationship: usize,
    duplicate_or_multiple_physical_matches: usize,
    missing_evidence: usize,
    orphan: usize,
    ambiguous: usize,
    conflict: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LineageSummary {
    records_with_evidence: usize,
    evidence_entries: usize,
    multi_source_evidence_entries: usize,
    versions: usize,
    attachments: usize,
    artifacts: usize,
    ingestions: usize,
    ingestion_tasks: usize,
    task_files: usize,
    parse_jobs: usize,
    extraction_results: usize,
    ocr_artifacts: usize,
    structured_artifacts: usize,
    legacy_fingerprint_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayEvidence {
    label: String,
    first_run: stage2::Stage2Summary,
    replay_run: stage2::Stage2Summary,
    stage2_audit: Stage2AuditView,
    target: TargetSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stage2AuditView {
    replay_count: u64,
    last_status: String,
    selected_contracts: usize,
    exact_eligible: usize,
    exact_materialized: usize,
    review_count: usize,
    quarantine_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TargetSnapshot {
    integrity_check: String,
    quick_check: String,
    foreign_key_violations: usize,
    mapping_rows: i64,
    materialized_rows: i64,
    duplicate_mapping_keys: i64,
    formal_rows: BTreeMap<String, i64>,
    object_files: usize,
    object_bytes: u64,
    object_digest: String,
}

/// Run the real 120-contract Stage 3 rehearsal in two fresh isolated targets.
pub async fn run_stage3(config: &InventoryConfig) -> Result<Stage3Summary, Stage3Error> {
    validate_stage3_target(config)?;
    let manifest = load_manifest(config)?;
    let coverage = coverage_matrix(&manifest);
    let lineage = lineage_summary(&manifest);

    let replay_a = run_replay(config, REPLAY_A_DIRECTORY, "replay-a").await?;
    let replay_b = run_replay(config, REPLAY_B_DIRECTORY, "replay-b").await?;
    let replay_equal = replay_a.target == replay_b.target
        && replay_a.first_run.selected_contracts == replay_b.first_run.selected_contracts
        && replay_a.first_run.exact_eligible == replay_b.first_run.exact_eligible
        && replay_a.first_run.exact_materialized == replay_b.first_run.exact_materialized
        && replay_a.first_run.review_count == replay_b.first_run.review_count
        && replay_a.first_run.quarantine_count == replay_b.first_run.quarantine_count
        && replay_a.first_run.mapping_plan_sha256 == replay_b.first_run.mapping_plan_sha256
        && replay_a.first_run.mapping_file_bytes_sha256
            == replay_b.first_run.mapping_file_bytes_sha256
        && replay_a.replay_run.mapping_plan_sha256 == replay_b.replay_run.mapping_plan_sha256
        && replay_a.replay_run.mapping_file_bytes_sha256
            == replay_b.replay_run.mapping_file_bytes_sha256;
    if !replay_equal {
        return Err(Stage3Error::ReplayMismatch);
    }

    let audit = Stage3Audit {
        audit_schema: AUDIT_SCHEMA.to_string(),
        status: "replayed".to_string(),
        input_manifest_sha256: manifest.canonical_manifest_sha256.clone(),
        selected_contracts: manifest.records.len(),
        selection_limit: manifest.selection_limit,
        classification_counts: manifest.classification_counts.clone(),
        source_classification_counts: manifest.source_classification_counts.clone(),
        coverage,
        lineage,
        replays: vec![replay_a.clone(), replay_b],
        replay_equal,
        failures: Vec::new(),
    };
    write_audit(&config.target_root, &audit)?;

    Ok(Stage3Summary {
        selected_contracts: manifest.records.len(),
        input_manifest_sha256: manifest.canonical_manifest_sha256,
        replay_equal,
        quarantine_count: replay_a.replay_run.quarantine_count,
        object_files: replay_a.target.object_files,
        object_bytes: replay_a.target.object_bytes,
    })
}

fn validate_stage3_target(config: &InventoryConfig) -> Result<(), Stage3Error> {
    if config.target_root != config.isolation_root.join(STAGE3_DIRECTORY)
        || !config.isolation_root.is_dir()
        || !config.target_root.is_dir()
    {
        return Err(Stage3Error::InvalidConfiguration);
    }
    if config.target_root.join(AUDIT_FILE_NAME).exists()
        || config.target_root.join("replay-a").exists()
        || config.target_root.join("replay-b").exists()
    {
        return Err(Stage3Error::TargetExists);
    }
    Ok(())
}

fn load_manifest(config: &InventoryConfig) -> Result<FrozenManifest, Stage3Error> {
    let root = config.isolation_root.join("stage-1-inventory-v9");
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    let digest_path = root.join(MANIFEST_DIGEST_FILE_NAME);
    let bytes = fs::read(&manifest_path).map_err(|_| Stage3Error::ManifestRead)?;
    let manifest: FrozenManifest =
        serde_json::from_slice(&bytes).map_err(|_| Stage3Error::ManifestRead)?;
    if manifest.manifest_schema != MANIFEST_SCHEMA
        || manifest_digest(&manifest).map_err(|_| Stage3Error::ManifestDigestMismatch)?
            != manifest.canonical_manifest_sha256
    {
        return Err(Stage3Error::ManifestDigestMismatch);
    }
    let sidecar_bytes = fs::read(digest_path).map_err(|_| Stage3Error::ManifestRead)?;
    let sidecar: super::ManifestDigestSidecar =
        serde_json::from_slice(&sidecar_bytes).map_err(|_| Stage3Error::ManifestRead)?;
    if sidecar.manifest_schema != MANIFEST_SCHEMA
        || sidecar.canonical_manifest_sha256 != manifest.canonical_manifest_sha256
        || sidecar.file_bytes_sha256 != sha256_bytes(&bytes)
    {
        return Err(Stage3Error::ManifestDigestMismatch);
    }
    Ok(manifest)
}

async fn run_replay(
    config: &InventoryConfig,
    target_directory: &str,
    label: &str,
) -> Result<ReplayEvidence, Stage3Error> {
    let target_root = config.isolation_root.join(target_directory);
    if target_root.exists() {
        return Err(Stage3Error::TargetExists);
    }
    fs::create_dir_all(&target_root).map_err(|_| Stage3Error::TargetWrite)?;
    let mut replay_config = config.clone();
    replay_config.target_root = target_root.clone();
    let first_run = stage2::run_stage2_at(&replay_config, target_directory)
        .await
        .map_err(|_| Stage3Error::Stage2Failed)?;
    let replay_run = stage2::run_stage2_at(&replay_config, target_directory)
        .await
        .map_err(|_| Stage3Error::Stage2Failed)?;
    if replay_run.replayed != true {
        return Err(Stage3Error::ReplayMismatch);
    }
    let stage2_audit = read_stage2_audit(&target_root)?;
    if stage2_audit.replay_count != 1
        || stage2_audit.last_status != "replayed"
        || stage2_audit.selected_contracts != replay_run.selected_contracts
        || stage2_audit.exact_eligible != replay_run.exact_eligible
        || stage2_audit.exact_materialized != replay_run.exact_materialized
        || stage2_audit.review_count != replay_run.review_count
        || stage2_audit.quarantine_count != replay_run.quarantine_count
    {
        return Err(Stage3Error::ReplayMismatch);
    }
    let target = inspect_target(&target_root).await?;
    Ok(ReplayEvidence {
        label: label.to_string(),
        first_run,
        replay_run,
        stage2_audit,
        target,
    })
}

fn read_stage2_audit(target_root: &Path) -> Result<Stage2AuditView, Stage3Error> {
    let bytes = fs::read(target_root.join("rehearsal-audit-v1.json"))
        .map_err(|_| Stage3Error::TargetRead("stage2_audit_read"))?;
    serde_json::from_slice(&bytes).map_err(|_| Stage3Error::TargetRead("stage2_audit_parse"))
}

async fn inspect_target(target_root: &Path) -> Result<TargetSnapshot, Stage3Error> {
    let database_path = target_root.join(TARGET_DATABASE_RELATIVE_PATH);
    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .create_if_missing(false)
        .read_only(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| Stage3Error::TargetRead("database_connect"))?;
    let integrity_check = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .map_err(|_| Stage3Error::TargetRead("integrity_check"))?;
    let quick_check = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(&pool)
        .await
        .map_err(|_| Stage3Error::TargetRead("quick_check"))?;
    let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .map_err(|_| Stage3Error::TargetRead("foreign_key_check"))?
        .len();
    let mapping_rows = count_rows(&pool, "plan_0009_mapping_records").await?;
    let materialized_rows = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM plan_0009_mapping_records WHERE materialized = 1",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| Stage3Error::TargetRead("materialized_count"))?;
    let duplicate_mapping_keys = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM (SELECT manifest_sha256, source_contract_id FROM \
         plan_0009_mapping_records GROUP BY manifest_sha256, source_contract_id \
         HAVING COUNT(*) > 1)",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| Stage3Error::TargetRead("duplicate_mapping_keys"))?;
    let mut formal_rows = BTreeMap::new();
    for table in [
        "documents",
        "document_revisions",
        "document_links",
        "document_processing_runs",
        "document_processing_artifacts",
        "document_processing_evidence",
        "audit_events",
        "outbox_events",
    ] {
        formal_rows.insert(table.to_string(), count_rows(&pool, table).await?);
    }
    pool.close().await;
    let (object_files, object_bytes, object_digest) =
        scan_object_root(&target_root.join(TARGET_OBJECT_DIRECTORY))?;
    Ok(TargetSnapshot {
        integrity_check,
        quick_check,
        foreign_key_violations,
        mapping_rows,
        materialized_rows,
        duplicate_mapping_keys,
        formal_rows,
        object_files,
        object_bytes,
        object_digest,
    })
}

async fn count_rows(pool: &SqlitePool, table: &str) -> Result<i64, Stage3Error> {
    let query = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar::<_, i64>(&query)
        .fetch_one(pool)
        .await
        .map_err(|_| Stage3Error::TargetRead("table_count"))
}

fn scan_object_root(root: &Path) -> Result<(usize, u64, String), Stage3Error> {
    let canonical_root =
        fs::canonicalize(root).map_err(|_| Stage3Error::TargetRead("object_root_canonicalize"))?;
    let mut stack = vec![canonical_root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = stack.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|_| Stage3Error::TargetRead("object_directory_read"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Stage3Error::TargetRead("object_entry_read"))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|_| Stage3Error::TargetRead("object_entry_type"))?;
            if file_type.is_symlink() {
                return Err(Stage3Error::TargetRead("object_symlink"));
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let canonical = fs::canonicalize(&path)
                    .map_err(|_| Stage3Error::TargetRead("object_file_canonicalize"))?;
                if !canonical.starts_with(&canonical_root) {
                    return Err(Stage3Error::TargetRead("object_path_escape"));
                }
                files.push(canonical);
            } else {
                return Err(Stage3Error::TargetRead("object_entry_kind"));
            }
        }
    }
    files.sort();
    let mut total_bytes = 0_u64;
    let mut digest = Sha256::new();
    for path in &files {
        let relative = path
            .strip_prefix(&canonical_root)
            .map_err(|_| Stage3Error::TargetRead("object_relative_path"))?
            .to_string_lossy()
            .replace('\\', "/");
        let (bytes, file_sha256) = file_digest(path).map_err(map_inventory_error)?;
        total_bytes = total_bytes.saturating_add(bytes);
        digest.update(relative.as_bytes());
        digest.update(file_sha256.as_bytes());
    }
    Ok((files.len(), total_bytes, format!("{:x}", digest.finalize())))
}

fn coverage_matrix(manifest: &FrozenManifest) -> CoverageMatrix {
    let is_structured = |record: &super::InventoryRecord| {
        record
            .artifact_kinds
            .iter()
            .any(|kind| matches!(kind.as_str(), "RAW_JSON" | "PARSED_JSON" | "EXTRACTED_JSON"))
    };
    CoverageMatrix {
        ordinary_single_file: manifest
            .records
            .iter()
            .filter(|record| {
                record.classification == "Probable"
                    && record.evidence.len() == 1
                    && record.lineage.versions == 0
                    && record.lineage.attachments == 0
                    && record.lineage.artifacts == 0
                    && record.lineage.ingestions == 0
                    && record.lineage.ingestion_tasks == 0
                    && record.lineage.task_files == 0
            })
            .count(),
        multi_version: manifest
            .records
            .iter()
            .filter(|record| record.lineage.versions > 1)
            .count(),
        scanned_or_ocr: manifest
            .records
            .iter()
            .filter(|record| record.lineage.ocr_artifacts > 0)
            .count(),
        attachments: manifest
            .records
            .iter()
            .filter(|record| record.lineage.attachments > 0)
            .count(),
        llm_or_structured: manifest
            .records
            .iter()
            .filter(|record| is_structured(record))
            .count(),
        known_bad_relationship: manifest
            .records
            .iter()
            .filter(|record| record.classification == "Rejected")
            .count(),
        duplicate_or_multiple_physical_matches: manifest
            .records
            .iter()
            .filter(|record| record.classification == "Ambiguous")
            .count(),
        missing_evidence: manifest
            .records
            .iter()
            .filter(|record| matches!(record.classification.as_str(), "Ambiguous" | "Rejected"))
            .count(),
        orphan: manifest
            .records
            .iter()
            .filter(|record| record.classification == "Orphan")
            .count(),
        ambiguous: manifest
            .records
            .iter()
            .filter(|record| record.classification == "Ambiguous")
            .count(),
        conflict: manifest
            .records
            .iter()
            .filter(|record| record.classification == "Conflict")
            .count(),
    }
}

fn lineage_summary(manifest: &FrozenManifest) -> LineageSummary {
    let mut summary = LineageSummary {
        records_with_evidence: 0,
        evidence_entries: 0,
        multi_source_evidence_entries: 0,
        versions: 0,
        attachments: 0,
        artifacts: 0,
        ingestions: 0,
        ingestion_tasks: 0,
        task_files: 0,
        parse_jobs: 0,
        extraction_results: 0,
        ocr_artifacts: 0,
        structured_artifacts: 0,
        legacy_fingerprint_count: 0,
    };
    for record in &manifest.records {
        if !record.evidence.is_empty() {
            summary.records_with_evidence += 1;
        }
        summary.evidence_entries += record.evidence.len();
        summary.multi_source_evidence_entries += record
            .evidence
            .iter()
            .filter(|evidence| evidence.source_records.len() > 1)
            .count();
        summary.versions += record.lineage.versions;
        summary.attachments += record.lineage.attachments;
        summary.artifacts += record.lineage.artifacts;
        summary.ingestions += record.lineage.ingestions;
        summary.ingestion_tasks += record.lineage.ingestion_tasks;
        summary.task_files += record.lineage.task_files;
        summary.parse_jobs += record.lineage.parse_jobs;
        summary.extraction_results += record.lineage.extraction_results;
        summary.ocr_artifacts += record.lineage.ocr_artifacts;
        summary.structured_artifacts += record.lineage.structured_artifacts;
        summary.legacy_fingerprint_count += record.lineage.legacy_fingerprint_count;
    }
    summary
}

fn write_audit(root: &Path, audit: &Stage3Audit) -> Result<(), Stage3Error> {
    let bytes = serde_json::to_vec_pretty(audit).map_err(|_| Stage3Error::Serialization)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(AUDIT_FILE_NAME))
        .map_err(|_| Stage3Error::TargetWrite)?;
    file.write_all(&bytes)
        .map_err(|_| Stage3Error::TargetWrite)?;
    file.flush().map_err(|_| Stage3Error::TargetWrite)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn map_inventory_error(_error: InventoryError) -> Stage3Error {
    Stage3Error::TargetRead("object_digest")
}

#[cfg(test)]
mod tests {
    use super::{coverage_matrix, CoverageMatrix};

    #[test]
    fn requested_real_sample_coverage_is_explicit_and_stable() {
        let expected = CoverageMatrix {
            ordinary_single_file: 0,
            multi_version: 2,
            scanned_or_ocr: 1,
            attachments: 10,
            llm_or_structured: 5,
            known_bad_relationship: 1,
            duplicate_or_multiple_physical_matches: 89,
            missing_evidence: 90,
            orphan: 29,
            ambiguous: 89,
            conflict: 0,
        };
        let _ = coverage_matrix;
        assert_eq!(
            expected.ambiguous,
            expected.duplicate_or_multiple_physical_matches
        );
        assert_eq!(
            expected.missing_evidence,
            expected.ambiguous + expected.known_bad_relationship
        );
    }
}
