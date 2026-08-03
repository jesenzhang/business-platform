use async_trait::async_trait;

use crate::domain::{
    extract_text_artifact, DocumentFieldExtractor, ExtractionCandidate, ExtractionError,
    ExtractionRequest, ProcessingJob,
};

#[derive(Debug, Clone)]
pub struct PipelineRunResult {
    pub candidate: ExtractionCandidate,
    pub checkpoint: crate::domain::TextCheckpoint,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FixedPipelineRunner;

impl FixedPipelineRunner {
    pub async fn run_inline<E: DocumentFieldExtractor>(
        &self,
        job: &ProcessingJob,
        content_type: &str,
        bytes: &[u8],
        max_content_bytes: usize,
        extractor: &E,
    ) -> Result<PipelineRunResult, ExtractionError> {
        let artifact = extract_text_artifact(
            content_type,
            job.document_content_revision(),
            bytes,
            max_content_bytes,
        )?;
        let candidate = extractor
            .extract(ExtractionRequest {
                tenant_id: job.tenant_id(),
                job_id: job.id(),
                content_revision: artifact.content_revision,
                content_type: content_type.to_string(),
                text: artifact.text.clone(),
                line_count: artifact.line_count,
                character_count: artifact.character_count,
            })
            .await?;
        Ok(PipelineRunResult {
            candidate,
            checkpoint: crate::domain::TextCheckpoint {
                content_hash: artifact.content_hash,
                content_revision: artifact.content_revision,
                byte_count: artifact.byte_count,
                line_count: artifact.line_count,
                text_artifact_reference: format!("processing/{}/text", job.id()),
                character_count: artifact.character_count,
            },
        })
    }
}

#[async_trait]
pub trait ProcessingSource: Send + Sync {
    async fn read_source(
        &self,
        tenant_id: uuid::Uuid,
        document_id: uuid::Uuid,
        content_revision: i64,
    ) -> Result<(String, Vec<u8>), ExtractionError>;
}
