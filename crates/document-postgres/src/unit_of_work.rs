use async_trait::async_trait;
use document::domain::DocumentMetadata;
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use messaging::{DomainEvent, ReliableOutbox};
use sqlx::PgPool;

use crate::repository::DocumentRow;

pub struct PostgresCreateDocumentUnitOfWork {
    pool: PgPool,
}

impl PostgresCreateDocumentUnitOfWork {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CreateDocumentUnitOfWork for PostgresCreateDocumentUnitOfWork {
    async fn execute(
        &self,
        command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError> {
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        let lock_key = format!("{}:{}", command.document.tenant_id, command.idempotency_key);

        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *transaction)
            .await
            .map_err(map_sqlx_error)?;

        let existing = sqlx::query_as::<_, ExistingCreateRow>(
            r"
            SELECT i.request_fingerprint,
                   d.id, d.tenant_id, d.original_filename, d.content_type,
                   d.object_key, d.status, d.version, d.size_bytes, d.created_by,
                   d.created_at, d.updated_at
            FROM document_idempotency i
            JOIN documents d ON d.id = i.document_id
            WHERE i.tenant_id = $1 AND i.idempotency_key = $2
            ",
        )
        .bind(command.document.tenant_id)
        .bind(&command.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;

        if let Some(existing) = existing {
            if existing.request_fingerprint != command.request_fingerprint {
                return Err(ApplicationPortError::IdempotencyConflict);
            }
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(CreateDocumentResult {
                document: existing.into_document(),
                replayed: true,
            });
        }

        insert_document(&mut transaction, &command.document).await?;
        insert_audit(&mut transaction, &command.document).await?;
        insert_outbox(&mut transaction, &command.document).await?;
        insert_idempotency(&mut transaction, &command).await?;
        transaction.commit().await.map_err(map_sqlx_error)?;

        Ok(CreateDocumentResult {
            document: command.document,
            replayed: false,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ExistingCreateRow {
    request_fingerprint: String,
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    original_filename: String,
    content_type: String,
    object_key: String,
    status: String,
    version: i64,
    size_bytes: Option<i64>,
    created_by: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl ExistingCreateRow {
    fn into_document(self) -> DocumentMetadata {
        DocumentRow {
            id: self.id,
            tenant_id: self.tenant_id,
            original_filename: self.original_filename,
            content_type: self.content_type,
            object_key: self.object_key,
            status: self.status,
            version: self.version,
            size_bytes: self.size_bytes,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
        .into()
    }
}

async fn insert_document(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document: &DocumentMetadata,
) -> Result<(), ApplicationPortError> {
    sqlx::query(
        r"
        INSERT INTO documents
            (id, tenant_id, original_filename, content_type, object_key,
             status, version, size_bytes, created_by, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ",
    )
    .bind(document.id)
    .bind(document.tenant_id)
    .bind(&document.original_filename)
    .bind(&document.content_type)
    .bind(&document.object_key)
    .bind(document.status.as_str())
    .bind(document.version)
    .bind(document.size_bytes)
    .bind(document.created_by)
    .bind(document.created_at)
    .bind(document.updated_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document: &DocumentMetadata,
) -> Result<(), ApplicationPortError> {
    sqlx::query(
        r"
        INSERT INTO audit_events
            (tenant_id, user_id, action, resource_type, resource_id, details)
        VALUES ($1, $2, 'document.created', 'document', $3, $4)
        ",
    )
    .bind(document.tenant_id)
    .bind(document.created_by)
    .bind(document.id.to_string())
    .bind(serde_json::json!({
        "original_filename": document.original_filename,
        "content_type": document.content_type,
        "object_key": document.object_key,
    }))
    .execute(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

async fn insert_outbox(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document: &DocumentMetadata,
) -> Result<(), ApplicationPortError> {
    let event = DomainEvent::new(
        "document.created",
        document.tenant_id.to_string(),
        document.id.to_string(),
        "document",
        serde_json::json!({
            "document_id": document.id,
            "original_filename": document.original_filename,
            "content_type": document.content_type,
            "object_key": document.object_key,
            "created_by": document.created_by,
        }),
    );
    ReliableOutbox::append_in_tx(transaction, &event)
        .await
        .map_err(map_sqlx_error)
}

async fn insert_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    command: &PersistNewDocument,
) -> Result<(), ApplicationPortError> {
    sqlx::query(
        r"
        INSERT INTO document_idempotency
            (tenant_id, idempotency_key, request_fingerprint, document_id)
        VALUES ($1, $2, $3, $4)
        ",
    )
    .bind(command.document.tenant_id)
    .bind(&command.idempotency_key)
    .bind(&command.request_fingerprint)
    .bind(command.document.id)
    .execute(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn map_sqlx_error(error: sqlx::Error) -> ApplicationPortError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            ApplicationPortError::Unavailable
        }
        _ => ApplicationPortError::Failed,
    }
}
