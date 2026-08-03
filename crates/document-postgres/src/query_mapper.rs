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
        validate_detail(&row)?;
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
        validate_list(&row)?;
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

fn validate_detail(row: &DetailRow) -> Result<(), QueryError> {
    if row.id.is_nil()
        || row.tenant_id.is_nil()
        || row.created_by.is_nil()
        || row.version <= 0
        || row.size_bytes.is_some_and(|size| size < 0)
        || row.original_filename.trim().is_empty()
        || row.content_type.trim().is_empty()
        || row.updated_at < row.created_at
    {
        return Err(QueryError::InvalidStoredData);
    }
    Ok(())
}

fn validate_list(row: &ListRow) -> Result<(), QueryError> {
    if row.id.is_nil()
        || row.version <= 0
        || row.size_bytes.is_some_and(|size| size < 0)
        || row.original_filename.trim().is_empty()
        || row.content_type.trim().is_empty()
        || row.updated_at < row.created_at
    {
        return Err(QueryError::InvalidStoredData);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn detail_row() -> DetailRow {
        let now = Utc::now();
        DetailRow {
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            original_filename: "report.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            status: "active".to_string(),
            version: 1,
            size_bytes: Some(1),
            created_by: Uuid::now_v7(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn query_mapper_fails_closed_for_invalid_stored_values() {
        let mut negative_size = detail_row();
        negative_size.size_bytes = Some(-1);
        assert_eq!(
            DocumentDetailView::try_from(negative_size),
            Err(QueryError::InvalidStoredData)
        );

        let mut invalid_version = detail_row();
        invalid_version.version = 0;
        assert_eq!(
            DocumentDetailView::try_from(invalid_version),
            Err(QueryError::InvalidStoredData)
        );

        let mut invalid_timestamp = detail_row();
        invalid_timestamp.updated_at = invalid_timestamp.created_at - chrono::Duration::seconds(1);
        assert_eq!(
            DocumentDetailView::try_from(invalid_timestamp),
            Err(QueryError::InvalidStoredData)
        );

        let mut unknown_status = detail_row();
        unknown_status.status = "future".to_string();
        assert_eq!(
            DocumentDetailView::try_from(unknown_status),
            Err(QueryError::InvalidStoredData)
        );
    }
}
