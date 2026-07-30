//! API layer: HTTP handlers, request/response DTOs, and router.

pub mod handler;
pub mod request;
pub mod response;

use axum::routing::{get, post};
use axum::Router;

pub use handler::DocumentServices;

/// Create the document routes router.
///
/// Returns a fully-resolved `Router<()>` that can be nested into any
/// parent router regardless of its state type.
pub fn router(services: DocumentServices) -> Router<()> {
    Router::new()
        .route(
            "/",
            post(handler::create_document).get(handler::list_documents),
        )
        .route("/{id}", get(handler::get_document))
        .with_state(services)
}
