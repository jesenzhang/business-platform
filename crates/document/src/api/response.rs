//! Response DTOs for the document API.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::DocumentMetadata;

/// Response representation of a document metadata record.
#[derive(Debug, Serialize)]
pub struct DocumentResponse {
    /// Document ID.
    pub id: Uuid,
    /// Owning tenant ID.
    pub tenant_id: Uuid,
    /// Original filename.
    pub original_filename: String,
    /// MIME content type.
    pub content_type: String,
    /// Object storage key.
    pub object_key: String,
    /// Lifecycle status.
    pub status: String,
    /// Optimistic locking version.
    pub version: i64,
    /// File size in bytes (may be null).
    pub size_bytes: Option<i64>,
    /// User who created the document.
    pub created_by: Uuid,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<DocumentMetadata> for DocumentResponse {
    fn from(doc: DocumentMetadata) -> Self {
        Self {
            id: doc.id,
            tenant_id: doc.tenant_id,
            original_filename: doc.original_filename,
            content_type: doc.content_type,
            object_key: doc.object_key,
            status: doc.status.as_str().to_string(),
            version: doc.version,
            size_bytes: doc.size_bytes,
            created_by: doc.created_by,
            created_at: doc.created_at,
            updated_at: doc.updated_at,
        }
    }
}
