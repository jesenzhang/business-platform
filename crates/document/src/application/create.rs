use std::sync::Arc;

use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{DocumentDomainError, DocumentMetadata};
use crate::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};

#[derive(Debug, Clone)]
pub struct CreateDocumentCommand {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    /// Logical object path. The domain adds tenant/document/version segments.
    pub object_key: String,
    pub size_bytes: Option<i64>,
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
        let idempotency_key = command.idempotency_key.trim();
        if idempotency_key.is_empty() || idempotency_key.len() > 255 {
            return Err(CreateDocumentError::Validation(
                "Idempotency-Key must contain 1 to 255 characters".to_string(),
            ));
        }

        let fingerprint = request_fingerprint(&command);
        let document = DocumentMetadata::create(
            command.tenant_id,
            command.original_filename,
            command.content_type,
            command.object_key,
            command.user_id,
            command.size_bytes,
        )
        .map_err(|error| map_domain_error(&error))?;

        self.unit_of_work
            .execute(PersistNewDocument {
                document,
                idempotency_key: idempotency_key.to_string(),
                request_fingerprint: fingerprint,
            })
            .await
            .map_err(map_port_error)
    }
}

fn request_fingerprint(command: &CreateDocumentCommand) -> String {
    let mut hasher = Sha256::new();
    hasher.update(command.tenant_id.as_bytes());
    hasher.update(command.user_id.as_bytes());
    hasher.update(command.original_filename.as_bytes());
    hasher.update([0]);
    hasher.update(command.content_type.as_bytes());
    hasher.update([0]);
    hasher.update(command.object_key.as_bytes());
    hasher.update(command.size_bytes.unwrap_or_default().to_be_bytes());
    format!("{:x}", hasher.finalize())
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
                idempotency_key: "key-1".to_string(),
            })
            .await
            .expect("fake port should succeed");

        assert!(!result.replayed);
        assert_eq!(result.document.tenant_id, tenant_id);
        assert_eq!(unit_of_work.calls.lock().expect("test lock").len(), 1);
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
                idempotency_key: String::new(),
            })
            .await;

        assert!(matches!(result, Err(CreateDocumentError::Validation(_))));
        assert!(unit_of_work.calls.lock().expect("test lock").is_empty());
    }
}
