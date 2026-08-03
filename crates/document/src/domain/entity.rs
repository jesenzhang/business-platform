//! Document metadata aggregate root.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::DocumentDomainError;
use super::object_key::DocumentObjectKey;

/// Error returned when persistence contains an unknown document status.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown document status")]
pub struct DocumentStatusParseError;

/// Lifecycle status of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DocumentStatus {
    /// Document is active and accessible.
    Active,
    /// Document is archived (read-only, hidden from default lists).
    Archived,
    /// Document is soft-deleted.
    Deleted,
}

impl DocumentStatus {
    /// Return the database string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Deleted => "deleted",
        }
    }
}

impl TryFrom<&str> for DocumentStatus {
    type Error = DocumentStatusParseError;

    /// Parse the database representation without accepting unknown values.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "deleted" => Ok(Self::Deleted),
            _ => Err(DocumentStatusParseError),
        }
    }
}

/// Document metadata aggregate root.
///
/// Invariants:
/// - `original_filename` is non-empty
/// - `content_type` is non-empty
/// - `object_key` is a valid, non-empty storage key (no path traversal)
/// - `version` starts at 1 and increments on mutation
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentMetadata {
    /// Unique identifier (`UUIDv7`, time-ordered).
    id: Uuid,
    /// Owning tenant.
    tenant_id: Uuid,
    /// Original filename as uploaded by the user.
    original_filename: String,
    /// MIME content type.
    content_type: String,
    /// Validated object storage key.
    #[serde(skip_serializing)]
    object_key: String,
    /// Lifecycle status.
    status: DocumentStatus,
    /// Optimistic locking version.
    version: i64,
    /// File size in bytes (may be unknown at creation).
    size_bytes: Option<i64>,
    /// User who created this document.
    created_by: Uuid,
    /// Creation timestamp.
    created_at: DateTime<Utc>,
    /// Last modification timestamp.
    updated_at: DateTime<Utc>,
}

/// Validated state supplied by an Infrastructure Adapter when rebuilding an
/// aggregate from durable storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehydrateDocumentMetadata {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub object_key: String,
    pub status: DocumentStatus,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DocumentMetadata {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    #[must_use]
    pub fn original_filename(&self) -> &str {
        &self.original_filename
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub const fn status(&self) -> DocumentStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    #[must_use]
    pub const fn size_bytes(&self) -> Option<i64> {
        self.size_bytes
    }

    #[must_use]
    pub const fn created_by(&self) -> Uuid {
        self.created_by
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Rebuild an aggregate after validating every persisted invariant.
    pub fn rehydrate(state: RehydrateDocumentMetadata) -> Result<Self, DocumentDomainError> {
        if state.tenant_id.is_nil() || state.created_by.is_nil() || state.id.is_nil() {
            return Err(DocumentDomainError::InvalidIdentity);
        }
        if state.version <= 0 {
            return Err(DocumentDomainError::InvalidVersion);
        }
        if state.size_bytes.is_some_and(|size| size < 0) {
            return Err(DocumentDomainError::InvalidSize);
        }
        if state.original_filename.trim().is_empty() {
            return Err(DocumentDomainError::EmptyFilename);
        }
        if state.content_type.trim().is_empty() {
            return Err(DocumentDomainError::EmptyContentType);
        }
        if state.updated_at < state.created_at {
            return Err(DocumentDomainError::InvalidTimestamps);
        }
        DocumentObjectKey::new(
            state.tenant_id,
            state.id,
            state.version,
            state.object_key.clone(),
        )
        .map_err(|error| DocumentDomainError::InvalidObjectKey(error.to_string()))?;
        Ok(Self {
            id: state.id,
            tenant_id: state.tenant_id,
            original_filename: state.original_filename,
            content_type: state.content_type,
            object_key: state.object_key,
            status: state.status,
            version: state.version,
            size_bytes: state.size_bytes,
            created_by: state.created_by,
            created_at: state.created_at,
            updated_at: state.updated_at,
        })
    }

    /// Factory method: create new document metadata with validation.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentDomainError`] if any invariant is violated.
    pub fn create(
        tenant_id: Uuid,
        original_filename: String,
        content_type: String,
        object_key: String,
        created_by: Uuid,
        size_bytes: Option<i64>,
    ) -> Result<Self, DocumentDomainError> {
        if tenant_id.is_nil() || created_by.is_nil() {
            return Err(DocumentDomainError::InvalidIdentity);
        }
        if size_bytes.is_some_and(|size| size < 0) {
            return Err(DocumentDomainError::InvalidSize);
        }
        if original_filename.trim().is_empty() {
            return Err(DocumentDomainError::EmptyFilename);
        }
        if content_type.trim().is_empty() {
            return Err(DocumentDomainError::EmptyContentType);
        }
        if object_key.trim().is_empty() {
            return Err(DocumentDomainError::EmptyObjectKey);
        }

        let now = Utc::now();
        let id = Uuid::now_v7();
        let object_key = DocumentObjectKey::new(tenant_id, id, 1, object_key)
            .map_err(|e| DocumentDomainError::InvalidObjectKey(e.to_string()))?
            .as_storage_key();

        Ok(Self {
            id,
            tenant_id,
            original_filename,
            content_type,
            object_key,
            status: DocumentStatus::Active,
            version: 1,
            size_bytes,
            created_by,
            created_at: now,
            updated_at: now,
        })
    }

    /// Archive an active document.
    pub fn archive(&mut self) -> Result<(), DocumentDomainError> {
        if self.status != DocumentStatus::Active {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "archive",
                status: self.status,
            });
        }
        self.bump_version()?;
        self.status = DocumentStatus::Archived;
        Ok(())
    }

    /// Restore an archived document to active state.
    pub fn restore(&mut self) -> Result<(), DocumentDomainError> {
        if self.status != DocumentStatus::Archived {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "restore",
                status: self.status,
            });
        }
        self.bump_version()?;
        self.status = DocumentStatus::Active;
        Ok(())
    }

    /// Soft-delete an active or archived document.
    pub fn mark_deleted(&mut self) -> Result<(), DocumentDomainError> {
        if !matches!(
            self.status,
            DocumentStatus::Active | DocumentStatus::Archived
        ) {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "mark_deleted",
                status: self.status,
            });
        }
        self.bump_version()?;
        self.status = DocumentStatus::Deleted;
        Ok(())
    }

    fn bump_version(&mut self) -> Result<(), DocumentDomainError> {
        if self.version == i64::MAX {
            return Err(DocumentDomainError::InvalidVersion);
        }
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_with_valid_data_succeeds() {
        let doc = DocumentMetadata::create(
            Uuid::now_v7(),
            "report.pdf".to_string(),
            "application/pdf".to_string(),
            "documents/tenant-1/report.pdf".to_string(),
            Uuid::now_v7(),
            Some(1024),
        );

        assert!(doc.is_ok());
        let doc = doc.unwrap_or_else(|_| unreachable!());
        assert_eq!(doc.original_filename(), "report.pdf");
        assert_eq!(doc.content_type(), "application/pdf");
        assert_eq!(doc.status(), DocumentStatus::Active);
        assert_eq!(doc.version(), 1);
        assert_eq!(doc.size_bytes(), Some(1024));
    }

    #[test]
    fn create_with_empty_filename_fails() {
        let result = DocumentMetadata::create(
            Uuid::now_v7(),
            String::new(),
            "application/pdf".to_string(),
            "documents/file.pdf".to_string(),
            Uuid::now_v7(),
            None,
        );

        assert!(matches!(result, Err(DocumentDomainError::EmptyFilename)));
    }

    #[test]
    fn create_with_blank_filename_fails() {
        let result = DocumentMetadata::create(
            Uuid::now_v7(),
            "   ".to_string(),
            "application/pdf".to_string(),
            "documents/file.pdf".to_string(),
            Uuid::now_v7(),
            None,
        );

        assert!(matches!(result, Err(DocumentDomainError::EmptyFilename)));
    }

    #[test]
    fn create_with_empty_content_type_fails() {
        let result = DocumentMetadata::create(
            Uuid::now_v7(),
            "file.pdf".to_string(),
            String::new(),
            "documents/file.pdf".to_string(),
            Uuid::now_v7(),
            None,
        );

        assert!(matches!(result, Err(DocumentDomainError::EmptyContentType)));
    }

    #[test]
    fn create_with_empty_object_key_fails() {
        let result = DocumentMetadata::create(
            Uuid::now_v7(),
            "file.pdf".to_string(),
            "application/pdf".to_string(),
            String::new(),
            Uuid::now_v7(),
            None,
        );

        assert!(matches!(result, Err(DocumentDomainError::EmptyObjectKey)));
    }

    #[test]
    fn create_with_negative_size_fails() {
        let result = DocumentMetadata::create(
            Uuid::now_v7(),
            "file.pdf".to_string(),
            "application/pdf".to_string(),
            "documents/file.pdf".to_string(),
            Uuid::now_v7(),
            Some(-1),
        );

        assert!(result.is_err());
    }

    #[test]
    fn create_with_path_traversal_object_key_fails() {
        let result = DocumentMetadata::create(
            Uuid::now_v7(),
            "file.pdf".to_string(),
            "application/pdf".to_string(),
            "../etc/passwd".to_string(),
            Uuid::now_v7(),
            None,
        );

        assert!(matches!(
            result,
            Err(DocumentDomainError::InvalidObjectKey(_))
        ));
    }

    #[test]
    fn status_round_trip() {
        assert_eq!(
            DocumentStatus::try_from("active").unwrap_or_else(|_| unreachable!()),
            DocumentStatus::Active
        );
        assert_eq!(
            DocumentStatus::try_from("archived").unwrap_or_else(|_| unreachable!()),
            DocumentStatus::Archived
        );
        assert_eq!(
            DocumentStatus::try_from("deleted").unwrap_or_else(|_| unreachable!()),
            DocumentStatus::Deleted
        );
    }

    #[test]
    fn unknown_database_status_is_not_accepted_as_active() {
        assert!(DocumentStatus::try_from("unknown").is_err());
    }

    #[test]
    fn lifecycle_transitions_are_versioned_and_terminal_delete_is_enforced() {
        let mut document = DocumentMetadata::create(
            Uuid::now_v7(),
            "report.pdf".to_string(),
            "application/pdf".to_string(),
            "report.pdf".to_string(),
            Uuid::now_v7(),
            None,
        )
        .unwrap_or_else(|_| unreachable!());
        assert!(document.archive().is_ok());
        assert_eq!(document.status(), DocumentStatus::Archived);
        assert_eq!(document.version(), 2);
        assert!(document.restore().is_ok());
        assert_eq!(document.status(), DocumentStatus::Active);
        assert_eq!(document.version(), 3);
        assert!(document.mark_deleted().is_ok());
        assert_eq!(document.status(), DocumentStatus::Deleted);
        assert_eq!(document.version(), 4);
        assert!(matches!(
            document.restore(),
            Err(DocumentDomainError::InvalidStatusTransition { .. })
        ));
    }

    #[test]
    fn rehydrate_rejects_invalid_persisted_state() {
        let now = Utc::now();
        let invalid = RehydrateDocumentMetadata {
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            original_filename: "report.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            object_key: "report.pdf".to_string(),
            status: DocumentStatus::Active,
            version: 0,
            size_bytes: Some(-1),
            created_by: Uuid::now_v7(),
            created_at: now,
            updated_at: now - chrono::Duration::seconds(1),
        };
        assert!(DocumentMetadata::rehydrate(invalid).is_err());
    }
}
