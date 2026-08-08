//! Document domain layer.
//!
//! Contains the aggregate root, domain errors, and repository port.

pub mod entity;
pub mod error;
pub mod link;
pub mod object_key;
pub mod repository;
pub mod revision;
pub mod version;

pub use entity::{
    DocumentDeletionState, DocumentLifecycleState, DocumentMetadata, DocumentStatus,
    DocumentStatusParseError, RehydrateDocumentMetadata,
};
pub use error::DocumentDomainError;
pub use link::{DocumentLink, DocumentLinkError, DocumentLinkRole, DocumentResourceKind};
pub use object_key::{ContentReferenceError, DocumentContentReference};
pub use repository::{DocumentRepository, RepositoryError};
pub use revision::{DocumentRevision, DocumentRevisionError};
pub use version::{AggregateVersion, ContentRevision, InvalidVersionValue};
