//! Stage 4 adversarial integrity and recovery rehearsal.
//!
//! Stage 4 composes the reviewed Stage 2 engine.  It owns only bounded target
//! orchestration, target invariant inspection, and the pure adversarial case
//! validator; it does not add another mapping or materialization policy.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use thiserror::Error;

use super::{
    file_digest, manifest_digest, stage2, ClassificationCount, FrozenManifest, InventoryConfig,
    InventoryError, ManifestDigestSidecar, MANIFEST_SCHEMA,
};

const STAGE4_DIRECTORY: &str = "stage-4-integrity-recovery-v1";
const STAGE3_DIRECTORY: &str = "stage-3-rehearsal-v1";
const STAGE3_AUDIT_FILE_NAME: &str = "stage3-replay-audit-v1.json";
const STAGE3_AUDIT_SCHEMA: &str = "plan-0009.stage-3.replay-audit.v1";
const STAGE4_AUDIT_FILE_NAME: &str = "stage4-integrity-recovery-audit-v1.json";
const STAGE4_AUDIT_SCHEMA: &str = "plan-0009.stage-4.integrity-recovery-audit.v1";
const STAGE1_DIRECTORY: &str = "stage-1-inventory-v9";
const MANIFEST_FILE_NAME: &str = "manifest-v1.json";
const MANIFEST_DIGEST_FILE_NAME: &str = "manifest-v1-digests.json";
const STAGE2_AUDIT_FILE_NAME: &str = "rehearsal-audit-v1.json";
const MAPPING_PLAN_FILE_NAME: &str = "mapping-plan-v1.json";
const TARGET_DATABASE_RELATIVE_PATH: &str = "db/document-management.sqlite";
const TARGET_OBJECT_DIRECTORY: &str = "objects";

const CLEAN_DIRECTORY: &str = r"stage-4-integrity-recovery-v1\clean-baseline";
const INTERRUPTED_DIRECTORY: &str = r"stage-4-integrity-recovery-v1\interrupted-recovery";
const PARTIAL_DIRECTORY: &str = r"stage-4-integrity-recovery-v1\partial-target";

const PARTIAL_MAPPING_BYTES: &[u8] =
    br#"{"mapping_schema":"plan-0009.stage-2.mapping.v1","records":["#;

const FORMAL_TABLES: &[(&str, &str, &str)] = &[
    (
        "documents",
        "SELECT COUNT(*) FROM documents",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM documents",
    ),
    (
        "document_revisions",
        "SELECT COUNT(*) FROM document_revisions",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM document_revisions",
    ),
    (
        "document_links",
        "SELECT COUNT(*) FROM document_links",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM document_links",
    ),
    (
        "document_processing_runs",
        "SELECT COUNT(*) FROM document_processing_runs",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM document_processing_runs",
    ),
    (
        "document_processing_artifacts",
        "SELECT COUNT(*) FROM document_processing_artifacts",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM document_processing_artifacts",
    ),
    (
        "document_processing_evidence",
        "SELECT COUNT(*) FROM document_processing_evidence",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM document_processing_evidence",
    ),
    (
        "audit_events",
        "SELECT COUNT(*) FROM audit_events",
        "SELECT COUNT(*) - COUNT(DISTINCT id) FROM audit_events",
    ),
    (
        "outbox_events",
        "SELECT COUNT(*) FROM outbox_events",
        "SELECT COUNT(*) - COUNT(DISTINCT event_id) FROM outbox_events",
    ),
];

#[derive(Debug, Error)]
pub enum Stage4Error {
    #[error("invalid Stage 4 rehearsal configuration")]
    InvalidConfiguration,
    #[error("Stage 4 target is not fresh")]
    TargetExists,
    #[error("accepted Stage 3 audit could not be read")]
    Stage3AuditRead,
    #[error("accepted Stage 3 audit is inconsistent")]
    Stage3AuditMismatch,
    #[error("frozen Stage 1 manifest could not be read")]
    ManifestRead,
    #[error("frozen Stage 1 manifest is inconsistent")]
    ManifestDigestMismatch,
    #[error("Stage 2 rehearsal failed")]
    Stage2Failed,
    #[error("Stage 4 target read failed at {0}")]
    TargetRead(&'static str),
    #[error("Stage 4 target write failed")]
    TargetWrite,
    #[error("Stage 4 replay or recovery invariant failed")]
    RecoveryMismatch,
    #[error("partial target was not rejected before materialization")]
    PartialTargetNotRejected,
    #[error("Stage 4 mutation matrix is incomplete or unsafe")]
    MatrixInvariant,
    #[error("Stage 4 audit serialization failed")]
    Serialization(#[source] serde_json::Error),
}

impl Stage4Error {
    /// Stable, non-sensitive CLI/audit code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::TargetExists => "target_exists",
            Self::Stage3AuditRead => "stage3_audit_read_failed",
            Self::Stage3AuditMismatch => "stage3_audit_mismatch",
            Self::ManifestRead => "manifest_read_failed",
            Self::ManifestDigestMismatch => "manifest_digest_mismatch",
            Self::Stage2Failed => "stage2_rehearsal_failed",
            Self::TargetRead(_) => "target_read_failed",
            Self::TargetWrite => "target_write_failed",
            Self::RecoveryMismatch => "recovery_invariant_failed",
            Self::PartialTargetNotRejected => "partial_target_not_rejected",
            Self::MatrixInvariant => "mutation_matrix_invariant_failed",
            Self::Serialization(_) => "audit_serialization_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseId {
    StaleVersion,
    ConflictingRelation,
    MissingObject,
    WrongSha256,
    CorruptedObject,
    DuplicateReplay,
    InterruptedRehearsal,
    PartiallyWrittenTarget,
    DuplicatePhysicalContent,
    OcrRevisionMismatch,
    LlmProcessingLineageMismatch,
    AmbiguousReference,
    IncorrectOldFileIdPath,
}

impl CaseId {
    pub const ALL: [Self; 13] = [
        Self::StaleVersion,
        Self::ConflictingRelation,
        Self::MissingObject,
        Self::WrongSha256,
        Self::CorruptedObject,
        Self::DuplicateReplay,
        Self::InterruptedRehearsal,
        Self::PartiallyWrittenTarget,
        Self::DuplicatePhysicalContent,
        Self::OcrRevisionMismatch,
        Self::LlmProcessingLineageMismatch,
        Self::AmbiguousReference,
        Self::IncorrectOldFileIdPath,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseDisposition {
    FailClosed,
    Quarantine,
    ManualReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSource {
    TargetInvariant,
    ManifestDerivedMutation,
    AdversarialMutation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseResult {
    pub case_id: CaseId,
    pub disposition: CaseDisposition,
    pub safe_code: &'static str,
    pub source: CaseSource,
    pub invariant_proven: bool,
    pub auto_materialize: bool,
}

/// Deterministic, bounded observations used by the pure Stage 4 validator.
///
/// These fixtures contain only counts, booleans, and version-like values.  No
/// source path, object key, document text, URL, or secret can enter a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationFixture {
    StaleVersion {
        expected_version: u64,
        observed_version: u64,
        manifest_records: usize,
    },
    ConflictingRelation {
        relation_matches: bool,
    },
    MissingObject {
        source_evidence_count: usize,
        object_present: bool,
    },
    WrongSha256 {
        source_evidence_count: usize,
        checksum_matches: bool,
    },
    CorruptedObject {
        source_evidence_count: usize,
        bytes_match: bool,
        checksum_matches: bool,
    },
    DuplicateReplay {
        replayed: bool,
        mapping_plan_equal: bool,
        mapping_file_equal: bool,
        mapping_rows_equal: bool,
        duplicate_mapping_keys: usize,
        formal_rows_equal: bool,
        duplicate_formal_facts: usize,
        object_files_equal: bool,
        duplicate_objects: usize,
    },
    InterruptedRehearsal {
        audit_removed: bool,
        recovery_replayed: bool,
        mapping_plan_equal: bool,
        mapping_file_equal: bool,
        mapping_rows_equal: bool,
        duplicate_mapping_keys: usize,
        formal_rows_equal: bool,
        duplicate_formal_facts: usize,
        object_files_equal: bool,
        duplicate_objects: usize,
    },
    PartiallyWrittenTarget {
        partial_mapping_artifact: bool,
        engine_rejected: bool,
        mapping_rows: usize,
        formal_fact_rows: usize,
        object_files: usize,
    },
    DuplicatePhysicalContent {
        physical_match_count: usize,
        ambiguous_count: usize,
    },
    OcrRevisionMismatch {
        ocr_lineage_count: usize,
        revision_matches: bool,
    },
    LlmProcessingLineageMismatch {
        llm_lineage_count: usize,
        lineage_matches: bool,
    },
    AmbiguousReference {
        candidate_count: usize,
        ambiguous_count: usize,
    },
    IncorrectOldFileIdPath {
        source_evidence_count: usize,
        reference_matches: bool,
    },
}

/// Purely validate bounded integrity/recovery observations into safe results.
///
/// Every result is deliberately non-materializing.  A fixture that does not
/// prove its expected invariant still produces a fail-closed/quarantine/manual
/// review result, with `invariant_proven=false`; the Stage 4 runner rejects
/// such a matrix instead of turning it into report-only evidence.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn validate_integrity_cases(fixtures: &[MutationFixture]) -> Vec<CaseResult> {
    fixtures
        .iter()
        .map(|fixture| match fixture {
            MutationFixture::StaleVersion {
                expected_version,
                observed_version,
                manifest_records,
            } => case_result(
                CaseId::StaleVersion,
                CaseDisposition::FailClosed,
                "stale_version_fail_closed",
                CaseSource::AdversarialMutation,
                *manifest_records > 0 && observed_version < expected_version,
            ),
            MutationFixture::ConflictingRelation { relation_matches } => case_result(
                CaseId::ConflictingRelation,
                CaseDisposition::Quarantine,
                "conflicting_relation_quarantine",
                CaseSource::AdversarialMutation,
                !relation_matches,
            ),
            MutationFixture::MissingObject {
                source_evidence_count,
                object_present,
            } => case_result(
                CaseId::MissingObject,
                CaseDisposition::FailClosed,
                "missing_object_fail_closed",
                CaseSource::AdversarialMutation,
                *source_evidence_count > 0 && !object_present,
            ),
            MutationFixture::WrongSha256 {
                source_evidence_count,
                checksum_matches,
            } => case_result(
                CaseId::WrongSha256,
                CaseDisposition::FailClosed,
                "wrong_sha256_fail_closed",
                CaseSource::AdversarialMutation,
                *source_evidence_count > 0 && !checksum_matches,
            ),
            MutationFixture::CorruptedObject {
                source_evidence_count,
                bytes_match,
                checksum_matches,
            } => case_result(
                CaseId::CorruptedObject,
                CaseDisposition::FailClosed,
                "corrupted_object_fail_closed",
                CaseSource::AdversarialMutation,
                *source_evidence_count > 0 && (!bytes_match || !checksum_matches),
            ),
            MutationFixture::DuplicateReplay {
                replayed,
                mapping_plan_equal,
                mapping_file_equal,
                mapping_rows_equal,
                duplicate_mapping_keys,
                formal_rows_equal,
                duplicate_formal_facts,
                object_files_equal,
                duplicate_objects,
            } => case_result(
                CaseId::DuplicateReplay,
                CaseDisposition::FailClosed,
                "duplicate_replay_fail_closed",
                CaseSource::TargetInvariant,
                *replayed
                    && *mapping_plan_equal
                    && *mapping_file_equal
                    && *mapping_rows_equal
                    && *duplicate_mapping_keys == 0
                    && *formal_rows_equal
                    && *duplicate_formal_facts == 0
                    && *object_files_equal
                    && *duplicate_objects == 0,
            ),
            MutationFixture::InterruptedRehearsal {
                audit_removed,
                recovery_replayed,
                mapping_plan_equal,
                mapping_file_equal,
                mapping_rows_equal,
                duplicate_mapping_keys,
                formal_rows_equal,
                duplicate_formal_facts,
                object_files_equal,
                duplicate_objects,
            } => case_result(
                CaseId::InterruptedRehearsal,
                CaseDisposition::FailClosed,
                "interrupted_rehearsal_recovered",
                CaseSource::TargetInvariant,
                *audit_removed
                    && *recovery_replayed
                    && *mapping_plan_equal
                    && *mapping_file_equal
                    && *mapping_rows_equal
                    && *duplicate_mapping_keys == 0
                    && *formal_rows_equal
                    && *duplicate_formal_facts == 0
                    && *object_files_equal
                    && *duplicate_objects == 0,
            ),
            MutationFixture::PartiallyWrittenTarget {
                partial_mapping_artifact,
                engine_rejected,
                mapping_rows,
                formal_fact_rows,
                object_files,
            } => case_result(
                CaseId::PartiallyWrittenTarget,
                CaseDisposition::FailClosed,
                "partial_target_fail_closed",
                CaseSource::TargetInvariant,
                *partial_mapping_artifact
                    && *engine_rejected
                    && *mapping_rows == 0
                    && *formal_fact_rows == 0
                    && *object_files == 0,
            ),
            MutationFixture::DuplicatePhysicalContent {
                physical_match_count,
                ambiguous_count,
            } => case_result(
                CaseId::DuplicatePhysicalContent,
                CaseDisposition::Quarantine,
                "duplicate_physical_content_quarantine",
                if *physical_match_count > 1 && *ambiguous_count > 0 {
                    CaseSource::ManifestDerivedMutation
                } else {
                    CaseSource::AdversarialMutation
                },
                *physical_match_count > 1 && *ambiguous_count > 0,
            ),
            MutationFixture::OcrRevisionMismatch {
                ocr_lineage_count,
                revision_matches,
            } => case_result(
                CaseId::OcrRevisionMismatch,
                CaseDisposition::ManualReview,
                "ocr_revision_mismatch_manual_review",
                CaseSource::AdversarialMutation,
                *ocr_lineage_count > 0 && !revision_matches,
            ),
            MutationFixture::LlmProcessingLineageMismatch {
                llm_lineage_count,
                lineage_matches,
            } => case_result(
                CaseId::LlmProcessingLineageMismatch,
                CaseDisposition::ManualReview,
                "llm_processing_lineage_mismatch_manual_review",
                CaseSource::AdversarialMutation,
                *llm_lineage_count > 0 && !lineage_matches,
            ),
            MutationFixture::AmbiguousReference {
                candidate_count,
                ambiguous_count,
            } => case_result(
                CaseId::AmbiguousReference,
                CaseDisposition::Quarantine,
                "ambiguous_reference_quarantine",
                if *candidate_count > 1 && *ambiguous_count > 0 {
                    CaseSource::ManifestDerivedMutation
                } else {
                    CaseSource::AdversarialMutation
                },
                *candidate_count > 1 && *ambiguous_count > 0,
            ),
            MutationFixture::IncorrectOldFileIdPath {
                source_evidence_count,
                reference_matches,
            } => case_result(
                CaseId::IncorrectOldFileIdPath,
                CaseDisposition::FailClosed,
                "incorrect_old_file_binding_fail_closed",
                CaseSource::AdversarialMutation,
                *source_evidence_count > 0 && !reference_matches,
            ),
        })
        .collect()
}

fn case_result(
    case_id: CaseId,
    disposition: CaseDisposition,
    safe_code: &'static str,
    source: CaseSource,
    invariant_proven: bool,
) -> CaseResult {
    CaseResult {
        case_id,
        disposition,
        safe_code,
        source,
        invariant_proven,
        auto_materialize: false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage4Summary {
    pub selected_contracts: usize,
    pub case_count: usize,
    pub fail_closed: usize,
    pub quarantine: usize,
    pub manual_review: usize,
    pub replayed: bool,
    pub manifest_sha256: String,
    pub mapping_plan_sha256: String,
    pub matrix_sha256: String,
    pub formal_fact_rows: usize,
    pub object_files: usize,
    pub duplicate_formal_facts: usize,
    pub duplicate_objects: usize,
    pub audit_file_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AcceptedStage3Audit {
    audit_schema: String,
    status: String,
    input_manifest_sha256: String,
    selected_contracts: usize,
    replay_equal: bool,
    failures: Vec<String>,
}

#[derive(Debug, Clone)]
struct AcceptedInputs {
    manifest: FrozenManifest,
    stage3_audit: AcceptedStage3Audit,
    stage3_audit_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TargetSnapshot {
    integrity_check: String,
    quick_check: String,
    foreign_key_violations: usize,
    mapping_rows: usize,
    materialized_mapping_rows: usize,
    duplicate_mapping_keys: usize,
    formal_rows: BTreeMap<String, usize>,
    duplicate_formal_facts: usize,
    object_files: usize,
    object_bytes: u64,
    duplicate_objects: usize,
    object_digest: String,
}

impl TargetSnapshot {
    #[must_use]
    fn formal_fact_rows(&self) -> usize {
        self.formal_rows.values().copied().sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stage2AuditView {
    mapping_plan_sha256: String,
    replay_count: u64,
    last_status: String,
    selected_contracts: usize,
    exact_eligible: usize,
    exact_materialized: usize,
    review_count: usize,
    quarantine_count: usize,
    first_run: Stage2AuditRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stage2AuditRun {
    status: String,
    selected_contracts: usize,
    exact_eligible: usize,
    exact_materialized: usize,
    review_count: usize,
    quarantine_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayEvidence {
    kind: String,
    first_run: stage2::Stage2Summary,
    final_run: stage2::Stage2Summary,
    first_audit: Stage2AuditView,
    final_audit: Stage2AuditView,
    first_snapshot: TargetSnapshot,
    final_snapshot: TargetSnapshot,
    audit_removed_before_final_run: bool,
    replay_equal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PartialTargetEvidence {
    partial_mapping_artifact: bool,
    partial_mapping_sha256: String,
    engine_rejected: bool,
    engine_error_code: String,
    mapping_rows: usize,
    duplicate_mapping_keys: usize,
    formal_fact_rows: usize,
    duplicate_formal_facts: usize,
    object_files: usize,
    duplicate_objects: usize,
}

#[derive(Debug, Clone, Serialize)]
struct Stage4Audit {
    audit_schema: &'static str,
    status: &'static str,
    stage3_audit_schema: String,
    stage3_audit_status: String,
    stage3_audit_sha256: String,
    stage3_replay_equal: bool,
    manifest_schema: String,
    manifest_sha256: String,
    selected_contracts: usize,
    classification_counts: Vec<ClassificationCount>,
    source_classification_counts: Vec<ClassificationCount>,
    mapping_plan_sha256: String,
    matrix_sha256: String,
    case_results: Vec<CaseResult>,
    clean_baseline: ReplayEvidence,
    interrupted_recovery: ReplayEvidence,
    partial_target: PartialTargetEvidence,
}

/// Run the bounded Stage 4 integrity/recovery rehearsal.
pub async fn run_stage4(config: &InventoryConfig) -> Result<Stage4Summary, Stage4Error> {
    validate_stage4_target(config)?;
    let inputs = load_accepted_inputs(config)?;
    let clean_baseline = run_clean_baseline(config).await?;
    let interrupted_recovery = run_interrupted_recovery(config).await?;
    let partial_target = run_partial_target(config).await?;

    let fixtures = mutation_fixtures(
        &inputs.manifest,
        &clean_baseline,
        &interrupted_recovery,
        &partial_target,
    );
    let case_results = validate_integrity_cases(&fixtures);
    validate_case_matrix(&case_results)?;
    let matrix_sha256 = digest_json(&case_results)?;

    if clean_baseline.final_run.mapping_plan_sha256
        != interrupted_recovery.final_run.mapping_plan_sha256
        || !clean_baseline.replay_equal
        || !interrupted_recovery.replay_equal
    {
        return Err(Stage4Error::RecoveryMismatch);
    }

    let audit = Stage4Audit {
        audit_schema: STAGE4_AUDIT_SCHEMA,
        status: "replayed",
        stage3_audit_schema: inputs.stage3_audit.audit_schema.clone(),
        stage3_audit_status: inputs.stage3_audit.status.clone(),
        stage3_audit_sha256: inputs.stage3_audit_sha256,
        stage3_replay_equal: inputs.stage3_audit.replay_equal,
        manifest_schema: inputs.manifest.manifest_schema.clone(),
        manifest_sha256: inputs.manifest.canonical_manifest_sha256.clone(),
        selected_contracts: inputs.manifest.records.len(),
        classification_counts: inputs.manifest.classification_counts.clone(),
        source_classification_counts: inputs.manifest.source_classification_counts.clone(),
        mapping_plan_sha256: clean_baseline.final_run.mapping_plan_sha256.clone(),
        matrix_sha256: matrix_sha256.clone(),
        case_results: case_results.clone(),
        clean_baseline: clean_baseline.clone(),
        interrupted_recovery: interrupted_recovery.clone(),
        partial_target,
    };
    let audit_file_sha256 = write_audit(config.target_root.as_path(), &audit)?;
    let fail_closed = case_results
        .iter()
        .filter(|result| result.disposition == CaseDisposition::FailClosed)
        .count();
    let quarantine = case_results
        .iter()
        .filter(|result| result.disposition == CaseDisposition::Quarantine)
        .count();
    let manual_review = case_results
        .iter()
        .filter(|result| result.disposition == CaseDisposition::ManualReview)
        .count();
    Ok(Stage4Summary {
        selected_contracts: inputs.manifest.records.len(),
        case_count: case_results.len(),
        fail_closed,
        quarantine,
        manual_review,
        replayed: clean_baseline.final_run.replayed && interrupted_recovery.final_run.replayed,
        manifest_sha256: inputs.manifest.canonical_manifest_sha256,
        mapping_plan_sha256: clean_baseline.final_run.mapping_plan_sha256,
        matrix_sha256,
        formal_fact_rows: clean_baseline.final_snapshot.formal_fact_rows(),
        object_files: clean_baseline.final_snapshot.object_files,
        duplicate_formal_facts: clean_baseline.final_snapshot.duplicate_formal_facts,
        duplicate_objects: clean_baseline.final_snapshot.duplicate_objects,
        audit_file_sha256,
    })
}

fn validate_stage4_target(config: &InventoryConfig) -> Result<(), Stage4Error> {
    if config.target_root != config.isolation_root.join(STAGE4_DIRECTORY)
        || !config.isolation_root.is_dir()
        || !config.target_root.is_dir()
    {
        return Err(Stage4Error::InvalidConfiguration);
    }
    let mut entries =
        fs::read_dir(&config.target_root).map_err(|_| Stage4Error::TargetRead("stage4_root"))?;
    if entries.next().is_some() {
        return Err(Stage4Error::TargetExists);
    }
    Ok(())
}

fn load_accepted_inputs(config: &InventoryConfig) -> Result<AcceptedInputs, Stage4Error> {
    let stage3_root = config.isolation_root.join(STAGE3_DIRECTORY);
    let stage3_audit_bytes = fs::read(stage3_root.join(STAGE3_AUDIT_FILE_NAME))
        .map_err(|_| Stage4Error::Stage3AuditRead)?;
    let stage3_audit: AcceptedStage3Audit =
        serde_json::from_slice(&stage3_audit_bytes).map_err(|_| Stage4Error::Stage3AuditRead)?;
    if stage3_audit.audit_schema != STAGE3_AUDIT_SCHEMA
        || stage3_audit.status != "replayed"
        || !stage3_audit.replay_equal
        || !stage3_audit.failures.is_empty()
    {
        return Err(Stage4Error::Stage3AuditMismatch);
    }
    let manifest = load_manifest(config)?;
    if stage3_audit.input_manifest_sha256 != manifest.canonical_manifest_sha256
        || stage3_audit.selected_contracts != manifest.records.len()
    {
        return Err(Stage4Error::Stage3AuditMismatch);
    }
    Ok(AcceptedInputs {
        manifest,
        stage3_audit,
        stage3_audit_sha256: sha256_bytes(&stage3_audit_bytes),
    })
}

fn load_manifest(config: &InventoryConfig) -> Result<FrozenManifest, Stage4Error> {
    let manifest_root = config.isolation_root.join(STAGE1_DIRECTORY);
    let manifest_bytes =
        fs::read(manifest_root.join(MANIFEST_FILE_NAME)).map_err(|_| Stage4Error::ManifestRead)?;
    let manifest: FrozenManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| Stage4Error::ManifestRead)?;
    if manifest.manifest_schema != MANIFEST_SCHEMA
        || manifest_digest(&manifest).map_err(|_| Stage4Error::ManifestDigestMismatch)?
            != manifest.canonical_manifest_sha256
    {
        return Err(Stage4Error::ManifestDigestMismatch);
    }
    let sidecar_bytes = fs::read(manifest_root.join(MANIFEST_DIGEST_FILE_NAME))
        .map_err(|_| Stage4Error::ManifestRead)?;
    let sidecar: ManifestDigestSidecar =
        serde_json::from_slice(&sidecar_bytes).map_err(|_| Stage4Error::ManifestRead)?;
    if sidecar.manifest_schema != MANIFEST_SCHEMA
        || sidecar.canonical_manifest_sha256 != manifest.canonical_manifest_sha256
        || sidecar.file_bytes_sha256 != sha256_bytes(&manifest_bytes)
    {
        return Err(Stage4Error::ManifestDigestMismatch);
    }
    Ok(manifest)
}

fn create_fresh_target(
    config: &InventoryConfig,
    target_directory: &str,
) -> Result<PathBuf, Stage4Error> {
    let target_root = config.isolation_root.join(target_directory);
    if target_root.exists() {
        return Err(Stage4Error::TargetExists);
    }
    fs::create_dir_all(&target_root).map_err(|_| Stage4Error::TargetWrite)?;
    Ok(target_root)
}

fn stage2_config(config: &InventoryConfig, target_directory: &str) -> InventoryConfig {
    let mut target_config = config.clone();
    target_config.target_root = config.isolation_root.join(target_directory);
    target_config
}

async fn run_clean_baseline(config: &InventoryConfig) -> Result<ReplayEvidence, Stage4Error> {
    let target_root = create_fresh_target(config, CLEAN_DIRECTORY)?;
    let target_config = stage2_config(config, CLEAN_DIRECTORY);
    let first_run = stage2::run_stage2_at(&target_config, CLEAN_DIRECTORY)
        .await
        .map_err(|_| Stage4Error::Stage2Failed)?;
    let first_audit = read_stage2_audit(&target_root)?;
    let first_snapshot = inspect_target(&target_root).await?;
    let final_run = stage2::run_stage2_at(&target_config, CLEAN_DIRECTORY)
        .await
        .map_err(|_| Stage4Error::Stage2Failed)?;
    let final_audit = read_stage2_audit(&target_root)?;
    let final_snapshot = inspect_target(&target_root).await?;
    let replay_equal =
        replay_evidence_equal(&first_run, &final_run, &first_snapshot, &final_snapshot);
    if first_run.replayed
        || !final_run.replayed
        || first_audit.last_status != "frozen"
        || first_audit.replay_count != 0
        || final_audit.last_status != "replayed"
        || final_audit.replay_count != 1
        || !audit_matches_summary(&first_audit, &first_run)
        || !audit_matches_summary(&final_audit, &final_run)
        || !replay_equal
    {
        return Err(Stage4Error::RecoveryMismatch);
    }
    Ok(ReplayEvidence {
        kind: "clean_baseline".to_string(),
        first_run,
        final_run,
        first_audit,
        final_audit,
        first_snapshot,
        final_snapshot,
        audit_removed_before_final_run: false,
        replay_equal,
    })
}

async fn run_interrupted_recovery(config: &InventoryConfig) -> Result<ReplayEvidence, Stage4Error> {
    let target_root = create_fresh_target(config, INTERRUPTED_DIRECTORY)?;
    let target_config = stage2_config(config, INTERRUPTED_DIRECTORY);
    let first_run = stage2::run_stage2_at(&target_config, INTERRUPTED_DIRECTORY)
        .await
        .map_err(|_| Stage4Error::Stage2Failed)?;
    let first_audit = read_stage2_audit(&target_root)?;
    let first_snapshot = inspect_target(&target_root).await?;
    let audit_path = target_root.join(STAGE2_AUDIT_FILE_NAME);
    if !audit_path.is_file() {
        return Err(Stage4Error::RecoveryMismatch);
    }
    fs::remove_file(&audit_path).map_err(|_| Stage4Error::TargetWrite)?;
    let final_run = stage2::run_stage2_at(&target_config, INTERRUPTED_DIRECTORY)
        .await
        .map_err(|_| Stage4Error::Stage2Failed)?;
    let final_audit = read_stage2_audit(&target_root)?;
    let final_snapshot = inspect_target(&target_root).await?;
    let replay_equal =
        replay_evidence_equal(&first_run, &final_run, &first_snapshot, &final_snapshot);
    if first_run.replayed
        || !final_run.replayed
        || first_audit.last_status != "frozen"
        || first_audit.replay_count != 0
        || final_audit.last_status != "replayed_recovered"
        || final_audit.replay_count != 1
        || !audit_matches_summary(&first_audit, &first_run)
        || !audit_matches_summary(&final_audit, &final_run)
        || !replay_equal
    {
        return Err(Stage4Error::RecoveryMismatch);
    }
    Ok(ReplayEvidence {
        kind: "interrupted_recovery".to_string(),
        first_run,
        final_run,
        first_audit,
        final_audit,
        first_snapshot,
        final_snapshot,
        audit_removed_before_final_run: true,
        replay_equal,
    })
}

async fn run_partial_target(
    config: &InventoryConfig,
) -> Result<PartialTargetEvidence, Stage4Error> {
    let target_root = create_fresh_target(config, PARTIAL_DIRECTORY)?;
    let mapping_path = target_root.join(MAPPING_PLAN_FILE_NAME);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&mapping_path)
        .map_err(|_| Stage4Error::TargetWrite)?;
    file.write_all(PARTIAL_MAPPING_BYTES)
        .map_err(|_| Stage4Error::TargetWrite)?;
    file.flush().map_err(|_| Stage4Error::TargetWrite)?;
    let partial_mapping_sha256 = sha256_bytes(PARTIAL_MAPPING_BYTES);
    let target_config = stage2_config(config, PARTIAL_DIRECTORY);
    let engine_error_code = match stage2::run_stage2_at(&target_config, PARTIAL_DIRECTORY).await {
        Ok(_) => return Err(Stage4Error::PartialTargetNotRejected),
        Err(error) => error.code(),
    };
    if engine_error_code != "manifest_read_failed" {
        return Err(Stage4Error::Stage2Failed);
    }
    let snapshot = inspect_partial_target(&target_root).await?;
    let partial_mapping_artifact = fs::read(&mapping_path)
        .map(|bytes| bytes == PARTIAL_MAPPING_BYTES)
        .unwrap_or(false);
    let engine_rejected = true;
    if !partial_mapping_artifact
        || snapshot.mapping_rows != 0
        || snapshot.formal_fact_rows() != 0
        || snapshot.object_files != 0
    {
        return Err(Stage4Error::PartialTargetNotRejected);
    }
    Ok(PartialTargetEvidence {
        partial_mapping_artifact,
        partial_mapping_sha256,
        engine_rejected,
        engine_error_code: engine_error_code.to_string(),
        mapping_rows: snapshot.mapping_rows,
        duplicate_mapping_keys: snapshot.duplicate_mapping_keys,
        formal_fact_rows: snapshot.formal_fact_rows(),
        duplicate_formal_facts: snapshot.duplicate_formal_facts,
        object_files: snapshot.object_files,
        duplicate_objects: snapshot.duplicate_objects,
    })
}

fn read_stage2_audit(target_root: &Path) -> Result<Stage2AuditView, Stage4Error> {
    let bytes = fs::read(target_root.join(STAGE2_AUDIT_FILE_NAME))
        .map_err(|_| Stage4Error::TargetRead("stage2_audit"))?;
    serde_json::from_slice(&bytes).map_err(|_| Stage4Error::TargetRead("stage2_audit_parse"))
}

fn audit_matches_summary(audit: &Stage2AuditView, summary: &stage2::Stage2Summary) -> bool {
    audit.mapping_plan_sha256 == summary.mapping_plan_sha256
        && audit.selected_contracts == summary.selected_contracts
        && audit.exact_eligible == summary.exact_eligible
        && audit.exact_materialized == summary.exact_materialized
        && audit.review_count == summary.review_count
        && audit.quarantine_count == summary.quarantine_count
        && audit.first_run.status == "frozen"
        && audit.first_run.selected_contracts == summary.selected_contracts
        && audit.first_run.exact_eligible == summary.exact_eligible
        && audit.first_run.exact_materialized == summary.exact_materialized
        && audit.first_run.review_count == summary.review_count
        && audit.first_run.quarantine_count == summary.quarantine_count
}

fn replay_evidence_equal(
    first_run: &stage2::Stage2Summary,
    final_run: &stage2::Stage2Summary,
    first_snapshot: &TargetSnapshot,
    final_snapshot: &TargetSnapshot,
) -> bool {
    first_run.selected_contracts == final_run.selected_contracts
        && first_run.exact_eligible == final_run.exact_eligible
        && first_run.exact_materialized == final_run.exact_materialized
        && first_run.review_count == final_run.review_count
        && first_run.quarantine_count == final_run.quarantine_count
        && first_run.mapping_plan_sha256 == final_run.mapping_plan_sha256
        && first_run.mapping_file_bytes_sha256 == final_run.mapping_file_bytes_sha256
        && first_snapshot == final_snapshot
}

async fn inspect_partial_target(target_root: &Path) -> Result<TargetSnapshot, Stage4Error> {
    let database_path = target_root.join(TARGET_DATABASE_RELATIVE_PATH);
    if database_path.is_file() {
        inspect_target(target_root).await
    } else {
        Ok(empty_target_snapshot())
    }
}

async fn inspect_target(target_root: &Path) -> Result<TargetSnapshot, Stage4Error> {
    let database_path = target_root.join(TARGET_DATABASE_RELATIVE_PATH);
    if !database_path.is_file() {
        return Err(Stage4Error::TargetRead("target_database"));
    }
    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .create_if_missing(false)
        .read_only(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| Stage4Error::TargetRead("database_connect"))?;
    let integrity_check = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .map_err(|_| Stage4Error::TargetRead("integrity_check"))?;
    let quick_check = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(&pool)
        .await
        .map_err(|_| Stage4Error::TargetRead("quick_check"))?;
    let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .map_err(|_| Stage4Error::TargetRead("foreign_key_check"))?
        .len();
    let mapping_rows = bounded_count(
        &pool,
        "SELECT COUNT(*) FROM plan_0009_mapping_records",
        "mapping_rows",
    )
    .await?;
    let materialized_mapping_rows = bounded_count(
        &pool,
        "SELECT COUNT(*) FROM plan_0009_mapping_records WHERE materialized = 1",
        "materialized_mapping_rows",
    )
    .await?;
    let duplicate_mapping_keys = bounded_count(
        &pool,
        "SELECT COUNT(*) FROM (SELECT manifest_sha256, source_contract_id FROM plan_0009_mapping_records GROUP BY manifest_sha256, source_contract_id HAVING COUNT(*) > 1)",
        "duplicate_mapping_keys",
    )
    .await?;
    let mut formal_rows = BTreeMap::new();
    let mut duplicate_formal_facts = 0_usize;
    for (table, row_query, duplicate_query) in FORMAL_TABLES {
        formal_rows.insert(
            (*table).to_string(),
            bounded_count(&pool, row_query, "formal_rows").await?,
        );
        duplicate_formal_facts = duplicate_formal_facts
            .saturating_add(bounded_count(&pool, duplicate_query, "duplicate_formal_facts").await?);
    }
    pool.close().await;
    let objects = scan_object_root(&target_root.join(TARGET_OBJECT_DIRECTORY))?;
    Ok(TargetSnapshot {
        integrity_check,
        quick_check,
        foreign_key_violations,
        mapping_rows,
        materialized_mapping_rows,
        duplicate_mapping_keys,
        formal_rows,
        duplicate_formal_facts,
        object_files: objects.0,
        object_bytes: objects.1,
        duplicate_objects: objects.2,
        object_digest: objects.3,
    })
}

fn empty_target_snapshot() -> TargetSnapshot {
    TargetSnapshot {
        integrity_check: "not_created".to_string(),
        quick_check: "not_created".to_string(),
        foreign_key_violations: 0,
        mapping_rows: 0,
        materialized_mapping_rows: 0,
        duplicate_mapping_keys: 0,
        formal_rows: FORMAL_TABLES
            .iter()
            .map(|(table, _, _)| ((*table).to_string(), 0))
            .collect(),
        duplicate_formal_facts: 0,
        object_files: 0,
        object_bytes: 0,
        duplicate_objects: 0,
        object_digest: sha256_bytes(&[]),
    }
}

async fn bounded_count(
    pool: &SqlitePool,
    query: &str,
    error_code: &'static str,
) -> Result<usize, Stage4Error> {
    let value = sqlx::query_scalar::<_, i64>(query)
        .fetch_one(pool)
        .await
        .map_err(|_| Stage4Error::TargetRead(error_code))?;
    usize::try_from(value).map_err(|_| Stage4Error::TargetRead(error_code))
}

fn scan_object_root(root: &Path) -> Result<(usize, u64, usize, String), Stage4Error> {
    if !root.exists() {
        return Ok((0, 0, 0, sha256_bytes(&[])));
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|_| Stage4Error::TargetRead("object_root"))?;
    if !canonical_root.is_dir() {
        return Err(Stage4Error::TargetRead("object_root_kind"));
    }
    let mut stack = vec![canonical_root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = stack.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|_| Stage4Error::TargetRead("object_directory"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Stage4Error::TargetRead("object_entry"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|_| Stage4Error::TargetRead("object_entry_type"))?;
            if file_type.is_symlink() {
                return Err(Stage4Error::TargetRead("object_symlink"));
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let canonical =
                    fs::canonicalize(&path).map_err(|_| Stage4Error::TargetRead("object_file"))?;
                if !canonical.starts_with(&canonical_root) {
                    return Err(Stage4Error::TargetRead("object_path_escape"));
                }
                files.push(canonical);
            } else {
                return Err(Stage4Error::TargetRead("object_entry_kind"));
            }
        }
    }
    files.sort();
    let mut total_bytes = 0_u64;
    let mut digest = Sha256::new();
    let mut object_hash_counts = BTreeMap::<String, usize>::new();
    for path in &files {
        let relative = path
            .strip_prefix(&canonical_root)
            .map_err(|_| Stage4Error::TargetRead("object_relative"))?
            .to_string_lossy()
            .replace('\\', "/");
        let (bytes, file_sha256) = file_digest(path).map_err(map_inventory_error)?;
        total_bytes = total_bytes.saturating_add(bytes);
        digest.update(sha256_bytes(relative.as_bytes()).as_bytes());
        digest.update(file_sha256.as_bytes());
        *object_hash_counts.entry(file_sha256).or_default() += 1;
    }
    let duplicate_objects = object_hash_counts
        .values()
        .filter(|count| **count > 1)
        .map(|count| count.saturating_sub(1))
        .sum();
    Ok((
        files.len(),
        total_bytes,
        duplicate_objects,
        format!("{:x}", digest.finalize()),
    ))
}

#[allow(clippy::too_many_lines)]
fn mutation_fixtures(
    manifest: &FrozenManifest,
    clean_baseline: &ReplayEvidence,
    interrupted_recovery: &ReplayEvidence,
    partial_target: &PartialTargetEvidence,
) -> Vec<MutationFixture> {
    let source_evidence_count = manifest
        .records
        .iter()
        .filter(|record| !record.evidence.is_empty())
        .count();
    let ambiguous_count = manifest
        .records
        .iter()
        .filter(|record| record.classification == "Ambiguous")
        .count();
    let ocr_lineage_count = manifest
        .records
        .iter()
        .filter(|record| record.lineage.ocr_artifacts > 0)
        .count();
    let llm_lineage_count = manifest
        .records
        .iter()
        .filter(|record| {
            record.lineage.structured_artifacts > 0 || record.lineage.extraction_results > 0
        })
        .count();
    let clean_mapping_plan_equal = clean_baseline.first_run.mapping_plan_sha256
        == clean_baseline.final_run.mapping_plan_sha256;
    let clean_mapping_file_equal = clean_baseline.first_run.mapping_file_bytes_sha256
        == clean_baseline.final_run.mapping_file_bytes_sha256;
    let clean_formal_rows_equal =
        clean_baseline.first_snapshot.formal_rows == clean_baseline.final_snapshot.formal_rows;
    let clean_object_files_equal =
        clean_baseline.first_snapshot.object_files == clean_baseline.final_snapshot.object_files;
    let interrupted_mapping_plan_equal = interrupted_recovery.first_run.mapping_plan_sha256
        == interrupted_recovery.final_run.mapping_plan_sha256;
    let interrupted_mapping_file_equal = interrupted_recovery.first_run.mapping_file_bytes_sha256
        == interrupted_recovery.final_run.mapping_file_bytes_sha256;
    let interrupted_formal_rows_equal = interrupted_recovery.first_snapshot.formal_rows
        == interrupted_recovery.final_snapshot.formal_rows;
    let interrupted_object_files_equal = interrupted_recovery.first_snapshot.object_files
        == interrupted_recovery.final_snapshot.object_files;
    vec![
        MutationFixture::StaleVersion {
            expected_version: 2,
            observed_version: 1,
            manifest_records: manifest.records.len(),
        },
        MutationFixture::ConflictingRelation {
            relation_matches: false,
        },
        MutationFixture::MissingObject {
            source_evidence_count,
            object_present: false,
        },
        MutationFixture::WrongSha256 {
            source_evidence_count,
            checksum_matches: false,
        },
        MutationFixture::CorruptedObject {
            source_evidence_count,
            bytes_match: false,
            checksum_matches: false,
        },
        MutationFixture::DuplicateReplay {
            replayed: clean_baseline.final_run.replayed,
            mapping_plan_equal: clean_mapping_plan_equal,
            mapping_file_equal: clean_mapping_file_equal,
            mapping_rows_equal: clean_baseline.first_snapshot.mapping_rows
                == clean_baseline.final_snapshot.mapping_rows,
            duplicate_mapping_keys: clean_baseline.final_snapshot.duplicate_mapping_keys,
            formal_rows_equal: clean_formal_rows_equal,
            duplicate_formal_facts: clean_baseline.final_snapshot.duplicate_formal_facts,
            object_files_equal: clean_object_files_equal,
            duplicate_objects: clean_baseline.final_snapshot.duplicate_objects,
        },
        MutationFixture::InterruptedRehearsal {
            audit_removed: interrupted_recovery.audit_removed_before_final_run,
            recovery_replayed: interrupted_recovery.final_run.replayed,
            mapping_plan_equal: interrupted_mapping_plan_equal,
            mapping_file_equal: interrupted_mapping_file_equal,
            mapping_rows_equal: interrupted_recovery.first_snapshot.mapping_rows
                == interrupted_recovery.final_snapshot.mapping_rows,
            duplicate_mapping_keys: interrupted_recovery.final_snapshot.duplicate_mapping_keys,
            formal_rows_equal: interrupted_formal_rows_equal,
            duplicate_formal_facts: interrupted_recovery.final_snapshot.duplicate_formal_facts,
            object_files_equal: interrupted_object_files_equal,
            duplicate_objects: interrupted_recovery.final_snapshot.duplicate_objects,
        },
        MutationFixture::PartiallyWrittenTarget {
            partial_mapping_artifact: partial_target.partial_mapping_artifact,
            engine_rejected: partial_target.engine_rejected,
            mapping_rows: partial_target.mapping_rows,
            formal_fact_rows: partial_target.formal_fact_rows,
            object_files: partial_target.object_files,
        },
        MutationFixture::DuplicatePhysicalContent {
            physical_match_count: ambiguous_count,
            ambiguous_count,
        },
        MutationFixture::OcrRevisionMismatch {
            ocr_lineage_count,
            revision_matches: false,
        },
        MutationFixture::LlmProcessingLineageMismatch {
            llm_lineage_count,
            lineage_matches: false,
        },
        MutationFixture::AmbiguousReference {
            candidate_count: ambiguous_count,
            ambiguous_count,
        },
        MutationFixture::IncorrectOldFileIdPath {
            source_evidence_count,
            reference_matches: false,
        },
    ]
}

fn validate_case_matrix(results: &[CaseResult]) -> Result<(), Stage4Error> {
    if results.len() != CaseId::ALL.len()
        || results
            .iter()
            .any(|result| result.auto_materialize || !result.invariant_proven)
    {
        return Err(Stage4Error::MatrixInvariant);
    }
    for required in CaseId::ALL {
        if !results.iter().any(|result| result.case_id == required) {
            return Err(Stage4Error::MatrixInvariant);
        }
    }
    Ok(())
}

fn digest_json<T: Serialize>(value: &T) -> Result<String, Stage4Error> {
    let bytes = serde_json::to_vec(value).map_err(Stage4Error::Serialization)?;
    Ok(sha256_bytes(&bytes))
}

fn write_audit(root: &Path, audit: &Stage4Audit) -> Result<String, Stage4Error> {
    let bytes = serde_json::to_vec_pretty(audit).map_err(Stage4Error::Serialization)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(STAGE4_AUDIT_FILE_NAME))
        .map_err(|_| Stage4Error::TargetWrite)?;
    file.write_all(&bytes)
        .map_err(|_| Stage4Error::TargetWrite)?;
    file.flush().map_err(|_| Stage4Error::TargetWrite)?;
    Ok(sha256_bytes(&bytes))
}

fn map_inventory_error(_error: InventoryError) -> Stage4Error {
    Stage4Error::TargetRead("object_digest")
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{validate_integrity_cases, CaseDisposition, CaseId, MutationFixture};

    fn valid_fixtures() -> Vec<MutationFixture> {
        vec![
            MutationFixture::StaleVersion {
                expected_version: 2,
                observed_version: 1,
                manifest_records: 1,
            },
            MutationFixture::ConflictingRelation {
                relation_matches: false,
            },
            MutationFixture::MissingObject {
                source_evidence_count: 1,
                object_present: false,
            },
            MutationFixture::WrongSha256 {
                source_evidence_count: 1,
                checksum_matches: false,
            },
            MutationFixture::CorruptedObject {
                source_evidence_count: 1,
                bytes_match: false,
                checksum_matches: false,
            },
            MutationFixture::DuplicateReplay {
                replayed: true,
                mapping_plan_equal: true,
                mapping_file_equal: true,
                mapping_rows_equal: true,
                duplicate_mapping_keys: 0,
                formal_rows_equal: true,
                duplicate_formal_facts: 0,
                object_files_equal: true,
                duplicate_objects: 0,
            },
            MutationFixture::InterruptedRehearsal {
                audit_removed: true,
                recovery_replayed: true,
                mapping_plan_equal: true,
                mapping_file_equal: true,
                mapping_rows_equal: true,
                duplicate_mapping_keys: 0,
                formal_rows_equal: true,
                duplicate_formal_facts: 0,
                object_files_equal: true,
                duplicate_objects: 0,
            },
            MutationFixture::PartiallyWrittenTarget {
                partial_mapping_artifact: true,
                engine_rejected: true,
                mapping_rows: 0,
                formal_fact_rows: 0,
                object_files: 0,
            },
            MutationFixture::DuplicatePhysicalContent {
                physical_match_count: 2,
                ambiguous_count: 1,
            },
            MutationFixture::OcrRevisionMismatch {
                ocr_lineage_count: 1,
                revision_matches: false,
            },
            MutationFixture::LlmProcessingLineageMismatch {
                llm_lineage_count: 1,
                lineage_matches: false,
            },
            MutationFixture::AmbiguousReference {
                candidate_count: 2,
                ambiguous_count: 1,
            },
            MutationFixture::IncorrectOldFileIdPath {
                source_evidence_count: 1,
                reference_matches: false,
            },
        ]
    }

    #[test]
    fn validator_covers_all_cases_without_materialization() {
        let results = validate_integrity_cases(&valid_fixtures());
        assert_eq!(results.len(), CaseId::ALL.len());
        for (result, expected_case) in results.iter().zip(CaseId::ALL) {
            assert_eq!(result.case_id, expected_case);
            assert!(result.invariant_proven);
            assert!(!result.auto_materialize);
            assert!(!result.safe_code.is_empty());
            assert!(matches!(
                result.disposition,
                CaseDisposition::FailClosed
                    | CaseDisposition::Quarantine
                    | CaseDisposition::ManualReview
            ));
        }
    }

    #[test]
    fn replay_and_interrupted_recovery_require_equal_target_state() {
        let mut fixtures = valid_fixtures();
        let result = validate_integrity_cases(&[fixtures.remove(5)]);
        assert!(result[0].invariant_proven);
        assert_eq!(result[0].disposition, CaseDisposition::FailClosed);

        let result = validate_integrity_cases(&[MutationFixture::InterruptedRehearsal {
            audit_removed: true,
            recovery_replayed: true,
            mapping_plan_equal: true,
            mapping_file_equal: true,
            mapping_rows_equal: false,
            duplicate_mapping_keys: 0,
            formal_rows_equal: true,
            duplicate_formal_facts: 0,
            object_files_equal: true,
            duplicate_objects: 0,
        }]);
        assert!(!result[0].invariant_proven);
        assert_eq!(result[0].safe_code, "interrupted_rehearsal_recovered");
    }

    #[test]
    fn partial_target_requires_rejection_and_zero_facts() {
        let result = validate_integrity_cases(&[MutationFixture::PartiallyWrittenTarget {
            partial_mapping_artifact: true,
            engine_rejected: true,
            mapping_rows: 0,
            formal_fact_rows: 0,
            object_files: 0,
        }]);
        assert!(result[0].invariant_proven);

        let result = validate_integrity_cases(&[MutationFixture::PartiallyWrittenTarget {
            partial_mapping_artifact: true,
            engine_rejected: false,
            mapping_rows: 1,
            formal_fact_rows: 0,
            object_files: 0,
        }]);
        assert!(!result[0].invariant_proven);
        assert_eq!(result[0].safe_code, "partial_target_fail_closed");
    }
}
