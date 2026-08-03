use async_trait::async_trait;
use chrono::{DateTime, Utc};
use document::domain::{
    DocumentDomainError, DocumentMetadata, DocumentPage, DocumentQueryRepository, DocumentStatus,
    ListDocumentsQuery, RehydrateDocumentMetadata, RepositoryError,
};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub(crate) struct DocumentRow {
    pub(crate) id: Uuid,
    pub(crate) tenant_id: Uuid,
    pub(crate) original_filename: String,
    pub(crate) content_type: String,
    pub(crate) object_key: String,
    pub(crate) status: String,
    pub(crate) version: i64,
    pub(crate) size_bytes: Option<i64>,
    pub(crate) created_by: Uuid,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

impl TryFrom<DocumentRow> for DocumentMetadata {
    type Error = DocumentDomainError;

    fn try_from(row: DocumentRow) -> Result<Self, Self::Error> {
        DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: row.id,
            tenant_id: row.tenant_id,
            original_filename: row.original_filename,
            content_type: row.content_type,
            object_key: row.object_key,
            status: DocumentStatus::try_from(row.status.as_str())
                .map_err(|_| DocumentDomainError::InvalidStatus)?,
            version: row.version,
            size_bytes: row.size_bytes,
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

pub struct PostgresDocumentQueryRepository {
    pool: PgPool,
}

impl PostgresDocumentQueryRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentQueryRepository for PostgresDocumentQueryRepository {
    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError> {
        sqlx::query_as::<_, DocumentRow>(
            r"
            SELECT id, tenant_id, original_filename, content_type, object_key,
                   status, version, size_bytes, created_by, created_at, updated_at
            FROM documents
            WHERE tenant_id = $1 AND id = $2
            ",
        )
        .bind(tenant_id)
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)
        .and_then(|row| {
            row.map(DocumentMetadata::try_from)
                .transpose()
                .map_err(|_| RepositoryError::Failed)
        })
    }

    async fn list(&self, query: ListDocumentsQuery) -> Result<DocumentPage, RepositoryError> {
        let rows = sqlx::query_as::<_, DocumentRow>(
            r"
            SELECT id, tenant_id, original_filename, content_type, object_key,
                   status, version, size_bytes, created_by, created_at, updated_at
            FROM documents
            WHERE tenant_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(query.tenant_id)
        .bind(query.limit)
        .bind(query.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let total: i64 =
            sqlx::query("SELECT COUNT(*) AS count FROM documents WHERE tenant_id = $1")
                .bind(query.tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx_error)?
                .get("count");

        Ok(DocumentPage {
            items: rows
                .into_iter()
                .map(DocumentMetadata::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| RepositoryError::Failed)?,
            total,
        })
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            RepositoryError::Unavailable
        }
        sqlx::Error::Database(ref database_error)
            if database_error.is_unique_violation() || database_error.is_check_violation() =>
        {
            RepositoryError::Conflict
        }
        _ => RepositoryError::Failed,
    }
}
