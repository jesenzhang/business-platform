//! A standalone model provider protocol and transport library.
//!
//! This crate intentionally knows nothing about Jarvis runtime concepts. It
//! owns only model messages, tool schemas, provider wire protocols, standalone
//! image generation, normalized streaming, usage, and provider errors.

mod auth;
mod budget;
mod catalog;
mod constraints;
mod continuation;
mod error;
mod factory;
mod history;
mod images;
mod models;
mod options;
mod preparation;
mod profiles;
mod provider;
mod store;
mod stream;
mod types;

pub mod providers;

pub use auth::{
    env_api_key, env_var_name, has_configured_credential, resolve_api_key, resolve_credential,
    resolve_optional_credential, Credential, CredentialKind, CredentialRefresher, CredentialStore,
    MemoryCredentialStore, OAuthCredential, ResolvedCredential,
};
pub(crate) use budget::validate_estimated_request_budget;
pub use budget::{
    estimate_request_budget, estimate_request_budget_with, validate_request_budget,
    BudgetViolation, ConservativeTokenEstimator, ContextBudgetStatus, InputTokenBreakdown,
    RequestTokenBudget, TokenEstimate, TokenEstimatePrecision, TokenEstimator, TokenLimit,
};
pub use catalog::ModelCatalog;
pub use constraints::{
    MAX_CONSTRAINT_GRAMMAR_BYTES, MAX_CONSTRAINT_NAME_BYTES, MAX_CONSTRAINT_SCHEMA_BYTES,
    MAX_CONSTRAINT_SCHEMA_DEPTH,
};
pub use continuation::*;
pub use error::{FailurePhase, ProviderError, ProviderErrorKind};
pub use factory::{ProviderConfig, ProviderFactory};
pub use history::{normalize_history, normalize_request_history, HistoryNormalization};
pub use images::{
    GeneratedImage, ImageBackground, ImageGenerationProvider, ImageGenerationRequest,
    ImageGenerationResponse, ImageInputTokenDetails, ImageOutputFormat, ImageOutputTokenDetails,
    ImageQuality, ImageResponseFormat, ImageSize, ImageUsage,
};
pub use models::{Models, RefreshOutcome, RefreshStatus, RestoreOutcome, RestoreStatus};
pub use options::{AbortSignal, RequestOptions};
pub use preparation::{prepare_request, PreparedRequest};
pub use profiles::{
    AuthRequirement, HttpModelCatalogSource, ModelCatalogSource, ProviderProfile,
    RemoteCatalogSnapshot, RemoteModelSource,
};
pub use provider::{ModelProvider, ProviderStream};
pub use store::{
    InMemoryModelsStore, ModelsStore, ModelsStoreError, StoreDisposition, StoredModelCatalog,
};
pub use stream::{collect_stream, collect_stream_with_started_at, StreamAccumulator, StreamEvent};
pub use types::*;

#[cfg(test)]
mod tests {
    use futures::stream;

    use super::*;

    #[test]
    fn provider_id_is_open_ended_and_validated() {
        assert!(ProviderId::new("openrouter").is_ok());
        assert!(ProviderId::new(" ").is_err());
        let encoded = serde_json::to_string(&ProviderId::new("company-gateway").unwrap()).unwrap();
        assert_eq!(encoded, "\"company-gateway\"");
    }

    #[test]
    fn normalized_message_round_trips() {
        let message = Message::Assistant(AssistantMessage {
            content: vec![
                AssistantContent::Reasoning(ReasoningContent {
                    text: "thinking".into(),
                    redacted: false,
                    portability: ReasoningPortability::Portable,
                    continuation_ref: None,
                }),
                AssistantContent::ToolCall(ToolCall {
                    id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: serde_json::json!({"key": "value"}),
                }),
            ],
        });
        let decoded: Message =
            serde_json::from_str(&serde_json::to_string(&message).unwrap()).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn legacy_reasoning_payload_deserialization_fails_closed() {
        let value: ReasoningContent = serde_json::from_value(serde_json::json!({
            "text": "provider-bound",
            "signature": "legacy-signature",
            "redacted": false,
        }))
        .unwrap();
        assert_eq!(value.portability, ReasoningPortability::ProviderBound);
        assert!(!serde_json::to_string(&value)
            .unwrap()
            .contains("legacy-signature"));
    }

    #[test]
    fn provider_configuration_debug_redacts_credentials() {
        let config = ProviderConfig {
            provider_id: ProviderId::new("openai").unwrap(),
            api: Api::OpenAiCompletions,
            api_key: "secret-canary".into(),
            base_url: None,
            endpoint_policy: providers::EndpointPolicy::SecureOrLoopback,
            request_timeout: std::time::Duration::from_secs(10),
        };
        assert!(!format!("{config:?}").contains("secret-canary"));
    }

    #[test]
    fn credential_material_never_enters_profile_or_catalog_serialization() {
        let provider = ProviderId::new("oauth-provider").unwrap();
        let model = ModelSpec::custom("oauth-model", provider.clone(), Api::OpenAiResponses);
        let models = Models::new()
            .with_profile(ProviderProfile::new(
                provider.clone(),
                Api::OpenAiResponses,
                ModelCatalog::new([model.clone()]),
            ))
            .unwrap();
        models
            .set_credential(ResolvedCredential::OAuth(OAuthCredential::new(
                provider.clone(),
                "serialization-access-secret",
                Some("serialization-refresh-secret"),
                None,
            )))
            .unwrap();

        let profile_json =
            serde_json::to_string(&models.profile(&provider.to_string()).unwrap()).unwrap();
        let stored_json = serde_json::to_string(&StoredModelCatalog::new(
            provider.clone(),
            Api::OpenAiResponses,
            ModelCatalog::new([model]),
            1,
            "models",
        ))
        .unwrap();
        assert!(!profile_json.contains("serialization-access-secret"));
        assert!(!profile_json.contains("serialization-refresh-secret"));
        assert!(!stored_json.contains("serialization-access-secret"));
        assert!(!stored_json.contains("serialization-refresh-secret"));
    }

    #[tokio::test]
    async fn collector_assembles_text_reasoning_and_multiple_tools() {
        let reasoning_ref = ContinuationRef::new("reasoning-a").unwrap();
        let redacted_ref = ContinuationRef::new("reasoning-b").unwrap();
        let events = vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::TextStart),
            Ok(StreamEvent::TextDelta {
                text: "hello".into(),
            }),
            Ok(StreamEvent::TextEnd),
            Ok(StreamEvent::ReasoningStart),
            Ok(StreamEvent::ReasoningReference {
                reference: reasoning_ref.clone(),
            }),
            Ok(StreamEvent::ReasoningDelta { text: "why".into() }),
            Ok(StreamEvent::ReasoningSignature {
                signature: "sig".into(),
            }),
            Ok(StreamEvent::ReasoningEnd),
            Ok(StreamEvent::ReasoningStart),
            Ok(StreamEvent::ReasoningReference {
                reference: redacted_ref.clone(),
            }),
            Ok(StreamEvent::ReasoningRedacted {
                data: Some("opaque".into()),
            }),
            Ok(StreamEvent::ReasoningEnd),
            Ok(StreamEvent::ToolCallStart {
                index: 0,
                id: "call-1".into(),
                name: "one".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "{\"a\":".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "1}".into(),
            }),
            Ok(StreamEvent::ToolCallEnd {
                index: 0,
                tool_call: ToolCall {
                    id: "call-1".into(),
                    name: "one".into(),
                    arguments: serde_json::json!({"a": 1}),
                },
            }),
            Ok(StreamEvent::ToolCallStart {
                index: 1,
                id: "call-2".into(),
                name: "two".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                index: 1,
                arguments_delta: "{}".into(),
            }),
            Ok(StreamEvent::ToolCallEnd {
                index: 1,
                tool_call: ToolCall {
                    id: "call-2".into(),
                    name: "two".into(),
                    arguments: serde_json::json!({}),
                },
            }),
            Ok(StreamEvent::Continuation(
                ProviderContinuation::AnthropicMessages(
                    AnthropicMessagesContinuation::with_scope(
                        ProviderId::new("anthropic").unwrap(),
                        "test-model",
                        ContinuationScope::empty(),
                        vec![
                            AnthropicReasoningReplayEntry::new(
                                reasoning_ref,
                                AnthropicReasoningReplay::thinking("sig"),
                            ),
                            AnthropicReasoningReplayEntry::new(
                                redacted_ref,
                                AnthropicReasoningReplay::redacted("opaque"),
                            ),
                        ],
                    )
                    .unwrap(),
                ),
            )),
            Ok(StreamEvent::Usage(Usage {
                input_tokens: 2,
                output_tokens: 3,
                total_tokens: 5,
                ..Usage::default()
            })),
            Ok(StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            }),
        ];
        let completion = collect_stream(stream::iter(events)).await.unwrap();
        assert_eq!(completion.message.text_value(), "hello");
        assert_eq!(completion.message.reasoning_chars(), 3);
        let reasoning = completion
            .message
            .content
            .iter()
            .filter_map(|part| match part {
                AssistantContent::Reasoning(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            reasoning[0].portability,
            ReasoningPortability::ProviderBound
        );
        assert!(reasoning[1].redacted);
        assert_eq!(
            reasoning[1].portability,
            ReasoningPortability::ProviderBound
        );
        assert_eq!(completion.message.tool_calls().len(), 2);
        assert_eq!(completion.usage.unwrap().total_tokens, 5);
    }

    #[test]
    fn accumulator_tracks_elapsed_without_sleeping() {
        let mut accumulator =
            StreamAccumulator::new().with_elapsed(std::time::Duration::from_millis(7));
        accumulator
            .push(StreamEvent::Start {
                model: "test".into(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .unwrap();
        assert_eq!(accumulator.finish().unwrap().metadata.elapsed_ms, 7);
    }

    #[test]
    fn accumulator_rejects_data_after_done() {
        let mut accumulator = StreamAccumulator::new();
        accumulator
            .push(StreamEvent::Start {
                model: "test".into(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .unwrap();
        assert_eq!(
            accumulator.push(StreamEvent::TextStart).unwrap_err().kind,
            ProviderErrorKind::Protocol
        );
    }

    #[tokio::test]
    async fn collector_rejects_missing_terminal_done() {
        let result = collect_stream(stream::iter(vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::TextStart),
            Ok(StreamEvent::TextDelta {
                text: "partial".into(),
            }),
        ]))
        .await;
        assert_eq!(
            result.unwrap_err().kind,
            ProviderErrorKind::StreamInterrupted
        );
    }

    #[tokio::test]
    async fn collector_rejects_incomplete_tool_call() {
        let result = collect_stream(stream::iter(vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::ToolCallStart {
                index: 0,
                id: "call-1".into(),
                name: "lookup".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "{\"key\":".into(),
            }),
            Ok(StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            }),
        ]))
        .await;
        assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Protocol);
    }

    #[test]
    fn accumulator_exposes_partial_history_for_abort_continuation() {
        let mut accumulator = StreamAccumulator::new();
        accumulator
            .push(StreamEvent::Start {
                model: "test-model".into(),
            })
            .unwrap();
        accumulator.push(StreamEvent::TextStart).unwrap();
        accumulator
            .push(StreamEvent::TextDelta {
                text: "partial".into(),
            })
            .unwrap();

        let message = accumulator.partial_message();
        assert_eq!(message.text_value(), "partial");
        assert_eq!(message.content.len(), 1);
    }

    #[tokio::test]
    async fn accumulator_preserves_wire_content_order() {
        let completion = collect_stream(stream::iter(vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::ReasoningStart),
            Ok(StreamEvent::ReasoningDelta {
                text: "plan".into(),
            }),
            Ok(StreamEvent::ReasoningEnd),
            Ok(StreamEvent::TextStart),
            Ok(StreamEvent::TextDelta {
                text: "answer".into(),
            }),
            Ok(StreamEvent::TextEnd),
            Ok(StreamEvent::ToolCallStart {
                index: 0,
                id: "call-1".into(),
                name: "lookup".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "{}".into(),
            }),
            Ok(StreamEvent::ToolCallEnd {
                index: 0,
                tool_call: ToolCall {
                    id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: serde_json::json!({}),
                },
            }),
            Ok(StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            }),
        ]))
        .await
        .unwrap();
        assert!(matches!(
            &completion.message.content[0],
            AssistantContent::Reasoning(_)
        ));
        assert!(matches!(
            &completion.message.content[1],
            AssistantContent::Text(_)
        ));
        assert!(matches!(
            &completion.message.content[2],
            AssistantContent::ToolCall(_)
        ));
    }

    #[tokio::test]
    async fn accumulator_preserves_interleaved_tool_identity_and_order() {
        let completion = collect_stream(stream::iter(vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::TextStart),
            Ok(StreamEvent::TextDelta {
                text: "before".into(),
            }),
            Ok(StreamEvent::ReasoningStart),
            Ok(StreamEvent::ReasoningDelta {
                text: "plan".into(),
            }),
            Ok(StreamEvent::ToolCallStart {
                index: 3,
                id: "call-3".into(),
                name: "lookup".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                index: 3,
                arguments_delta: "{\"q\":1}".into(),
            }),
            Ok(StreamEvent::ToolCallEnd {
                index: 3,
                tool_call: ToolCall {
                    id: "call-3".into(),
                    name: "lookup".into(),
                    arguments: serde_json::json!({"q": 1}),
                },
            }),
            Ok(StreamEvent::ReasoningEnd),
            Ok(StreamEvent::TextEnd),
            Ok(StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            }),
        ]))
        .await
        .unwrap();
        assert!(matches!(
            &completion.message.content[0],
            AssistantContent::Text(_)
        ));
        assert!(matches!(
            &completion.message.content[1],
            AssistantContent::Reasoning(_)
        ));
        let AssistantContent::ToolCall(call) = &completion.message.content[2] else {
            panic!("expected tool call")
        };
        assert_eq!(call.id, "call-3");
    }
}
