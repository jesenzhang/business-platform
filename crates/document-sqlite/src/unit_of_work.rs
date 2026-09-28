use async_trait::async_trait;
use audit::{AuditAction, AuditActor, AuditActorType, AuditEvent, AuditResource, AuditResult};
use chrono::{DateTime, Utc};
use document::domain::{
    AggregateVersion, ContentRevision, DocumentMetadata, DocumentRevision, DocumentStatus,
    RehydrateDocumentMetadata,
};
use document::domain::{DocumentRepository, RepositoryError};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

pub struct SqliteCreateDocumentUnitOfWork {
    pool: SqlitePool,
    deterministic_event_time: Option<DateTime<Utc>>,
}

impl SqliteCreateDocumentUnitOfWork {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            deterministic_event_time: None,
        }
    }

    /// Build a create adapter whose audit/outbox timestamps and identities are
    /// derived from the caller's deterministic document identity.
    #[must_use]
    pub fn new_for_rehearsal(pool: SqlitePool, event_time: DateTime<Utc>) -> Self {
        Self {
            pool,
            deterministic_event_time: Some(event_time),
        }
    }

    /// Verify every deterministic audit/outbox value emitted by a rehearsal
    /// create. This is intentionally adapter-owned because those records are
    /// persistence facts, not Document domain state.
    #[allow(clippy::cmp_owned, clippy::too_many_lines)]
    pub async fn verify_rehearsal_events(
        &self,
        document: &DocumentMetadata,
    ) -> Result<(), ApplicationPortError> {
        let event_time = self
            .deterministic_event_time
            .ok_or(ApplicationPortError::Failed)?;
        let audit = sqlx::query_as::<_, RehearsalAuditRow>(
            "SELECT tenant_id, user_id, action, resource_type, resource_id,
                    details, created_at, occurred_at, operation_id, actor_type,
                    actor_id, result, changed_fields, schema_version,
                    previous_hash, record_hash, chain_version, stream_sequence,
                    recorded_at
             FROM audit_events WHERE id = ?1",
        )
        .bind(document.id().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(ApplicationPortError::Failed)?;
        let action =
            AuditAction::new("document.created").map_err(|_| ApplicationPortError::Failed)?;
        let resource = AuditResource::new("document", document.id().to_string())
            .map_err(|_| ApplicationPortError::Failed)?;
        let event = AuditEvent::new(
            document.id(),
            document.tenant_id(),
            AuditActor {
                actor_type: AuditActorType::User,
                actor_id: document.created_by(),
            },
            action,
            resource,
            document.current_revision_id(),
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
            event_time,
        )
        .map_err(|_| ApplicationPortError::Failed)?;
        // Rows written by the current writer are protected chain members and
        // must be recomputed against their stored chain metadata, the same
        // way `verify_chain` does. Rows written before the unified chain
        // columns existed keep the legacy `with_chain` hash shape.
        let expected = match (
            audit.chain_version,
            audit.stream_sequence,
            audit.recorded_at.as_deref(),
        ) {
            (Some(1), Some(stream_sequence), Some(recorded_at)) => {
                let recorded_at = chrono::DateTime::parse_from_rfc3339(recorded_at)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|_| ApplicationPortError::Failed)?;
                event.with_chain_metadata(
                    stream_sequence,
                    recorded_at,
                    1,
                    audit.previous_hash.clone(),
                )
            }
            _ => event.with_chain(audit.previous_hash.clone()),
        }
        .map_err(|_| ApplicationPortError::Failed)?;
        if audit.tenant_id != expected.tenant_id().to_string()
            || audit.user_id != document.created_by().to_string()
            || audit.action != expected.action().as_str()
            || audit.resource_type != expected.resource().resource_type
            || audit.resource_id != expected.resource().resource_id
            || audit.details != expected.details().to_string()
            || audit.created_at != expected.occurred_at().to_rfc3339()
            || audit.occurred_at != expected.occurred_at().to_rfc3339()
            || audit.operation_id != expected.operation_id().to_string()
            || audit.actor_type != "user"
            || audit.actor_id != expected.actor().actor_id.to_string()
            || audit.result != "succeeded"
            || audit.changed_fields
                != serde_json::to_string(expected.changed_fields())
                    .map_err(|_| ApplicationPortError::Failed)?
            || audit.schema_version != expected.schema_version()
            || audit.previous_hash != expected.previous_hash().map(ToOwned::to_owned)
            || audit.record_hash.as_deref() != expected.record_hash()
        {
            return Err(ApplicationPortError::Failed);
        }
        if let Some(previous_hash) = audit.previous_hash.as_deref() {
            // The chain head after genesis links to the tenant's genesis
            // anchor, which is not a stored row; every other link must
            // resolve to a persisted predecessor in the same tenant.
            if previous_hash != audit::audit_chain_genesis(document.tenant_id()) {
                let previous_exists = sqlx::query_scalar::<_, i64>(
                    "SELECT 1 FROM audit_events WHERE tenant_id = ?1 AND record_hash = ?2 LIMIT 1",
                )
                .bind(document.tenant_id().to_string())
                .bind(previous_hash)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_error)?
                .is_some();
                if !previous_exists {
                    return Err(ApplicationPortError::Failed);
                }
            }
        }

        let outbox = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
            "SELECT event_type, tenant_id, aggregate_id, aggregate_type,
                    payload, schema_version, occurred_at
             FROM outbox_events WHERE event_id = ?1",
        )
        .bind(document.id().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(ApplicationPortError::Failed)?;
        let payload = serde_json::json!({
            "document_id": document.id(),
            "original_filename": document.original_filename(),
        })
        .to_string();
        if outbox.0 != "document.created"
            || outbox.1 != document.tenant_id().to_string()
            || outbox.2 != document.id().to_string()
            || outbox.3 != "document"
            || outbox.4 != payload
            || outbox.5 != "v1"
            || outbox.6 != event_time.to_rfc3339()
        {
            return Err(ApplicationPortError::Failed);
        }
        Ok(())
    }
}

/// Audit row shape read back by `verify_rehearsal_events`; the chain columns
/// are optional because rows written before the unified chain existed (and
/// the rehearsal fixtures that emulate them) carry NULL there.
#[derive(sqlx::FromRow)]
struct RehearsalAuditRow {
    tenant_id: String,
    user_id: String,
    action: String,
    resource_type: String,
    resource_id: String,
    details: String,
    created_at: String,
    occurred_at: String,
    operation_id: String,
    actor_type: String,
    actor_id: String,
    result: String,
    changed_fields: String,
    schema_version: String,
    previous_hash: Option<String>,
    record_hash: Option<String>,
    chain_version: Option<i16>,
    stream_sequence: Option<i64>,
    recorded_at: Option<String>,
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
    current_revision_id: Option<String>,
    deletion_state: String,
    pre_trash_lifecycle: String,
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
        let result =
            execute_in_transaction(&mut connection, command, self.deterministic_event_time).await;
        match result {
            Ok(result) => {
                sqlx::query("COMMIT")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_error)?;
                Ok(result)
            }
            Err(error) => {
                rollback(&mut connection).await?;
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
    current_revision_id: Option<String>,
    deletion_state: String,
    pre_trash_lifecycle: String,
    size_bytes: Option<i64>,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl RepositoryDocumentRow {
    fn into_document(self) -> Result<DocumentMetadata, RepositoryError> {
        let status =
            DocumentStatus::try_from(self.status.as_str()).map_err(|_| RepositoryError::Failed)?;
        let lifecycle_state =
            document::domain::DocumentLifecycleState::try_from(self.pre_trash_lifecycle.as_str())
                .unwrap_or_else(|_| DocumentMetadata::legacy_states(status).0);
        let deletion_state =
            document::domain::DocumentDeletionState::try_from(self.deletion_state.as_str())
                .unwrap_or_else(|_| DocumentMetadata::legacy_states(status).1);
        DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: Uuid::parse_str(&self.id).map_err(|_| RepositoryError::Failed)?,
            tenant_id: Uuid::parse_str(&self.tenant_id).map_err(|_| RepositoryError::Failed)?,
            original_filename: self.original_filename,
            content_type: self.content_type,
            object_key: self.object_key,
            status,
            aggregate_version: AggregateVersion::new(self.version)
                .map_err(|_| RepositoryError::Failed)?,
            content_revision: ContentRevision::new(self.content_revision)
                .map_err(|_| RepositoryError::Failed)?,
            current_revision_id: self
                .current_revision_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| RepositoryError::Failed)?
                .ok_or(RepositoryError::Failed)?,
            lifecycle_state,
            deletion_state,
            pre_trash_lifecycle: lifecycle_state,
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
            "SELECT id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, current_revision_id, deletion_state, pre_trash_lifecycle, size_bytes, created_by, created_at, updated_at FROM documents WHERE tenant_id = ?1 AND id = ?2",
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
        new_revision: Option<&DocumentRevision>,
        expected_version: AggregateVersion,
    ) -> Result<(), RepositoryError> {
        if let Some(revision) = new_revision {
            validate_new_revision(document, revision)?;
        }
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|_| RepositoryError::Unavailable)?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepositoryError::Unavailable)?;
        let persisted_current_revision_id = if new_revision.is_some() {
            None
        } else {
            Some(document.current_revision_id().to_string())
        };
        let result = sqlx::query(
            "UPDATE documents SET original_filename = ?1, content_type = ?2, object_key = ?3, status = ?4, version = ?5, content_revision = ?6, current_revision_id = ?7, deletion_state = ?8, pre_trash_lifecycle = ?9, size_bytes = ?10, updated_at = ?11 WHERE tenant_id = ?12 AND id = ?13 AND version = ?14",
        )
        .bind(document.original_filename())
        .bind(document.content_type())
        .bind(document.object_key())
        .bind(document.status().as_str())
        .bind(document.aggregate_version().value())
        .bind(document.content_revision().value())
        .bind(persisted_current_revision_id)
        .bind(document.deletion_state().as_str())
        .bind(document.lifecycle_state().as_str())
        .bind(document.size_bytes())
        .bind(document.updated_at().to_rfc3339())
        .bind(document.tenant_id().to_string())
        .bind(document.id().to_string())
        .bind(expected_version.value())
        .execute(&mut *connection)
        .await;
        let Ok(result) = result else {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(RepositoryError::Unavailable);
        };
        if result.rows_affected() != 1 {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(RepositoryError::Conflict);
        }
        if let Some(revision) = new_revision {
            if let Err(error) = insert_revision(&mut connection, revision).await {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                return Err(map_revision_repository_error(error));
            }
            if let Err(error) = set_current_revision(&mut connection, document).await {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                return Err(map_revision_repository_error(error));
            }
        }
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(|_| RepositoryError::Unavailable)?;
        Ok(())
    }
}

async fn execute_in_transaction(
    connection: &mut SqliteConnection,
    command: PersistNewDocument,
    deterministic_event_time: Option<DateTime<Utc>>,
) -> Result<CreateDocumentResult, ApplicationPortError> {
    let existing = sqlx::query_as::<_, ExistingCreateRow>(
            "SELECT i.request_fingerprint, i.fingerprint_version, d.id, d.tenant_id, d.original_filename, d.content_type, d.object_key, d.status, d.version, d.content_revision, d.current_revision_id, d.deletion_state, d.pre_trash_lifecycle, d.size_bytes, d.created_by, d.created_at, d.updated_at FROM document_idempotency i JOIN documents d ON d.id = i.document_id AND d.tenant_id = i.tenant_id WHERE i.tenant_id = ?1 AND i.idempotency_key = ?2",
        )
        .bind(command.document.tenant_id().to_string())
        .bind(&command.idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|error| {
            // Bounded signal only: the raw driver error stays out of the log
            // stream (AGENTS §10) and the classified error is what callers act
            // on. Gap OBS-W4-04 tracks richer adapter diagnostics.
            tracing::warn!("document idempotency read failed");
            map_error(error)
        })?;
    if let Some(existing) = existing {
        if existing.request_fingerprint != command.request_fingerprint
            || existing.fingerprint_version != i64::from(command.fingerprint_version)
        {
            return Err(ApplicationPortError::IdempotencyConflict);
        }
        let status = DocumentStatus::try_from(existing.status.as_str())
            .map_err(|_| ApplicationPortError::Failed)?;
        let lifecycle_state = document::domain::DocumentLifecycleState::try_from(
            existing.pre_trash_lifecycle.as_str(),
        )
        .unwrap_or_else(|_| DocumentMetadata::legacy_states(status).0);
        let deletion_state =
            document::domain::DocumentDeletionState::try_from(existing.deletion_state.as_str())
                .unwrap_or_else(|_| DocumentMetadata::legacy_states(status).1);
        let document = DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
            id: Uuid::parse_str(&existing.id).map_err(|_| ApplicationPortError::Failed)?,
            tenant_id: Uuid::parse_str(&existing.tenant_id)
                .map_err(|_| ApplicationPortError::Failed)?,
            original_filename: existing.original_filename,
            content_type: existing.content_type,
            object_key: existing.object_key,
            status,
            aggregate_version: AggregateVersion::new(existing.version)
                .map_err(|_| ApplicationPortError::Failed)?,
            content_revision: ContentRevision::new(existing.content_revision)
                .map_err(|_| ApplicationPortError::Failed)?,
            current_revision_id: existing
                .current_revision_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| ApplicationPortError::Failed)?
                .ok_or(ApplicationPortError::Failed)?,
            lifecycle_state,
            deletion_state,
            pre_trash_lifecycle: lifecycle_state,
            size_bytes: existing.size_bytes,
            created_by: Uuid::parse_str(&existing.created_by)
                .map_err(|_| ApplicationPortError::Failed)?,
            created_at: parse_timestamp(&existing.created_at)?,
            updated_at: parse_timestamp(&existing.updated_at)?,
        })
        .map_err(|_| ApplicationPortError::Failed)?;
        // Defense in depth for gap D-W4-01: `document_idempotency.document_id`
        // is only a foreign key onto `documents(id)`, so a mis-bound durable
        // row must never be replayed across the tenant boundary.
        if document.tenant_id() != command.document.tenant_id() {
            return Err(ApplicationPortError::Failed);
        }
        return Ok(CreateDocumentResult {
            document,
            replayed: true,
        });
    }

    insert_document(connection, &command).await?;
    let initial_revision = command
        .document
        .initial_revision_with_sha256(command.initial_revision_sha256.as_deref())
        .map_err(|_| ApplicationPortError::Failed)?;
    insert_revision(connection, &initial_revision).await?;
    set_current_revision(connection, &command.document).await?;
    insert_audit(connection, &command, deterministic_event_time).await?;
    insert_outbox(connection, &command, deterministic_event_time).await?;
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
    sqlx::query("INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, content_revision, current_revision_id, deletion_state, pre_trash_lifecycle, size_bytes, created_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)")
        .bind(document.id().to_string()).bind(document.tenant_id().to_string())
        .bind(document.original_filename()).bind(document.content_type()).bind(document.object_key())
        .bind(document.status().as_str()).bind(document.version())
        .bind(document.content_revision().value()).bind(Option::<String>::None)
        .bind(document.deletion_state().as_str()).bind(document.lifecycle_state().as_str())
        .bind(document.size_bytes())
        .bind(document.created_by().to_string()).bind(document.created_at().to_rfc3339())
        .bind(document.updated_at().to_rfc3339()).execute(&mut *tx).await.map_err(map_error)?;
    Ok(())
}

async fn insert_revision(
    tx: &mut SqliteConnection,
    revision: &DocumentRevision,
) -> Result<(), ApplicationPortError> {
    sqlx::query("INSERT INTO document_revisions (id, tenant_id, document_id, revision_no, parent_revision_id, source_object_ref, sha256, content_type, size_bytes, original_filename, created_by, created_at, change_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)")
        .bind(revision.id().to_string())
        .bind(revision.tenant_id().to_string())
        .bind(revision.document_id().to_string())
        .bind(revision.revision_no())
        .bind(revision.parent_revision_id().map(|id| id.to_string()))
        .bind(revision.source_object_ref())
        .bind(revision.sha256())
        .bind(revision.content_type())
        .bind(revision.size_bytes())
        .bind(revision.original_filename())
        .bind(revision.created_by().to_string())
        .bind(revision.created_at().to_rfc3339())
        .bind(revision.change_reason())
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
    Ok(())
}

async fn set_current_revision(
    tx: &mut SqliteConnection,
    document: &DocumentMetadata,
) -> Result<(), ApplicationPortError> {
    let result = sqlx::query(
        "UPDATE documents SET current_revision_id = ?1 WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind(document.current_revision_id().to_string())
    .bind(document.tenant_id().to_string())
    .bind(document.id().to_string())
    .execute(&mut *tx)
    .await
    .map_err(map_error)?;
    if result.rows_affected() != 1 {
        return Err(ApplicationPortError::Failed);
    }
    Ok(())
}

fn validate_new_revision(
    document: &DocumentMetadata,
    revision: &DocumentRevision,
) -> Result<(), RepositoryError> {
    if revision.tenant_id() != document.tenant_id()
        || revision.document_id() != document.id()
        || revision.id() != document.current_revision_id()
        || revision.revision_no() != document.content_revision().value()
        || revision.revision_no() <= 1
    {
        return Err(RepositoryError::Failed);
    }
    Ok(())
}

fn map_revision_repository_error(error: ApplicationPortError) -> RepositoryError {
    match error {
        ApplicationPortError::Unavailable => RepositoryError::Unavailable,
        ApplicationPortError::IdempotencyConflict | ApplicationPortError::Failed => {
            RepositoryError::Failed
        }
    }
}

async fn insert_audit(
    tx: &mut SqliteConnection,
    command: &PersistNewDocument,
    deterministic_event_time: Option<DateTime<Utc>>,
) -> Result<(), ApplicationPortError> {
    let document = &command.document;
    let (event_id, operation_id, occurred_at) = match deterministic_event_time {
        Some(event_time) => (document.id(), document.current_revision_id(), event_time),
        None => (Uuid::now_v7(), Uuid::now_v7(), Utc::now()),
    };
    let action = AuditAction::new("document.created").map_err(|_| ApplicationPortError::Failed)?;
    let resource = AuditResource::new("document", document.id().to_string())
        .map_err(|_| ApplicationPortError::Failed)?;
    let event = AuditEvent::new(
        event_id,
        document.tenant_id(),
        AuditActor {
            actor_type: AuditActorType::User,
            actor_id: document.created_by(),
        },
        action,
        resource,
        operation_id,
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
    // ADR-0013 Revision 1: join the tenant-local `chain_version=1` append
    // stream exactly like the canonical audit adapter (audit-sqlite and
    // document-postgres use this shape). Writing chain metadata here keeps
    // the row readable by `SqliteAuditStore` and `verify_chain`; the outer
    // `BEGIN IMMEDIATE` single-writer reservation serializes the append.
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT record_hash FROM audit_events WHERE tenant_id = ?1 AND chain_version = 1 ORDER BY stream_sequence DESC LIMIT 1",
    )
    .bind(document.tenant_id().to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_error)?
    .flatten()
    .unwrap_or_else(|| audit::audit_chain_genesis(document.tenant_id()));
    let sequence = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(stream_sequence), 0) + 1 FROM audit_events WHERE tenant_id = ?1",
    )
    .bind(document.tenant_id().to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(map_error)?;
    let event = event
        .with_chain_metadata(sequence, Utc::now(), 1, Some(previous))
        .map_err(|_| ApplicationPortError::Failed)?;
    sqlx::query("INSERT INTO audit_events (id,tenant_id,user_id,action,resource_type,resource_id,details,created_at,occurred_at,recorded_at,stream_sequence,chain_version,operation_id,actor_type,actor_id,result,changed_fields,schema_version,previous_hash,record_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)")
        .bind(event.id().to_string())
        .bind(event.tenant_id().to_string())
        .bind(document.created_by().to_string())
        .bind(event.action().as_str())
        .bind(&event.resource().resource_type)
        .bind(&event.resource().resource_id)
        .bind(event.details().to_string())
        .bind(event.occurred_at().to_rfc3339())
        .bind(event.recorded_at().to_rfc3339())
        .bind(event.stream_sequence())
        .bind(event.chain_version())
        .bind(event.operation_id().to_string())
        .bind("user")
        .bind(event.actor().actor_id.to_string())
        .bind("succeeded")
        .bind(serde_json::to_string(event.changed_fields()).map_err(|_| ApplicationPortError::Failed)?)
        .bind(event.schema_version())
        .bind(event.previous_hash())
        .bind(event.record_hash())
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
    Ok(())
}

async fn insert_outbox(
    tx: &mut SqliteConnection,
    command: &PersistNewDocument,
    deterministic_event_time: Option<DateTime<Utc>>,
) -> Result<(), ApplicationPortError> {
    let document = &command.document;
    let payload = serde_json::json!({"document_id": document.id(), "original_filename": document.original_filename()});
    let (event_id, occurred_at) = match deterministic_event_time {
        Some(event_time) => (document.id(), event_time),
        None => (Uuid::now_v7(), Utc::now()),
    };
    sqlx::query("INSERT INTO outbox_events (event_id, event_type, tenant_id, aggregate_id, aggregate_type, payload, schema_version, occurred_at) VALUES (?1, 'document.created', ?2, ?3, 'document', ?4, 'v1', ?5)")
        .bind(event_id.to_string()).bind(document.tenant_id().to_string())
        .bind(document.id().to_string()).bind(payload.to_string()).bind(occurred_at.to_rfc3339())
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

async fn rollback(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
) -> Result<(), ApplicationPortError> {
    sqlx::query("ROLLBACK")
        .execute(&mut **connection)
        .await
        .map(|_| ())
        .map_err(map_error)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, ApplicationPortError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| ApplicationPortError::Failed)
}
