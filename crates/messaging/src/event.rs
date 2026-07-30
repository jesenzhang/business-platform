use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 领域事件基础结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub tenant_id: String,
    pub aggregate_id: String,
    pub aggregate_type: String,
    pub payload: serde_json::Value,
    pub schema_version: String,
    pub trace_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

impl DomainEvent {
    pub fn new(
        event_type: impl Into<String>,
        tenant_id: impl Into<String>,
        aggregate_id: impl Into<String>,
        aggregate_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            event_type: event_type.into(),
            tenant_id: tenant_id.into(),
            aggregate_id: aggregate_id.into(),
            aggregate_type: aggregate_type.into(),
            payload,
            schema_version: "v1".to_string(),
            trace_id: None,
            occurred_at: Utc::now(),
        }
    }

    /// Attach a trace ID to the event for distributed tracing correlation.
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
}
