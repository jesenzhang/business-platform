use chrono::Utc;
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
}
