//! Infrastructure layer: persistence adapters.

pub mod postgres;

pub use postgres::PostgresDocumentRepository;
