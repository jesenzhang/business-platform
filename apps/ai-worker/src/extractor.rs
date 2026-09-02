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
//!   existing AI-task retry/classification semantics are reused: timeout and
//!   5xx become retryable `AiProviderUnavailable`; rate-limit becomes
//!   `AiProviderRateLimited` carrying the provider's pacing hint (capped by
//!   the retry policy); credential rejection becomes the non-retryable
//!   `AiProviderRejected`; invalid-request stays permanent via
//!   `AiInvalidResponse`.
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
    /// base URLs fail here before any request is dispatched. Plaintext HTTP to
    /// RFC1918 addresses requires the explicit `allow_private_http` opt-in;
    /// HTTPS and loopback HTTP are always accepted.
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
        let endpoint_policy = if config.allow_private_http {
            tracing::warn!(
                "AI provider endpoint policy is trusted-private-http: plaintext \
                 intranet transport; credentials, prompts and model responses \
                 are not encrypted in transit"
            );
            jarvis_model_provider::providers::EndpointPolicy::TrustedPrivateHttp
        } else {
            jarvis_model_provider::providers::EndpointPolicy::SecureOrLoopback
        };
        let provider_id = ProviderId::new(config.provider_id.clone())
            .map_err(|message| anyhow::anyhow!("invalid AI provider id: {message}"))?;
        let provider = ProviderFactory::build(ProviderConfig {
            api_key,
            base_url: config.base_url.as_ref().map(|url| url.expose().to_string()),
            endpoint_policy,
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
///
/// Retry semantics (PLAN-0012 release hardening):
/// - `RateLimit` keeps the provider's `Retry-After` hint on
///   `AiProviderRateLimited`; the retry policy in `retry::classify_failure`
///   clamps it. The hint must never be dropped.
/// - `Timeout`/`Unavailable` remain transient `AiProviderUnavailable` and use
///   the platform backoff ladder.
/// - `Authentication` is a configuration fault: `AiProviderRejected` is
///   permanent and must not be retried as a transient error.
/// - `InvalidRequest` and protocol faults stay permanent `AiInvalidResponse`.
fn map_provider_error(error: &ProviderError) -> ExtractionError {
    match error.kind {
        ProviderErrorKind::RateLimit => ExtractionError::AiProviderRateLimited {
            retry_after: error.retry_after,
        },
        ProviderErrorKind::Timeout | ProviderErrorKind::Unavailable => {
            ExtractionError::AiProviderUnavailable
        }
        ProviderErrorKind::Authentication => ExtractionError::AiProviderRejected,
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
    use crate::config::{AiProviderConfig, AiProviderMode};
    use document_processing::{DocumentFieldExtractor, ExtractionError, ExtractionRequest};
    use jarvis_model_provider::providers::{MockProvider, ScriptedProvider};
    use jarvis_model_provider::{
        AssistantMessage, Completion, CompletionMetadata, FailurePhase, ProviderError,
        ProviderErrorKind, StopReason, StreamEvent,
    };
    use runtime_config::{Secret, SecretUrl};
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
    fn err_kind_maps_rate_limit_to_rate_limited_without_hint() {
        let error = map_provider_error(&ProviderError::new(
            ProviderErrorKind::RateLimit,
            FailurePhase::AfterDispatch,
            "rate limited",
        ));
        assert_eq!(
            error,
            ExtractionError::AiProviderRateLimited { retry_after: None }
        );
    }

    #[test]
    fn err_kind_preserves_rate_limit_retry_after_hint() {
        let error = map_provider_error(
            &ProviderError::new(
                ProviderErrorKind::RateLimit,
                FailurePhase::AfterDispatch,
                "rate limited",
            )
            .with_retry_after(std::time::Duration::from_secs(11)),
        );
        assert_eq!(
            error,
            ExtractionError::AiProviderRateLimited {
                retry_after: Some(std::time::Duration::from_secs(11)),
            }
        );
    }

    #[test]
    fn err_kind_maps_authentication_to_permanent_rejection() {
        let error = map_provider_error(&ProviderError::new(
            ProviderErrorKind::Authentication,
            FailurePhase::BeforeDispatch,
            "bad credentials",
        ));
        assert_eq!(error, ExtractionError::AiProviderRejected);
        assert_eq!(error.code(), "ai_provider_rejected");
    }

    #[test]
    fn err_kind_maps_invalid_request_to_invalid_response() {
        let error = map_provider_error(&ProviderError::new(
            ProviderErrorKind::InvalidRequest,
            FailurePhase::BeforeDispatch,
            "bad request",
        ));
        assert_eq!(error, ExtractionError::AiInvalidResponse);
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
    async fn provider_rate_limit_end_to_end_preserves_retry_after_hint() {
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
            Err(error) => assert_eq!(
                error,
                ExtractionError::AiProviderRateLimited {
                    retry_after: Some(std::time::Duration::from_secs(7)),
                }
            ),
        }
    }

    #[tokio::test]
    async fn provider_authentication_end_to_end_is_permanent_rejection() {
        let error = ProviderError::new(
            ProviderErrorKind::Authentication,
            FailurePhase::BeforeDispatch,
            "invalid api key",
        );
        let provider = scripted_provider(vec![
            Ok(StreamEvent::Start { model: "m".into() }),
            Err(error),
        ]);
        let extractor = ModelBackedExtractor::new(Arc::new(provider), "test-model".into());
        match extractor.extract(request("text")).await {
            Ok(_) => unreachable!("expected authentication failure"),
            Err(error) => assert_eq!(error, ExtractionError::AiProviderRejected),
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

    fn provider_config(base_url: &str, allow_private_http: bool) -> AiProviderConfig {
        AiProviderConfig {
            mode: AiProviderMode::Real,
            provider_id: "smoke".to_string(),
            model: "test-model".to_string(),
            api: "openai_completions".to_string(),
            base_url: Some(
                SecretUrl::parse(base_url)
                    .unwrap_or_else(|error| unreachable!("valid base url: {error}")),
            ),
            api_key: Some(Secret::new("dummy".to_string())),
            request_timeout_secs: 120,
            max_output_tokens: Some(512),
            allow_private_http,
        }
    }

    #[test]
    fn from_config_rejects_plaintext_private_http_by_default() {
        let result = ModelBackedExtractor::from_config(&provider_config(
            "http://192.168.1.10:8080/v1",
            false,
        ));
        let Err(error) = result else {
            unreachable!("plaintext RFC1918 endpoint must fail closed without opt-in")
        };
        assert!(error.to_string().contains("failed to build AI provider"));
    }

    #[test]
    fn from_config_accepts_private_http_with_explicit_opt_in() {
        ModelBackedExtractor::from_config(&provider_config("http://192.168.1.10:8080/v1", true))
            .unwrap_or_else(|error| unreachable!("opt-in private http must build: {error}"));
    }

    #[test]
    fn from_config_still_rejects_unknown_api() {
        let mut config = provider_config("http://192.168.1.10:8080/v1", true);
        config.api = "unknown_api".to_string();
        let result = ModelBackedExtractor::from_config(&config);
        let Err(error) = result else {
            unreachable!("unknown api name must fail closed")
        };
        assert!(error.to_string().contains("unsupported AI provider api"));
    }

    /// PLAN-0012 T2.5 manual smoke against a real OpenAI-compatible endpoint.
    ///
    /// Ignored by default because it requires a reachable model endpoint. Run
    /// locally with:
    ///
    /// ```text
    /// AI_SMOKE_BASE_URL=http://<host>:<port>/v1 \
    /// cargo test -p ai-worker --all-features real_provider_smoke -- --ignored --nocapture
    /// ```
    ///
    /// `AI_SMOKE_MODEL` (default `qwen3_vl`) and `AI_SMOKE_API_KEY` (default
    /// `dummy`) are optional. The endpoint address is intentionally read from
    /// the environment so no intranet URL is committed to the repository.
    #[tokio::test]
    #[ignore = "requires a reachable intranet model provider; run manually with --ignored"]
    async fn real_provider_smoke_extracts_candidate() -> Result<(), Box<dyn std::error::Error>> {
        let base_url = std::env::var("AI_SMOKE_BASE_URL")
            .map_err(|_| "set AI_SMOKE_BASE_URL to run the real provider smoke")?;
        let model = std::env::var("AI_SMOKE_MODEL").unwrap_or_else(|_| "qwen3_vl".to_string());
        let api_key = std::env::var("AI_SMOKE_API_KEY").unwrap_or_else(|_| "dummy".to_string());
        let mut config = provider_config(&base_url, true);
        config.model = model;
        config.api_key = Some(Secret::new(api_key));
        // Reasoning-style deployments (e.g. qwen3 served via vLLM) spend output
        // budget on hidden reasoning before emitting the final JSON; keep the
        // budget generous so the answer is not truncated to an empty payload.
        config.max_output_tokens = Some(4096);
        let Ok(extractor) = ModelBackedExtractor::from_config(&config) else {
            unreachable!("smoke config must build");
        };
        let sample = "MEMORANDUM\n\nTo: All staff\nFrom: Operations\nDate: 2026-08-29\n\
Subject: Quarterly facility maintenance window\n\nThe data center will undergo \
power maintenance from 2026-09-05 to 2026-09-06. Services may be degraded.\n";
        let candidate = match extractor.extract(request(sample)).await {
            Ok(candidate) => candidate,
            Err(error) => return Err(format!("real provider smoke failed: {error:?}").into()),
        };
        println!(
            "smoke ok: document_type={} title={:?} language={:?} fields={} warnings={}",
            candidate.payload.document_type,
            candidate.payload.title,
            candidate.payload.language,
            candidate.payload.fields.len(),
            candidate.payload.warnings.len(),
        );
        assert!(!candidate.payload.document_type.trim().is_empty());
        Ok(())
    }
}
