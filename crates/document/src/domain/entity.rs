//! Document metadata aggregate root.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::DocumentDomainError;
use super::object_key::DocumentContentReference;
use super::version::{AggregateVersion, ContentRevision};

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

/// Business lifecycle, independent from deletion/recovery state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DocumentLifecycleState {
    Active,
    Archived,
}

impl DocumentLifecycleState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }
}

impl TryFrom<&str> for DocumentLifecycleState {
    type Error = DocumentStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            _ => Err(DocumentStatusParseError),
        }
    }
}

/// Deletion state. A trashed document still owns its revisions and evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DocumentDeletionState {
    Present,
    Trashed,
    PendingPurge,
    Purged,
}

impl DocumentDeletionState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Trashed => "trashed",
            Self::PendingPurge => "pending_purge",
            Self::Purged => "purged",
        }
    }
}

impl TryFrom<&str> for DocumentDeletionState {
    type Error = DocumentStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "present" => Ok(Self::Present),
            "trashed" => Ok(Self::Trashed),
            "pending_purge" => Ok(Self::PendingPurge),
            "purged" => Ok(Self::Purged),
            _ => Err(DocumentStatusParseError),
        }
    }
}

/// Document metadata aggregate root.
///
/// Invariants:
/// - `original_filename` is non-empty
/// - `content_type` is non-empty
/// - `content_reference` is a canonical, tenant-owned storage reference
/// - `aggregate_version` starts at 1 and increments on business mutation
/// - `content_revision` changes only when file content is replaced
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentMetadata {
    /// Unique identifier (`UUIDv7`, time-ordered).
    id: Uuid,
    /// Owning tenant.
    tenant_id: Uuid,
    /// Original filename as uploaded by the user.
    original_filename: String,
    /// MIME content type.
    content_type: String,
    /// Validated tenant-owned content reference.
    content_reference: DocumentContentReference,
    /// Lifecycle status.
    status: DocumentStatus,
    /// Optimistic locking version.
    aggregate_version: AggregateVersion,
    /// File-content revision used by object storage paths.
    content_revision: ContentRevision,
    /// Stable identity of the current immutable content revision.
    current_revision_id: Uuid,
    /// Business lifecycle, kept separate from deletion state.
    lifecycle_state: DocumentLifecycleState,
    /// Deletion/recovery state.
    deletion_state: DocumentDeletionState,
    /// Lifecycle to restore after a trash operation.
    pre_trash_lifecycle: DocumentLifecycleState,
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
    pub aggregate_version: AggregateVersion,
    pub content_revision: ContentRevision,
    pub current_revision_id: Uuid,
    pub lifecycle_state: DocumentLifecycleState,
    pub deletion_state: DocumentDeletionState,
    pub pre_trash_lifecycle: DocumentLifecycleState,
    pub size_bytes: Option<i64>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DocumentMetadata {
    #[must_use]
    pub const fn legacy_states(
        status: DocumentStatus,
    ) -> (DocumentLifecycleState, DocumentDeletionState) {
        match status {
            DocumentStatus::Active => (
                DocumentLifecycleState::Active,
                DocumentDeletionState::Present,
            ),
            DocumentStatus::Archived => (
                DocumentLifecycleState::Archived,
                DocumentDeletionState::Present,
            ),
            DocumentStatus::Deleted => (
                DocumentLifecycleState::Active,
                DocumentDeletionState::Trashed,
            ),
        }
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
    pub fn original_filename(&self) -> &str {
        &self.original_filename
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    #[must_use]
    pub fn object_key(&self) -> String {
        self.content_reference.as_storage_key()
    }

    #[must_use]
    pub fn content_reference(&self) -> &DocumentContentReference {
        &self.content_reference
    }

    #[must_use]
    pub const fn status(&self) -> DocumentStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.aggregate_version.value()
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    #[must_use]
    pub const fn content_revision(&self) -> ContentRevision {
        self.content_revision
    }

    #[must_use]
    pub const fn current_revision_id(&self) -> Uuid {
        self.current_revision_id
    }

    #[must_use]
    pub const fn lifecycle_state(&self) -> DocumentLifecycleState {
        self.lifecycle_state
    }

    #[must_use]
    pub const fn deletion_state(&self) -> DocumentDeletionState {
        self.deletion_state
    }

    /// Materialize the immutable R1 record for persistence.
    pub fn initial_revision(
        &self,
    ) -> Result<super::revision::DocumentRevision, super::revision::DocumentRevisionError> {
        super::revision::DocumentRevision::initial(self)
    }

    /// Materialize the immutable R1 record with a verified source checksum.
    pub fn initial_revision_with_sha256(
        &self,
        sha256: Option<&str>,
    ) -> Result<super::revision::DocumentRevision, super::revision::DocumentRevisionError> {
        super::revision::DocumentRevision::initial_with_sha256(self, sha256)
    }

    pub fn assert_expected_revision(
        &self,
        expected: Uuid,
    ) -> Result<(), super::revision::DocumentRevisionError> {
        if expected != self.current_revision_id {
            return Err(
                super::revision::DocumentRevisionError::StaleCurrentRevision {
                    expected,
                    actual: self.current_revision_id,
                },
            );
        }
        Ok(())
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
        let parsed_reference = DocumentContentReference::parse_storage_key(
            state.tenant_id,
            state.id,
            &state.object_key,
        )
        .map_err(|error| DocumentDomainError::InvalidObjectKey(error.to_string()))?;
        if state.current_revision_id.is_nil() {
            return Err(DocumentDomainError::InvalidIdentity);
        }
        let content_reference = if parsed_reference.revision_id().is_some() {
            if parsed_reference.revision_id() != Some(state.current_revision_id) {
                return Err(DocumentDomainError::ContentRevisionMismatch);
            }
            DocumentContentReference::new_revision(
                state.tenant_id,
                state.id,
                state.current_revision_id,
                state.content_revision,
            )
            .map_err(|error| DocumentDomainError::InvalidObjectKey(error.to_string()))?
        } else {
            if parsed_reference.content_revision() != state.content_revision {
                return Err(DocumentDomainError::ContentRevisionMismatch);
            }
            parsed_reference
        };
        if matches!(state.deletion_state, DocumentDeletionState::Present)
            && state.lifecycle_state.as_str() != state.status.as_str()
        {
            return Err(DocumentDomainError::InvalidStatus);
        }
        if !matches!(state.deletion_state, DocumentDeletionState::Present)
            && state.status != DocumentStatus::Deleted
        {
            return Err(DocumentDomainError::InvalidStatus);
        }
        if state.pre_trash_lifecycle != state.lifecycle_state
            && matches!(state.deletion_state, DocumentDeletionState::Present)
        {
            return Err(DocumentDomainError::InvalidStatus);
        }
        if state.aggregate_version.value() <= 0 {
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
        Ok(Self {
            id: state.id,
            tenant_id: state.tenant_id,
            original_filename: state.original_filename,
            content_type: state.content_type,
            content_reference,
            status: state.status,
            aggregate_version: state.aggregate_version,
            content_revision: state.content_revision,
            current_revision_id: state.current_revision_id,
            lifecycle_state: state.lifecycle_state,
            deletion_state: state.deletion_state,
            pre_trash_lifecycle: state.pre_trash_lifecycle,
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
        Self::create_with_id(
            Uuid::now_v7(),
            tenant_id,
            original_filename,
            content_type,
            object_key,
            created_by,
            size_bytes,
        )
    }

    pub fn create_with_id(
        id: Uuid,
        tenant_id: Uuid,
        original_filename: String,
        content_type: String,
        object_key: String,
        created_by: Uuid,
        size_bytes: Option<i64>,
    ) -> Result<Self, DocumentDomainError> {
        if id.is_nil() || tenant_id.is_nil() || created_by.is_nil() {
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
        Self::create_with_revision_id(
            id,
            tenant_id,
            original_filename,
            content_type,
            object_key,
            created_by,
            size_bytes,
            Uuid::now_v7(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_with_revision_id(
        id: Uuid,
        tenant_id: Uuid,
        original_filename: String,
        content_type: String,
        object_key: String,
        created_by: Uuid,
        size_bytes: Option<i64>,
        revision_id: Uuid,
    ) -> Result<Self, DocumentDomainError> {
        Self::create_with_revision_id_at(
            id,
            tenant_id,
            original_filename,
            content_type,
            object_key,
            created_by,
            size_bytes,
            revision_id,
            Utc::now(),
        )
    }

    /// Create a document with a caller-supplied timestamp.
    ///
    /// Rehearsal adapters use this entry point when the input manifest is the
    /// clock authority. Normal application callers should use
    /// [`Self::create_with_revision_id`].
    #[allow(clippy::too_many_arguments)]
    pub fn create_with_revision_id_at(
        id: Uuid,
        tenant_id: Uuid,
        original_filename: String,
        content_type: String,
        object_key: String,
        created_by: Uuid,
        size_bytes: Option<i64>,
        revision_id: Uuid,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DocumentDomainError> {
        if object_key.trim().is_empty() {
            return Err(DocumentDomainError::EmptyObjectKey);
        }

        let aggregate_version =
            AggregateVersion::new(1).map_err(|_| DocumentDomainError::InvalidVersion)?;
        let content_revision =
            ContentRevision::new(1).map_err(|_| DocumentDomainError::InvalidContentRevision)?;
        DocumentContentReference::new(tenant_id, id, content_revision, object_key)
            .map_err(|error| DocumentDomainError::InvalidObjectKey(error.to_string()))?;
        let content_reference =
            DocumentContentReference::new_revision(tenant_id, id, revision_id, content_revision)
                .map_err(|error| DocumentDomainError::InvalidObjectKey(error.to_string()))?;

        Ok(Self {
            id,
            tenant_id,
            original_filename,
            content_type,
            content_reference,
            status: DocumentStatus::Active,
            aggregate_version,
            content_revision,
            current_revision_id: revision_id,
            lifecycle_state: DocumentLifecycleState::Active,
            deletion_state: DocumentDeletionState::Present,
            pre_trash_lifecycle: DocumentLifecycleState::Active,
            size_bytes,
            created_by,
            created_at,
            updated_at: created_at,
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
        self.lifecycle_state = DocumentLifecycleState::Archived;
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
        self.lifecycle_state = DocumentLifecycleState::Active;
        Ok(())
    }

    /// Move a document to the recoverable trash state without changing its
    /// content revision or current revision identity.
    pub fn trash(&mut self) -> Result<(), DocumentDomainError> {
        if self.deletion_state != DocumentDeletionState::Present {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "trash",
                status: self.status,
            });
        }
        self.bump_version()?;
        self.pre_trash_lifecycle = self.lifecycle_state;
        self.deletion_state = DocumentDeletionState::Trashed;
        self.status = DocumentStatus::Deleted;
        Ok(())
    }

    /// Restore a trashed document to the lifecycle it had before trashing.
    pub fn restore_from_trash(&mut self) -> Result<(), DocumentDomainError> {
        if self.deletion_state != DocumentDeletionState::Trashed {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "restore_from_trash",
                status: self.status,
            });
        }
        self.bump_version()?;
        self.deletion_state = DocumentDeletionState::Present;
        self.lifecycle_state = self.pre_trash_lifecycle;
        self.status = match self.lifecycle_state {
            DocumentLifecycleState::Active => DocumentStatus::Active,
            DocumentLifecycleState::Archived => DocumentStatus::Archived,
        };
        Ok(())
    }

    /// Mark a trashed document for asynchronous object-store garbage
    /// collection after all cross-context preconditions have been checked.
    pub fn request_purge(
        &mut self,
        retention_released: bool,
        has_business_reference: bool,
        has_active_hold: bool,
    ) -> Result<(), DocumentDomainError> {
        if self.deletion_state != DocumentDeletionState::Trashed {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "request_purge",
                status: self.status,
            });
        }
        if !retention_released {
            return Err(DocumentDomainError::PurgeRetentionNotReleased);
        }
        if has_business_reference {
            return Err(DocumentDomainError::PurgeReferenced);
        }
        if has_active_hold {
            return Err(DocumentDomainError::PurgeHeld);
        }
        self.bump_version()?;
        self.deletion_state = DocumentDeletionState::PendingPurge;
        Ok(())
    }

    /// Complete the database side of an idempotent asynchronous purge after
    /// the storage GC worker has deleted and verified all source objects.
    pub fn complete_purge(&mut self) -> Result<(), DocumentDomainError> {
        if self.deletion_state != DocumentDeletionState::PendingPurge {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "complete_purge",
                status: self.status,
            });
        }
        self.bump_version()?;
        self.deletion_state = DocumentDeletionState::Purged;
        Ok(())
    }

    /// Soft-delete an active or archived document.
    pub fn mark_deleted(&mut self) -> Result<(), DocumentDomainError> {
        self.trash()
    }

    /// Replace the file content while preserving lifecycle state.
    pub fn replace_content(&mut self, logical_path: String) -> Result<(), DocumentDomainError> {
        let _ = self.replace_content_revision(logical_path, None)?;
        Ok(())
    }

    /// Create and select a new immutable content revision.
    pub fn replace_content_revision(
        &mut self,
        logical_path: String,
        change_reason: Option<String>,
    ) -> Result<super::revision::DocumentRevision, DocumentDomainError> {
        self.replace_content_revision_with_sha256(logical_path, change_reason, None)
    }

    /// Create and select a new immutable content revision with a verified
    /// source checksum.
    pub fn replace_content_revision_with_sha256(
        &mut self,
        _logical_path: String,
        change_reason: Option<String>,
        sha256: Option<String>,
    ) -> Result<super::revision::DocumentRevision, DocumentDomainError> {
        if !matches!(self.deletion_state, DocumentDeletionState::Present) {
            return Err(DocumentDomainError::InvalidStatusTransition {
                operation: "replace_content",
                status: self.status,
            });
        }
        let previous_revision_id = self.current_revision_id;
        let content_revision = self
            .content_revision
            .increment()
            .map_err(|_| DocumentDomainError::InvalidContentRevision)?;
        let revision_id = Uuid::now_v7();
        let content_reference = DocumentContentReference::new_revision(
            self.tenant_id,
            self.id,
            revision_id,
            content_revision,
        )
        .map_err(|error| DocumentDomainError::InvalidObjectKey(error.to_string()))?;
        let revision = super::revision::DocumentRevision::new(
            revision_id,
            self.tenant_id,
            self.id,
            content_revision.value(),
            Some(previous_revision_id),
            content_reference.as_storage_key(),
            sha256,
            self.content_type.clone(),
            self.size_bytes,
            self.original_filename.clone(),
            self.created_by,
            Utc::now(),
            change_reason,
        )
        .map_err(|_| DocumentDomainError::InvalidObjectKey("invalid revision".to_string()))?;
        self.bump_version()?;
        self.content_revision = content_revision;
        self.content_reference = content_reference;
        self.current_revision_id = revision_id;
        Ok(revision)
    }

    fn bump_version(&mut self) -> Result<(), DocumentDomainError> {
        self.aggregate_version = self
            .aggregate_version
            .increment()
            .map_err(|_| DocumentDomainError::InvalidVersion)?;
        if self.aggregate_version.value() <= 0 {
            return Err(DocumentDomainError::InvalidVersion);
        }
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn create_with_valid_data_succeeds() {
        let doc = DocumentMetadata::create(
            Uuid::now_v7(),
            "report.pdf".to_string(),
            "application/pdf".to_string(),
            "incoming/report.pdf".to_string(),
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
    fn deterministic_constructor_preserves_timestamp_and_revision_key() {
        let tenant_id = Uuid::now_v7();
        let document_id = Uuid::now_v7();
        let revision_id = Uuid::now_v7();
        let created_at = Utc
            .timestamp_opt(1_735_689_600, 0)
            .single()
            .unwrap_or_else(|| unreachable!());
        let document = DocumentMetadata::create_with_revision_id_at(
            document_id,
            tenant_id,
            "report.pdf".to_string(),
            "application/pdf".to_string(),
            "incoming/report.pdf".to_string(),
            Uuid::now_v7(),
            Some(12),
            revision_id,
            created_at,
        )
        .unwrap_or_else(|_| unreachable!());

        assert_eq!(document.created_at(), created_at);
        assert_eq!(document.updated_at(), created_at);
        assert_eq!(
            document.object_key(),
            format!("tenants/{tenant_id}/documents/{document_id}/revisions/{revision_id}/source")
        );
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
    fn lifecycle_version_does_not_change_content_revision() {
        let mut document = DocumentMetadata::create(
            Uuid::now_v7(),
            "report.txt".to_string(),
            "text/plain".to_string(),
            "report.txt".to_string(),
            Uuid::now_v7(),
            None,
        )
        .unwrap_or_else(|_| unreachable!());
        let reference = document.object_key();
        assert_eq!(document.content_revision().value(), 1);
        document.archive().unwrap_or_else(|_| unreachable!());
        document.restore().unwrap_or_else(|_| unreachable!());
        assert_eq!(document.content_revision().value(), 1);
        assert_eq!(document.object_key(), reference);
        document
            .replace_content("replacement.txt".to_string())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(document.content_revision().value(), 2);
        assert_ne!(document.object_key(), reference);
    }

    #[test]
    fn rehydrate_rejects_invalid_persisted_state() {
        let now = Utc::now();
        let invalid = RehydrateDocumentMetadata {
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            original_filename: "report.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            object_key: "not-a-storage-key".to_string(),
            status: DocumentStatus::Active,
            aggregate_version: AggregateVersion::new(0)
                .unwrap_or_else(|_| AggregateVersion::new(1).unwrap_or_else(|_| unreachable!())),
            content_revision: ContentRevision::new(1).unwrap_or_else(|_| unreachable!()),
            current_revision_id: Uuid::now_v7(),
            lifecycle_state: DocumentLifecycleState::Active,
            deletion_state: DocumentDeletionState::Present,
            pre_trash_lifecycle: DocumentLifecycleState::Active,
            size_bytes: Some(-1),
            created_by: Uuid::now_v7(),
            created_at: now,
            updated_at: now - chrono::Duration::seconds(1),
        };
        assert!(DocumentMetadata::rehydrate(invalid).is_err());
    }
}
