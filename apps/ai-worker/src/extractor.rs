//! Live model-backed field extraction (PLAN-0012 M2).
//!
//! `ModelBackedExtractor` implements `DocumentFieldExtractor` by delegating to
//! a `jarvis_model_provider` client. It lives in the `ai-worker` composition
//! root so `document-processing` core keeps zero dependencies on the model
//! provider (ADR-0023). The provider is injected as `Arc<dyn ModelProvider>`,
//! which keeps the port substitution seam: contract tests inject
//! `MockProvider`/`ScriptedProvider`, production injects a provider built from
//! runtime configuration.
//!
//! Contract guarantees (ADR-0023):
//! - Provider errors map onto the bounded `ExtractionError` variants so the
//!   existing AI-task retry/classification semantics are reused; rate-limit,
//!   timeout and 5xx become retryable `AiProviderUnavailable`.
//! - Provider/raw responses never enter logs, DTOs or the returned candidate
//!   beyond the parsed `CandidatePayload`; credentials never cross this module.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use document_processing::{
    CandidatePayload, DocumentFieldExtractor, ExtractionCandidate, ExtractionError,
    ExtractionRequest,
};
use jarvis_model_provider::{
    Api, CompletionRequest, Message, ModelProvider, ModelSpec, ProviderConfig, ProviderError,
    ProviderErrorKind, ProviderFactory, ProviderId,
};

use crate::config::AiProviderConfig;

/// Prompt iteration marker emitted on every candidate. Bump deliberately when
/// the system instruction or parsing contract changes.
const EXTRACTION_PROMPT_VERSION: &str = "document-extraction-v1";
/// Maximum serialized size guard used when validating a parsed payload. It is
/// far below the provider response bound and keeps `CandidatePayload`
/// homogeneous with the deterministic extractor.
const MAX_PAYLOAD_BYTES: usize = 256 * 1024;

/// Field-extraction port implementation backed by a chat-completion provider.
pub struct ModelBackedExtractor {
    provider: Arc<dyn ModelProvider>,
    model: String,
    prompt_version: String,
    max_output_tokens: Option<u32>,
}

impl ModelBackedExtractor {
    /// Wrap an already-constructed provider. Used by production wiring and by
    /// contract tests that inject a mock/scripted provider.
    pub fn new(provider: Arc<dyn ModelProvider>, model: String) -> Self {
        Self {
            provider,
            model,
            prompt_version: EXTRACTION_PROMPT_VERSION.to_string(),
            max_output_tokens: None,
        }
    }

    /// Build the live provider from runtime configuration (fail-closed).
    ///
    /// Only reachable when `mode == real`; configuration validation already
    /// requires a non-empty API key. Unsupported API names and non-conformant
    /// base URLs fail here before any request is dispatched.
    pub fn from_config(config: &AiProviderConfig) -> anyhow::Result<Self> {
        let api_key = config
            .api_key
            .as_ref()
            .map(|secret| secret.expose().clone())
            .unwrap_or_default();
        let api = match config.api.as_str() {
            "openai" | "openai_completions" => Api::OpenAiCompletions,
            "openai_responses" => Api::OpenAiResponses,
            "anthropic" | "anthropic_messages" => Api::AnthropicMessages,
            other => anyhow::bail!("unsupported AI provider api: {other}"),
        };
        let provider_id = ProviderId::new(config.provider_id.clone())
            .map_err(|message| anyhow::anyhow!("invalid AI provider id: {message}"))?;
        let provider = ProviderFactory::build(ProviderConfig {
            api_key,
            base_url: config.base_url.as_ref().map(|url| url.expose().to_string()),
            endpoint_policy: jarvis_model_provider::providers::EndpointPolicy::SecureOrLoopback,
            request_timeout: Duration::from_secs(config.request_timeout_secs),
            provider_id,
            api,
        })
        .map_err(|error| anyhow::anyhow!("failed to build AI provider: {error}"))?;
        let mut extractor = Self::new(provider, config.model.clone());
        extractor.max_output_tokens = config.max_output_tokens;
        Ok(extractor)
    }
}

#[async_trait]
impl DocumentFieldExtractor for ModelBackedExtractor {
    async fn extract(
        &self,
        request: ExtractionRequest,
    ) -> Result<ExtractionCandidate, ExtractionError> {
        let mut completion_request = CompletionRequest::new(
            ModelSpec::custom(
                self.model.clone(),
                self.provider.provider_id().clone(),
                self.provider.api().clone(),
            ),
            build_messages(&request),
        );
        completion_request.temperature = Some(0.0);
        completion_request.max_output_tokens = self.max_output_tokens;
        let completion = self
            .provider
            .complete(completion_request)
            .await
            .map_err(|error| map_provider_error(&error))?;
        let payload = parse_payload(&completion.message.text_value(), &request)?;
        ExtractionCandidate::new(
            request.tenant_id,
            request.job_id,
            request.content_revision,
            payload,
            Vec::new(),
            self.provider.provider_id().as_str().to_string(),
            self.model.clone(),
            self.prompt_version.clone(),
            request.line_count,
            chrono::Utc::now(),
        )
        .inspect_err(|error| {
            tracing::debug!(failure_code = %error.code(), "model-backed candidate rejected");
        })
    }
}

/// Compose the provider-neutral instruction message set for one document.
///
/// The document text is intentionally sent as trusted-to-provider content; it
/// is never echoed to logs, DTOs or error responses on the return path.
fn build_messages(request: &ExtractionRequest) -> Vec<Message> {
    let system = "Extract structured metadata from the supplied document text. \
Respond with a single JSON object only, matching this schema exactly: \
{\"title\": string|null, \"document_type\": string, \"language\": string|null, \
\"summary\": string|null, \"fields\": object, \"warnings\": array of string}. \
Do not include any prose or code fence around the JSON.";
    let user = format!(
        "document_type: {}\n---document---\n{}",
        request.content_type, request.text
    );
    vec![Message::system(system), Message::user(user)]
}

/// Translate a provider failure into the bounded extraction error, preserving
/// the AI-task retry classification without leaking provider body text.
fn map_provider_error(error: &ProviderError) -> ExtractionError {
    match error.kind {
        ProviderErrorKind::Authentication
        | ProviderErrorKind::RateLimit
        | ProviderErrorKind::Timeout
        | ProviderErrorKind::Unavailable => ExtractionError::AiProviderUnavailable,
        ProviderErrorKind::Aborted => ExtractionError::Cancelled,
        ProviderErrorKind::InvalidRequest
        | ProviderErrorKind::Serialization
        | ProviderErrorKind::Protocol
        | ProviderErrorKind::StreamInterrupted
        | ProviderErrorKind::Unsupported
        | ProviderErrorKind::Other => ExtractionError::AiInvalidResponse,
    }
}

/// Parse the model text response into a validated `CandidatePayload`.
///
/// Tolerates surrounding prose/code fences by extracting the outermost JSON
/// object literally; a missing or malformed object is a provider protocol
/// failure (`AiInvalidResponse`).
fn parse_payload(
    text: &str,
    request: &ExtractionRequest,
) -> Result<CandidatePayload, ExtractionError> {
    let object_start = text.find('{').ok_or(ExtractionError::AiInvalidResponse)?;
    let object_end = text.rfind('}').ok_or(ExtractionError::AiInvalidResponse)?;
    if object_end <= object_start {
        return Err(ExtractionError::AiInvalidResponse);
    }
    let slice = &text[object_start..=object_end];
    let value: serde_json::Value =
        serde_json::from_str(slice).map_err(|_| ExtractionError::AiInvalidResponse)?;
    let object = value
        .as_object()
        .ok_or(ExtractionError::AiInvalidResponse)?;
    let document_type = object
        .get("document_type")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| request.content_type.clone(), str::to_string);
    if document_type.trim().is_empty() {
        return Err(ExtractionError::AiInvalidResponse);
    }
    let payload = CandidatePayload {
        schema_version: CandidatePayload::SCHEMA_VERSION.to_string(),
        title: object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        document_type,
        language: object
            .get("language")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        summary: object
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        fields: object
            .get("fields")
            .and_then(serde_json::Value::as_object)
            .map(|map| map.clone().into_iter().collect())
            .unwrap_or_default(),
        warnings: object
            .get("warnings")
            .and_then(serde_json::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    };
    payload.validate(MAX_PAYLOAD_BYTES)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::{map_provider_error, ModelBackedExtractor};
    use document_processing::{DocumentFieldExtractor, ExtractionError, ExtractionRequest};
    use jarvis_model_provider::providers::{MockProvider, ScriptedProvider};
    use jarvis_model_provider::{
        AssistantMessage, Completion, CompletionMetadata, FailurePhase, ProviderError,
        ProviderErrorKind, StopReason, StreamEvent,
    };
    use std::sync::Arc;
    use uuid::Uuid;

    fn request(text: &str) -> ExtractionRequest {
        ExtractionRequest {
            tenant_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            content_revision: 1,
            content_type: "text/plain".to_string(),
            text: text.to_string(),
            line_count: u32::try_from(text.lines().count()).unwrap_or(u32::MAX),
            character_count: u64::try_from(text.chars().count()).unwrap_or(u64::MAX),
        }
    }

    fn provider_completion(text: &str) -> Completion {
        Completion {
            message: AssistantMessage::text(text),
            usage: None,
            continuation: None,
            stop_reason: StopReason::Stop,
            metadata: CompletionMetadata::default(),
        }
    }

    fn mock_provider(text: &str) -> MockProvider {
        match MockProvider::new(provider_completion(text)) {
            Ok(provider) => provider,
            Err(error) => unreachable!("mock provider must build: {error}"),
        }
    }

    fn scripted_provider(events: Vec<Result<StreamEvent, ProviderError>>) -> ScriptedProvider {
        match ScriptedProvider::new(events) {
            Ok(provider) => provider,
            Err(error) => unreachable!("scripted provider must build: {error}"),
        }
    }

    #[test]
    fn err_kind_maps_rate_limit_to_unavailable() {
        let error = map_provider_error(&ProviderError::new(
            ProviderErrorKind::RateLimit,
            FailurePhase::AfterDispatch,
            "rate limited",
        ));
        assert_eq!(error, ExtractionError::AiProviderUnavailable);
    }

    #[test]
    fn err_kind_maps_protocol_to_invalid_response() {
        let error = map_provider_error(&ProviderError::new(
            ProviderErrorKind::Protocol,
            FailurePhase::DuringStream,
            "malformed",
        ));
        assert_eq!(error, ExtractionError::AiInvalidResponse);
    }

    #[tokio::test]
    async fn extracts_candidate_from_valid_json() {
        let provider = mock_provider(
            "```json\n{\"title\": \"Annual Report\", \"document_type\": \"report\", \
\"language\": \"en\", \"summary\": \"Q1 summary\", \
\"fields\": {\"page_count\": 12}, \"warnings\": []}\n```",
        );
        let extractor = ModelBackedExtractor::new(Arc::new(provider), "test-model".into());
        match extractor.extract(request("Quarterly results.")).await {
            Ok(candidate) => {
                assert_eq!(candidate.payload.title.as_deref(), Some("Annual Report"));
                assert_eq!(candidate.payload.document_type, "report");
                assert_eq!(candidate.provider, "mock");
                assert_eq!(candidate.model, "test-model");
                assert_eq!(candidate.prompt_version, "document-extraction-v1");
            }
            Err(error) => unreachable!("expected a valid candidate, got: {error}"),
        }
    }

    #[tokio::test]
    async fn refuses_non_json_provider_output() {
        let provider = mock_provider("not json at all");
        let extractor = ModelBackedExtractor::new(Arc::new(provider), "test-model".into());
        match extractor.extract(request("text")).await {
            Ok(_) => unreachable!("expected invalid response"),
            Err(error) => assert_eq!(error, ExtractionError::AiInvalidResponse),
        }
    }

    #[tokio::test]
    async fn provider_unavailable_error_is_mapped() {
        let error = ProviderError::new(
            ProviderErrorKind::Unavailable,
            FailurePhase::AfterDispatch,
            "upstream 503",
        );
        let provider = scripted_provider(vec![
            Ok(StreamEvent::Start { model: "m".into() }),
            Err(error),
        ]);
        let extractor = ModelBackedExtractor::new(Arc::new(provider), "test-model".into());
        match extractor.extract(request("text")).await {
            Ok(_) => unreachable!("expected provider failure"),
            Err(error) => assert_eq!(error, ExtractionError::AiProviderUnavailable),
        }
    }

    #[tokio::test]
    async fn provider_rate_limit_and_retry_after_is_mapped_to_retry() {
        let error = ProviderError::new(
            ProviderErrorKind::RateLimit,
            FailurePhase::AfterDispatch,
            "rate limited",
        )
        .with_status(429)
        .with_retry_after(std::time::Duration::from_secs(7));
        let provider = scripted_provider(vec![
            Ok(StreamEvent::Start { model: "m".into() }),
            Err(error),
        ]);
        let extractor = ModelBackedExtractor::new(Arc::new(provider), "test-model".into());
        match extractor.extract(request("text")).await {
            Ok(_) => unreachable!("expected rate-limit failure"),
            Err(error) => assert_eq!(error, ExtractionError::AiProviderUnavailable),
        }
    }

    #[tokio::test]
    async fn provider_abort_is_mapped_to_cancelled() {
        let error = ProviderError::new(
            ProviderErrorKind::Aborted,
            FailurePhase::BeforeDispatch,
            "provider aborted",
        );
        let provider = scripted_provider(vec![
            Ok(StreamEvent::Start { model: "m".into() }),
            Err(error),
        ]);
        let extractor = ModelBackedExtractor::new(Arc::new(provider), "test-model".into());
        match extractor.extract(request("text")).await {
            Ok(_) => unreachable!("expected abort failure"),
            Err(error) => assert_eq!(error, ExtractionError::Cancelled),
        }
    }
}
