use async_trait::async_trait;
use document::domain::{
    DocumentMetadata, DocumentPage, DocumentQueryRepository, ListDocumentsQuery, RepositoryError,
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::mapper::DocumentRow;

pub struct SqliteDocumentQueryRepository {
    pool: SqlitePool,
}

impl SqliteDocumentQueryRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentQueryRepository for SqliteDocumentQueryRepository {
    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError> {
        sqlx::query_as::<_, DocumentRow>(
            "SELECT id, tenant_id, original_filename, content_type, object_key, status, version, size_bytes, created_by, created_at, updated_at FROM documents WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(document_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_repository_error)?
        .map(TryInto::try_into)
        .transpose()
        .map_err(|_| RepositoryError::Failed)
    }

    async fn list(&self, query: ListDocumentsQuery) -> Result<DocumentPage, RepositoryError> {
        let rows = sqlx::query_as::<_, DocumentRow>(
            "SELECT id, tenant_id, original_filename, content_type, object_key, status, version, size_bytes, created_by, created_at, updated_at FROM documents WHERE tenant_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )
        .bind(query.tenant_id.to_string())
        .bind(query.limit)
        .bind(query.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_repository_error)?;
        let total =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE tenant_id = ?1")
                .bind(query.tenant_id.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(map_repository_error)?;
        Ok(DocumentPage {
            items: rows
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| RepositoryError::Failed)?,
            total,
        })
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_repository_error(error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            RepositoryError::Unavailable
        }
        sqlx::Error::Database(ref error)
            if error.is_unique_violation() || error.is_check_violation() =>
        {
            RepositoryError::Conflict
        }
        _ => RepositoryError::Failed,
    }
}
