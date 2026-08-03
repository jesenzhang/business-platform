//! Database-neutral query-side contracts and read models.

mod detail;
mod list;
mod model;

pub use detail::DocumentDetailQuery;
pub use list::{DocumentListQuery, DocumentListRequest};
pub use model::{
    escape_like_literal, DocumentDetailView, DocumentListCursor, DocumentListFilter,
    DocumentListItem, DocumentListPage, DocumentStatusFilter, DocumentStatusView, QueryError,
};
