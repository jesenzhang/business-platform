//! `CreateDocumentMetadata` use case.
//!
//! Flow: validate → create domain object → begin tx → save → write outbox → commit.

use messaging::{DomainEvent, ReliableOutbox};
use shared_kernel::error::AppError;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{DocumentDomainError, DocumentMetadata, DocumentRepository};

/// Command to create a new document metadata record.
#[derive(Debug, Clone)]
pub struct CreateDocumentCommand {
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// User performing the action.
    pub user_id: Uuid,
    /// Original filename as uploaded.
    pub original_filename: String,
    /// MIME content type.
    pub content_type: String,
    /// Object storage key (already validated by the caller or domain).
    pub object_key: String,
    /// Optional file size in bytes.
    pub size_bytes: Option<i64>,
}

/// Use case: create document metadata with transactional outbox.
pub struct CreateDocumentMetadata<'a> {
    repo: &'a dyn DocumentRepository,
    pool: &'a PgPool,
}

impl<'a> CreateDocumentMetadata<'a> {
    /// Create a new instance of the use case.
    pub fn new(repo: &'a dyn DocumentRepository, pool: &'a PgPool) -> Self {
        Self { repo, pool }
    }

    /// Execute the create document use case.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Validation`] if domain invariants are violated,
    /// or [`AppError::Database`] if persistence fails.
    pub async fn execute(&self, cmd: CreateDocumentCommand) -> Result<DocumentMetadata, AppError> {
        // 1. Create domain entity (validates invariants).
        let doc = DocumentMetadata::create(
            cmd.tenant_id,
            cmd.original_filename,
            cmd.content_type,
            cmd.object_key,
            cmd.user_id,
            cmd.size_bytes,
        )
        .map_err(|e| map_domain_error(&e))?;

        // 2. Begin transaction.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        // 3. Save to repository.
        self.repo
            .save(&mut tx, &doc)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        // 4. Write outbox event (document.created).
        let event = DomainEvent::new(
            "document.created",
            cmd.tenant_id.to_string(),
            doc.id.to_string(),
            "document",
            serde_json::json!({
                "document_id": doc.id,
                "original_filename": doc.original_filename,
                "content_type": doc.content_type,
                "object_key": doc.object_key,
                "created_by": doc.created_by,
            }),
        );

        ReliableOutbox::append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        // 5. Commit.
        tx.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(document_id = %doc.id, tenant_id = %cmd.tenant_id, "document metadata created");

        Ok(doc)
    }
}

/// Map domain errors to application errors at the boundary.
fn map_domain_error(err: &DocumentDomainError) -> AppError {
    match err {
        DocumentDomainError::EmptyFilename
        | DocumentDomainError::EmptyContentType
        | DocumentDomainError::EmptyObjectKey
        | DocumentDomainError::InvalidObjectKey(_) => AppError::Validation(err.to_string()),
        DocumentDomainError::NotFound(id) => AppError::NotFound {
            resource: "document".to_string(),
            id: id.to_string(),
        },
        DocumentDomainError::VersionConflict { expected, actual } => AppError::Conflict(format!(
            "version conflict: expected {expected}, got {actual}"
        )),
    }
}
