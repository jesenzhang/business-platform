//! business-api library surface.
//!
//! Exposes the HTTP composition (router, auth middleware, shared state) so it
//! can be exercised by integration tests and reused by the binary entrypoint.
//! The binary (`main.rs`) only wires configuration, infrastructure and process
//! lifecycle around this composition.

pub mod api_error;
pub mod api_response;
pub mod auth;
pub mod routes;
pub mod state;
