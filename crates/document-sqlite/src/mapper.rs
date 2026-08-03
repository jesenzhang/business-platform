use chrono::{DateTime, Utc};
use document::domain::{DocumentMetadata, DocumentStatus};
use document::query::{DocumentDetailView, DocumentListItem, DocumentStatusView, QueryError};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub(crate) struct DocumentRow {
    pub id: String,
    pub tenant_id: String,
    pub original_filename: String,
    pub content_type: String,
    pub object_key: String,
    pub status: String,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

fn uuid(value: &str) -> Result<Uuid, QueryError> {
    Uuid::parse_str(value).map_err(|_| QueryError::InvalidStoredData)
}

fn timestamp(value: &str) -> Result<DateTime<Utc>, QueryError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| QueryError::InvalidStoredData)
}

impl TryFrom<DocumentRow> for DocumentMetadata {
    type Error = QueryError;

    fn try_from(row: DocumentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: uuid(&row.id)?,
            tenant_id: uuid(&row.tenant_id)?,
            original_filename: row.original_filename,
            content_type: row.content_type,
            object_key: row.object_key,
            status: DocumentStatus::try_from(row.status.as_str())
                .map_err(|_| QueryError::InvalidStoredData)?,
            version: row.version,
            size_bytes: row.size_bytes,
            created_by: uuid(&row.created_by)?,
            created_at: timestamp(&row.created_at)?,
            updated_at: timestamp(&row.updated_at)?,
        })
    }
}

impl TryFrom<DocumentRow> for DocumentDetailView {
    type Error = QueryError;

    fn try_from(row: DocumentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: uuid(&row.id)?,
            tenant_id: uuid(&row.tenant_id)?,
            original_filename: row.original_filename,
            content_type: row.content_type,
            status: DocumentStatusView::parse(&row.status)?,
            version: row.version,
            size_bytes: row.size_bytes,
            created_by: uuid(&row.created_by)?,
            created_at: timestamp(&row.created_at)?,
            updated_at: timestamp(&row.updated_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct ListRow {
    pub id: String,
    pub original_filename: String,
    pub content_type: String,
    pub status: String,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

impl TryFrom<ListRow> for DocumentListItem {
    type Error = QueryError;

    fn try_from(row: ListRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: uuid(&row.id)?,
            original_filename: row.original_filename,
            content_type: row.content_type,
            status: DocumentStatusView::parse(&row.status)?,
            version: row.version,
            size_bytes: row.size_bytes,
            created_at: timestamp(&row.created_at)?,
            updated_at: timestamp(&row.updated_at)?,
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
