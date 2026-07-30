//! `ListDocumentMetadata` use case.

use shared_kernel::error::AppError;
use shared_kernel::pagination::{PageRequest, PageResponse};
use uuid::Uuid;

use crate::domain::{DocumentMetadata, DocumentRepository};

/// Use case: list documents for a tenant with pagination.
pub struct ListDocumentMetadata<'a> {
    repo: &'a dyn DocumentRepository,
}

impl<'a> ListDocumentMetadata<'a> {
    /// Create a new instance of the use case.
    pub fn new(repo: &'a dyn DocumentRepository) -> Self {
        Self { repo }
    }

    /// Execute the list documents use case.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Database`] if the query fails.
    pub async fn execute(
        &self,
        tenant_id: Uuid,
        page: &PageRequest,
    ) -> Result<PageResponse<DocumentMetadata>, AppError> {
        let limit = page.limit();
        let offset = page.offset();

        let (items, total) = self
            .repo
            .list(tenant_id, limit, offset)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(PageResponse::new(items, total, page.page, page.page_size))
    }
}
