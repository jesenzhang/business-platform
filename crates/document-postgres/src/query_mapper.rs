use chrono::{DateTime, Utc};
use document::query::{DocumentDetailView, DocumentListItem, DocumentStatusView, QueryError};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub(crate) struct DetailRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub status: String,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<DetailRow> for DocumentDetailView {
    type Error = QueryError;

    fn try_from(row: DetailRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            tenant_id: row.tenant_id,
            original_filename: row.original_filename,
            content_type: row.content_type,
            status: DocumentStatusView::parse(&row.status)?,
            version: row.version,
            size_bytes: row.size_bytes,
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct ListRow {
    pub id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub status: String,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<ListRow> for DocumentListItem {
    type Error = QueryError;

    fn try_from(row: ListRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            original_filename: row.original_filename,
            content_type: row.content_type,
            status: DocumentStatusView::parse(&row.status)?,
            version: row.version,
            size_bytes: row.size_bytes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn map_query_error(error: sqlx::Error) -> QueryError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            QueryError::Unavailable
        }
        _ => QueryError::Failed,
    }
}
