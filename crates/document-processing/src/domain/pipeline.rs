use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::error::ExtractionError;
use super::{CandidateEvidence, CandidatePayload, ExtractionCandidate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextArtifact {
    pub content_revision: i64,
    pub content_hash: String,
    pub byte_count: u64,
    pub line_count: u32,
    pub character_count: u64,
    pub text: String,
}

pub fn extract_text_artifact(
    content_type: &str,
    content_revision: i64,
    bytes: &[u8],
    max_content_bytes: usize,
) -> Result<TextArtifact, ExtractionError> {
    if content_revision <= 0 {
        return Err(ExtractionError::SourceRevisionMismatch);
    }
    if !matches!(
        content_type,
        "text/plain" | "text/markdown" | "application/json"
    ) {
        return Err(ExtractionError::UnsupportedContentType);
    }
    if bytes.len() > max_content_bytes {
        return Err(ExtractionError::ContentTooLarge);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ExtractionError::InvalidTextEncoding)?
        .to_string();
    let line_count = if text.is_empty() {
        0
    } else {
        u32::try_from(text.lines().count()).unwrap_or(u32::MAX)
    };
    let character_count = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(TextArtifact {
        content_revision,
        content_hash: format!("{:x}", digest.finalize()),
        byte_count: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        line_count,
        character_count,
        text,
    })
}

#[derive(Debug, Clone)]
pub struct ExtractionRequest {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub content_revision: i64,
    pub content_type: String,
    pub text: String,
    pub line_count: u32,
    pub character_count: u64,
}

#[async_trait]
pub trait DocumentFieldExtractor: Send + Sync {
    async fn extract(
        &self,
        request: ExtractionRequest,
    ) -> Result<ExtractionCandidate, ExtractionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DeterministicLocalExtractor;

#[async_trait]
impl DocumentFieldExtractor for DeterministicLocalExtractor {
    async fn extract(
        &self,
        request: ExtractionRequest,
    ) -> Result<ExtractionCandidate, ExtractionError> {
        let title = request
            .text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string);
        let payload = CandidatePayload {
            schema_version: CandidatePayload::SCHEMA_VERSION.to_string(),
            title: title.clone(),
            document_type: request.content_type.clone(),
            language: None,
            summary: None,
            fields: serde_json::Map::from_iter([
                (
                    "line_count".to_string(),
                    serde_json::Value::from(request.line_count),
                ),
                (
                    "character_count".to_string(),
                    serde_json::Value::from(request.character_count),
                ),
            ])
            .into_iter()
            .collect(),
            warnings: Vec::new(),
        };
        let evidence = title.map(|_| CandidateEvidence {
            field: "title".to_string(),
            source: super::CandidateEvidenceSource {
                content_revision: request.content_revision,
                line_start: request
                    .text
                    .lines()
                    .position(|line| !line.trim().is_empty())
                    .map_or(1, |line| u32::try_from(line + 1).unwrap_or(u32::MAX)),
                line_end: request
                    .text
                    .lines()
                    .position(|line| !line.trim().is_empty())
                    .map_or(1, |line| u32::try_from(line + 1).unwrap_or(u32::MAX)),
            },
        });
        ExtractionCandidate::new(
            request.tenant_id,
            request.job_id,
            request.content_revision,
            payload,
            evidence.into_iter().collect(),
            "deterministic-local".to_string(),
            "none".to_string(),
            "v1".to_string(),
            request.line_count,
            chrono::Utc::now(),
        )
        .map_err(|_| ExtractionError::CandidateValidationFailed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextCheckpoint {
    pub content_hash: String,
    pub content_revision: i64,
    pub byte_count: u64,
    pub line_count: u32,
    pub character_count: u64,
    pub text_artifact_reference: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deterministic_extractor_is_truthful_and_evidence_bounded() {
        let result = DeterministicLocalExtractor
            .extract(ExtractionRequest {
                tenant_id: Uuid::now_v7(),
                job_id: Uuid::now_v7(),
                content_revision: 1,
                content_type: "text/plain".to_string(),
                text: "Title\nsecond".to_string(),
                line_count: 2,
                character_count: 12,
            })
            .await;
        assert!(result.is_ok());
        let Ok(candidate) = result else {
            unreachable!()
        };
        assert_eq!(candidate.payload.title.as_deref(), Some("Title"));
        assert!(candidate.payload.summary.is_none());
        assert_eq!(candidate.evidence[0].source.line_start, 1);
    }

    #[test]
    fn source_reader_supports_only_declared_text_types_and_strict_utf8() {
        let artifact = extract_text_artifact("text/markdown", 1, b"# Title\n", 100);
        assert!(artifact.is_ok());
        assert!(matches!(
            extract_text_artifact("application/pdf", 1, b"%PDF", 100),
            Err(ExtractionError::UnsupportedContentType)
        ));
        assert!(matches!(
            extract_text_artifact("text/plain", 1, &[0xff], 100),
            Err(ExtractionError::InvalidTextEncoding)
        ));
    }
}
