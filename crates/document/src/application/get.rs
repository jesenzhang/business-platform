use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::domain::{DocumentMetadata, DocumentQueryRepository, RepositoryError};

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum QueryDocumentError {
    #[error("document repository is unavailable")]
    Unavailable,
    #[error("document query failed")]
    Failed,
}

pub struct GetDocumentMetadata {
    repository: Arc<dyn DocumentQueryRepository>,
}

impl GetDocumentMetadata {
    #[must_use]
    pub fn new(repository: Arc<dyn DocumentQueryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, QueryDocumentError> {
        self.repository
            .find_by_id(tenant_id, document_id)
            .await
            .map_err(|error| map_repository_error(&error))
    }
}

pub(crate) fn map_repository_error(error: &RepositoryError) -> QueryDocumentError {
    match error {
        RepositoryError::Unavailable => QueryDocumentError::Unavailable,
        RepositoryError::Conflict | RepositoryError::Failed => QueryDocumentError::Failed,
    }
}
