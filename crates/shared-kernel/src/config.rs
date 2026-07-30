use serde::Deserialize;

/// 应用全局配置，从 config/ 目录和环境变量加载
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
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
    pub access_key: String,
    pub secret_key: String,
    #[serde(default = "default_bucket")]
    pub bucket: String,
    #[serde(default = "default_region")]
    pub region: String,
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
    pub issuer_url: String,
    /// 预期 audience
    #[serde(default)]
    pub audience: Option<String>,
    /// JWT 密钥（开发环境用，生产环境应使用 OIDC）
    #[serde(default)]
    pub dev_secret: Option<String>,
}

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

fn default_bucket() -> String {
    "enterprise-platform".to_string()
}

fn default_region() -> String {
    "us-east-1".to_string()
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
