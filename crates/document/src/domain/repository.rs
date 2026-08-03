//! Aggregate persistence port for Document Management.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use super::entity::DocumentMetadata;
use super::version::AggregateVersion;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryError {
    #[error("document repository unavailable")]
    Unavailable,
    #[error("document repository conflict")]
    Conflict,
    #[error("document repository failed")]
    Failed,
}

/// Command-side aggregate persistence. Query-side reads remain in the formal
/// `DocumentDetailQuery` and `DocumentListQuery` ports.
#[async_trait]
pub trait DocumentRepository: Send + Sync {
    async fn load(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError>;

    async fn save(
        &self,
        document: &DocumentMetadata,
        expected_version: AggregateVersion,
    ) -> Result<(), RepositoryError>;
}
