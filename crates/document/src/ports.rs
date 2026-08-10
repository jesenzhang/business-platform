use async_trait::async_trait;
use thiserror::Error;

use crate::domain::DocumentMetadata;

/// Atomic document-create persistence request.
#[derive(Debug, Clone)]
pub struct PersistNewDocument {
    pub document: DocumentMetadata,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub fingerprint_version: i16,
    pub initial_revision_sha256: Option<String>,
}

/// Result of an atomic create. `replayed` is true for an idempotent retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDocumentResult {
    pub document: DocumentMetadata,
    pub replayed: bool,
}

/// Stable failure categories exposed by an application port.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationPortError {
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    #[error("document persistence is unavailable")]
    Unavailable,
    #[error("document persistence failed")]
    Failed,
}

/// Atomically persists Document, Audit, Outbox, and Idempotency state.
#[async_trait]
pub trait CreateDocumentUnitOfWork: Send + Sync {
    async fn execute(
        &self,
        command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError>;
}
