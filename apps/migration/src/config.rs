use runtime_config::{load_process_config, ConfigLoadError, ConfigValidationError, SecretUrl};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MigrationConfig {
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: SecretUrl,
}

impl MigrationConfig {
    pub fn load() -> Result<Self, ConfigLoadError> {
        let mut config: Self = load_process_config("MIGRATION")?;
        let process_url = std::env::var("MIGRATION__DATABASE__URL").ok();
        let legacy_url = std::env::var("DATABASE_URL").ok();
        if process_url.is_some() && legacy_url.is_some() {
            return Err(ConfigLoadError);
        }
        if let Some(legacy_url) = legacy_url {
            config.database.url = SecretUrl::parse(&legacy_url).map_err(|_| ConfigLoadError)?;
        }
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if matches!(
            self.database.url.expose().split(':').next(),
            Some("postgres" | "postgresql")
        ) {
            Ok(())
        } else {
            Err(ConfigValidationError {
                messages: vec!["database.url must use a PostgreSQL scheme".to_string()],
            })
        }
    }
}
