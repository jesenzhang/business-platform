use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::RuntimeEnvironment;

/// A safe, field-oriented validation error for process configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationError {
    pub messages: Vec<String>,
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("configuration validation failed")?;
        for message in &self.messages {
            write!(formatter, "\n  - {message}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigValidationError {}

/// Error returned without including values from configuration sources.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("configuration loading failed")]
pub struct ConfigLoadError;

/// Load one process configuration root from the common physical configuration
/// files and its own environment-variable prefix.
///
/// The process prefix must be uppercase with underscores, for example
/// `BUSINESS_API`; values then use `BUSINESS_API__DATABASE__URL`.
pub fn load_process_config<T>(prefix: &str) -> Result<T, ConfigLoadError>
where
    T: DeserializeOwned,
{
    let environment = read_environment(prefix);
    let environment_source = config::Environment::with_prefix(prefix)
        .separator("__")
        .try_parsing(true)
        .list_separator(",")
        .with_list_parse_key("server.cors_origins")
        .with_list_parse_key("auth.dev_permissions")
        .with_list_parse_key("auth.dev_roles");
    let settings = config::Config::builder()
        .add_source(config::File::with_name("config/default").required(false))
        .add_source(
            config::File::with_name(&format!("config/{}", environment.config_name()))
                .required(false),
        )
        .add_source(environment_source)
        .build()
        .map_err(|_| ConfigLoadError)?;
    settings.try_deserialize().map_err(|_| ConfigLoadError)
}

fn read_environment(prefix: &str) -> RuntimeEnvironment {
    let key = format!("{prefix}__ENV");
    let process_environment = std::env::var(key).ok();
    let legacy_environment = std::env::var("APP_ENV").ok();
    match process_environment
        .as_deref()
        .or(legacy_environment.as_deref())
    {
        Some("production") => RuntimeEnvironment::Production,
        _ => RuntimeEnvironment::Development,
    }
}
