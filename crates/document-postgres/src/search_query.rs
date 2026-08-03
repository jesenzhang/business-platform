use async_trait::async_trait;
use document::query::{DocumentListPage, DocumentSearchQuery, DocumentSearchRequest, QueryError};
use sqlx::PgPool;

use crate::query_mapper::{map_query_error, ListRow};

pub struct PostgresDocumentSearchQuery {
    pool: PgPool,
}

impl PostgresDocumentSearchQuery {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentSearchQuery for PostgresDocumentSearchQuery {
    async fn execute(
        &self,
        request: DocumentSearchRequest,
    ) -> Result<DocumentListPage, QueryError> {
        let rows = sqlx::query_as::<_, ListRow>(
            r"SELECT id, original_filename, content_type, status, version,
                     size_bytes, created_at, updated_at
              FROM documents
              WHERE tenant_id = $1 AND original_filename ILIKE '%' || $2 || '%'
              ORDER BY created_at DESC, id DESC LIMIT $3",
        )
        .bind(request.tenant_id)
        .bind(request.terms)
        .bind(i64::from(request.limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(map_query_error)?;
        Ok(DocumentListPage {
            items: rows
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            next_cursor: None,
        })
    }
}
