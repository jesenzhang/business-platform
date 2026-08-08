use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcessingDomainError {
    #[error("invalid processing job identity")]
    InvalidIdentity,
    #[error("content revision must be positive")]
    InvalidContentRevision,
    #[error("document revision identity must not be nil")]
    InvalidDocumentRevision,
    #[error("request key must not be empty")]
    EmptyRequestKey,
    #[error("max attempts must be at least one")]
    InvalidMaxAttempts,
    #[error("invalid state transition from {from} via {action}")]
    InvalidTransition { from: String, action: String },
    #[error("processing job is terminal")]
    Terminal,
    #[error("processing step is not the current step")]
    InvalidStep,
    #[error("lease is missing, expired, or fenced")]
    LeaseLost,
    #[error("cancel has not been requested")]
    CancelNotRequested,
    #[error("retry is not available")]
    RetryUnavailable,
    #[error("attempt count overflow")]
    AttemptOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReviewError {
    #[error("candidate is not awaiting review")]
    NotAwaitingReview,
    #[error("candidate version does not match")]
    VersionConflict,
    #[error("review decision is invalid for the supplied patch")]
    InvalidDecision,
    #[error("review is already final")]
    AlreadyReviewed,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExtractionError {
    #[error("source object was not found")]
    SourceNotFound,
    #[error("source content revision does not match the job")]
    SourceRevisionMismatch,
    #[error("unsupported content type")]
    UnsupportedContentType,
    #[error("content exceeds configured limit")]
    ContentTooLarge,
    #[error("source is not valid UTF-8")]
    InvalidTextEncoding,
    #[error("AI provider is unavailable")]
    AiProviderUnavailable,
    #[error("AI provider returned an invalid response")]
    AiInvalidResponse,
    #[error("candidate validation failed")]
    CandidateValidationFailed,
    #[error("lease was lost")]
    LeaseLost,
    #[error("processing was cancelled")]
    Cancelled,
    #[error("internal processing error")]
    Internal,
}

impl ExtractionError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SourceNotFound => "source_not_found",
            Self::SourceRevisionMismatch => "source_revision_mismatch",
            Self::UnsupportedContentType => "unsupported_content_type",
            Self::ContentTooLarge => "content_too_large",
            Self::InvalidTextEncoding => "invalid_text_encoding",
            Self::AiProviderUnavailable => "ai_provider_unavailable",
            Self::AiInvalidResponse => "ai_invalid_response",
            Self::CandidateValidationFailed => "candidate_validation_failed",
            Self::LeaseLost => "lease_lost",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal_error",
        }
    }
}
