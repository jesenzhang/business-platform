//! Shared contract helpers for `PostgreSQL`, `SQLite`, and local process tests.

use chrono::{DateTime, Utc};
use document_processing::{ProcessingJob, ProcessingJobStatus, ProcessingStepKind};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingJobSnapshot {
    pub tenant_id: Uuid,
    pub document_id: Uuid,
    pub status: ProcessingJobStatus,
    pub current_step: ProcessingStepKind,
    pub version: i64,
    pub observed_at: DateTime<Utc>,
}

impl ProcessingJobSnapshot {
    #[must_use]
    pub fn from_job(job: &ProcessingJob, observed_at: DateTime<Utc>) -> Self {
        Self {
            tenant_id: job.tenant_id(),
            document_id: job.document_id(),
            status: job.status(),
            current_step: job.current_step(),
            version: job.aggregate_version().value(),
            observed_at,
        }
    }
}

#[must_use]
pub fn safe_failure_code(code: &str) -> &str {
    match code {
        "source_not_found"
        | "source_revision_mismatch"
        | "unsupported_content_type"
        | "content_too_large"
        | "invalid_text_encoding"
        | "ai_provider_unavailable"
        | "ai_provider_rejected"
        | "ai_provider_rate_limited"
        | "ai_invalid_response"
        | "candidate_validation_failed"
        | "lease_lost"
        | "cancelled"
        | "internal_error" => code,
        _ => "internal_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_projection_is_allow_listed() {
        assert_eq!(
            safe_failure_code("database connection string"),
            "internal_error"
        );
        assert_eq!(
            safe_failure_code("unsupported_content_type"),
            "unsupported_content_type"
        );
        assert_eq!(
            safe_failure_code("ai_provider_rejected"),
            "ai_provider_rejected"
        );
        assert_eq!(
            safe_failure_code("ai_provider_rate_limited"),
            "ai_provider_rate_limited"
        );
    }
}
