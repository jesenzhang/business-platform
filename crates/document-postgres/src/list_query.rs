use async_trait::async_trait;
use document::query::{
    DocumentListCursor, DocumentListPage, DocumentListQuery, DocumentListRequest, QueryError,
};
use sqlx::PgPool;

use crate::query_mapper::{map_query_error, ListRow};

const MAX_PAGE_SIZE: u32 = 200;

pub struct PostgresDocumentListQuery {
    pool: PgPool,
}

impl PostgresDocumentListQuery {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentListQuery for PostgresDocumentListQuery {
    async fn execute(&self, request: DocumentListRequest) -> Result<DocumentListPage, QueryError> {
        let limit = request.limit.clamp(1, MAX_PAGE_SIZE);
        let status = request
            .filter
            .status
            .map(document::query::DocumentStatusFilter::as_str);
        let filename = request.filter.filename_contains.as_deref();
        let cursor_created_at = request.cursor.map(|cursor| cursor.created_at);
        let cursor_id = request.cursor.map(|cursor| cursor.id);
        let rows = sqlx::query_as::<_, ListRow>(
            r"SELECT id, original_filename, content_type, status, version,
                     size_bytes, created_at, updated_at
              FROM documents
              WHERE tenant_id = $1
                AND ($2::text IS NULL OR status = $2)
                AND ($3::text IS NULL OR original_filename ILIKE '%' || $3 || '%')
                AND ($4::timestamptz IS NULL OR created_at >= $4)
                AND ($5::timestamptz IS NULL OR created_at < $5)
                AND ($6::timestamptz IS NULL OR (created_at, id) < ($6, $7))
              ORDER BY created_at DESC, id DESC
              LIMIT $8",
        )
        .bind(request.tenant_id)
        .bind(status)
        .bind(filename)
        .bind(request.filter.created_after)
        .bind(request.filter.created_before)
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(map_query_error)?;

        let has_more = rows.len() > limit as usize;
        let mut items: Vec<document::query::DocumentListItem> = rows
            .into_iter()
            .take(limit as usize)
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            items.last().map(|item| DocumentListCursor {
                created_at: item.created_at,
                id: item.id,
            })
        } else {
            None
        };
        items.shrink_to_fit();
        Ok(DocumentListPage { items, next_cursor })
    }
}
