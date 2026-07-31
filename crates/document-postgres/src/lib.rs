//! `PostgreSQL` adapters for the Document Management bounded context.

mod repository;
mod unit_of_work;

pub use repository::PostgresDocumentQueryRepository;
pub use unit_of_work::PostgresCreateDocumentUnitOfWork;
