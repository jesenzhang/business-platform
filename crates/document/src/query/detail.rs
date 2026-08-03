use async_trait::async_trait;
use uuid::Uuid;

use super::{DocumentDetailView, QueryError};

#[async_trait]
pub trait DocumentDetailQuery: Send + Sync {
    async fn execute(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentDetailView>, QueryError>;
}
