use runtime_config::{load_process_config, ConfigLoadError, RuntimeEnvironment};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentAdapterConfig {
    #[serde(default)]
    pub env: RuntimeEnvironment,
    #[serde(default)]
    pub server: ServerConfig,
    pub business_api: BusinessApiConfig,
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BusinessApiConfig {
    #[serde(default = "default_api_url")]
    pub base_url: String,
    pub bearer_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub bearer_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            otlp_endpoint: None,
        }
    }
}

impl AgentAdapterConfig {
    pub fn load() -> Result<Self, ConfigLoadError> {
        load_process_config("AGENT_ADAPTER")
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
const fn default_port() -> u16 {
    3100
}
fn default_api_url() -> String {
    "http://localhost:3000".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}
