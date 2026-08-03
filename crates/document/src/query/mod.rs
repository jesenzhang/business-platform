//! Database-neutral query-side contracts and read models.

mod detail;
mod list;
mod model;
mod search;

pub use detail::DocumentDetailQuery;
pub use list::{DocumentListQuery, DocumentListRequest};
pub use model::{
    DocumentDetailView, DocumentListCursor, DocumentListFilter, DocumentListItem, DocumentListPage,
    DocumentStatusFilter, DocumentStatusView, QueryError,
};
pub use search::{DocumentSearchQuery, DocumentSearchRequest};
