use chrono::{DateTime, Utc};
use document::domain::{DocumentMetadata, DocumentStatus, RehydrateDocumentMetadata};
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
        DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
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
        .map_err(|_| QueryError::InvalidStoredData)
    }
}

impl TryFrom<DocumentRow> for DocumentDetailView {
    type Error = QueryError;

    fn try_from(row: DocumentRow) -> Result<Self, Self::Error> {
        validate_document_row(&row)?;
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
        validate_list_row(&row)?;
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

fn validate_document_row(row: &DocumentRow) -> Result<(), QueryError> {
    if row.id.is_empty()
        || row.tenant_id.is_empty()
        || row.created_by.is_empty()
        || row.version <= 0
        || row.size_bytes.is_some_and(|size| size < 0)
        || row.original_filename.trim().is_empty()
        || row.content_type.trim().is_empty()
    {
        return Err(QueryError::InvalidStoredData);
    }
    let created_at = timestamp(&row.created_at)?;
    let updated_at = timestamp(&row.updated_at)?;
    if updated_at < created_at {
        return Err(QueryError::InvalidStoredData);
    }
    Ok(())
}

fn validate_list_row(row: &ListRow) -> Result<(), QueryError> {
    if row.id.is_empty()
        || row.version <= 0
        || row.size_bytes.is_some_and(|size| size < 0)
        || row.original_filename.trim().is_empty()
        || row.content_type.trim().is_empty()
    {
        return Err(QueryError::InvalidStoredData);
    }
    let created_at = timestamp(&row.created_at)?;
    let updated_at = timestamp(&row.updated_at)?;
    if updated_at < created_at {
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

    fn document_row() -> DocumentRow {
        let now = Utc::now().to_rfc3339();
        DocumentRow {
            id: Uuid::now_v7().to_string(),
            tenant_id: Uuid::now_v7().to_string(),
            original_filename: "report.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            object_key: "uploads/report.pdf".to_string(),
            status: "active".to_string(),
            version: 1,
            size_bytes: Some(1),
            created_by: Uuid::now_v7().to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn query_mapper_fails_closed_for_invalid_stored_values() {
        let mut negative_size = document_row();
        negative_size.size_bytes = Some(-1);
        assert_eq!(
            DocumentDetailView::try_from(negative_size),
            Err(QueryError::InvalidStoredData)
        );

        let mut invalid_version = document_row();
        invalid_version.version = 0;
        assert_eq!(
            DocumentDetailView::try_from(invalid_version),
            Err(QueryError::InvalidStoredData)
        );

        let mut invalid_timestamp = document_row();
        invalid_timestamp.updated_at = (Utc::now() - chrono::Duration::seconds(1)).to_rfc3339();
        invalid_timestamp.created_at = Utc::now().to_rfc3339();
        assert_eq!(
            DocumentDetailView::try_from(invalid_timestamp),
            Err(QueryError::InvalidStoredData)
        );

        let mut unknown_status = document_row();
        unknown_status.status = "future".to_string();
        assert_eq!(
            DocumentDetailView::try_from(unknown_status),
            Err(QueryError::InvalidStoredData)
        );
    }
}
