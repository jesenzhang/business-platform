use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use document::domain::{
    AggregateVersion, ContentRevision, DocumentMetadata, DocumentStatus, RehydrateDocumentMetadata,
};
use document::domain::{DocumentRepository, RepositoryError};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

pub struct SqliteCreateDocumentUnitOfWork {
    pool: SqlitePool,
}

impl SqliteCreateDocumentUnitOfWork {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct ExistingCreateRow {
    request_fingerprint: String,
    fingerprint_version: i64,
    id: String,
    tenant_id: String,
    original_filename: String,
    content_type: String,
    object_key: String,
    status: String,
    version: i64,
    content_revision: i64,
    size_bytes: Option<i64>,
    created_by: String,
    created_at: String,
    updated_at: String,
}

#[async_trait]
impl CreateDocumentUnitOfWork for SqliteCreateDocumentUnitOfWork {
    async fn execute(
        &self,
        command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError> {
        // BEGIN IMMEDIATE obtains the database writer reservation before the
        // idempotency read. This protects retries across independent adapter
        // instances, not merely calls sharing one Rust object.
        let mut connection = self.pool.acquire().await.map_err(map_error)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(map_error)?;
        let result = execute_in_transaction(&mut connection, command).await;
        match result {
            Ok(result) => {
                sqlx::query("COMMIT")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_error)?;
                Ok(result)
            }
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                Err(error)
            }
        }
    }
}

#[derive(sqlx::FromRow)]
struct RepositoryDocumentRow {
    id: String,
    tenant_id: String,
    original_filename: String,
    content_type: String,
    object_key: String,
    status: String,
    version: i64,
    content_revision: i64,
    size_bytes: Option<i64>,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl RepositoryDocumentRow {
    fn into_document(self) -> Result<DocumentMetadata, RepositoryError> {
        DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: Uuid::parse_str(&self.id).map_err(|_| RepositoryError::Failed)?,
            tenant_id: Uuid::parse_str(&self.tenant_id).map_err(|_| RepositoryError::Failed)?,
            original_filename: self.original_filename,
            content_type: self.content_type,
            object_key: self.object_key,
            status: DocumentStatus::try_from(self.status.as_str())
                .map_err(|_| RepositoryError::Failed)?,
            aggregate_version: AggregateVersion::new(self.version)
                .map_err(|_| RepositoryError::Failed)?,
            content_revision: ContentRevision::new(self.content_revision)
                .map_err(|_| RepositoryError::Failed)?,
            size_bytes: self.size_bytes,
            created_by: Uuid::parse_str(&self.created_by).map_err(|_| RepositoryError::Failed)?,
            created_at: parse_timestamp(&self.created_at).map_err(|_| RepositoryError::Failed)?,
            updated_at: parse_timestamp(&self.updated_at).map_err(|_| RepositoryError::Failed)?,
        })
        .map_err(|_| RepositoryError::Failed)
    }
}

#[async_trait]
impl DocumentRepository for SqliteCreateDocumentUnitOfWork {
    async fn load(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError> {
        sqlx::query_as::<_, RepositoryDocumentRow>(
            "SELECT id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, size_bytes, created_by, created_at, updated_at FROM documents WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id.to_string())
        .bind(document_id.to_string())
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
            "UPDATE documents SET original_filename = ?1, content_type = ?2, object_key = ?3, status = ?4, version = ?5, content_revision = ?6, size_bytes = ?7, updated_at = ?8 WHERE tenant_id = ?9 AND id = ?10 AND version = ?11",
        )
        .bind(document.original_filename())
        .bind(document.content_type())
        .bind(document.object_key())
        .bind(document.status().as_str())
        .bind(document.aggregate_version().value())
        .bind(document.content_revision().value())
        .bind(document.size_bytes())
        .bind(document.updated_at().to_rfc3339())
        .bind(document.tenant_id().to_string())
        .bind(document.id().to_string())
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

async fn execute_in_transaction(
    connection: &mut SqliteConnection,
    command: PersistNewDocument,
) -> Result<CreateDocumentResult, ApplicationPortError> {
    let existing = sqlx::query_as::<_, ExistingCreateRow>(
            "SELECT i.request_fingerprint, i.fingerprint_version, d.id, d.tenant_id, d.original_filename, d.content_type, d.object_key, d.status, d.version, d.content_revision, d.size_bytes, d.created_by, d.created_at, d.updated_at FROM document_idempotency i JOIN documents d ON d.id = i.document_id WHERE i.tenant_id = ?1 AND i.idempotency_key = ?2",
        )
        .bind(command.document.tenant_id().to_string())
        .bind(&command.idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_error)?;
    if let Some(existing) = existing {
        if existing.request_fingerprint != command.request_fingerprint
            || existing.fingerprint_version != i64::from(command.fingerprint_version)
        {
            return Err(ApplicationPortError::IdempotencyConflict);
        }
        let document = DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: Uuid::parse_str(&existing.id).map_err(|_| ApplicationPortError::Failed)?,
            tenant_id: Uuid::parse_str(&existing.tenant_id)
                .map_err(|_| ApplicationPortError::Failed)?,
            original_filename: existing.original_filename,
            content_type: existing.content_type,
            object_key: existing.object_key,
            status: DocumentStatus::try_from(existing.status.as_str())
                .map_err(|_| ApplicationPortError::Failed)?,
            aggregate_version: AggregateVersion::new(existing.version)
                .map_err(|_| ApplicationPortError::Failed)?,
            content_revision: ContentRevision::new(existing.content_revision)
                .map_err(|_| ApplicationPortError::Failed)?,
            size_bytes: existing.size_bytes,
            created_by: Uuid::parse_str(&existing.created_by)
                .map_err(|_| ApplicationPortError::Failed)?,
            created_at: parse_timestamp(&existing.created_at)?,
            updated_at: parse_timestamp(&existing.updated_at)?,
        })
        .map_err(|_| ApplicationPortError::Failed)?;
        return Ok(CreateDocumentResult {
            document,
            replayed: true,
        });
    }

    insert_document(connection, &command).await?;
    insert_audit(connection, &command).await?;
    insert_outbox(connection, &command).await?;
    sqlx::query("INSERT INTO document_idempotency (tenant_id, idempotency_key, request_fingerprint, fingerprint_version, document_id) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(command.document.tenant_id().to_string()).bind(&command.idempotency_key)
            .bind(&command.request_fingerprint).bind(command.fingerprint_version)
            .bind(command.document.id().to_string()).execute(&mut *connection).await.map_err(map_error)?;
    Ok(CreateDocumentResult {
        document: command.document,
        replayed: false,
    })
}

async fn insert_document(
    tx: &mut SqliteConnection,
    command: &PersistNewDocument,
) -> Result<(), ApplicationPortError> {
    let document = &command.document;
    sqlx::query("INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, size_bytes, created_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)")
        .bind(document.id().to_string()).bind(document.tenant_id().to_string())
        .bind(document.original_filename()).bind(document.content_type()).bind(document.object_key())
        .bind(document.status().as_str()).bind(document.version())
        .bind(document.content_revision().value()).bind(document.size_bytes())
        .bind(document.created_by().to_string()).bind(document.created_at().to_rfc3339())
        .bind(document.updated_at().to_rfc3339()).execute(&mut *tx).await.map_err(map_error)?;
    Ok(())
}

async fn insert_audit(
    tx: &mut SqliteConnection,
    command: &PersistNewDocument,
) -> Result<(), ApplicationPortError> {
    let document = &command.document;
    let occurred_at = Utc::now();
    let action = AuditAction::new("document.created").map_err(|_| ApplicationPortError::Failed)?;
    let resource = AuditResource::new("document", document.id().to_string())
        .map_err(|_| ApplicationPortError::Failed)?;
    let event = AuditEvent::new(
        Uuid::now_v7(),
        document.tenant_id(),
        AuditActor {
            actor_type: AuditActorType::User,
            actor_id: document.created_by(),
        },
        action,
        resource,
        Uuid::now_v7(),
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
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT record_hash FROM audit_events WHERE tenant_id=?1 ORDER BY occurred_at DESC,id DESC LIMIT 1",
    )
    .bind(document.tenant_id().to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_error)?
    .flatten();
    let event = event.with_chain(previous);
    sqlx::query("INSERT INTO audit_events (id,tenant_id,user_id,action,resource_type,resource_id,details,created_at,occurred_at,operation_id,actor_type,actor_id,result,changed_fields,schema_version,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?10,?11,?12,?13,?14,?15,?16)")
        .bind(event.id.to_string())
        .bind(event.tenant_id.to_string())
        .bind(document.created_by().to_string())
        .bind(event.action.as_str())
        .bind(&event.resource.resource_type)
        .bind(&event.resource.resource_id)
        .bind(event.details.to_string())
        .bind(event.occurred_at.to_rfc3339())
        .bind(event.operation_id.to_string())
        .bind("user")
        .bind(event.actor.actor_id.to_string())
        .bind("succeeded")
        .bind(serde_json::to_string(&event.changed_fields).map_err(|_| ApplicationPortError::Failed)?)
        .bind(&event.schema_version)
        .bind(&event.previous_hash)
        .bind(&event.record_hash)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
    Ok(())
}

async fn insert_outbox(
    tx: &mut SqliteConnection,
    command: &PersistNewDocument,
) -> Result<(), ApplicationPortError> {
    let document = &command.document;
    let payload = serde_json::json!({"document_id": document.id(), "original_filename": document.original_filename()});
    sqlx::query("INSERT INTO outbox_events (event_id, event_type, tenant_id, aggregate_id, aggregate_type, payload, schema_version, occurred_at) VALUES (?1, 'document.created', ?2, ?3, 'document', ?4, 'v1', ?5)")
        .bind(Uuid::now_v7().to_string()).bind(document.tenant_id().to_string())
        .bind(document.id().to_string()).bind(payload.to_string()).bind(Utc::now().to_rfc3339())
        .execute(&mut *tx).await.map_err(map_error)?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn map_error(error: sqlx::Error) -> ApplicationPortError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            ApplicationPortError::Unavailable
        }
        _ => ApplicationPortError::Failed,
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, ApplicationPortError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| ApplicationPortError::Failed)
}
