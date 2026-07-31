use serde::Deserialize;

/// Runtime environment selected by the process configuration source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeEnvironment {
    #[default]
    Development,
    Production,
}

impl RuntimeEnvironment {
    #[must_use]
    pub const fn config_name(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }
}
