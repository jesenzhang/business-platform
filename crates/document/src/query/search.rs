use async_trait::async_trait;
use uuid::Uuid;

use super::{DocumentListPage, QueryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSearchRequest {
    pub tenant_id: Uuid,
    pub terms: String,
    pub limit: u32,
}

#[async_trait]
pub trait DocumentSearchQuery: Send + Sync {
    async fn execute(&self, request: DocumentSearchRequest)
        -> Result<DocumentListPage, QueryError>;
}
