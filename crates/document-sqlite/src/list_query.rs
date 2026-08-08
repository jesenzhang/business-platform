use async_trait::async_trait;
use document::query::{
    DocumentListCursor, DocumentListPage, DocumentListQuery, DocumentListRequest, QueryError,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::mapper::{map_query_error, ListRow};

pub struct SqliteDocumentListQuery {
    pool: SqlitePool,
}

impl SqliteDocumentListQuery {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentListQuery for SqliteDocumentListQuery {
    async fn execute(&self, request: DocumentListRequest) -> Result<DocumentListPage, QueryError> {
        let limit = request.limit.clamp(1, 200);
        let mut builder = QueryBuilder::<Sqlite>::new(
            "SELECT id, original_filename, content_type, status, version, content_revision, current_revision_id, size_bytes, created_at, updated_at FROM documents WHERE tenant_id = ",
        );
        builder.push_bind(request.tenant_id.to_string());
        if let Some(status) = request.filter.status {
            builder.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(filename) = request.filter.filename_contains {
            builder
                .push(" AND original_filename LIKE ")
                .push_bind(format!(
                    "%{}%",
                    document::query::escape_like_literal(&filename)
                ))
                .push(" ESCAPE '\\'");
        }
        if let Some(after) = request.filter.created_after {
            builder
                .push(" AND created_at >= ")
                .push_bind(after.to_rfc3339());
        }
        if let Some(before) = request.filter.created_before {
            builder
                .push(" AND created_at < ")
                .push_bind(before.to_rfc3339());
        }
        if let Some(cursor) = request.cursor {
            builder
                .push(" AND (created_at < ")
                .push_bind(cursor.created_at.to_rfc3339())
                .push(" OR (created_at = ")
                .push_bind(cursor.created_at.to_rfc3339())
                .push(" AND id < ")
                .push_bind(cursor.id.to_string())
                .push("))");
        }
        builder
            .push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(i64::from(limit) + 1);
        let rows = builder
            .build_query_as::<ListRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(map_query_error)?;
        let has_more = rows.len() > limit as usize;
        let items: Vec<document::query::DocumentListItem> = rows
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
        Ok(DocumentListPage { items, next_cursor })
    }

    async fn count(
        &self,
        tenant_id: uuid::Uuid,
        filter: document::query::DocumentListFilter,
    ) -> Result<u64, QueryError> {
        let count = sqlx::query_scalar::<_, i64>(
            r"SELECT COUNT(*)
              FROM documents
              WHERE tenant_id = ?1
                AND (?2 IS NULL OR status = ?2)
                AND (?3 IS NULL OR original_filename LIKE '%' || ?3 || '%' ESCAPE '\')
                AND (?4 IS NULL OR created_at >= ?4)
                AND (?5 IS NULL OR created_at < ?5)",
        )
        .bind(tenant_id.to_string())
        .bind(filter.status.map(|value| value.as_str().to_string()))
        .bind(
            filter
                .filename_contains
                .as_deref()
                .map(document::query::escape_like_literal),
        )
        .bind(filter.created_after.map(|value| value.to_rfc3339()))
        .bind(filter.created_before.map(|value| value.to_rfc3339()))
        .fetch_one(&self.pool)
        .await
        .map_err(map_query_error)?;
        u64::try_from(count).map_err(|_| QueryError::InvalidStoredData)
    }
}
