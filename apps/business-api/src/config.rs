use runtime_config::{
    load_process_config, ConfigLoadError, ConfigValidationError, RuntimeEnvironment, Secret,
    SecretUrl,
};
use serde::Deserialize;
use std::collections::BTreeSet;

/// Process-local configuration for the HTTP business API composition root.
#[derive(Debug, Clone, Deserialize)]
pub struct BusinessApiConfig {
    #[serde(default)]
    pub env: RuntimeEnvironment,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub cors_origins: Vec<String>,
    #[serde(default = "default_body_limit_bytes")]
    pub body_limit_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub backend: DatabaseBackend,
    pub url: SecretUrl,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseBackend {
    #[default]
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    #[default]
    S3,
    Local,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub backend: StorageBackend,
    #[serde(default = "default_storage_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_storage_access_key")]
    pub access_key: String,
    #[serde(default = "default_storage_secret_key")]
    pub secret_key: String,
    #[serde(default = "default_storage_region")]
    pub region: String,
    #[serde(default = "default_storage_bucket")]
    pub bucket: String,
    #[serde(default = "default_storage_local_path")]
    pub local_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::S3,
            endpoint: default_storage_endpoint(),
            access_key: default_storage_access_key(),
            secret_key: default_storage_secret_key(),
            region: default_storage_region(),
            bucket: default_storage_bucket(),
            local_path: default_storage_local_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub issuer_url: String,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub dev_secret: Option<Secret<String>>,
    #[serde(default)]
    pub dev_auth_enabled: bool,
    /// Server-side development grants; never sourced from request headers.
    #[serde(default)]
    pub dev_permissions: BTreeSet<String>,
    #[serde(default)]
    pub dev_tenant_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub dev_user_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub dev_subject: Option<String>,
    #[serde(default)]
    pub dev_roles: BTreeSet<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            service_name: default_service_name(),
            otlp_endpoint: None,
            log_level: default_log_level(),
        }
    }
}

impl BusinessApiConfig {
    pub fn load() -> Result<Self, ConfigLoadError> {
        load_process_config("BUSINESS_API")
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        let mut messages = Vec::new();
        if self.server.port == 0 {
            messages.push("server.port must be > 0".to_string());
        }
        if self.server.request_timeout_secs == 0 {
            messages.push("server.request_timeout_secs must be > 0".to_string());
        }
        if self.server.body_limit_bytes == 0 {
            messages.push("server.body_limit_bytes must be > 0".to_string());
        }
        if self.storage.backend == StorageBackend::S3
            && (self.storage.endpoint.trim().is_empty() || self.storage.bucket.trim().is_empty())
        {
            messages.push("storage endpoint and bucket must be configured".to_string());
        }
        if self.env == RuntimeEnvironment::Production
            && self.storage.backend == StorageBackend::Local
        {
            messages.push("storage.backend must not be local in production".to_string());
        }
        if self.database.max_connections == 0 {
            messages.push("database.max_connections must be > 0".to_string());
        } else if self.database.backend == DatabaseBackend::Sqlite
            && self.database.max_connections > document_sqlite::MAX_SQLITE_CONNECTIONS
        {
            messages.push(format!(
                "database.max_connections must be <= {} for sqlite",
                document_sqlite::MAX_SQLITE_CONNECTIONS
            ));
        } else if self.database.backend == DatabaseBackend::Postgres
            && self.database.max_connections > 200
        {
            messages.push("database.max_connections must be <= 200 for postgres".to_string());
        }
        if self.database.acquire_timeout_secs == 0 {
            messages.push("database.acquire_timeout_secs must be > 0".to_string());
        }
        if self.auth.dev_auth_enabled {
            if self
                .auth
                .dev_secret
                .as_ref()
                .is_none_or(|secret| secret.expose().trim().is_empty())
            {
                messages.push(
                    "auth.dev_secret must be configured when dev auth is enabled".to_string(),
                );
            }
            if self.auth.dev_tenant_id.is_none_or(|id| id.is_nil()) {
                messages.push(
                    "auth.dev_tenant_id must be a non-nil UUID when dev auth is enabled"
                        .to_string(),
                );
            }
            if self.auth.dev_user_id.is_none_or(|id| id.is_nil()) {
                messages.push(
                    "auth.dev_user_id must be a non-nil UUID when dev auth is enabled".to_string(),
                );
            }
            if self
                .auth
                .dev_subject
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                messages.push(
                    "auth.dev_subject must be configured when dev auth is enabled".to_string(),
                );
            }
            if self
                .auth
                .dev_roles
                .iter()
                .any(|value| value.trim().is_empty())
            {
                messages.push("auth.dev_roles must not contain empty values".to_string());
            }
        }
        let scheme = self.database.url.expose().split(':').next();
        match self.database.backend {
            DatabaseBackend::Postgres if !matches!(scheme, Some("postgres" | "postgresql")) => {
                messages.push(
                    "database.url must use a PostgreSQL scheme for postgres backend".to_string(),
                );
            }
            DatabaseBackend::Sqlite if scheme != Some("sqlite") => {
                messages
                    .push("database.url must use a SQLite scheme for sqlite backend".to_string());
            }
            _ => {}
        }
        if self.env == RuntimeEnvironment::Production {
            if self.database.backend == DatabaseBackend::Sqlite {
                messages.push("database.backend must not be sqlite in production".to_string());
            }
            if self.auth.dev_auth_enabled {
                messages.push("auth.dev_auth_enabled must be false in production".to_string());
            }
            if self.auth.dev_secret.is_some() {
                messages.push("auth.dev_secret must be absent in production".to_string());
            }
            if self.auth.dev_tenant_id.is_some()
                || self.auth.dev_user_id.is_some()
                || self.auth.dev_subject.is_some()
                || !self.auth.dev_roles.is_empty()
                || !self.auth.dev_permissions.is_empty()
            {
                messages.push("development identity must be absent in production".to_string());
            }
            if self.auth.issuer_url.trim().is_empty() {
                messages.push("auth.issuer_url must not be empty in production".to_string());
            }
            if self.server.cors_origins.iter().any(|origin| origin == "*") {
                messages.push("server.cors_origins must not contain * in production".to_string());
            }
        }
        if messages.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError { messages })
        }
    }
}

fn default_storage_endpoint() -> String {
    "http://localhost:9000".to_string()
}
fn default_storage_access_key() -> String {
    "minioadmin".to_string()
}
fn default_storage_secret_key() -> String {
    "minioadmin".to_string()
}
fn default_storage_region() -> String {
    "us-east-1".to_string()
}
fn default_storage_bucket() -> String {
    "enterprise-documents".to_string()
}
fn default_storage_local_path() -> String {
    ".data/storage".to_string()
}
fn default_host() -> String {
    "0.0.0.0".to_string()
}

const fn default_port() -> u16 {
    3000
}

const fn default_request_timeout_secs() -> u64 {
    30
}

const fn default_body_limit_bytes() -> usize {
    10 * 1024 * 1024
}

const fn default_max_connections() -> u32 {
    20
}

const fn default_min_connections() -> u32 {
    2
}

const fn default_acquire_timeout_secs() -> u64 {
    10
}

fn default_service_name() -> String {
    "business-api".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATABASE_PASSWORD: &str = "DO_NOT_LEAK_DATABASE_PASSWORD";

    fn valid_config() -> BusinessApiConfig {
        BusinessApiConfig {
            env: RuntimeEnvironment::Development,
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                request_timeout_secs: 30,
                cors_origins: Vec::new(),
                body_limit_bytes: 1024,
            },
            database: DatabaseConfig {
                backend: DatabaseBackend::Postgres,
                url: SecretUrl::parse(&format!(
                    "postgres://user:{DATABASE_PASSWORD}@db.internal/platform"
                ))
                .unwrap_or_else(|_| unreachable!()),
                max_connections: 10,
                min_connections: 0,
                acquire_timeout_secs: 1,
            },
            storage: StorageConfig::default(),
            auth: AuthConfig {
                issuer_url: String::new(),
                audience: None,
                dev_secret: None,
                dev_auth_enabled: false,
                dev_permissions: BTreeSet::new(),
                dev_tenant_id: None,
                dev_user_id: None,
                dev_subject: None,
                dev_roles: BTreeSet::new(),
            },
            observability: ObservabilityConfig::default(),
        }
    }

    #[test]
    fn api_configuration_does_not_require_storage_or_messaging() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn production_rejects_development_auth_and_wildcard_cors_without_secret_leakage() {
        let mut config = valid_config();
        config.env = RuntimeEnvironment::Production;
        config.auth.dev_auth_enabled = true;
        config.auth.dev_secret = Some(Secret::new("development-token".to_string()));
        config.server.cors_origins = vec!["*".to_string()];

        let Err(error) = config.validate() else {
            unreachable!();
        };
        let rendered = error.to_string();
        assert!(rendered.contains("auth.dev_auth_enabled"));
        assert!(rendered.contains("auth.dev_secret"));
        assert!(rendered.contains("server.cors_origins"));
        assert!(!rendered.contains(DATABASE_PASSWORD));
        assert!(!rendered.contains("development-token"));
    }

    #[test]
    fn production_rejects_sqlite_before_connecting() {
        let mut config = valid_config();
        config.env = RuntimeEnvironment::Production;
        config.database.backend = DatabaseBackend::Sqlite;
        config.database.url = SecretUrl::parse("sqlite://data/business-platform.db")
            .unwrap_or_else(|_| unreachable!());
        config.auth.issuer_url = "https://identity.example.test".to_string();
        let Err(error) = config.validate() else {
            unreachable!()
        };
        assert!(error.to_string().contains("database.backend"));
    }

    #[test]
    fn sqlite_pool_limit_is_fail_closed() {
        let mut config = valid_config();
        config.database.backend = DatabaseBackend::Sqlite;
        config.database.url = SecretUrl::parse("sqlite://data/business-platform.db")
            .unwrap_or_else(|_| unreachable!());
        config.database.max_connections = document_sqlite::MAX_SQLITE_CONNECTIONS + 1;
        let Err(error) = config.validate() else {
            unreachable!();
        };
        assert!(error.to_string().contains("max_connections"));
    }
}
