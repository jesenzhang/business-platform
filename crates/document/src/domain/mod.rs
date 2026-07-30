//! Document domain layer.
//!
//! Contains the aggregate root, domain errors, and repository port.

pub mod entity;
pub mod error;
pub mod repository;

pub use entity::{DocumentMetadata, DocumentStatus};
pub use error::DocumentDomainError;
pub use repository::DocumentRepository;
