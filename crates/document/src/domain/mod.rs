//! Document domain layer.
//!
//! Contains the aggregate root, domain errors, and repository port.

pub mod entity;
pub mod error;
pub mod object_key;
pub mod repository;

pub use entity::{DocumentMetadata, DocumentStatus};
pub use error::DocumentDomainError;
pub use object_key::{DocumentObjectKey, DocumentObjectKeyError};
pub use repository::{DocumentPage, DocumentQueryRepository, ListDocumentsQuery, RepositoryError};
