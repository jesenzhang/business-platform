use std::sync::Arc;

use shared_kernel::pagination::{PageRequest, PageResponse};
use uuid::Uuid;

use crate::domain::{
    DocumentMetadata, DocumentQueryRepository, ListDocumentsQuery, RepositoryError,
};

use super::get::{map_repository_error, QueryDocumentError};

pub struct ListDocumentMetadata {
    repository: Arc<dyn DocumentQueryRepository>,
}

impl ListDocumentMetadata {
    #[must_use]
    pub fn new(repository: Arc<dyn DocumentQueryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        page: &PageRequest,
    ) -> Result<PageResponse<DocumentMetadata>, QueryDocumentError> {
        let result = self
            .repository
            .list(ListDocumentsQuery {
                tenant_id,
                limit: page.limit(),
                offset: page.offset(),
            })
            .await
            .map_err(|error: RepositoryError| map_repository_error(&error))?;

        Ok(PageResponse::new(
            result.items,
            result.total,
            page.page,
            page.page_size,
        ))
    }
}
