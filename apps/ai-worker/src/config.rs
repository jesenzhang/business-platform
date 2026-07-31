use runtime_config::{load_process_config, ConfigLoadError, RuntimeEnvironment};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AiWorkerConfig {
    #[serde(default)]
    pub env: RuntimeEnvironment,
    #[serde(default)]
    pub observability: ObservabilityConfig,
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

impl AiWorkerConfig {
    pub fn load() -> Result<Self, ConfigLoadError> {
        load_process_config("AI_WORKER")
    }
}

fn default_log_level() -> String {
    "info".to_string()
}
