use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::job::{ProcessingJobStatus, ProcessingStepKind};

#[derive(Debug, Clone, Serialize)]
pub struct ProcessingEventEnvelope<T> {
    pub event_id: Uuid,
    pub event_type: &'static str,
    pub schema_version: &'static str,
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub document_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub trace_id: Option<String>,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize)]
pub enum ProcessingEvent {
    Requested,
    Started { step: ProcessingStepKind },
    StepCompleted { step: ProcessingStepKind },
    WaitingForReview,
    Succeeded,
    Failed { failure_code: String },
    Cancelled,
}

impl ProcessingEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Requested => "document.processing.requested.v1",
            Self::Started { .. } => "document.processing.started.v1",
            Self::StepCompleted { .. } => "document.processing.step-completed.v1",
            Self::WaitingForReview => "document.processing.waiting-for-review.v1",
            Self::Succeeded => "document.processing.succeeded.v1",
            Self::Failed { .. } => "document.processing.failed.v1",
            Self::Cancelled => "document.processing.cancelled.v1",
        }
    }

    #[must_use]
    pub const fn status(&self) -> Option<ProcessingJobStatus> {
        match self {
            Self::Requested | Self::Started { .. } | Self::StepCompleted { .. } => None,
            Self::WaitingForReview => Some(ProcessingJobStatus::WaitingForReview),
            Self::Succeeded => Some(ProcessingJobStatus::Succeeded),
            Self::Failed { .. } => Some(ProcessingJobStatus::Failed),
            Self::Cancelled => Some(ProcessingJobStatus::Cancelled),
        }
    }
}
