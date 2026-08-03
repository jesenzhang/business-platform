use runtime_config::{load_process_config, ConfigLoadError, RuntimeEnvironment, Secret, SecretUrl};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiMode {
    #[default]
    Separate,
    Inline,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkerDatabaseBackend {
    #[default]
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerDatabaseConfig {
    #[serde(default)]
    pub backend: WorkerDatabaseBackend,
    pub url: Option<SecretUrl>,
}

impl Default for WorkerDatabaseConfig {
    fn default() -> Self {
        Self {
            backend: WorkerDatabaseBackend::Postgres,
            url: None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerStorageConfig {
    #[serde(default = "default_storage_backend")]
    pub backend: String,
    #[serde(default)]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub endpoint: Option<SecretUrl>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<Secret<String>>,
    #[serde(default = "default_region")]
    pub region: String,
}

impl Default for WorkerStorageConfig {
    fn default() -> Self {
        Self {
            backend: default_storage_backend(),
            base_dir: None,
            endpoint: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            region: default_region(),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct BusinessWorkerConfig {
    #[serde(default)]
    pub env: RuntimeEnvironment,
    #[serde(default)]
    pub database: WorkerDatabaseConfig,
    #[serde(default)]
    pub storage: WorkerStorageConfig,
    #[serde(default = "default_worker_id")]
    pub worker_id: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default = "default_lease_duration")]
    pub lease_duration_secs: i64,
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: i64,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_millis: u64,
    #[serde(default = "default_max_content_bytes")]
    pub max_content_bytes: usize,
    /// Test-only deterministic delay used by the process crash-recovery E2E.
    #[serde(default)]
    pub test_step_delay_millis: u64,
    #[serde(default)]
    pub ai_mode: AiMode,
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

impl BusinessWorkerConfig {
    pub fn load() -> Result<Self, ConfigLoadError> {
        load_process_config("BUSINESS_WORKER")
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.worker_id.trim().is_empty() {
            return Err("worker_id must not be empty".to_string());
        }
        if self.concurrency == 0 {
            return Err("concurrency must be positive".to_string());
        }
        if self.lease_duration_secs <= 0
            || self.heartbeat_interval_secs <= 0
            || self.heartbeat_interval_secs * 2 >= self.lease_duration_secs
        {
            return Err("heartbeat interval must be less than half the lease duration".to_string());
        }
        let sqlite = self.database.backend == WorkerDatabaseBackend::Sqlite
            || self
                .database
                .url
                .as_ref()
                .is_some_and(|url| url.expose().starts_with("sqlite:"));
        if sqlite && self.concurrency != 1 {
            return Err("SQLite worker concurrency must be exactly 1".to_string());
        }
        if sqlite && self.ai_mode == AiMode::Separate {
            return Err("SQLite only supports inline AI mode".to_string());
        }
        if sqlite && self.env == RuntimeEnvironment::Production {
            return Err("SQLite is local-only and cannot be used in production".to_string());
        }
        if self.max_content_bytes == 0 {
            return Err("max_content_bytes must be positive".to_string());
        }
        if self.env == RuntimeEnvironment::Production && self.test_step_delay_millis != 0 {
            return Err("test_step_delay_millis must be zero in production".to_string());
        }
        Ok(())
    }
}

fn default_storage_backend() -> String {
    "local".to_string()
}
fn default_region() -> String {
    "us-east-1".to_string()
}
fn default_worker_id() -> String {
    format!("business-worker-{}", std::process::id())
}
const fn default_concurrency() -> u32 {
    1
}
const fn default_lease_duration() -> i64 {
    30
}
const fn default_heartbeat_interval() -> i64 {
    10
}
const fn default_poll_interval() -> u64 {
    500
}
const fn default_max_content_bytes() -> usize {
    16 * 1024 * 1024
}
fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_rejects_separate_ai_and_parallelism() {
        let mut config = BusinessWorkerConfig {
            database: WorkerDatabaseConfig {
                backend: WorkerDatabaseBackend::Sqlite,
                url: None,
            },
            ai_mode: AiMode::Separate,
            ..BusinessWorkerConfig::default_for_test()
        };
        assert!(config.validate().is_err());
        config.ai_mode = AiMode::Inline;
        config.concurrency = 2;
        assert!(config.validate().is_err());
    }

    impl BusinessWorkerConfig {
        fn default_for_test() -> Self {
            Self {
                env: RuntimeEnvironment::Development,
                database: WorkerDatabaseConfig::default(),
                storage: WorkerStorageConfig::default(),
                worker_id: "worker".to_string(),
                concurrency: 1,
                lease_duration_secs: 30,
                heartbeat_interval_secs: 10,
                poll_interval_millis: 10,
                max_content_bytes: 1024,
                test_step_delay_millis: 0,
                ai_mode: AiMode::Inline,
                observability: ObservabilityConfig::default(),
            }
        }
    }
}
