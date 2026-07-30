use serde::Deserialize;

use crate::secret::Secret;

/// 应用运行环境
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppEnv {
    #[default]
    Development,
    Production,
}

/// 应用全局配置，从 config/ 目录和环境变量加载
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub env: AppEnv,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub messaging: MessagingConfig,
    pub observability: ObservabilityConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub endpoint: String,
    pub access_key: Secret<String>,
    pub secret_key: Secret<String>,
    #[serde(default = "default_region")]
    pub region: String,
    /// Bucket configuration per purpose
    #[serde(default)]
    pub buckets: BucketConfig,
}

/// Per-purpose bucket names
#[derive(Debug, Clone, Deserialize)]
pub struct BucketConfig {
    #[serde(default = "default_documents_bucket")]
    pub documents: String,
    #[serde(default = "default_temp_bucket")]
    pub temp: String,
    #[serde(default = "default_exports_bucket")]
    pub exports: String,
    #[serde(default = "default_backups_bucket")]
    pub backups: String,
}

impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            documents: default_documents_bucket(),
            temp: default_temp_bucket(),
            exports: default_exports_bucket(),
            backups: default_backups_bucket(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagingConfig {
    #[serde(default = "default_nats_url")]
    pub nats_url: String,
    #[serde(default)]
    pub enabled: bool,
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

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// OIDC Issuer URL
    #[serde(default)]
    pub issuer_url: String,
    /// 预期 audience
    #[serde(default)]
    pub audience: Option<String>,
    /// JWT 密钥（开发环境用，生产环境应使用 OIDC）
    #[serde(default)]
    pub dev_secret: Option<Secret<String>>,
}

/// Configuration validation errors collected during `AppConfig::validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationError {
    pub messages: Vec<String>,
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "configuration validation failed:")?;
        for msg in &self.messages {
            write!(f, "\n  - {msg}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigValidationError {}

impl AppConfig {
    /// 从配置文件和环境变量加载配置
    ///
    /// 优先级：环境变量 > config/{env}.toml > config/default.toml
    pub fn load() -> Result<Self, config::ConfigError> {
        let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

        let settings = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name(&format!("config/{env}")).required(false))
            .add_source(
                config::Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        settings.try_deserialize()
    }

    /// Validate configuration invariants.
    ///
    /// Checks structural constraints and environment-specific security rules.
    /// In Production, `auth.dev_secret` must be `None` (fail-closed) and
    /// `auth.issuer_url` must not be empty.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        let mut messages = Vec::new();

        // Database checks
        if self.database.url.is_empty() {
            messages.push("database.url must not be empty".to_string());
        } else if !self.database.url.starts_with("postgres://")
            && !self.database.url.starts_with("postgresql://")
        {
            messages.push(
                "database.url must start with \"postgres://\" or \"postgresql://\"".to_string(),
            );
        }

        if self.database.max_connections == 0 || self.database.max_connections > 200 {
            messages.push("database.max_connections must be > 0 and <= 200".to_string());
        }

        if self.database.acquire_timeout_secs == 0 {
            messages.push("database.acquire_timeout_secs must be > 0".to_string());
        }

        // Server checks
        if self.server.port == 0 {
            messages.push("server.port must be > 0".to_string());
        }

        // Storage checks
        if self.storage.endpoint.is_empty() {
            messages.push("storage.endpoint must not be empty".to_string());
        }

        if self.storage.buckets.documents.is_empty() {
            messages.push("storage.buckets.documents must not be empty".to_string());
        }
        if self.storage.buckets.temp.is_empty() {
            messages.push("storage.buckets.temp must not be empty".to_string());
        }
        if self.storage.buckets.exports.is_empty() {
            messages.push("storage.buckets.exports must not be empty".to_string());
        }
        if self.storage.buckets.backups.is_empty() {
            messages.push("storage.buckets.backups must not be empty".to_string());
        }

        // Environment-specific security checks
        if self.env == AppEnv::Production {
            if self.auth.dev_secret.is_some() {
                messages
                    .push("auth.dev_secret must be None in production (fail-closed)".to_string());
            }
            if self.auth.issuer_url.is_empty() {
                messages.push("auth.issuer_url must not be empty in production".to_string());
            }
        }

        if messages.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError { messages })
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_request_timeout_secs() -> u64 {
    30
}

fn default_max_connections() -> u32 {
    20
}

fn default_min_connections() -> u32 {
    2
}

fn default_acquire_timeout_secs() -> u64 {
    10
}

fn default_region() -> String {
    "us-east-1".to_string()
}

fn default_documents_bucket() -> String {
    "enterprise-documents".to_string()
}

fn default_temp_bucket() -> String {
    "enterprise-temp".to_string()
}

fn default_exports_bucket() -> String {
    "enterprise-exports".to_string()
}

fn default_backups_bucket() -> String {
    "enterprise-backups".to_string()
}

fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_service_name() -> String {
    "business-api".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn valid_config() -> AppConfig {
        AppConfig {
            env: AppEnv::Development,
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 3000,
                request_timeout_secs: 30,
            },
            database: DatabaseConfig {
                url: "postgres://user:pass@localhost:5432/db".to_string(),
                max_connections: 20,
                min_connections: 2,
                acquire_timeout_secs: 10,
            },
            storage: StorageConfig {
                endpoint: "http://localhost:9000".to_string(),
                access_key: Secret::new("minioadmin".to_string()),
                secret_key: Secret::new("minioadmin".to_string()),
                region: "us-east-1".to_string(),
                buckets: BucketConfig::default(),
            },
            messaging: MessagingConfig {
                nats_url: "nats://localhost:4222".to_string(),
                enabled: false,
            },
            observability: ObservabilityConfig {
                service_name: "test".to_string(),
                otlp_endpoint: None,
                log_level: "info".to_string(),
            },
            auth: AuthConfig {
                issuer_url: "http://localhost:8080/realms/test".to_string(),
                audience: None,
                dev_secret: Some(Secret::new("dev-only".to_string())),
            },
        }
    }

    #[test]
    fn valid_development_config_passes() {
        let config = valid_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn empty_database_url_fails() {
        let mut config = valid_config();
        config.database.url = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("database.url")));
    }

    #[test]
    fn invalid_database_url_scheme_fails() {
        let mut config = valid_config();
        config.database.url = "mysql://localhost/db".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("database.url")));
    }

    #[test]
    fn zero_max_connections_fails() {
        let mut config = valid_config();
        config.database.max_connections = 0;
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("max_connections")));
    }

    #[test]
    fn excessive_max_connections_fails() {
        let mut config = valid_config();
        config.database.max_connections = 201;
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("max_connections")));
    }

    #[test]
    fn zero_acquire_timeout_fails() {
        let mut config = valid_config();
        config.database.acquire_timeout_secs = 0;
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("acquire_timeout")));
    }

    #[test]
    fn zero_port_fails() {
        let mut config = valid_config();
        config.server.port = 0;
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("server.port")));
    }

    #[test]
    fn empty_storage_endpoint_fails() {
        let mut config = valid_config();
        config.storage.endpoint = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("storage.endpoint")));
    }

    #[test]
    fn production_with_dev_secret_fails() {
        let mut config = valid_config();
        config.env = AppEnv::Production;
        config.auth.dev_secret = Some(Secret::new("should-not-exist".to_string()));
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("dev_secret")));
    }

    #[test]
    fn production_without_issuer_url_fails() {
        let mut config = valid_config();
        config.env = AppEnv::Production;
        config.auth.dev_secret = None;
        config.auth.issuer_url = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("issuer_url")));
    }

    #[test]
    fn production_valid_config_passes() {
        let mut config = valid_config();
        config.env = AppEnv::Production;
        config.auth.dev_secret = None;
        config.auth.issuer_url = "https://auth.example.com/realms/prod".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validation_error_display() {
        let err = ConfigValidationError {
            messages: vec!["error one".to_string(), "error two".to_string()],
        };
        let display = format!("{err}");
        assert!(display.contains("error one"));
        assert!(display.contains("error two"));
    }
}
