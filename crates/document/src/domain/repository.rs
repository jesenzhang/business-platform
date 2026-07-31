use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use super::entity::DocumentMetadata;

/// Bounded query used by the list use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListDocumentsQuery {
    pub tenant_id: Uuid,
    pub limit: i64,
    pub offset: i64,
}

/// A page of tenant-scoped documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPage {
    pub items: Vec<DocumentMetadata>,
    pub total: i64,
}

/// Stable repository error classification; `SQLx` stays in the adapter.
#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("document repository unavailable")]
    Unavailable,
    #[error("document repository conflict")]
    Conflict,
    #[error("document repository failed")]
    Failed,
}

/// Read-only persistence port for document metadata.
#[async_trait]
pub trait DocumentQueryRepository: Send + Sync {
    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError>;

    async fn list(&self, query: ListDocumentsQuery) -> Result<DocumentPage, RepositoryError>;
}
