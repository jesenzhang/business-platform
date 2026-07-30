//! `PostgreSQL` implementation of the document repository port.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::domain::{DocumentMetadata, DocumentRepository, DocumentStatus};

/// A database row mapping for the `documents` table.
#[derive(Debug, FromRow)]
struct DocumentRow {
    id: Uuid,
    tenant_id: Uuid,
    original_filename: String,
    content_type: String,
    object_key: String,
    status: String,
    version: i64,
    size_bytes: Option<i64>,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DocumentRow> for DocumentMetadata {
    fn from(row: DocumentRow) -> Self {
        Self {
            id: row.id,
            tenant_id: row.tenant_id,
            original_filename: row.original_filename,
            content_type: row.content_type,
            object_key: row.object_key,
            status: DocumentStatus::from_db_str(&row.status),
            version: row.version,
            size_bytes: row.size_bytes,
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// PostgreSQL-backed document repository.
pub struct PostgresDocumentRepository {
    pool: PgPool,
}

impl PostgresDocumentRepository {
    /// Create a new repository instance.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentRepository for PostgresDocumentRepository {
    async fn save(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        doc: &DocumentMetadata,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO documents
                (id, tenant_id, original_filename, content_type, object_key,
                 status, version, size_bytes, created_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ",
        )
        .bind(doc.id)
        .bind(doc.tenant_id)
        .bind(&doc.original_filename)
        .bind(&doc.content_type)
        .bind(&doc.object_key)
        .bind(doc.status.as_str())
        .bind(doc.version)
        .bind(doc.size_bytes)
        .bind(doc.created_by)
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DocumentMetadata>, sqlx::Error> {
        let row = sqlx::query_as::<_, DocumentRow>(
            r"
            SELECT id, tenant_id, original_filename, content_type, object_key,
                   status, version, size_bytes, created_by, created_at, updated_at
            FROM documents
            WHERE tenant_id = $1 AND id = $2
            ",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(DocumentMetadata::from))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DocumentMetadata>, i64), sqlx::Error> {
        let rows = sqlx::query_as::<_, DocumentRow>(
            r"
            SELECT id, tenant_id, original_filename, content_type, object_key,
                   status, version, size_bytes, created_by, created_at, updated_at
            FROM documents
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 = sqlx::query(
            r"
            SELECT COUNT(*) as count FROM documents WHERE tenant_id = $1
            ",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?
        .get("count");

        let items = rows.into_iter().map(DocumentMetadata::from).collect();

        Ok((items, total))
    }
}
