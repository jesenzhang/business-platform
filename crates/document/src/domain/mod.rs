//! Document domain layer.
//!
//! Contains the aggregate root, domain errors, and repository port.

pub mod entity;
pub mod error;
pub mod object_key;
pub mod repository;
pub mod version;

pub use entity::{
    DocumentMetadata, DocumentStatus, DocumentStatusParseError, RehydrateDocumentMetadata,
};
pub use error::DocumentDomainError;
pub use object_key::{ContentReferenceError, DocumentContentReference};
pub use repository::{DocumentRepository, RepositoryError};
pub use version::{AggregateVersion, ContentRevision, InvalidVersionValue};
