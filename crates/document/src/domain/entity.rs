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
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// Original filename as uploaded by the user.
    pub original_filename: String,
    /// MIME content type.
    pub content_type: String,
    /// Validated object storage key.
    pub object_key: String,
    /// Lifecycle status.
    pub status: DocumentStatus,
    /// Optimistic locking version.
    pub version: i64,
    /// File size in bytes (may be unknown at creation).
    pub size_bytes: Option<i64>,
    /// User who created this document.
    pub created_by: Uuid,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub updated_at: DateTime<Utc>,
}

impl DocumentMetadata {
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
        assert_eq!(doc.original_filename, "report.pdf");
        assert_eq!(doc.content_type, "application/pdf");
        assert_eq!(doc.status, DocumentStatus::Active);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.size_bytes, Some(1024));
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
}
