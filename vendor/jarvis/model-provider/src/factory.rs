use std::sync::Arc;
use std::time::Duration;

use crate::auth::{env_api_key, resolve_api_key, CredentialKind, CredentialStore};
use crate::providers::{
    AnthropicProvider, OpenAiCompatibleProvider, OpenAiImageProvider, OpenAiResponsesProvider,
};
use crate::{
    Api, FailurePhase, ImageGenerationProvider, MaxOutputTokensField, ModelProvider,
    OpenAiCompletionsCompatibility, ProviderError, ProviderErrorKind, ProviderId,
};

#[derive(Clone)]
pub struct ProviderConfig {
    pub provider_id: ProviderId,
    pub api: Api,
    pub api_key: String,
    pub base_url: Option<String>,
    pub endpoint_policy: crate::providers::EndpointPolicy,
    pub request_timeout: Duration,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderConfig")
            .field("provider_id", &self.provider_id)
            .field("api", &self.api)
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("endpoint_policy", &self.endpoint_policy)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

pub struct ProviderFactory;

impl ProviderFactory {
    pub fn build(config: ProviderConfig) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Self::build_with_credential(config, CredentialKind::ApiKey)
    }

    pub fn build_with_credential(
        config: ProviderConfig,
        credential_kind: CredentialKind,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Self::build_with_openai_completions_compatibility_and_credential(
            config,
            OpenAiCompletionsCompatibility::default(),
            credential_kind,
        )
    }

    pub fn build_with_max_output_tokens_field(
        config: ProviderConfig,
        max_output_tokens_field: MaxOutputTokensField,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Self::build_with_openai_completions_compatibility(
            config,
            OpenAiCompletionsCompatibility {
                max_output_tokens_field,
                ..OpenAiCompletionsCompatibility::default()
            },
        )
    }

    pub fn build_with_openai_completions_compatibility(
        config: ProviderConfig,
        compatibility: OpenAiCompletionsCompatibility,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Self::build_with_openai_completions_compatibility_and_credential(
            config,
            compatibility,
            CredentialKind::ApiKey,
        )
    }

    pub fn build_with_openai_completions_compatibility_and_credential(
        config: ProviderConfig,
        compatibility: OpenAiCompletionsCompatibility,
        credential_kind: CredentialKind,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        match config.api {
            Api::OpenAiCompletions => {
                let mut provider =
                    OpenAiCompatibleProvider::new(config.provider_id, config.api_key)?
                        .with_compatibility(compatibility)
                        .with_request_timeout(config.request_timeout);
                if let Some(base_url) = config.base_url {
                    provider =
                        provider.with_base_url_and_policy(base_url, config.endpoint_policy)?;
                }
                Ok(Arc::new(provider))
            }
            Api::OpenAiResponses => {
                let mut provider =
                    OpenAiResponsesProvider::new(config.provider_id, config.api_key)?
                        .with_request_timeout(config.request_timeout);
                if let Some(base_url) = config.base_url {
                    provider =
                        provider.with_base_url_and_policy(base_url, config.endpoint_policy)?;
                }
                Ok(Arc::new(provider))
            }
            Api::AnthropicMessages => {
                let mut provider = AnthropicProvider::new(config.provider_id, config.api_key)?
                    .with_bearer_auth(matches!(credential_kind, CredentialKind::OAuth))
                    .with_request_timeout(config.request_timeout);
                if let Some(base_url) = config.base_url {
                    provider =
                        provider.with_base_url_and_policy(base_url, config.endpoint_policy)?;
                }
                Ok(Arc::new(provider))
            }
            Api::OpenAiImages => Err(ProviderError::new(
                ProviderErrorKind::Unsupported,
                crate::FailurePhase::BeforeDispatch,
                "standalone image generation requires build_image_generator",
            )),
            Api::Custom(api) => Err(ProviderError::new(
                ProviderErrorKind::Unsupported,
                crate::FailurePhase::BeforeDispatch,
                format!("unsupported provider API {api}"),
            )),
        }
    }

    pub fn build_image_generator(
        config: ProviderConfig,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        Self::build_image_generator_with_credential(config, CredentialKind::ApiKey)
    }

    pub fn build_image_generator_with_credential(
        config: ProviderConfig,
        _credential_kind: CredentialKind,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        if config.api != Api::OpenAiImages {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                FailurePhase::BeforeDispatch,
                format!(
                    "image generator requires Api::OpenAiImages, got {:?}",
                    config.api
                ),
            ));
        }
        let mut provider = OpenAiImageProvider::new(config.provider_id, config.api_key)?
            .with_request_timeout(config.request_timeout);
        if let Some(base_url) = config.base_url {
            provider = provider.with_base_url_and_policy(base_url, config.endpoint_policy)?;
        }
        Ok(Arc::new(provider))
    }

    pub fn from_env(
        provider_id: ProviderId,
        api: Api,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let api_key = env_api_key(provider_id.as_str()).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Authentication,
                FailurePhase::BeforeDispatch,
                format!("missing environment API key for {provider_id}"),
            )
        })?;
        Self::build(ProviderConfig {
            provider_id,
            api,
            api_key,
            base_url: None,
            endpoint_policy: crate::providers::EndpointPolicy::SecureOrLoopback,
            request_timeout: Duration::from_secs(120),
        })
    }

    pub fn from_store(
        provider_id: ProviderId,
        api: Api,
        store: &dyn CredentialStore,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let api_key = resolve_api_key(&provider_id, Some(store))?;
        Self::build(ProviderConfig {
            provider_id,
            api,
            api_key,
            base_url: None,
            endpoint_policy: crate::providers::EndpointPolicy::SecureOrLoopback,
            request_timeout: Duration::from_secs(120),
        })
    }
}
