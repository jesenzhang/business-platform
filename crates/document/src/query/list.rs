use async_trait::async_trait;
use uuid::Uuid;

use super::{DocumentListCursor, DocumentListFilter, DocumentListPage, QueryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentListRequest {
    pub tenant_id: Uuid,
    pub filter: DocumentListFilter,
    pub cursor: Option<DocumentListCursor>,
    pub limit: u32,
}

#[async_trait]
pub trait DocumentListQuery: Send + Sync {
    async fn execute(&self, request: DocumentListRequest) -> Result<DocumentListPage, QueryError>;
}
