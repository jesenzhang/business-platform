use runtime_config::{load_process_config, ConfigLoadError, RuntimeEnvironment, Secret, SecretUrl};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkerDatabaseBackend {
    #[default]
    Postgres,
    Sqlite,
}

/// Selects the field-extraction implementation used by the worker.
///
/// `deterministic` is the offline/test default. `real` wires a live
/// model-provider endpoint and fails closed at startup when required
/// credentials are missing (PLAN-0012 M2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderMode {
    #[default]
    Deterministic,
    Real,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub backend: WorkerDatabaseBackend,
    pub url: Option<SecretUrl>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: WorkerDatabaseBackend::Postgres,
            url: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiWorkerConfig {
    #[serde(default)]
    pub env: RuntimeEnvironment,
    #[serde(default)]
    pub database: DatabaseConfig,
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
    pub test_task_delay_millis: u64,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub ai_provider: AiProviderConfig,
}

/// Runtime configuration for the live model-provider integration.
///
/// Only the `real` mode consumes these values; `deterministic` ignores them.
/// The API key and base URL are never logged or exposed through DTOs
/// (ADR-0023). The base URL must resolve to HTTPS or loopback HTTP, which is
/// validated at provider build time by `check-architecture`/`ProviderFactory`.
/// Plaintext HTTP to RFC1918 addresses additionally requires the explicit
/// `allow_private_http` opt-in, which stays fail-closed (default off) in every
/// environment.
#[derive(Debug, Clone, Deserialize)]
pub struct AiProviderConfig {
    #[serde(default)]
    pub mode: AiProviderMode,
    #[serde(default = "default_provider_id")]
    pub provider_id: String,
    #[serde(default = "default_provider_model")]
    pub model: String,
    #[serde(default = "default_provider_api")]
    pub api: String,
    #[serde(default)]
    pub base_url: Option<SecretUrl>,
    #[serde(default)]
    pub api_key: Option<Secret<String>>,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    /// Explicit opt-in to plaintext HTTP against RFC1918 intranet endpoints
    /// (`EndpointPolicy::TrustedPrivateHttp`). Loopback HTTP and HTTPS remain
    /// allowed without this flag. Default false so a non-TLS base URL fails
    /// closed at provider build time.
    #[serde(default)]
    pub allow_private_http: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            mode: AiProviderMode::default(),
            provider_id: default_provider_id(),
            model: default_provider_model(),
            api: default_provider_api(),
            base_url: None,
            api_key: None,
            request_timeout_secs: default_request_timeout_secs(),
            max_output_tokens: None,
            allow_private_http: false,
        }
    }
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    /// `text` (development default) or `json` (preproduction/production).
    #[serde(default = "default_log_format")]
    pub log_format: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            otlp_endpoint: None,
            log_format: default_log_format(),
        }
    }
}

impl AiWorkerConfig {
    pub fn load() -> Result<Self, ConfigLoadError> {
        load_process_config("AI_WORKER")
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.worker_id.trim().is_empty()
            || self.concurrency == 0
            || self.lease_duration_secs <= 0
            || self.heartbeat_interval_secs <= 0
            || self.heartbeat_interval_secs * 2 >= self.lease_duration_secs
        {
            return Err("worker_id and heartbeat/lease duration are invalid".to_string());
        }
        if self.database.backend == WorkerDatabaseBackend::Sqlite {
            return Err("AI Worker separate mode requires PostgreSQL".to_string());
        }
        match self.storage.backend.as_str() {
            "local" if self.storage.base_dir.is_none() => {
                return Err("local storage requires a base directory".to_string());
            }
            "local" => {}
            "s3" => {
                if self.storage.endpoint.is_none()
                    || self.storage.bucket.as_deref().is_none_or(str::is_empty)
                    || self.storage.access_key.as_deref().is_none_or(str::is_empty)
                    || self
                        .storage
                        .secret_key
                        .as_ref()
                        .is_none_or(|value| value.expose().is_empty())
                {
                    return Err(
                        "S3 storage requires endpoint, bucket, access key, and secret key"
                            .to_string(),
                    );
                }
            }
            _ => return Err("unsupported AI Worker storage backend".to_string()),
        }
        if self.max_content_bytes == 0 {
            return Err("max_content_bytes must be positive".to_string());
        }
        if self.env == RuntimeEnvironment::Production && self.test_task_delay_millis != 0 {
            return Err("test_task_delay_millis must be zero in production".to_string());
        }
        if self.env == RuntimeEnvironment::Production && self.database.url.is_none() {
            return Err("production AI Worker requires a database URL".to_string());
        }
        if self.ai_provider.mode == AiProviderMode::Real {
            if self.ai_provider.model.trim().is_empty() {
                return Err("real AI provider requires a non-empty model".to_string());
            }
            if self
                .ai_provider
                .api_key
                .as_ref()
                .is_none_or(|key| key.expose().trim().is_empty())
            {
                return Err("real AI provider requires an API key".to_string());
            }
        }
        Ok(())
    }
}

fn default_worker_id() -> String {
    format!("ai-worker-{}", std::process::id())
}
fn default_provider_id() -> String {
    "openai".to_string()
}
fn default_provider_model() -> String {
    String::new()
}
fn default_provider_api() -> String {
    "openai_completions".to_string()
}
const fn default_request_timeout_secs() -> u64 {
    120
}
const fn default_lease_duration() -> i64 {
    30
}
const fn default_concurrency() -> u32 {
    1
}
const fn default_heartbeat_interval() -> i64 {
    10
}
const fn default_poll_interval() -> u64 {
    500
}
fn default_log_format() -> String {
    "text".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_storage_backend() -> String {
    "local".to_string()
}
fn default_region() -> String {
    "us-east-1".to_string()
}
const fn default_max_content_bytes() -> usize {
    16 * 1024 * 1024
}
