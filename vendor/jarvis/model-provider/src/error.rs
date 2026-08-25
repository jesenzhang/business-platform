use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProviderErrorKind {
    Authentication,
    RateLimit,
    Timeout,
    InvalidRequest,
    Unavailable,
    Serialization,
    Protocol,
    StreamInterrupted,
    Unsupported,
    Aborted,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FailurePhase {
    BeforeDispatch,
    AfterDispatch,
    DuringStream,
    Unknown,
}

/// Provider failures are bounded and intentionally body-free. Callers may
/// safely display this value in runtime diagnostics.
#[derive(Clone)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub phase: FailurePhase,
    pub message: String,
    pub http_status: Option<u16>,
    pub retry_after: Option<Duration>,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, phase: FailurePhase, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            kind,
            phase,
            message: message.chars().take(2_048).collect(),
            http_status: None,
            retry_after: None,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_retry_after(mut self, retry_after: Duration) -> Self {
        self.retry_after = Some(retry_after);
        self
    }

    pub fn redacted_message(message: impl AsRef<str>, secret: &str) -> String {
        let message = if secret.is_empty() {
            message.as_ref().to_owned()
        } else {
            message.as_ref().replace(secret, "[REDACTED]")
        };
        message.chars().take(2_048).collect()
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderError")
            .field("kind", &self.kind)
            .field("phase", &self.phase)
            .field("message", &self.message)
            .field("http_status", &self.http_status)
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ProviderError {}
