//! `PostgreSQL` adapters for the Document Management bounded context.

mod detail_query;
mod list_query;
mod query_mapper;
mod repository;
mod unit_of_work;

pub use detail_query::PostgresDocumentDetailQuery;
pub use list_query::PostgresDocumentListQuery;
pub use repository::PostgresDocumentQueryRepository;
pub use unit_of_work::PostgresCreateDocumentUnitOfWork;
