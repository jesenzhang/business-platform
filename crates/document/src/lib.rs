//! Framework-free Document Management bounded context.
//!
//! HTTP, `SQLx`, object storage, and message broker adapters live outside this
//! crate. This crate contains only business data, invariants, and application
//! ports so it can be tested without a runtime or external services.

pub mod application;
pub mod domain;
pub mod ports;
pub mod query;
