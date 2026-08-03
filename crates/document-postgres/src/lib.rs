//! `PostgreSQL` adapters for the Document Management bounded context.

mod detail_query;
mod list_query;
mod query_mapper;
mod repository;
mod search_query;
mod unit_of_work;

pub use detail_query::PostgresDocumentDetailQuery;
pub use list_query::PostgresDocumentListQuery;
pub use repository::PostgresDocumentQueryRepository;
pub use search_query::PostgresDocumentSearchQuery;
pub use unit_of_work::PostgresCreateDocumentUnitOfWork;
