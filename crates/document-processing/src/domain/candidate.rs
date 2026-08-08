use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ReviewError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidatePayload {
    pub schema_version: String,
    pub title: Option<String>,
    pub document_type: String,
    pub language: Option<String>,
    pub summary: Option<String>,
    pub fields: BTreeMap<String, serde_json::Value>,
    pub warnings: Vec<String>,
}

impl CandidatePayload {
    pub const SCHEMA_VERSION: &'static str = "document.generic.v1";

    pub fn validate(&self, max_bytes: usize) -> Result<(), super::error::ExtractionError> {
        if self.schema_version != Self::SCHEMA_VERSION
            || self.document_type.trim().is_empty()
            || serde_json::to_vec(self)
                .map(|payload| payload.len() > max_bytes)
                .unwrap_or(true)
        {
            return Err(super::error::ExtractionError::CandidateValidationFailed);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateEvidenceSource {
    pub content_revision: i64,
    pub line_start: u32,
    pub line_end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateEvidence {
    pub field: String,
    pub source: CandidateEvidenceSource,
}

impl CandidateEvidence {
    pub fn validate(
        &self,
        content_revision: i64,
        line_count: u32,
    ) -> Result<(), super::error::ExtractionError> {
        if self.field.trim().is_empty()
            || self.source.content_revision != content_revision
            || self.source.line_start == 0
            || self.source.line_end < self.source.line_start
            || self.source.line_end > line_count
        {
            return Err(super::error::ExtractionError::CandidateValidationFailed);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionCandidate {
    id: Uuid,
    tenant_id: Uuid,
    job_id: Uuid,
    content_revision: i64,
    document_revision_id: Option<Uuid>,
    pub schema_version: String,
    pub payload: CandidatePayload,
    pub evidence: Vec<CandidateEvidence>,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    version: i64,
    created_at: DateTime<Utc>,
}

impl ExtractionCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        job_id: Uuid,
        content_revision: i64,
        payload: CandidatePayload,
        evidence: Vec<CandidateEvidence>,
        provider: String,
        model: String,
        prompt_version: String,
        line_count: u32,
        created_at: DateTime<Utc>,
    ) -> Result<Self, super::error::ExtractionError> {
        if tenant_id.is_nil() || job_id.is_nil() || content_revision <= 0 {
            return Err(super::error::ExtractionError::CandidateValidationFailed);
        }
        payload.validate(256 * 1024)?;
        for item in &evidence {
            item.validate(content_revision, line_count)?;
        }
        let mut fields = BTreeSet::new();
        fields.insert("title".to_string());
        for item in &evidence {
            if !fields.contains(&item.field) && !payload.fields.contains_key(&item.field) {
                return Err(super::error::ExtractionError::CandidateValidationFailed);
            }
        }
        Ok(Self {
            id: Uuid::now_v7(),
            tenant_id,
            job_id,
            content_revision,
            document_revision_id: None,
            schema_version: payload.schema_version.clone(),
            payload,
            evidence,
            provider,
            model,
            prompt_version,
            version: 1,
            created_at,
        })
    }

    /// Compatibility constructor that records the exact immutable revision.
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_revision(
        tenant_id: Uuid,
        job_id: Uuid,
        document_revision_id: Uuid,
        content_revision: i64,
        payload: CandidatePayload,
        evidence: Vec<CandidateEvidence>,
        provider: String,
        model: String,
        prompt_version: String,
        line_count: u32,
        created_at: DateTime<Utc>,
    ) -> Result<Self, super::error::ExtractionError> {
        if document_revision_id.is_nil() {
            return Err(super::error::ExtractionError::CandidateValidationFailed);
        }
        let mut candidate = Self::new(
            tenant_id,
            job_id,
            content_revision,
            payload,
            evidence,
            provider,
            model,
            prompt_version,
            line_count,
            created_at,
        )?;
        candidate.document_revision_id = Some(document_revision_id);
        Ok(candidate)
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
    pub const fn job_id(&self) -> Uuid {
        self.job_id
    }
    #[must_use]
    pub const fn content_revision(&self) -> i64 {
        self.content_revision
    }
    #[must_use]
    pub const fn document_revision_id(&self) -> Option<Uuid> {
        self.document_revision_id
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Accepted,
    Edited,
    Rejected,
}

impl ReviewDecision {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Edited => "edited",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateReview {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub candidate_id: Uuid,
    pub reviewer_id: Uuid,
    pub decision: ReviewDecision,
    pub patch: Option<serde_json::Value>,
    pub comment: Option<String>,
    pub candidate_version: i64,
    pub created_at: DateTime<Utc>,
}

impl CandidateReview {
    pub fn validate(&self, candidate: &ExtractionCandidate) -> Result<(), ReviewError> {
        if self.tenant_id != candidate.tenant_id()
            || self.candidate_id != candidate.id()
            || self.candidate_version != candidate.version()
            || self.reviewer_id.is_nil()
        {
            return Err(ReviewError::VersionConflict);
        }
        if matches!(self.decision, ReviewDecision::Edited) && self.patch.is_none() {
            return Err(ReviewError::InvalidDecision);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(title: Option<&str>) -> CandidatePayload {
        CandidatePayload {
            schema_version: CandidatePayload::SCHEMA_VERSION.to_string(),
            title: title.map(str::to_string),
            document_type: "text/plain".to_string(),
            language: None,
            summary: None,
            fields: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn candidate_evidence_is_revision_and_line_bounded() {
        let tenant = Uuid::now_v7();
        let job = Uuid::now_v7();
        let candidate = ExtractionCandidate::new(
            tenant,
            job,
            2,
            payload(Some("Title")),
            vec![CandidateEvidence {
                field: "title".to_string(),
                source: CandidateEvidenceSource {
                    content_revision: 2,
                    line_start: 1,
                    line_end: 1,
                },
            }],
            "deterministic-local".to_string(),
            "none".to_string(),
            "v1".to_string(),
            1,
            Utc::now(),
        );
        assert!(candidate.is_ok());
        let Ok(candidate) = candidate else {
            unreachable!()
        };
        let mut evidence = candidate.evidence[0].clone();
        evidence.source.content_revision = 1;
        assert!(evidence.validate(2, 1).is_err());
    }
}
