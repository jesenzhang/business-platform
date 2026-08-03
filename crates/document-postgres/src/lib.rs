//! `PostgreSQL` adapters for the Document Management bounded context.

mod detail_query;
mod list_query;
mod query_mapper;
mod unit_of_work;

pub use detail_query::PostgresDocumentDetailQuery;
pub use list_query::PostgresDocumentListQuery;
pub use unit_of_work::PostgresCreateDocumentUnitOfWork;
