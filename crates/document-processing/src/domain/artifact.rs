//! Immutable processing run, artifact and evidence identities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    NormalizedPdf,
    OcrText,
    Layout,
    PageStructure,
    Thumbnail,
    FieldExtraction,
    Summary,
    Embedding,
}

impl ArtifactKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormalizedPdf => "normalized_pdf",
            Self::OcrText => "ocr_text",
            Self::Layout => "layout",
            Self::PageStructure => "page_structure",
            Self::Thumbnail => "thumbnail",
            Self::FieldExtraction => "field_extraction",
            Self::Summary => "summary",
            Self::Embedding => "embedding",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcessingArtifactError {
    #[error("processing identity is invalid")]
    InvalidIdentity,
    #[error("processing revision identity is invalid")]
    InvalidRevision,
    #[error("processing metadata must not be empty")]
    EmptyMetadata,
    #[error("processing checksum is invalid")]
    InvalidChecksum,
    #[error("evidence references a different revision, run, or artifact")]
    SourceMismatch,
    #[error("processing run status transition is invalid")]
    InvalidStatus,
    #[error("processing run timestamp is invalid")]
    InvalidTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingRun {
    id: Uuid,
    tenant_id: Uuid,
    document_revision_id: Uuid,
    pipeline_version: String,
    parser_name: String,
    parser_version: String,
    model_provider: Option<String>,
    model_name: Option<String>,
    status: ProcessingRunStatus,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    failure_code: Option<String>,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

impl ProcessingRun {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        tenant_id: Uuid,
        document_revision_id: Uuid,
        pipeline_version: String,
        parser_name: String,
        parser_version: String,
        model_provider: Option<String>,
        model_name: Option<String>,
        created_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Self, ProcessingArtifactError> {
        Self::start_with_id(
            Uuid::now_v7(),
            tenant_id,
            document_revision_id,
            pipeline_version,
            parser_name,
            parser_version,
            model_provider,
            model_name,
            created_by,
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_with_id(
        id: Uuid,
        tenant_id: Uuid,
        document_revision_id: Uuid,
        pipeline_version: String,
        parser_name: String,
        parser_version: String,
        model_provider: Option<String>,
        model_name: Option<String>,
        created_by: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Self, ProcessingArtifactError> {
        if id.is_nil() || tenant_id.is_nil() || document_revision_id.is_nil() || created_by.is_nil()
        {
            return Err(ProcessingArtifactError::InvalidIdentity);
        }
        if pipeline_version.trim().is_empty()
            || parser_name.trim().is_empty()
            || parser_version.trim().is_empty()
        {
            return Err(ProcessingArtifactError::EmptyMetadata);
        }
        Ok(Self {
            id,
            tenant_id,
            document_revision_id,
            pipeline_version,
            parser_name,
            parser_version,
            model_provider,
            model_name,
            status: ProcessingRunStatus::Running,
            started_at: Some(now),
            finished_at: None,
            failure_code: None,
            created_by,
            created_at: now,
        })
    }

    /// Complete an imported run after its immutable output is validated.
    pub fn finish_succeeded(&mut self, now: DateTime<Utc>) -> Result<(), ProcessingArtifactError> {
        if self.status != ProcessingRunStatus::Running {
            return Err(ProcessingArtifactError::InvalidStatus);
        }
        if self.started_at.is_some_and(|started_at| now < started_at) {
            return Err(ProcessingArtifactError::InvalidTimestamp);
        }
        self.status = ProcessingRunStatus::Succeeded;
        self.finished_at = Some(now);
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub const fn document_revision_id(&self) -> Uuid {
        self.document_revision_id
    }
    #[must_use]
    pub fn pipeline_version(&self) -> &str {
        &self.pipeline_version
    }
    #[must_use]
    pub fn parser_name(&self) -> &str {
        &self.parser_name
    }
    #[must_use]
    pub fn parser_version(&self) -> &str {
        &self.parser_version
    }
    #[must_use]
    pub fn model_provider(&self) -> Option<&str> {
        self.model_provider.as_deref()
    }
    #[must_use]
    pub fn model_name(&self) -> Option<&str> {
        self.model_name.as_deref()
    }
    #[must_use]
    pub const fn status(&self) -> ProcessingRunStatus {
        self.status
    }
    #[must_use]
    pub const fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }
    #[must_use]
    pub const fn finished_at(&self) -> Option<DateTime<Utc>> {
        self.finished_at
    }
    #[must_use]
    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }
    #[must_use]
    pub const fn created_by(&self) -> Uuid {
        self.created_by
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingArtifact {
    id: Uuid,
    tenant_id: Uuid,
    processing_run_id: Uuid,
    kind: ArtifactKind,
    storage_ref: String,
    checksum: String,
    schema_version: String,
    created_at: DateTime<Utc>,
}

impl ProcessingArtifact {
    pub fn new(
        tenant_id: Uuid,
        processing_run_id: Uuid,
        kind: ArtifactKind,
        storage_ref: String,
        checksum: String,
        schema_version: String,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ProcessingArtifactError> {
        Self::new_with_id(
            Uuid::now_v7(),
            tenant_id,
            processing_run_id,
            kind,
            storage_ref,
            checksum,
            schema_version,
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: Uuid,
        tenant_id: Uuid,
        processing_run_id: Uuid,
        kind: ArtifactKind,
        storage_ref: String,
        checksum: String,
        schema_version: String,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ProcessingArtifactError> {
        if id.is_nil() || tenant_id.is_nil() || processing_run_id.is_nil() {
            return Err(ProcessingArtifactError::InvalidIdentity);
        }
        if storage_ref.trim().is_empty() || schema_version.trim().is_empty() {
            return Err(ProcessingArtifactError::EmptyMetadata);
        }
        if checksum.len() != 64 || !checksum.chars().all(|value| value.is_ascii_hexdigit()) {
            return Err(ProcessingArtifactError::InvalidChecksum);
        }
        Ok(Self {
            id,
            tenant_id,
            processing_run_id,
            kind,
            storage_ref,
            checksum,
            schema_version,
            created_at,
        })
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub const fn processing_run_id(&self) -> Uuid {
        self.processing_run_id
    }
    #[must_use]
    pub const fn kind(&self) -> ArtifactKind {
        self.kind
    }
    #[must_use]
    pub fn storage_ref(&self) -> &str {
        &self.storage_ref
    }
    #[must_use]
    pub fn checksum(&self) -> &str {
        &self.checksum
    }
    #[must_use]
    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    id: Uuid,
    tenant_id: Uuid,
    document_revision_id: Uuid,
    processing_run_id: Uuid,
    artifact_id: Uuid,
    location: serde_json::Value,
    source_checksum: String,
    created_at: DateTime<Utc>,
}

impl Evidence {
    pub fn new(
        tenant_id: Uuid,
        document_revision_id: Uuid,
        run: &ProcessingRun,
        artifact: &ProcessingArtifact,
        location: serde_json::Value,
        source_checksum: String,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ProcessingArtifactError> {
        Self::new_with_id(
            Uuid::now_v7(),
            tenant_id,
            document_revision_id,
            run,
            artifact,
            location,
            source_checksum,
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: Uuid,
        tenant_id: Uuid,
        document_revision_id: Uuid,
        run: &ProcessingRun,
        artifact: &ProcessingArtifact,
        location: serde_json::Value,
        source_checksum: String,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ProcessingArtifactError> {
        if id.is_nil() {
            return Err(ProcessingArtifactError::InvalidIdentity);
        }
        if tenant_id != run.tenant_id()
            || tenant_id != artifact.tenant_id()
            || document_revision_id != run.document_revision_id()
            || artifact.processing_run_id() != run.id()
        {
            return Err(ProcessingArtifactError::SourceMismatch);
        }
        if source_checksum.len() != 64
            || !source_checksum
                .chars()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(ProcessingArtifactError::InvalidChecksum);
        }
        Ok(Self {
            id,
            tenant_id,
            document_revision_id,
            processing_run_id: run.id(),
            artifact_id: artifact.id(),
            location,
            source_checksum,
            created_at,
        })
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub const fn document_revision_id(&self) -> Uuid {
        self.document_revision_id
    }
    #[must_use]
    pub const fn processing_run_id(&self) -> Uuid {
        self.processing_run_id
    }
    #[must_use]
    pub const fn artifact_id(&self) -> Uuid {
        self.artifact_id
    }
    #[must_use]
    pub fn location(&self) -> &serde_json::Value {
        &self.location
    }
    #[must_use]
    pub fn source_checksum(&self) -> &str {
        &self.source_checksum
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_artifact_and_evidence_are_bound_to_one_revision() {
        let tenant = Uuid::now_v7();
        let revision = Uuid::now_v7();
        let run = ProcessingRun::start(
            tenant,
            revision,
            "pipeline.v1".to_string(),
            "deterministic".to_string(),
            "1".to_string(),
            None,
            None,
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap_or_else(|_| unreachable!());
        let artifact = ProcessingArtifact::new(
            tenant,
            run.id(),
            ArtifactKind::OcrText,
            "artifact/ref".to_string(),
            "a".repeat(64),
            "text.v1".to_string(),
            Utc::now(),
        )
        .unwrap_or_else(|_| unreachable!());
        let evidence = Evidence::new(
            tenant,
            revision,
            &run,
            &artifact,
            serde_json::json!({"page": 1, "line_start": 1}),
            "b".repeat(64),
            Utc::now(),
        );
        assert!(evidence.is_ok());
        assert!(Evidence::new(
            tenant,
            Uuid::now_v7(),
            &run,
            &artifact,
            serde_json::json!({}),
            "b".repeat(64),
            Utc::now(),
        )
        .is_err());
    }

    #[test]
    fn deterministic_id_constructors_keep_immutable_lineage() {
        let tenant = Uuid::now_v7();
        let revision = Uuid::now_v7();
        let actor = Uuid::now_v7();
        let run_id = Uuid::from_u128(0x1000);
        let artifact_id = Uuid::from_u128(0x2000);
        let evidence_id = Uuid::from_u128(0x3000);
        let now = Utc::now();
        let mut run = ProcessingRun::start_with_id(
            run_id,
            tenant,
            revision,
            "pipeline.v1".to_string(),
            "deterministic".to_string(),
            "1".to_string(),
            None,
            None,
            actor,
            now,
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(run.id(), run_id);
        assert!(run.finish_succeeded(now).is_ok());
        assert_eq!(run.status(), ProcessingRunStatus::Succeeded);
        let artifact = ProcessingArtifact::new_with_id(
            artifact_id,
            tenant,
            run_id,
            ArtifactKind::OcrText,
            "artifact/ref".to_string(),
            "a".repeat(64),
            "text.v1".to_string(),
            now,
        )
        .unwrap_or_else(|_| unreachable!());
        let evidence = Evidence::new_with_id(
            evidence_id,
            tenant,
            revision,
            &run,
            &artifact,
            serde_json::json!({"page": 1}),
            "b".repeat(64),
            now,
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(artifact.id(), artifact_id);
        assert_eq!(evidence.id(), evidence_id);
        assert_eq!(evidence.processing_run_id(), run_id);
    }
}
