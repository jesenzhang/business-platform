use std::sync::Arc;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{DocumentDomainError, DocumentMetadata};
use crate::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};

const REQUEST_FINGERPRINT_VERSION: i16 = 1;

#[derive(Debug, Clone)]
pub struct CreateDocumentCommand {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    /// Logical object path. The domain adds tenant/document/version segments.
    pub object_key: String,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    /// Optional caller-generated revision identity used by streaming upload so
    /// the storage write and metadata transaction share one business ID.
    pub revision_id: Option<Uuid>,
    pub idempotency_key: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CreateDocumentError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    #[error("document persistence is unavailable")]
    Unavailable,
    #[error("document creation failed")]
    Failed,
}

pub struct CreateDocumentMetadata {
    unit_of_work: Arc<dyn CreateDocumentUnitOfWork>,
}

impl CreateDocumentMetadata {
    #[must_use]
    pub fn new(unit_of_work: Arc<dyn CreateDocumentUnitOfWork>) -> Self {
        Self { unit_of_work }
    }

    pub async fn execute(
        &self,
        command: CreateDocumentCommand,
    ) -> Result<CreateDocumentResult, CreateDocumentError> {
        self.execute_with_id(None, command).await
    }

    pub async fn execute_with_id(
        &self,
        document_id: Option<Uuid>,
        command: CreateDocumentCommand,
    ) -> Result<CreateDocumentResult, CreateDocumentError> {
        self.execute_with_id_at(document_id, command, Utc::now())
            .await
    }

    /// Execute a create command with a caller-supplied creation timestamp.
    ///
    /// Rehearsal adapters use this when replaying a frozen manifest. Ordinary
    /// callers should use [`Self::execute_with_id`], which uses the current
    /// clock.
    pub async fn execute_with_id_at(
        &self,
        document_id: Option<Uuid>,
        command: CreateDocumentCommand,
        created_at: DateTime<Utc>,
    ) -> Result<CreateDocumentResult, CreateDocumentError> {
        let idempotency_key = command.idempotency_key.trim();
        if idempotency_key.is_empty() || idempotency_key.len() > 255 {
            return Err(CreateDocumentError::Validation(
                "Idempotency-Key must contain 1 to 255 characters".to_string(),
            ));
        }

        let fingerprint = request_fingerprint(&command);
        let document = match document_id {
            Some(document_id) => match command.revision_id {
                Some(revision_id) => DocumentMetadata::create_with_revision_id_at(
                    document_id,
                    command.tenant_id,
                    command.original_filename.clone(),
                    command.content_type.clone(),
                    command.object_key.clone(),
                    command.user_id,
                    command.size_bytes,
                    revision_id,
                    created_at,
                ),
                None => DocumentMetadata::create_with_id(
                    document_id,
                    command.tenant_id,
                    command.original_filename.clone(),
                    command.content_type.clone(),
                    command.object_key.clone(),
                    command.user_id,
                    command.size_bytes,
                ),
            },
            None => match command.revision_id {
                Some(revision_id) => DocumentMetadata::create_with_revision_id_at(
                    Uuid::now_v7(),
                    command.tenant_id,
                    command.original_filename,
                    command.content_type,
                    command.object_key,
                    command.user_id,
                    command.size_bytes,
                    revision_id,
                    created_at,
                ),
                None => DocumentMetadata::create(
                    command.tenant_id,
                    command.original_filename,
                    command.content_type,
                    command.object_key,
                    command.user_id,
                    command.size_bytes,
                ),
            },
        }
        .map_err(|error| map_domain_error(&error))?;

        self.unit_of_work
            .execute(PersistNewDocument {
                document,
                idempotency_key: idempotency_key.to_string(),
                request_fingerprint: fingerprint,
                fingerprint_version: REQUEST_FINGERPRINT_VERSION,
                initial_revision_sha256: command.sha256,
            })
            .await
            .map_err(map_port_error)
    }
}

fn request_fingerprint(command: &CreateDocumentCommand) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"document-create-fingerprint:v1\0");
    hasher.update(command.tenant_id.as_bytes());
    hasher.update(command.user_id.as_bytes());
    update_string(&mut hasher, &command.original_filename);
    update_string(&mut hasher, &command.content_type);
    update_string(&mut hasher, &command.object_key);
    match command.revision_id {
        None => hasher.update([0]),
        Some(revision_id) => {
            hasher.update([1]);
            hasher.update(revision_id.as_bytes());
        }
    }
    match command.size_bytes {
        None => hasher.update([0]),
        Some(size) => {
            hasher.update([1]);
            hasher.update(size.to_be_bytes());
        }
    }
    match command.sha256.as_deref() {
        None => {}
        Some(sha256) => {
            hasher.update([1]);
            update_string(&mut hasher, sha256);
        }
    }
    format!("{:x}", hasher.finalize())
}

fn update_string(hasher: &mut Sha256, value: &str) {
    let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
    hasher.update(length.to_be_bytes());
    hasher.update(value.as_bytes());
}

fn map_domain_error(error: &DocumentDomainError) -> CreateDocumentError {
    CreateDocumentError::Validation(error.to_string())
}

fn map_port_error(error: ApplicationPortError) -> CreateDocumentError {
    match error {
        ApplicationPortError::IdempotencyConflict => CreateDocumentError::IdempotencyConflict,
        ApplicationPortError::Unavailable => CreateDocumentError::Unavailable,
        ApplicationPortError::Failed => CreateDocumentError::Failed,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;

    #[derive(Default)]
    struct FakeUnitOfWork {
        calls: Mutex<Vec<PersistNewDocument>>,
    }

    #[async_trait]
    impl CreateDocumentUnitOfWork for FakeUnitOfWork {
        async fn execute(
            &self,
            command: PersistNewDocument,
        ) -> Result<CreateDocumentResult, ApplicationPortError> {
            self.calls.lock().expect("test lock").push(command.clone());
            Ok(CreateDocumentResult {
                document: command.document,
                replayed: false,
            })
        }
    }

    #[tokio::test]
    async fn create_uses_application_port_without_database_types() {
        let unit_of_work = Arc::new(FakeUnitOfWork::default());
        let service = CreateDocumentMetadata::new(unit_of_work.clone());
        let tenant_id = Uuid::now_v7();
        let result = service
            .execute(CreateDocumentCommand {
                tenant_id,
                user_id: Uuid::now_v7(),
                original_filename: "report.pdf".to_string(),
                content_type: "application/pdf".to_string(),
                object_key: "report.pdf".to_string(),
                size_bytes: Some(10),
                sha256: None,
                revision_id: None,
                idempotency_key: "key-1".to_string(),
            })
            .await
            .expect("fake port should succeed");

        assert!(!result.replayed);
        assert_eq!(result.document.tenant_id(), tenant_id);
        assert_eq!(unit_of_work.calls.lock().expect("test lock").len(), 1);
        assert_eq!(
            unit_of_work.calls.lock().expect("test lock")[0].fingerprint_version,
            REQUEST_FINGERPRINT_VERSION
        );
    }

    #[tokio::test]
    async fn create_rejects_missing_idempotency_key_before_port_call() {
        let unit_of_work = Arc::new(FakeUnitOfWork::default());
        let service = CreateDocumentMetadata::new(unit_of_work.clone());
        let result = service
            .execute(CreateDocumentCommand {
                tenant_id: Uuid::now_v7(),
                user_id: Uuid::now_v7(),
                original_filename: "report.pdf".to_string(),
                content_type: "application/pdf".to_string(),
                object_key: "report.pdf".to_string(),
                size_bytes: None,
                sha256: None,
                revision_id: None,
                idempotency_key: String::new(),
            })
            .await;

        assert!(matches!(result, Err(CreateDocumentError::Validation(_))));
        assert!(unit_of_work.calls.lock().expect("test lock").is_empty());
    }

    #[test]
    fn fingerprint_distinguishes_missing_size_from_zero_size() {
        let tenant_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let command = CreateDocumentCommand {
            tenant_id,
            user_id,
            original_filename: "report.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            object_key: "report.pdf".to_string(),
            size_bytes: None,
            sha256: None,
            revision_id: None,
            idempotency_key: "key-1".to_string(),
        };
        let zero_sized = CreateDocumentCommand {
            size_bytes: Some(0),
            ..command.clone()
        };

        assert_ne!(
            request_fingerprint(&command),
            request_fingerprint(&zero_sized)
        );
        assert_eq!(request_fingerprint(&command), request_fingerprint(&command));
    }
}
