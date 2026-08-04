use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{CandidateReview, ProcessingJob};

#[derive(Debug, Clone)]
pub struct ReviewCandidateCommand {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub reviewer_id: Uuid,
    pub decision: crate::domain::ReviewDecision,
    pub candidate_version: i64,
    pub patch: Option<serde_json::Value>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReviewCandidateResult {
    pub review: CandidateReview,
    pub job: ProcessingJob,
}

impl ReviewCandidateCommand {
    pub fn build_review(&self, candidate_id: Uuid) -> CandidateReview {
        CandidateReview {
            id: Uuid::now_v7(),
            tenant_id: self.tenant_id,
            candidate_id,
            reviewer_id: self.reviewer_id,
            decision: self.decision,
            patch: self.patch.clone(),
            comment: self.comment.clone(),
            candidate_version: self.candidate_version,
            created_at: Utc::now(),
        }
    }

    /// Build the stable request fingerprint used by the durable review
    /// idempotency boundary. The generated review id and timestamp are
    /// deliberately excluded so a retry represents the same request.
    pub fn request_fingerprint(&self, candidate_id: Uuid) -> Result<String, serde_json::Error> {
        let canonical = serde_json::to_vec(&(
            "document-processing.review.v1",
            self.tenant_id,
            self.job_id,
            candidate_id,
            self.reviewer_id,
            self.decision,
            self.candidate_version,
            &self.patch,
            &self.comment,
        ))?;
        Ok(format!("{:x}", Sha256::digest(canonical)))
    }
}
