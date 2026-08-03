use async_trait::async_trait;
use document::query::{DocumentDetailQuery, DocumentDetailView, QueryError};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::mapper::{map_query_error, DocumentRow};

pub struct SqliteDocumentDetailQuery {
    pool: SqlitePool,
}

impl SqliteDocumentDetailQuery {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentDetailQuery for SqliteDocumentDetailQuery {
    async fn execute(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentDetailView>, QueryError> {
        sqlx::query_as::<_, DocumentRow>(
            "SELECT id, tenant_id, original_filename, content_type, status, version, content_revision, size_bytes, created_by, created_at, updated_at FROM documents WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(document_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_query_error)?
        .map(TryInto::try_into)
        .transpose()
    }
}
