use async_trait::async_trait;
use document::query::{DocumentDetailQuery, DocumentDetailView, QueryError};
use sqlx::PgPool;
use uuid::Uuid;

use crate::query_mapper::{map_query_error, DetailRow};

pub struct PostgresDocumentDetailQuery {
    pool: PgPool,
}

impl PostgresDocumentDetailQuery {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentDetailQuery for PostgresDocumentDetailQuery {
    async fn execute(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentDetailView>, QueryError> {
        sqlx::query_as::<_, DetailRow>(
            r"SELECT id, tenant_id, original_filename, content_type, status,
                     version, content_revision, size_bytes, created_by, created_at, updated_at
              FROM documents WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_query_error)?
        .map(TryInto::try_into)
        .transpose()
    }
}
