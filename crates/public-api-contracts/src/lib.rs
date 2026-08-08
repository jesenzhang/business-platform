//! Versioned, transport-neutral contracts for external Business API clients.
//!
//! This crate intentionally has no dependency on Axum, `SQLx`, repositories, or
//! infrastructure adapters. The API maps application read models into these
//! DTOs; CLI and MCP consume the same shapes over HTTP.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const API_VERSION: &str = "v1";
pub const OPENAPI_TITLE: &str = "Business Platform Public API";
pub const OPENAPI_VERSION: &str = "3.1.0";
pub const OPENAPI_DOCUMENT_PATH: &str = "openapi.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Document {
    pub id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub status: String,
    pub version: i64,
    pub content_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_no: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_current: Option<bool>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcessingJob {
    pub job_id: Uuid,
    pub document_id: Uuid,
    pub content_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<Uuid>,
    pub status: String,
    pub current_step: String,
    pub attempt_count: i32,
    #[serde(default)]
    pub failure_code: Option<String>,
    pub cancel_requested: bool,
    pub candidate_available: bool,
    pub review_available: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub candidate_id: Uuid,
    pub job_id: Uuid,
    pub content_revision: i64,
    pub schema_version: String,
    pub payload: Value,
    pub evidence: Vec<Value>,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Review {
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub candidate_version: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewResult {
    pub review: Review,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEvent {
    pub id: Uuid,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub stream_sequence: i64,
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntegrityFinding {
    pub id: Uuid,
    pub rule_id: String,
    pub bounded_context: String,
    pub resource_type: String,
    pub resource_id: String,
    pub severity: String,
    pub status: String,
    pub repairability: String,
    pub first_detected_at: DateTime<Utc>,
    pub last_detected_at: DateTime<Utc>,
    pub occurrence_count: i64,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationsOverview {
    pub document_total: u64,
    pub document_created_today: u64,
    pub processing_by_status: ProcessingStatusCounts,
    pub review_pending: u64,
    pub unresolved_findings: u64,
    pub recent_jobs: Vec<ProcessingJob>,
    pub recent_audit_events: Vec<AuditEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProcessingStatusCounts {
    pub queued: u64,
    pub running: u64,
    pub waiting_for_ai: u64,
    pub waiting_for_review: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub rejected: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_round_trips_without_internal_storage_fields() {
        let document = Document {
            id: Uuid::now_v7(),
            original_filename: "report.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            status: "active".to_string(),
            version: 1,
            content_revision: 1,
            revision_id: None,
            revision_no: None,
            is_current: None,
            size_bytes: Some(42),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let encoded = serde_json::to_string(&document).unwrap_or_default();
        assert!(!encoded.contains("object_key"));
        assert!(!encoded.contains("storage_key"));
        assert_eq!(
            serde_json::from_str::<Document>(&encoded).ok(),
            Some(document)
        );
    }
}
