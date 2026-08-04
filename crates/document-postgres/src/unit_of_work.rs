use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use document::domain::{
    AggregateVersion, ContentRevision, DocumentMetadata, RehydrateDocumentMetadata,
};
use document::domain::{DocumentRepository, RepositoryError};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use sqlx::PgPool;

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
        let lock_key = format!(
            "{}:{}",
            command.document.tenant_id(),
            command.idempotency_key
        );

        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *transaction)
            .await
            .map_err(map_sqlx_error)?;

        let existing = sqlx::query_as::<_, ExistingCreateRow>(
            r"
            SELECT i.request_fingerprint, i.fingerprint_version,
                   d.id, d.tenant_id, d.original_filename, d.content_type,
                   d.object_key, d.status, d.version, d.content_revision, d.size_bytes, d.created_by,
                   d.created_at, d.updated_at
            FROM document_idempotency i
            JOIN documents d ON d.id = i.document_id
            WHERE i.tenant_id = $1 AND i.idempotency_key = $2
            ",
        )
        .bind(command.document.tenant_id())
        .bind(&command.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_sqlx_error)?;

        if let Some(existing) = existing {
            if existing.request_fingerprint != command.request_fingerprint
                || existing.fingerprint_version != command.fingerprint_version
            {
                return Err(ApplicationPortError::IdempotencyConflict);
            }
            let document = existing.into_document()?;
            transaction.commit().await.map_err(map_sqlx_error)?;
            return Ok(CreateDocumentResult {
                document,
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
struct RepositoryDocumentRow {
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    original_filename: String,
    content_type: String,
    object_key: String,
    status: String,
    version: i64,
    content_revision: i64,
    size_bytes: Option<i64>,
    created_by: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl RepositoryDocumentRow {
    fn into_document(self) -> Result<DocumentMetadata, RepositoryError> {
        DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: self.id,
            tenant_id: self.tenant_id,
            original_filename: self.original_filename,
            content_type: self.content_type,
            object_key: self.object_key,
            status: document::domain::DocumentStatus::try_from(self.status.as_str())
                .map_err(|_| RepositoryError::Failed)?,
            aggregate_version: AggregateVersion::new(self.version)
                .map_err(|_| RepositoryError::Failed)?,
            content_revision: ContentRevision::new(self.content_revision)
                .map_err(|_| RepositoryError::Failed)?,
            size_bytes: self.size_bytes,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
        .map_err(|_| RepositoryError::Failed)
    }
}

#[async_trait]
impl DocumentRepository for PostgresCreateDocumentUnitOfWork {
    async fn load(
        &self,
        tenant_id: uuid::Uuid,
        document_id: uuid::Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError> {
        sqlx::query_as::<_, RepositoryDocumentRow>(
            "SELECT id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, size_bytes, created_by, created_at, updated_at FROM documents WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RepositoryError::Unavailable)?
        .map(RepositoryDocumentRow::into_document)
        .transpose()
    }

    async fn save(
        &self,
        document: &DocumentMetadata,
        expected_version: AggregateVersion,
    ) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE documents SET original_filename = $1, content_type = $2, object_key = $3, status = $4, version = $5, content_revision = $6, size_bytes = $7, updated_at = $8 WHERE tenant_id = $9 AND id = $10 AND version = $11",
        )
        .bind(document.original_filename())
        .bind(document.content_type())
        .bind(document.object_key())
        .bind(document.status().as_str())
        .bind(document.aggregate_version().value())
        .bind(document.content_revision().value())
        .bind(document.size_bytes())
        .bind(document.updated_at())
        .bind(document.tenant_id())
        .bind(document.id())
        .bind(expected_version.value())
        .execute(&self.pool)
        .await
        .map_err(|_| RepositoryError::Unavailable)?;
        if result.rows_affected() != 1 {
            return Err(RepositoryError::Conflict);
        }
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ExistingCreateRow {
    request_fingerprint: String,
    fingerprint_version: i16,
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    original_filename: String,
    content_type: String,
    object_key: String,
    status: String,
    version: i64,
    content_revision: i64,
    size_bytes: Option<i64>,
    created_by: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl ExistingCreateRow {
    fn into_document(self) -> Result<DocumentMetadata, ApplicationPortError> {
        DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: self.id,
            tenant_id: self.tenant_id,
            original_filename: self.original_filename,
            content_type: self.content_type,
            object_key: self.object_key,
            status: document::domain::DocumentStatus::try_from(self.status.as_str())
                .map_err(|_| ApplicationPortError::Failed)?,
            aggregate_version: AggregateVersion::new(self.version)
                .map_err(|_| ApplicationPortError::Failed)?,
            content_revision: ContentRevision::new(self.content_revision)
                .map_err(|_| ApplicationPortError::Failed)?,
            size_bytes: self.size_bytes,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
        .map_err(|_| ApplicationPortError::Failed)
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
             status, version, content_revision, size_bytes, created_by, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ",
    )
    .bind(document.id())
    .bind(document.tenant_id())
    .bind(document.original_filename())
    .bind(document.content_type())
    .bind(document.object_key())
    .bind(document.status().as_str())
    .bind(document.version())
    .bind(document.content_revision().value())
    .bind(document.size_bytes())
    .bind(document.created_by())
    .bind(document.created_at())
    .bind(document.updated_at())
    .execute(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document: &DocumentMetadata,
) -> Result<(), ApplicationPortError> {
    let occurred_at = chrono::Utc::now();
    let action = AuditAction::new("document.created").map_err(|_| ApplicationPortError::Failed)?;
    let resource = AuditResource::new("document", document.id().to_string())
        .map_err(|_| ApplicationPortError::Failed)?;
    let event = AuditEvent::new(
        uuid::Uuid::now_v7(),
        document.tenant_id(),
        AuditActor {
            actor_type: AuditActorType::User,
            actor_id: document.created_by(),
        },
        action,
        resource,
        uuid::Uuid::now_v7(),
        None,
        None,
        None,
        None,
        AuditResult::Succeeded,
        None,
        None,
        None,
        vec!["content_type".to_string(), "original_filename".to_string()],
        serde_json::json!({
            "original_filename": document.original_filename(),
            "content_type": document.content_type(),
            "content_revision": document.content_revision().value(),
        }),
        "audit.v1",
        occurred_at,
    )
    .map_err(|_| ApplicationPortError::Failed)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(document.tenant_id().to_string())
        .execute(&mut **transaction)
        .await
        .map_err(map_sqlx_error)?;
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT record_hash FROM audit_events WHERE tenant_id=$1 AND chain_version=1 ORDER BY stream_sequence DESC LIMIT 1",
    )
    .bind(document.tenant_id())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?
    .flatten();
    let sequence = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(stream_sequence),0)+1 FROM audit_events WHERE tenant_id=$1",
    )
    .bind(document.tenant_id())
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?;
    let event = event.with_chain_metadata(sequence, chrono::Utc::now(), 1, previous);
    sqlx::query("INSERT INTO audit_events (id,tenant_id,user_id,action,resource_type,resource_id,details,created_at,occurred_at,recorded_at,stream_sequence,chain_version,operation_id,actor_type,actor_id,result,changed_fields,schema_version,previous_hash,record_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)")
        .bind(event.id)
        .bind(event.tenant_id)
        .bind(document.created_by())
        .bind(event.action.as_str())
        .bind(&event.resource.resource_type)
        .bind(&event.resource.resource_id)
        .bind(&event.details)
        .bind(event.occurred_at)
        .bind(event.recorded_at)
        .bind(event.stream_sequence)
        .bind(event.chain_version)
        .bind(event.operation_id)
        .bind("user")
        .bind(event.actor.actor_id)
        .bind("succeeded")
        .bind(serde_json::to_value(&event.changed_fields).map_err(|_| ApplicationPortError::Failed)?)
        .bind(&event.schema_version)
        .bind(&event.previous_hash)
        .bind(&event.record_hash)
        .execute(&mut **transaction)
        .await
        .map_err(map_sqlx_error)?;
    Ok(())
}

async fn insert_outbox(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document: &DocumentMetadata,
) -> Result<(), ApplicationPortError> {
    sqlx::query(
        r"INSERT INTO outbox_events
           (event_id, event_type, tenant_id, aggregate_id, aggregate_type,
            payload, schema_version, occurred_at, published)
           VALUES ($1, 'document.created', $2, $3, 'document', $4, 'v1', $5, FALSE)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(document.tenant_id().to_string())
    .bind(document.id().to_string())
    .bind(serde_json::json!({
        "document_id": document.id(),
        "original_filename": document.original_filename(),
        "content_type": document.content_type(),
        "object_key": document.object_key(),
        "created_by": document.created_by(),
    }))
    .bind(chrono::Utc::now())
    .execute(&mut **transaction)
    .await
    .map_err(map_sqlx_error)?;
    Ok(())
}

async fn insert_idempotency(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    command: &PersistNewDocument,
) -> Result<(), ApplicationPortError> {
    sqlx::query(
        r"
        INSERT INTO document_idempotency
            (tenant_id, idempotency_key, request_fingerprint, fingerprint_version, document_id)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(command.document.tenant_id())
    .bind(&command.idempotency_key)
    .bind(&command.request_fingerprint)
    .bind(command.fingerprint_version)
    .bind(command.document.id())
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
