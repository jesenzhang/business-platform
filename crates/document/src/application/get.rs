//! `GetDocumentMetadata` use case.

use shared_kernel::error::AppError;
use uuid::Uuid;

use crate::domain::{DocumentMetadata, DocumentRepository};

/// Use case: retrieve a single document by ID within a tenant.
pub struct GetDocumentMetadata<'a> {
    repo: &'a dyn DocumentRepository,
}

impl<'a> GetDocumentMetadata<'a> {
    /// Create a new instance of the use case.
    pub fn new(repo: &'a dyn DocumentRepository) -> Self {
        Self { repo }
    }

    /// Execute the get document use case.
    ///
    /// Returns `None` if the document does not exist or belongs to a
    /// different tenant (tenant isolation).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Database`] if the query fails.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DocumentMetadata>, AppError> {
        self.repo
            .find_by_id(tenant_id, id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))
    }
}
