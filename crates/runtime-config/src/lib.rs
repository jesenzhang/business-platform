//! Runtime-only configuration support shared by process composition roots.
//!
//! This crate intentionally contains no process configuration root and no
//! infrastructure client. Applications own their configuration shape and use
//! these types only for secret-safe parsing and rendering.

mod environment;
mod loader;
mod secret;
mod secret_url;

pub use environment::RuntimeEnvironment;
pub use loader::{load_process_config, ConfigLoadError, ConfigValidationError};
pub use secret::Secret;
pub use secret_url::{SecretUrl, SecretUrlParseError};
