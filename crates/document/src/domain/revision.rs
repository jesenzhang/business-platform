//! Immutable document content revisions.

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use super::entity::DocumentMetadata;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DocumentRevisionError {
    #[error("revision identity is invalid")]
    InvalidIdentity,
    #[error("revision number must be positive")]
    InvalidRevisionNumber,
    #[error("revision source object reference is invalid")]
    InvalidSourceObjectRef,
    #[error("revision checksum is invalid")]
    InvalidChecksum,
    #[error("revision size must not be negative")]
    InvalidSize,
    #[error("revision content type must not be empty")]
    EmptyContentType,
    #[error("revision filename must not be empty")]
    EmptyFilename,
    #[error("revision creator must not be nil")]
    InvalidCreator,
    #[error("revision timestamp is invalid")]
    InvalidTimestamp,
    #[error("current revision is stale: expected {expected}, actual {actual}")]
    StaleCurrentRevision { expected: Uuid, actual: Uuid },
}

/// Immutable content identity owned by Document Management.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRevision {
    id: Uuid,
    tenant_id: Uuid,
    document_id: Uuid,
    revision_no: i64,
    parent_revision_id: Option<Uuid>,
    source_object_ref: String,
    sha256: Option<String>,
    content_type: String,
    size_bytes: Option<i64>,
    original_filename: String,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    change_reason: Option<String>,
    provider_version_id: Option<String>,
}

impl DocumentRevision {
    /// Build the initial revision for a newly created document.
    pub fn initial(document: &DocumentMetadata) -> Result<Self, DocumentRevisionError> {
        let source_object_ref = document.object_key();
        if Self::validate_source_object_ref(&source_object_ref).is_err() {
            // Existing PLAN-0003 rows retain their legacy vN key until the
            // storage relocation/reconciliation worker has completed. The
            // business revision is still a UUID; this is a read/backfill
            // compatibility path only.
            if source_object_ref.contains("/v") {
                return Ok(Self {
                    id: document.current_revision_id(),
                    tenant_id: document.tenant_id(),
                    document_id: document.id(),
                    revision_no: document.content_revision().value(),
                    parent_revision_id: None,
                    source_object_ref,
                    sha256: None,
                    content_type: document.content_type().to_string(),
                    size_bytes: document.size_bytes(),
                    original_filename: document.original_filename().to_string(),
                    created_by: document.created_by(),
                    created_at: document.created_at(),
                    change_reason: Some("legacy content reference backfill".to_string()),
                    provider_version_id: None,
                });
            }
        }
        Self::new(
            document.current_revision_id(),
            document.tenant_id(),
            document.id(),
            1,
            None,
            source_object_ref,
            None,
            document.content_type().to_string(),
            document.size_bytes(),
            document.original_filename().to_string(),
            document.created_by(),
            document.created_at(),
            Some("initial upload".to_string()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        document_id: Uuid,
        revision_no: i64,
        parent_revision_id: Option<Uuid>,
        source_object_ref: String,
        sha256: Option<String>,
        content_type: String,
        size_bytes: Option<i64>,
        original_filename: String,
        created_by: Uuid,
        created_at: DateTime<Utc>,
        change_reason: Option<String>,
    ) -> Result<Self, DocumentRevisionError> {
        if id.is_nil() || tenant_id.is_nil() || document_id.is_nil() {
            return Err(DocumentRevisionError::InvalidIdentity);
        }
        if parent_revision_id.is_some_and(|value| value.is_nil()) {
            return Err(DocumentRevisionError::InvalidIdentity);
        }
        if revision_no <= 0 {
            return Err(DocumentRevisionError::InvalidRevisionNumber);
        }
        if Self::validate_source_object_ref(&source_object_ref).is_err() {
            return Err(DocumentRevisionError::InvalidSourceObjectRef);
        }
        if sha256.as_deref().is_some_and(|value| {
            value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit())
        }) {
            return Err(DocumentRevisionError::InvalidChecksum);
        }
        if size_bytes.is_some_and(|size| size < 0) {
            return Err(DocumentRevisionError::InvalidSize);
        }
        if content_type.trim().is_empty() {
            return Err(DocumentRevisionError::EmptyContentType);
        }
        if original_filename.trim().is_empty() {
            return Err(DocumentRevisionError::EmptyFilename);
        }
        if created_by.is_nil() {
            return Err(DocumentRevisionError::InvalidCreator);
        }
        Ok(Self {
            id,
            tenant_id,
            document_id,
            revision_no,
            parent_revision_id,
            source_object_ref,
            sha256,
            content_type,
            size_bytes,
            original_filename,
            created_by,
            created_at,
            change_reason,
            provider_version_id: None,
        })
    }

    /// Rehydrate a stored revision. The public constructor remains the only
    /// route adapters use to rebuild the immutable entity.
    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: Uuid,
        tenant_id: Uuid,
        document_id: Uuid,
        revision_no: i64,
        parent_revision_id: Option<Uuid>,
        source_object_ref: String,
        sha256: Option<String>,
        content_type: String,
        size_bytes: Option<i64>,
        original_filename: String,
        created_by: Uuid,
        created_at: DateTime<Utc>,
        change_reason: Option<String>,
    ) -> Result<Self, DocumentRevisionError> {
        Self::new(
            id,
            tenant_id,
            document_id,
            revision_no,
            parent_revision_id,
            source_object_ref,
            sha256,
            content_type,
            size_bytes,
            original_filename,
            created_by,
            created_at,
            change_reason,
        )
    }

    /// Validate that storage identity is tenant/document/revision scoped.
    pub fn validate_source_object_ref(value: &str) -> Result<(), DocumentRevisionError> {
        let parts: Vec<_> = value.split('/').collect();
        if parts.len() != 7
            || parts[0] != "tenants"
            || parts[2] != "documents"
            || parts[4] != "revisions"
            || parts[6] != "source"
        {
            return Err(DocumentRevisionError::InvalidSourceObjectRef);
        }
        let tenant = Uuid::parse_str(parts[1]);
        let document = Uuid::parse_str(parts[3]);
        let revision = Uuid::parse_str(parts[5]);
        if tenant.is_err() || document.is_err() || revision.is_err() {
            return Err(DocumentRevisionError::InvalidSourceObjectRef);
        }
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub const fn document_id(&self) -> Uuid {
        self.document_id
    }
    #[must_use]
    pub const fn revision_no(&self) -> i64 {
        self.revision_no
    }
    #[must_use]
    pub const fn parent_revision_id(&self) -> Option<Uuid> {
        self.parent_revision_id
    }
    #[must_use]
    pub fn source_object_ref(&self) -> &str {
        &self.source_object_ref
    }
    #[must_use]
    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }
    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
    #[must_use]
    pub const fn size_bytes(&self) -> Option<i64> {
        self.size_bytes
    }
    #[must_use]
    pub fn original_filename(&self) -> &str {
        &self.original_filename
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
    pub fn change_reason(&self) -> Option<&str> {
        self.change_reason.as_deref()
    }
    #[must_use]
    pub fn provider_version_id(&self) -> Option<&str> {
        self.provider_version_id.as_deref()
    }
}
