use async_trait::async_trait;
use jarvis_model_provider::providers::{
    AnthropicProvider, EndpointPolicy, OpenAiCompatibleProvider, OpenAiImageProvider,
    OpenAiResponsesProvider, ScriptedProvider,
};
use jarvis_model_provider::{
    calculate_cost, protocol_constraint_capabilities, AbortSignal, AnthropicMessagesContinuation,
    AnthropicReasoningReplay, AnthropicReasoningReplayEntry, Api, AssistantContent,
    AssistantMessage, AuthRequirement, CompletionRequest, ConstraintCapabilities, ContinuationRef,
    ContinuationScope, CredentialRefresher, FailurePhase, ImageBackground, ImageContent,
    ImageGenerationProvider, ImageGenerationRequest, ImageOutputFormat, ImageQuality,
    ImageResponseFormat, ImageSize, MaxOutputTokensField, MemoryCredentialStore, Message,
    ModelCapabilities, ModelCatalog, ModelCost, ModelProvider, ModelSpec, Models, OAuthCredential,
    OpenAiCompletionsCompatibility, OpenAiResponsesContinuationMode, OpenAiResponsesReplayItem,
    OpenAiResponsesReplaySegment, OpenAiSystemRole, OpenAiThinkingDialect, OutputConstraint,
    ProviderConfig, ProviderContinuation, ProviderError, ProviderErrorKind, ProviderFactory,
    ProviderId, ProviderProfile, ReasoningConfig, ReasoningContent, ReasoningPortability,
    RequestOptions, StopReason, StreamAccumulator, StreamEvent, ToolChoice, ToolConstraint,
    ToolSpec, UserContent, MAX_CONSTRAINT_SCHEMA_BYTES,
};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::Duration;

struct OAuthTestRefresher;

#[async_trait]
impl CredentialRefresher for OAuthTestRefresher {
    async fn refresh(
        &self,
        provider: &ProviderId,
        _refresh_token: Option<&str>,
    ) -> Result<OAuthCredential, ProviderError> {
        Ok(OAuthCredential::new(
            provider.clone(),
            "fresh-wif-access-token",
            Some("fresh-wif-refresh-token"),
            None,
        ))
    }
}

async fn fixture(
    response_body: String,
    content_type: &'static str,
) -> (String, oneshot::Receiver<String>) {
    fixture_with_status(200, response_body, content_type).await
}

async fn fixture_with_status(
    status: u16,
    response_body: String,
    content_type: &'static str,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let observed = format!("http://{address}/v1");
    let (request_sender, request_receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = socket.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let content_length = String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                while request.len() < header_end + content_length {
                    let count = socket.read(&mut chunk).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                }
                let body = String::from_utf8_lossy(
                    &request
                        [header_end..header_end + content_length.min(request.len() - header_end)],
                );
                let _ = request_sender.send(body.into_owned());
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 {status} Fixture\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (observed, request_receiver)
}

async fn request_line_fixture(
    response_body: String,
    content_type: &'static str,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let observed = format!("http://{address}/v1");
    let (request_sender, request_receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = socket.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            if let Some(line_end) = request.windows(2).position(|window| window == b"\r\n") {
                let request_line = String::from_utf8_lossy(&request[..line_end]).into_owned();
                let _ = request_sender.send(request_line);
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 Fixture\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (observed, request_receiver)
}

async fn header_fixture(
    response_body: String,
    content_type: &'static str,
) -> (String, oneshot::Receiver<Vec<(String, String)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let observed = format!("http://{address}/v1");
    let (request_sender, request_receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = socket.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            let Some(header_start) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let header_end = header_start + 4;
            let headers = String::from_utf8_lossy(&request[..header_end])
                .lines()
                .skip(1)
                .filter_map(|line| {
                    line.split_once(':')
                        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().into()))
                })
                .collect();
            let _ = request_sender.send(headers);
            break;
        }
        let response = format!(
            "HTTP/1.1 200 Fixture\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (observed, request_receiver)
}

async fn blackhole_loopback_base_url_with_acceptance() -> (String, oneshot::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted_sender, accepted_receiver) = oneshot::channel();
    tokio::spawn(async move {
        if let Ok((_socket, _)) = listener.accept().await {
            let _ = accepted_sender.send(());
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    (format!("http://{address}/v1"), accepted_receiver)
}

async fn openai_tool_start_indices(
    body: String,
) -> Result<Vec<usize>, jarvis_model_provider::ProviderError> {
    let (base_url, _) = fixture(body, "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut stream = provider
        .stream(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await?;
    let mut indices = Vec::new();
    while let Some(event) = futures::StreamExt::next(&mut stream).await {
        if let StreamEvent::ToolCallStart { index, .. } = event? {
            indices.push(index);
        }
    }
    Ok(indices)
}

async fn openai_stream_error(body: String) -> jarvis_model_provider::ProviderError {
    let (base_url, _) = fixture(body, "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut stream = provider
        .stream(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();
    while let Some(event) = futures::StreamExt::next(&mut stream).await {
        if let Err(error) = event {
            return error;
        }
    }
    panic!("OpenAI stream unexpectedly completed without an error");
}

fn request(model: ModelSpec) -> CompletionRequest {
    request_with_max_output_tokens(model, Some(64))
}

fn request_with_max_output_tokens(
    model: ModelSpec,
    max_output_tokens: Option<u32>,
) -> CompletionRequest {
    let mut request = CompletionRequest::new(
        model,
        vec![Message::system("You are concise."), Message::user("hello")],
    );
    request.max_output_tokens = max_output_tokens;
    request
}

fn reasoning_config(
    enabled: bool,
    budget_tokens: Option<u32>,
    effort: Option<&'static str>,
    summary: Option<&'static str>,
) -> ReasoningConfig {
    let mut config = if enabled {
        ReasoningConfig::enabled(budget_tokens)
    } else {
        ReasoningConfig::disabled()
    };
    if enabled {
        if let Some(effort) = effort {
            config = config.with_effort(effort);
        }
        if let Some(summary) = summary {
            config = config.with_summary(summary);
        }
    }
    config
}

fn without_reasoning_effort() -> OpenAiCompletionsCompatibility {
    OpenAiCompletionsCompatibility {
        supports_reasoning_effort: false,
        ..OpenAiCompletionsCompatibility::default()
    }
}

fn context_overflow_request(provider: &str, api: Api) -> CompletionRequest {
    let mut model = ModelSpec::custom(
        "small-context-model",
        ProviderId::new(provider).unwrap(),
        api,
    );
    model.context_window = Some(32);
    model.max_output_tokens = Some(64);
    let mut request = request_with_max_output_tokens(model, Some(16));
    request.messages = vec![Message::user("x".repeat(200))];
    request
}

#[tokio::test]
async fn known_context_overflow_fails_before_dispatch_for_all_protocols() {
    let (openai_url, openai_accepted) = blackhole_loopback_base_url_with_acceptance().await;
    let openai = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&openai_url)
        .unwrap();
    let error = openai
        .complete(context_overflow_request("openai", Api::OpenAiCompletions))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), openai_accepted)
            .await
            .is_err()
    );

    let (responses_url, responses_accepted) = blackhole_loopback_base_url_with_acceptance().await;
    let responses = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&responses_url)
        .unwrap();
    let error = responses
        .complete(context_overflow_request("openai", Api::OpenAiResponses))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), responses_accepted)
            .await
            .is_err()
    );

    let (anthropic_url, anthropic_accepted) = blackhole_loopback_base_url_with_acceptance().await;
    let anthropic = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&anthropic_url)
        .unwrap();
    let error = anthropic
        .complete(context_overflow_request(
            "anthropic",
            Api::AnthropicMessages,
        ))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), anthropic_accepted)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn scripted_provider_normalizes_history_before_capability_validation() {
    let provider = ScriptedProvider::new(vec![
        Ok(StreamEvent::Start {
            model: "scripted-model".into(),
        }),
        Ok(StreamEvent::Done {
            stop_reason: StopReason::Stop,
        }),
    ])
    .unwrap()
    .with_api(Api::OpenAiResponses);
    let model = ModelSpec::custom(
        "scripted-model",
        ProviderId::new("scripted").unwrap(),
        Api::OpenAiResponses,
    )
    .with_capabilities(ModelCapabilities {
        reasoning: false,
        ..ModelCapabilities::default()
    });
    let mut request = request(model);
    request.messages.insert(
        1,
        Message::assistant(AssistantMessage {
            content: vec![AssistantContent::Reasoning(ReasoningContent {
                text: "portable summary".into(),
                redacted: false,
                portability: ReasoningPortability::Portable,
                continuation_ref: None,
            })],
        }),
    );

    let completion = provider.complete(request).await.unwrap();
    assert_eq!(completion.stop_reason, StopReason::Stop);
}

#[test]
fn openai_compatibility_defaults_system_role_for_legacy_metadata() {
    let compatibility: OpenAiCompletionsCompatibility = serde_json::from_value(serde_json::json!({
        "max_output_tokens_field": "max_tokens",
    }))
    .unwrap();

    assert!(compatibility.supports_reasoning_effort);
    assert_eq!(compatibility.system_role, OpenAiSystemRole::System);
    assert_eq!(
        compatibility.thinking_dialect,
        OpenAiThinkingDialect::OpenAi
    );
}

#[tokio::test]
async fn openai_together_dialect_serializes_reasoning_toggle() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let compatibility: OpenAiCompletionsCompatibility = serde_json::from_value(serde_json::json!({
        "supports_reasoning_effort": false,
        "thinking_dialect": "together",
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(compatibility).unwrap()["thinking_dialect"],
        "together"
    );
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(compatibility)
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "together-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::enabled(None));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["reasoning"]["enabled"], true);
    assert!(body.get("thinking").is_none());
    assert!(body.get("reasoning_effort").is_none());
}

#[tokio::test]
async fn openai_together_dialect_serializes_disabled_reasoning() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            supports_reasoning_effort: false,
            thinking_dialect: OpenAiThinkingDialect::Together,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "together-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::disabled());
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["reasoning"]["enabled"], false);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn openai_qwen_dialect_serializes_enable_thinking_and_budget() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let compatibility: OpenAiCompletionsCompatibility = serde_json::from_value(serde_json::json!({
        "supports_reasoning_effort": false,
        "thinking_dialect": "qwen",
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(compatibility).unwrap()["thinking_dialect"],
        "qwen"
    );
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(compatibility)
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "qwen-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::enabled(Some(512)));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["enable_thinking"], true);
    assert_eq!(body["thinking_budget"], 512);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("reasoning").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn openai_qwen_chat_template_dialect_serializes_chat_template_kwargs() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let compatibility: OpenAiCompletionsCompatibility = serde_json::from_value(serde_json::json!({
        "supports_reasoning_effort": false,
        "thinking_dialect": "qwen_chat_template",
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(compatibility).unwrap()["thinking_dialect"],
        "qwen_chat_template"
    );
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(compatibility)
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "qwen-chat-template-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::disabled());
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(body["chat_template_kwargs"]["preserve_thinking"], true);
    assert!(body.get("enable_thinking").is_none());
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("reasoning").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn openai_qwen_chat_template_dialect_serializes_enabled_thinking() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            supports_reasoning_effort: false,
            thinking_dialect: OpenAiThinkingDialect::QwenChatTemplate,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "qwen-chat-template-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::enabled(Some(512)));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
    assert_eq!(body["chat_template_kwargs"]["preserve_thinking"], true);
    assert!(body.get("enable_thinking").is_none());
    assert!(body.get("thinking_budget").is_none());
}

#[tokio::test]
async fn openai_qwen_dialect_serializes_disabled_thinking() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            supports_reasoning_effort: false,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "qwen-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::disabled());
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["enable_thinking"], false);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("reasoning").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn openai_qwen_dialect_omits_toggle_without_reasoning() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete(request(ModelSpec::custom(
            "qwen-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert!(body.get("enable_thinking").is_none());
}

#[tokio::test]
async fn openai_qwen_reasoning_response_preserves_content_and_usage() {
    let body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "answer",
                "reasoning_content": "plan"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 9,
            "total_tokens": 21,
            "completion_tokens_details": {"reasoning_tokens": 5}
        }
    });
    let (base_url, _) = fixture(body.to_string(), "application/json").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            supports_reasoning_effort: false,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();

    let completion = provider
        .complete(request(ModelSpec::custom(
            "qwen-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    assert_eq!(completion.message.text_value(), "answer");
    let reasoning = completion
        .message
        .content
        .iter()
        .filter_map(|part| match part {
            AssistantContent::Reasoning(value) => Some(value.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(reasoning, ["plan"]);
    let usage = completion.usage.unwrap();
    assert_eq!(usage.input_tokens, 12);
    assert_eq!(usage.output_tokens, 9);
    assert_eq!(usage.total_tokens, 21);
    assert_eq!(usage.reasoning_tokens, Some(5));
    assert!(usage.has_consistent_accounting());
}

#[tokio::test]
async fn openai_qwen_reasoning_stream_preserves_content_and_usage() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"plan\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\" step\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":9,\"total_tokens\":21,\"completion_tokens_details\":{\"reasoning_tokens\":5}}}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            supports_reasoning_effort: false,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();

    let completion = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "qwen-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(completion.message.text_value(), "answer");
    assert_eq!(completion.message.reasoning_chars(), 9);
    let usage = completion.usage.unwrap();
    assert_eq!(usage.input_tokens, 12);
    assert_eq!(usage.output_tokens, 9);
    assert_eq!(usage.total_tokens, 21);
    assert_eq!(usage.reasoning_tokens, Some(5));
    assert!(usage.has_consistent_accounting());
}

#[tokio::test]
async fn openai_qwen_usage_preserves_explicit_zero_reasoning_tokens() {
    let body = serde_json::json!({
        "choices": [{
            "message": {"role": "assistant", "content": "answer"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 4,
            "completion_tokens": 2,
            "total_tokens": 6,
            "completion_tokens_details": {"reasoning_tokens": 0}
        }
    });
    let (base_url, _) = fixture(body.to_string(), "application/json").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            supports_reasoning_effort: false,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();

    let completion = provider
        .complete(request(ModelSpec::custom(
            "qwen-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    assert_eq!(completion.usage.unwrap().reasoning_tokens, Some(0));
}

#[tokio::test]
async fn openai_qwen_reasoning_history_round_trips_as_reasoning_content() {
    let first_body = serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "answer",
                "reasoning_content": "plan"
            },
            "finish_reason": "stop"
        }]
    });
    let (first_base_url, _) = fixture(first_body.to_string(), "application/json").await;
    let first_provider =
        OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
            .unwrap()
            .with_compatibility(OpenAiCompletionsCompatibility {
                supports_reasoning_effort: false,
                thinking_dialect: OpenAiThinkingDialect::Qwen,
                ..OpenAiCompletionsCompatibility::default()
            })
            .with_base_url(&first_base_url)
            .unwrap();
    let first = first_provider
        .complete(request(ModelSpec::custom(
            "qwen-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let second_body =
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"continued\"},\"finish_reason\":\"stop\"}]}";
    let (second_base_url, request_receiver) = fixture(second_body.into(), "application/json").await;
    let second_provider =
        OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
            .unwrap()
            .with_compatibility(OpenAiCompletionsCompatibility {
                supports_reasoning_effort: false,
                thinking_dialect: OpenAiThinkingDialect::Qwen,
                ..OpenAiCompletionsCompatibility::default()
            })
            .with_base_url(&second_base_url)
            .unwrap();
    let mut follow_up = request(ModelSpec::custom(
        "qwen-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    follow_up.messages.push(Message::Assistant(first.message));
    follow_up.messages.push(Message::user("continue"));
    second_provider.complete(follow_up).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    let assistant = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .expect("the follow-up request must contain the assistant history");
    assert_eq!(assistant["content"], "answer");
    assert_eq!(assistant["reasoning_content"], "plan");
    assert!(assistant.get("reasoning").is_none());
}

#[tokio::test]
async fn openai_thinking_object_dialect_serializes_enabled_reasoning() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            thinking_dialect: OpenAiThinkingDialect::ThinkingObject,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "high");
}

#[tokio::test]
async fn openai_thinking_object_dialect_serializes_disabled_reasoning() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            thinking_dialect: OpenAiThinkingDialect::ThinkingObject,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::disabled());
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["thinking"]["type"], "disabled");
    assert!(body.get("reasoning_effort").is_none());
}

fn header_values<'a>(headers: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect()
}

#[tokio::test]
async fn anthropic_extra_headers_override_defaults_without_duplicates() {
    let (base_url, request_headers) = header_fixture(
        "{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete_with(
            request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            )),
            RequestOptions {
                abort: None,
                headers: vec![
                    ("ANTHROPIC-VERSION".into(), "caller-first".into()),
                    ("anthropic-version".into(), "caller-last".into()),
                    ("Content-Type".into(), "application/custom".into()),
                    ("X-API-KEY".into(), "caller-key".into()),
                ],
            },
        )
        .await
        .unwrap();
    let headers = request_headers.await.unwrap();

    assert_eq!(
        header_values(&headers, "anthropic-version"),
        vec!["caller-last"]
    );
    assert_eq!(header_values(&headers, "x-api-key"), vec!["secret"]);
    assert_eq!(
        header_values(&headers, "content-type"),
        vec!["application/custom"]
    );
}

#[tokio::test]
async fn openai_extra_headers_protect_configured_authorization() {
    let (base_url, request_headers) = header_fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete_with(
            request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            )),
            RequestOptions {
                abort: None,
                headers: vec![
                    ("AUTHORIZATION".into(), "Bearer caller".into()),
                    ("x-request-id".into(), "caller-first".into()),
                    ("X-Request-ID".into(), "caller-last".into()),
                ],
            },
        )
        .await
        .unwrap();
    let headers = request_headers.await.unwrap();

    assert_eq!(
        header_values(&headers, "authorization"),
        vec!["Bearer secret"]
    );
    assert_eq!(header_values(&headers, "x-request-id"), vec!["caller-last"]);
    assert_eq!(
        header_values(&headers, "content-type"),
        vec!["application/json"]
    );
}

#[tokio::test]
async fn responses_extra_headers_protect_configured_authorization() {
    let (base_url, request_headers) = header_fixture(
        "{\"id\":\"resp_1\",\"error\":null,\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete_with(
            request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiResponses,
            )),
            RequestOptions {
                abort: None,
                headers: vec![
                    ("authorization".into(), "Bearer caller".into()),
                    ("X-Request-ID".into(), "caller-first".into()),
                    ("x-request-id".into(), "caller-last".into()),
                ],
            },
        )
        .await
        .unwrap();
    let headers = request_headers.await.unwrap();

    assert_eq!(
        header_values(&headers, "authorization"),
        vec!["Bearer secret"]
    );
    assert_eq!(header_values(&headers, "x-request-id"), vec!["caller-last"]);
    assert_eq!(
        header_values(&headers, "content-type"),
        vec!["application/json"]
    );
}

#[tokio::test]
async fn caller_credentials_are_allowed_when_provider_key_is_empty() {
    let (base_url, request_headers) = header_fixture(
        "{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete_with(
            request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            )),
            RequestOptions {
                abort: None,
                headers: vec![("x-api-key".into(), "caller-key".into())],
            },
        )
        .await
        .unwrap();
    let headers = request_headers.await.unwrap();

    assert_eq!(header_values(&headers, "x-api-key"), vec!["caller-key"]);
}

#[tokio::test]
async fn openai_compatible_normalizes_sse_and_usage() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3,\"total_tokens\":5}}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("gateway").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let model = ModelSpec::custom(
        "gateway-model",
        ProviderId::new("gateway").unwrap(),
        Api::OpenAiCompletions,
    );
    let completion =
        jarvis_model_provider::collect_stream(provider.stream(request(model)).await.unwrap())
            .await
            .unwrap();
    assert_eq!(completion.message.text_value(), "hello");
    assert_eq!(completion.stop_reason, StopReason::Stop);
    assert_eq!(completion.usage.unwrap().total_tokens, 5);
    let request = request_receiver.await.unwrap();
    assert!(request.contains("\"stream\":true"));
    assert!(request.contains("\"model\":\"gateway-model\""));
    let body: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(body["max_tokens"], 64);
    assert!(body.get("max_completion_tokens").is_none());
}

#[tokio::test]
async fn openai_max_output_defaults_to_max_tokens() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["max_tokens"], 64);
    assert!(body.get("max_completion_tokens").is_none());
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
}

#[tokio::test]
async fn openai_system_role_can_use_developer() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["messages"][0]["role"], "developer");
    assert_eq!(body["messages"][1]["role"], "user");
}

#[tokio::test]
async fn openai_max_output_can_use_max_completion_tokens() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_max_output_tokens_field(MaxOutputTokensField::MaxCompletionTokens)
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["max_completion_tokens"], 64);
    assert!(body.get("max_tokens").is_none());
}

#[tokio::test]
async fn openai_omits_max_output_fields_without_a_limit() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    provider
        .complete(request_with_max_output_tokens(
            ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            ),
            None,
        ))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("max_completion_tokens").is_none());
}

#[tokio::test]
async fn openai_stream_can_use_max_completion_tokens() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_max_output_tokens_field(MaxOutputTokensField::MaxCompletionTokens)
        .with_base_url(&base_url)
        .unwrap();
    jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["max_completion_tokens"], 64);
    assert!(body.get("max_tokens").is_none());
}

#[tokio::test]
async fn openai_stream_can_use_developer_system_role() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["messages"][0]["role"], "developer");
    assert_eq!(body["messages"][1]["role"], "user");
}

#[tokio::test]
async fn openai_thinking_object_dialect_works_for_streaming() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            thinking_dialect: OpenAiThinkingDialect::ThinkingObject,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    jarvis_model_provider::collect_stream(provider.stream(request).await.unwrap())
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "high");
}

#[tokio::test]
async fn openai_together_dialect_works_for_streaming() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            thinking_dialect: OpenAiThinkingDialect::Together,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "together-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    jarvis_model_provider::collect_stream(provider.stream(request).await.unwrap())
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["reasoning"]["enabled"], true);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn openai_qwen_dialect_works_for_streaming() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(OpenAiCompletionsCompatibility {
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            ..OpenAiCompletionsCompatibility::default()
        })
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "qwen-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    jarvis_model_provider::collect_stream(provider.stream(request).await.unwrap())
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["enable_thinking"], true);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("reasoning").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn provider_factory_can_select_max_completion_tokens() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = ProviderFactory::build_with_max_output_tokens_field(
        ProviderConfig {
            provider_id: ProviderId::new("openai").unwrap(),
            api: Api::OpenAiCompletions,
            api_key: "secret".into(),
            base_url: Some(base_url),
            endpoint_policy: EndpointPolicy::SecureOrLoopback,
            request_timeout: Duration::from_secs(5),
        },
        MaxOutputTokensField::MaxCompletionTokens,
    )
    .unwrap();
    provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["max_completion_tokens"], 64);
    assert!(body.get("max_tokens").is_none());
}

#[tokio::test]
async fn provider_factory_can_apply_combined_openai_compatibility() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = ProviderFactory::build_with_openai_completions_compatibility(
        ProviderConfig {
            provider_id: ProviderId::new("openai").unwrap(),
            api: Api::OpenAiCompletions,
            api_key: "secret".into(),
            base_url: Some(base_url),
            endpoint_policy: EndpointPolicy::SecureOrLoopback,
            request_timeout: Duration::from_secs(5),
        },
        OpenAiCompletionsCompatibility {
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            supports_reasoning_effort: false,
            system_role: OpenAiSystemRole::Developer,
            thinking_dialect: OpenAiThinkingDialect::ThinkingObject,
        },
    )
    .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["max_completion_tokens"], 64);
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("reasoning_effort").is_none());
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["messages"][0]["role"], "developer");
    assert_eq!(body["messages"][1]["role"], "user");
}

#[tokio::test]
async fn anthropic_normalizes_content_blocks_and_usage() {
    let body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":2,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let model = ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    );
    let completion =
        jarvis_model_provider::collect_stream(provider.stream(request(model)).await.unwrap())
            .await
            .unwrap();
    assert_eq!(completion.message.text_value(), "hello");
    assert_eq!(completion.stop_reason, StopReason::Stop);
    assert_eq!(completion.usage.unwrap().total_tokens, 5);
}

#[tokio::test]
async fn anthropic_stream_preserves_usage_cache_tokens() {
    let body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":2,\"output_tokens\":0,\"cache_read_input_tokens\":7,\"cache_creation_input_tokens\":11}}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let completion = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap();

    let usage = completion.usage.unwrap();
    assert_eq!(usage.input_tokens, 20);
    assert_eq!(usage.output_tokens, 3);
    assert_eq!(usage.total_tokens, 23);
    assert_eq!(usage.cache_read_tokens, Some(7));
    assert_eq!(usage.cache_write_tokens, Some(11));
    assert!(usage.has_consistent_accounting());
    assert_eq!(completion.stop_reason, StopReason::Stop);
}

#[tokio::test]
async fn anthropic_stream_preserves_thinking_signature_redaction_and_tools() {
    let body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":2,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"plan\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"opaque\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"lookup\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":1}\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":3}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    // Use the stream path so every wire event is exercised by the normalized
    // parser and accumulator.
    let completion = provider
        .stream(request(ModelSpec::custom(
            "claude-test",
            ProviderId::new("anthropic").unwrap(),
            Api::AnthropicMessages,
        )))
        .await
        .unwrap();
    let completion = jarvis_model_provider::collect_stream(completion)
        .await
        .unwrap();
    let reasoning = completion
        .message
        .content
        .iter()
        .filter_map(|part| match part {
            jarvis_model_provider::AssistantContent::Reasoning(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(reasoning[0].text, "plan");
    assert_eq!(
        reasoning[0].portability,
        ReasoningPortability::ProviderBound
    );
    assert!(reasoning[1].redacted);
    assert_eq!(
        reasoning[1].portability,
        ReasoningPortability::ProviderBound
    );
    let Some(ProviderContinuation::AnthropicMessages(continuation)) =
        completion.continuation.as_ref()
    else {
        panic!("Anthropic reasoning completion must expose a replay sidecar")
    };
    assert_eq!(continuation.reasoning_entry_count(), 2);
    assert_eq!(
        reasoning[0].continuation_ref.as_ref(),
        Some(continuation.reasoning_entries()[0].reference())
    );
    assert_eq!(
        reasoning[1].continuation_ref.as_ref(),
        Some(continuation.reasoning_entries()[1].reference())
    );
    let continuation_json = serde_json::to_string(continuation).unwrap();
    assert!(continuation_json.contains("sig"));
    assert!(continuation_json.contains("opaque"));
    assert_eq!(completion.message.tool_calls()[0].id, "call_1");
    assert_eq!(completion.usage.unwrap().total_tokens, 5);
    assert_eq!(completion.stop_reason, StopReason::ToolUse);
}

#[tokio::test]
async fn model_binding_rejects_before_network_dispatch() {
    let (base_url, request_receiver) = fixture("{}".into(), "application/json").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = match provider
        .stream(request(ModelSpec::custom(
            "claude-test",
            ProviderId::new("openai").unwrap(),
            Api::AnthropicMessages,
        )))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("mismatched provider/API binding was accepted"),
    };
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::BeforeDispatch
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), request_receiver)
            .await
            .is_err()
    );

    let (base_url, request_receiver) = fixture("{}".into(), "application/json").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = match provider
        .stream(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("anthropic").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("mismatched provider/API binding was accepted"),
    };
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::BeforeDispatch
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), request_receiver)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn openai_send_timeout_remains_ambiguous() {
    let (base_url, accepted) = blackhole_loopback_base_url_with_acceptance().await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap()
        .with_request_timeout(Duration::from_millis(100));
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(
            provider.complete(request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            ))),
            accepted
        )
    })
    .await
    .expect("provider timeout test did not complete");
    let error = result.unwrap_err();
    assert_eq!(
        error.kind,
        ProviderErrorKind::Timeout,
        "phase={:?} message={} status={:?}",
        error.phase,
        error.message,
        error.http_status
    );
    assert_eq!(error.phase, jarvis_model_provider::FailurePhase::Unknown);
}

#[tokio::test]
async fn anthropic_send_timeout_remains_ambiguous() {
    let (base_url, accepted) = blackhole_loopback_base_url_with_acceptance().await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap()
        .with_request_timeout(Duration::from_millis(100));
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(
            provider.complete(request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            ))),
            accepted
        )
    })
    .await
    .expect("provider timeout test did not complete");
    let error = result.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert_eq!(error.phase, jarvis_model_provider::FailurePhase::Unknown);
}

#[tokio::test]
async fn openai_http_408_is_timeout_after_dispatch() {
    let (base_url, _) = fixture_with_status(
        408,
        r#"{"error":{"message":"request timed out"}}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::AfterDispatch
    );
    assert_eq!(error.http_status, Some(408));
}

#[tokio::test]
async fn anthropic_http_408_is_timeout_after_dispatch() {
    let (base_url, _) = fixture_with_status(
        408,
        r#"{"error":{"message":"request timed out"}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = provider
        .complete(request(ModelSpec::custom(
            "claude-test",
            ProviderId::new("anthropic").unwrap(),
            Api::AnthropicMessages,
        )))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::AfterDispatch
    );
    assert_eq!(error.http_status, Some(408));
}

#[tokio::test]
async fn openai_missing_index_matches_existing_provider_call_id() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":2,\"id\":\"call_a\",\"function\":{}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_a\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let indices = openai_tool_start_indices(body.into())
        .await
        .expect("matching provider id should resolve the existing call");
    assert_eq!(indices, vec![2]);
}

#[tokio::test]
async fn openai_missing_index_uses_the_only_existing_non_zero_call() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":5,\"id\":\"call_a\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let indices = openai_tool_start_indices(body.into())
        .await
        .expect("the only partial call should resolve by its actual index");
    assert_eq!(indices, vec![5]);
}

#[tokio::test]
async fn openai_missing_index_with_new_id_allocates_next_index() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":2,\"id\":\"call_a\",\"function\":{\"name\":\"first\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_b\",\"function\":{\"name\":\"second\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let indices = openai_tool_start_indices(body.into())
        .await
        .expect("a new provider id should allocate a deterministic index");
    assert_eq!(indices, vec![2, 3]);
}

#[tokio::test]
async fn openai_ambiguous_missing_index_and_id_fails_closed() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_a\",\"function\":{\"name\":\"first\"}},{\"index\":3,\"id\":\"call_b\",\"function\":{\"name\":\"second\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n"
    );
    let error = openai_stream_error(body.into()).await;
    assert_eq!(error.kind, ProviderErrorKind::Protocol);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::DuringStream
    );
    assert!(error.message.contains("ambiguous"));
}

#[tokio::test]
async fn openai_non_function_streamed_tool_type_fails_closed() {
    let body =
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"custom\",\"function\":{\"name\":\"lookup\"}}]},\"finish_reason\":null}]}\n\n";
    let error = openai_stream_error(body.into()).await;
    assert_eq!(error.kind, ProviderErrorKind::Protocol);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::DuringStream
    );
}

#[tokio::test]
async fn openai_stream_rejects_tool_call_id_change() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_b\",\"function\":{}}]},\"finish_reason\":null}]}\n\n",
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut stream = provider
        .stream(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();
    let mut error = None;
    while let Some(event) = futures::StreamExt::next(&mut stream).await {
        if let Err(value) = event {
            error = Some(value);
            break;
        }
    }
    let error = error.expect("provider stream must reject tool identity mutation");
    assert_eq!(error.kind, ProviderErrorKind::Protocol);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::DuringStream
    );
}

#[tokio::test]
async fn openai_stream_rejects_tool_call_name_change() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"other\"}}]},\"finish_reason\":null}]}\n\n",
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut stream = provider
        .stream(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();
    let mut error = None;
    while let Some(event) = futures::StreamExt::next(&mut stream).await {
        if let Err(value) = event {
            error = Some(value);
            break;
        }
    }
    let error = error.expect("provider stream must reject tool identity mutation");
    assert_eq!(error.kind, ProviderErrorKind::Protocol);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::DuringStream
    );
}

#[tokio::test]
async fn anthropic_non_stream_completion_uses_normalized_response() {
    let body = serde_json::json!({
        "content": [{"type": "text", "text": "complete"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 4, "output_tokens": 2}
    });
    let (base_url, _) = fixture(body.to_string(), "application/json").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let completion = provider
        .complete(request(ModelSpec::custom(
            "claude-test",
            ProviderId::new("anthropic").unwrap(),
            Api::AnthropicMessages,
        )))
        .await
        .unwrap();
    assert_eq!(completion.message.text_value(), "complete");
    assert_eq!(completion.usage.unwrap().total_tokens, 6);
}

#[tokio::test]
async fn anthropic_unsigned_thinking_history_is_downgraded_before_dispatch() {
    let (base_url, request_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    request.messages.push(Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: "plan".into(),
            redacted: false,
            portability: ReasoningPortability::Portable,
            continuation_ref: None,
        })],
    }));
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["messages"][1]["content"][0]["type"], "text");
    assert_eq!(body["messages"][1]["content"][0]["text"], "plan");
}

#[tokio::test]
async fn anthropic_native_reasoning_history_preserves_signature_and_redaction() {
    let (base_url, request_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    let thinking_ref = ContinuationRef::new("anthropic-thinking").unwrap();
    let redacted_ref = ContinuationRef::new("anthropic-redacted").unwrap();
    request.messages.push(Message::Assistant(AssistantMessage {
        content: vec![
            AssistantContent::Reasoning(ReasoningContent {
                text: "plan".into(),
                redacted: false,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: Some(thinking_ref.clone()),
            }),
            AssistantContent::Reasoning(ReasoningContent {
                text: String::new(),
                redacted: true,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: Some(redacted_ref.clone()),
            }),
        ],
    }));
    request.continuation = Some(ProviderContinuation::AnthropicMessages(
        AnthropicMessagesContinuation::with_scope(
            ProviderId::new("anthropic").unwrap(),
            "claude-test",
            ContinuationScope::for_history(&request.messages).unwrap(),
            vec![
                AnthropicReasoningReplayEntry::new(
                    thinking_ref,
                    AnthropicReasoningReplay::thinking("sig"),
                ),
                AnthropicReasoningReplayEntry::new(
                    redacted_ref,
                    AnthropicReasoningReplay::redacted("opaque"),
                ),
            ],
        )
        .unwrap(),
    ));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    let blocks = body["messages"][1]["content"].as_array().unwrap();
    assert_eq!(blocks[0]["type"], "thinking");
    assert_eq!(blocks[0]["signature"], "sig");
    assert_eq!(blocks[1]["type"], "redacted_thinking");
    assert_eq!(blocks[1]["data"], "opaque");
}

#[tokio::test]
async fn anthropic_multi_turn_reasoning_replay_keeps_each_reference_bound() {
    let (first_url, _) = fixture(
        r#"{"content":[{"type":"thinking","thinking":"plan A","signature":"signature-A"},{"type":"tool_use","id":"call-A","name":"lookup","input":{"q":"a"}}],"stop_reason":"tool_use","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&first_url)
        .unwrap();
    let model = ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    );
    let first = provider.complete(request(model.clone())).await.unwrap();
    let first_call = first.message.tool_calls()[0].id.clone();
    let first_message = first.message.clone();
    let first_reference = first
        .message
        .content
        .iter()
        .find_map(|part| match part {
            AssistantContent::Reasoning(reasoning) => reasoning.continuation_ref.clone(),
            _ => None,
        })
        .expect("first reasoning block must have a stable reference");
    let first_continuation = first
        .continuation
        .clone()
        .expect("first reasoning block must have replay state");

    let (second_url, second_receiver) = fixture(
        r#"{"content":[{"type":"thinking","thinking":"plan B","signature":"signature-B"},{"type":"tool_use","id":"call-B","name":"lookup","input":{"q":"b"}}],"stop_reason":"tool_use","usage":{"input_tokens":2,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&second_url)
        .unwrap();
    let mut next = request(model.clone());
    next.messages
        .push(Message::Assistant(first_message.clone()));
    next.messages.push(Message::tool_result(
        first_call.clone(),
        Some("lookup".into()),
        "result A",
    ));
    next.messages.push(Message::user("second question"));
    next.continuation = Some(first_continuation);
    let second = provider.complete(next).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&second_receiver.await.unwrap()).unwrap();
    let prior_assistant = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .expect("prior assistant turn must be replayed");
    assert_eq!(prior_assistant["content"][0]["type"], "thinking");
    assert_eq!(prior_assistant["content"][0]["signature"], "signature-A");
    assert_eq!(prior_assistant["content"][1]["type"], "tool_use");
    assert_eq!(prior_assistant["content"][1]["id"], "call-A");

    let continuation = second
        .continuation
        .as_ref()
        .and_then(ProviderContinuation::anthropic_messages)
        .expect("second reasoning block must retain both replay entries");
    assert_eq!(continuation.reasoning_entry_count(), 2);
    let serialized = serde_json::to_value(continuation).unwrap();
    let entries = serialized["reasoning"].as_array().unwrap();
    let first_entry = entries
        .iter()
        .find(|entry| entry["reference"] == first_reference.as_str())
        .expect("first reference must have a replay entry");
    assert_eq!(first_entry["state"]["kind"], "thinking");
    assert_eq!(first_entry["state"]["signature"], "signature-A");
    let second_reference = second
        .message
        .content
        .iter()
        .find_map(|part| match part {
            AssistantContent::Reasoning(reasoning) => reasoning.continuation_ref.clone(),
            _ => None,
        })
        .expect("second reasoning block must have a stable reference");
    assert_ne!(first_reference, second_reference);
    let second_entry = entries
        .iter()
        .find(|entry| entry["reference"] == second_reference.as_str())
        .expect("second reference must have a replay entry");
    assert_eq!(second_entry["state"]["kind"], "thinking");
    assert_eq!(second_entry["state"]["signature"], "signature-B");

    let (third_url, third_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"done"}],"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&third_url)
        .unwrap();
    let second_call = second.message.tool_calls()[0].id.clone();
    let second_message = second.message.clone();
    let second_continuation = second
        .continuation
        .clone()
        .expect("second continuation must be available for turn three");
    let mut third = request(model);
    third.messages.push(Message::Assistant(first_message));
    third.messages.push(Message::tool_result(
        first_call,
        Some("lookup".into()),
        "result A",
    ));
    third.messages.push(Message::user("second question"));
    third.messages.push(Message::Assistant(second_message));
    third.messages.push(Message::tool_result(
        second_call,
        Some("lookup".into()),
        "result B",
    ));
    third.messages.push(Message::user("third question"));
    third.continuation = Some(second_continuation);
    provider.complete(third).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&third_receiver.await.unwrap()).unwrap();
    let assistants = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "assistant")
        .collect::<Vec<_>>();
    assert_eq!(assistants.len(), 2);
    assert_eq!(assistants[0]["content"][0]["signature"], "signature-A");
    assert_eq!(assistants[1]["content"][0]["signature"], "signature-B");
}

#[tokio::test]
async fn known_non_reasoning_model_uses_capability_aware_history_downgrade() {
    let (base_url, request_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(
        ModelSpec::custom(
            "no-reasoning",
            ProviderId::new("anthropic").unwrap(),
            Api::AnthropicMessages,
        )
        .with_capabilities(ModelCapabilities::default()),
    );
    request.messages.push(Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: "plan".into(),
            redacted: false,
            portability: ReasoningPortability::Portable,
            continuation_ref: None,
        })],
    }));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["messages"][1]["content"][0]["type"], "text");
    assert_eq!(body["messages"][1]["content"][0]["text"], "plan");
}

#[tokio::test]
async fn signed_anthropic_reasoning_replays_as_responses_summary_with_tool_identity() {
    let (base_url, request_receiver) = fixture(
        r#"{"id":"resp_1","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"continued"}]}],"usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    ));
    request.messages.push(Message::Assistant(AssistantMessage {
        content: vec![
            AssistantContent::Reasoning(ReasoningContent {
                text: "plan".into(),
                redacted: false,
                portability: ReasoningPortability::Portable,
                continuation_ref: None,
            }),
            AssistantContent::ToolCall(jarvis_model_provider::ToolCall {
                id: "call-42".into(),
                name: "lookup".into(),
                arguments: serde_json::json!({"q": 1}),
            }),
        ],
    }));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    let input = body["input"].as_array().unwrap();
    let reasoning = input
        .iter()
        .find(|item| item["type"] == "reasoning")
        .expect("signed reasoning should become a Responses summary");
    assert_eq!(reasoning["summary"][0]["text"], "plan");
    assert!(reasoning.get("signature").is_none());
    let tool = input
        .iter()
        .find(|item| item["type"] == "function_call")
        .expect("tool call should remain in history");
    assert_eq!(tool["call_id"], "call-42");
    assert_eq!(tool["name"], "lookup");
}

#[tokio::test]
async fn legacy_redacted_anthropic_reasoning_fails_closed_without_continuation() {
    let (base_url, _request_receiver) = fixture(
        r#"{"choices":[{"message":{"role":"assistant","content":"continued"},"finish_reason":"stop"}]}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.messages.push(Message::Assistant(AssistantMessage {
        content: vec![
            AssistantContent::Reasoning(ReasoningContent {
                text: String::new(),
                redacted: true,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: None,
            }),
            AssistantContent::ToolCall(jarvis_model_provider::ToolCall {
                id: "call-opaque".into(),
                name: "lookup".into(),
                arguments: serde_json::json!({"q": 1}),
            }),
        ],
    }));
    let error = provider.complete(request).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
}

#[tokio::test]
async fn mixed_reasoning_and_tools_keep_order_for_anthropic_and_responses() {
    let assistant = Message::Assistant(AssistantMessage {
        content: vec![
            AssistantContent::Text(jarvis_model_provider::TextContent::new("before")),
            AssistantContent::Reasoning(ReasoningContent {
                text: "plan".into(),
                redacted: false,
                portability: ReasoningPortability::Portable,
                continuation_ref: None,
            }),
            AssistantContent::ToolCall(jarvis_model_provider::ToolCall {
                id: "call-order".into(),
                name: "lookup".into(),
                arguments: serde_json::json!({"q": 1}),
            }),
            AssistantContent::Text(jarvis_model_provider::TextContent::new("after")),
        ],
    });

    let (responses_url, responses_receiver) = fixture(
        r#"{"id":"resp_1","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"continued"}]}],"usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let responses = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&responses_url)
        .unwrap();
    let mut responses_request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    ));
    responses_request.messages.push(assistant.clone());
    responses.complete(responses_request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&responses_receiver.await.unwrap()).unwrap();
    let input = body["input"].as_array().unwrap();
    let assistant_items = input
        .iter()
        .filter(|item| {
            item["role"] == "assistant"
                || item["type"] == "reasoning"
                || item["type"] == "function_call"
        })
        .collect::<Vec<_>>();
    assert_eq!(assistant_items[0]["role"], "assistant");
    assert_eq!(assistant_items[1]["type"], "reasoning");
    assert_eq!(assistant_items[2]["type"], "function_call");
    assert_eq!(assistant_items[2]["call_id"], "call-order");
    assert_eq!(assistant_items[3]["role"], "assistant");

    let mut anthropic_assistant = assistant.clone();
    let Message::Assistant(message) = &mut anthropic_assistant else {
        panic!("expected assistant history")
    };
    let AssistantContent::Reasoning(reasoning) = &mut message.content[1] else {
        panic!("expected reasoning history")
    };
    reasoning.portability = ReasoningPortability::ProviderBound;
    let continuation_ref = ContinuationRef::new("order-reasoning").unwrap();
    reasoning.continuation_ref = Some(continuation_ref.clone());

    let (anthropic_url, anthropic_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let anthropic = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&anthropic_url)
        .unwrap();
    let mut anthropic_request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    anthropic_request.messages.push(anthropic_assistant);
    anthropic_request.continuation = Some(ProviderContinuation::AnthropicMessages(
        AnthropicMessagesContinuation::with_scope(
            ProviderId::new("anthropic").unwrap(),
            "claude-test",
            ContinuationScope::for_history(&anthropic_request.messages).unwrap(),
            vec![AnthropicReasoningReplayEntry::new(
                continuation_ref,
                AnthropicReasoningReplay::thinking("sig"),
            )],
        )
        .unwrap(),
    ));
    anthropic.complete(anthropic_request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&anthropic_receiver.await.unwrap()).unwrap();
    let blocks = body["messages"][1]["content"].as_array().unwrap();
    assert_eq!(blocks[0]["type"], "text");
    assert_eq!(blocks[1]["type"], "thinking");
    assert_eq!(blocks[2]["type"], "tool_use");
    assert_eq!(blocks[2]["id"], "call-order");
    assert_eq!(blocks[3]["type"], "text");
}

#[tokio::test]
async fn aborted_stream_partial_history_can_continue_on_another_protocol() {
    let source = ScriptedProvider::new(vec![
        Ok(StreamEvent::Start {
            model: "source-model".into(),
        }),
        Ok(StreamEvent::TextStart),
        Ok(StreamEvent::TextDelta {
            text: "collected before abort".into(),
        }),
        Err(ProviderError::new(
            ProviderErrorKind::Aborted,
            FailurePhase::DuringStream,
            "test abort",
        )),
    ])
    .unwrap();
    let source_request = request(ModelSpec::custom(
        "source-model",
        ProviderId::new("scripted").unwrap(),
        Api::Custom("test".into()),
    ));
    let mut stream = source.stream(source_request).await.unwrap();
    let mut accumulator = StreamAccumulator::new();
    let mut aborted = false;
    while let Some(event) = futures::StreamExt::next(&mut stream).await {
        match event {
            Ok(event) => accumulator.push(event).unwrap(),
            Err(error) => {
                assert_eq!(error.kind, ProviderErrorKind::Aborted);
                aborted = true;
                break;
            }
        }
    }
    assert!(aborted);

    let (base_url, request_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let target = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let mut follow_up = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    follow_up
        .messages
        .push(Message::Assistant(accumulator.partial_message()));
    follow_up.messages.push(Message::user("continue"));
    target.complete(follow_up).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(
        body["messages"][1]["content"][0]["text"],
        "collected before abort"
    );
    assert_eq!(body["messages"][2]["content"][0]["text"], "continue");
}

#[tokio::test]
async fn anthropic_unknown_sse_event_fails_closed() {
    let body = "event: provider.future_event\ndata: {\"type\":\"provider.future_event\"}\n\n";
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Protocol);
    assert_eq!(
        error.phase,
        jarvis_model_provider::FailurePhase::DuringStream
    );
}

#[tokio::test]
async fn anthropic_stream_rejects_missing_thinking_integrity_fields() {
    let body = concat!(
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Protocol);

    let body = concat!(
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"redacted_thinking\"}}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let error = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "claude-test",
                ProviderId::new("anthropic").unwrap(),
                Api::AnthropicMessages,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Protocol);
}

#[tokio::test]
async fn scripted_events_are_available_without_http() {
    let events = vec![
        Ok(StreamEvent::Start {
            model: "mock".into(),
        }),
        Ok(StreamEvent::Done {
            stop_reason: StopReason::Stop,
        }),
    ];
    let provider = jarvis_model_provider::providers::ScriptedProvider::new(events).unwrap();
    let completion = provider
        .complete(request(ModelSpec::custom(
            "mock",
            ProviderId::new("scripted").unwrap(),
            Api::Custom("test".into()),
        )))
        .await
        .unwrap();
    assert!(completion.message.content.is_empty());
}

#[tokio::test]
async fn abort_before_dispatch_does_not_need_a_live_server() {
    let abort = AbortSignal::new();
    abort.abort();
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy("http://127.0.0.1:1/v1", EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let error = provider
        .complete_with(
            request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            )),
            RequestOptions {
                abort: Some(abort),
                headers: Vec::new(),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Aborted);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
}

#[tokio::test]
async fn openai_image_input_uses_data_url_parts() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.messages = vec![Message::user_parts(vec![
        UserContent::Text(jarvis_model_provider::TextContent::new("describe")),
        UserContent::Image(ImageContent::new("image/png", "AAAA")),
    ])];
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(
        body["messages"][0]["content"][1]["image_url"]["url"],
        "data:image/png;base64,AAAA"
    );
}

#[tokio::test]
async fn anthropic_image_input_uses_base64_source() {
    let (base_url, request_receiver) = fixture(
        "{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    request.messages = vec![Message::user_parts(vec![
        UserContent::Text(jarvis_model_provider::TextContent::new("describe")),
        UserContent::Image(ImageContent::new("image/png", "AAAA")),
    ])];
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["messages"][0]["content"][1]["source"]["data"], "AAAA");
    assert_eq!(
        body["messages"][0]["content"][1]["source"]["media_type"],
        "image/png"
    );
}

#[tokio::test]
async fn openai_tool_choice_and_reasoning_are_serialized() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: Some(ToolConstraint::StrictJsonSchema),
    }];
    request.tool_choice = Some(ToolChoice::Required);
    request.reasoning = Some(reasoning_config(true, Some(128), Some("high"), None));
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["tool_choice"], "required");
    assert_eq!(body["tools"][0]["function"]["strict"], true);
    assert_eq!(body["reasoning_effort"], "high");
    assert!(body.get("thinking_budget").is_none());
    assert!(body.get("reasoning").is_none());
    assert!(body.get("thinking").is_none());
    assert!(body.get("enable_thinking").is_none());
}

#[test]
fn constraint_capability_matrix_matches_the_three_wire_protocols() {
    assert_eq!(
        protocol_constraint_capabilities(&Api::OpenAiCompletions),
        ConstraintCapabilities {
            strict_json_schema: true,
            structured_output: true,
            grammar: false,
        }
    );
    assert_eq!(
        protocol_constraint_capabilities(&Api::OpenAiResponses),
        ConstraintCapabilities {
            strict_json_schema: true,
            structured_output: true,
            grammar: false,
        }
    );
    assert_eq!(
        protocol_constraint_capabilities(&Api::AnthropicMessages),
        ConstraintCapabilities {
            strict_json_schema: true,
            structured_output: true,
            grammar: false,
        }
    );
}

#[tokio::test]
async fn openai_structured_output_constraint_is_serialized() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.output_constraint = Some(OutputConstraint::JsonSchema {
        name: "answer".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        }),
        strict: true,
    });
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["name"], "answer");
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert_eq!(
        body["response_format"]["json_schema"]["schema"]["properties"]["answer"]["type"],
        "string"
    );
}

#[tokio::test]
async fn responses_structured_output_constraint_is_serialized() {
    let (base_url, request_receiver) = fixture(
        "{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    ));
    request.output_constraint = Some(OutputConstraint::JsonSchema {
        name: "answer".into(),
        schema: serde_json::json!({"type": "string"}),
        strict: true,
    });
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["name"], "answer");
    assert_eq!(body["text"]["format"]["strict"], true);
    assert_eq!(body["text"]["format"]["schema"]["type"], "string");
}

#[tokio::test]
async fn responses_strict_tool_schema_is_serialized_with_reasoning_and_choice() {
    let (base_url, request_receiver) = fixture(
        "{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    ));
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: Some(ToolConstraint::StrictJsonSchema),
    }];
    request.tool_choice = Some(ToolChoice::Required);
    request.reasoning = Some(ReasoningConfig::enabled(Some(128)));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["tools"][0]["strict"], true);
    assert_eq!(body["tool_choice"], "required");
    assert_eq!(body["reasoning"]["effort"], "medium");
}

#[tokio::test]
async fn grammar_constraints_fail_before_dispatch_for_all_supported_protocols() {
    for api in [
        Api::OpenAiCompletions,
        Api::OpenAiResponses,
        Api::AnthropicMessages,
    ] {
        let provider = ScriptedProvider::new(Vec::new())
            .unwrap()
            .with_api(api.clone());
        let mut request = request(ModelSpec::custom(
            "test-model",
            ProviderId::new("scripted").unwrap(),
            api,
        ));
        request.output_constraint = Some(OutputConstraint::Grammar {
            grammar: "root ::= word".into(),
        });
        let error = provider.complete(request).await.unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Unsupported);
        assert_eq!(error.phase, FailurePhase::BeforeDispatch);
        assert!(error.message.contains("grammar"));
    }
}

#[tokio::test]
async fn anthropic_strict_tool_schema_is_serialized() {
    let (base_url, request_receiver) = fixture(
        "{\"content\":[{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"lookup\",\"input\":{}}],\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "test-model",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: Some(ToolConstraint::StrictJsonSchema),
    }];
    request.tool_choice = Some(ToolChoice::Required);
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["tools"][0]["strict"], true);
    assert_eq!(body["tool_choice"]["type"], "any");
}

#[tokio::test]
async fn anthropic_structured_output_constraint_is_serialized() {
    let (base_url, request_receiver) = fixture(
        "{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    request.output_constraint = Some(OutputConstraint::JsonSchema {
        name: "ignored-by-anthropic".into(),
        schema: serde_json::json!({"type": "object"}),
        strict: true,
    });
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: Some(ToolConstraint::StrictJsonSchema),
    }];
    request.tool_choice = Some(ToolChoice::Auto);
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert_eq!(body["output_config"]["format"]["schema"]["type"], "object");
    assert!(body["output_config"]["format"].get("name").is_none());
    assert_eq!(body["tools"][0]["strict"], true);
    assert_eq!(body["tool_choice"]["type"], "auto");
}

#[tokio::test]
async fn anthropic_non_strict_structured_output_fails_before_dispatch() {
    let provider = ScriptedProvider::new(Vec::new())
        .unwrap()
        .with_api(Api::AnthropicMessages);
    let mut request = request(ModelSpec::custom(
        "test-model",
        ProviderId::new("scripted").unwrap(),
        Api::AnthropicMessages,
    ));
    request.output_constraint = Some(OutputConstraint::JsonSchema {
        name: "answer".into(),
        schema: serde_json::json!({"type": "string"}),
        strict: false,
    });
    let error = provider.complete(request).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Unsupported);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(error.message.contains("non-strict"));
}

#[tokio::test]
async fn anthropic_manual_thinking_rejects_forced_tool_choice_before_dispatch() {
    let provider = ScriptedProvider::new(Vec::new())
        .unwrap()
        .with_api(Api::AnthropicMessages);
    let mut request = request(ModelSpec::custom(
        "test-model",
        ProviderId::new("scripted").unwrap(),
        Api::AnthropicMessages,
    ));
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: None,
    }];
    request.tool_choice = Some(ToolChoice::Required);
    request.reasoning = Some(ReasoningConfig::enabled(Some(128)));
    let error = provider.complete(request).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(error.message.contains("manual thinking"));
}

#[tokio::test]
async fn oversized_schema_fails_before_dispatch() {
    let provider = ScriptedProvider::new(Vec::new())
        .unwrap()
        .with_api(Api::OpenAiCompletions);
    let mut request = request(ModelSpec::custom(
        "test-model",
        ProviderId::new("scripted").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "description": "x".repeat(MAX_CONSTRAINT_SCHEMA_BYTES),
        }),
        constraint: None,
    }];
    let error = provider.complete(request).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(error.message.contains("serialized bytes"));
}

#[tokio::test]
async fn known_model_without_structured_output_fails_before_dispatch() {
    let provider = ScriptedProvider::new(Vec::new())
        .unwrap()
        .with_api(Api::OpenAiCompletions);
    let model = ModelSpec::custom(
        "no-structured-output",
        ProviderId::new("scripted").unwrap(),
        Api::OpenAiCompletions,
    )
    .with_capabilities(ModelCapabilities {
        structured_output: false,
        ..ModelCapabilities::default()
    });
    let mut request = request(model);
    request.output_constraint = Some(OutputConstraint::JsonSchema {
        name: "answer".into(),
        schema: serde_json::json!({"type": "string"}),
        strict: true,
    });
    let error = provider.complete(request).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Unsupported);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(error.message.contains("model structured output"));
}

#[test]
fn known_model_strict_tool_schema_does_not_require_final_output_metadata() {
    let provider = ScriptedProvider::new(Vec::new())
        .unwrap()
        .with_api(Api::OpenAiCompletions);
    let model = ModelSpec::custom(
        "tool-only-model",
        ProviderId::new("scripted").unwrap(),
        Api::OpenAiCompletions,
    )
    .with_capabilities(ModelCapabilities {
        tools: true,
        structured_output: false,
        ..ModelCapabilities::default()
    });
    let mut request = request(model);
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: Some(ToolConstraint::StrictJsonSchema),
    }];
    provider.validate_request(&request).unwrap();
}

#[tokio::test]
async fn structured_output_rejects_ambiguous_tool_choice_before_dispatch() {
    let provider = ScriptedProvider::new(Vec::new())
        .unwrap()
        .with_api(Api::OpenAiCompletions);
    let mut request = request(ModelSpec::custom(
        "test-model",
        ProviderId::new("scripted").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: None,
    }];
    request.tool_choice = Some(ToolChoice::Required);
    request.output_constraint = Some(OutputConstraint::JsonSchema {
        name: "answer".into(),
        schema: serde_json::json!({"type": "string"}),
        strict: true,
    });
    let error = provider.complete(request).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(error.message.contains("conflicts"));
}

#[tokio::test]
async fn openai_reasoning_effort_defaults_to_none_when_disabled() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::disabled());
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["reasoning_effort"], "none");
    assert!(body.get("thinking_budget").is_none());
    assert!(body.get("thinking").is_none());
}

#[tokio::test]
async fn openai_reasoning_effort_can_be_omitted_for_enabled_reasoning() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(without_reasoning_effort())
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("thinking_budget").is_none());
}

#[tokio::test]
async fn openai_reasoning_effort_opt_out_omits_disabled_reasoning() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(without_reasoning_effort())
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(ReasoningConfig::disabled());
    provider.complete(request).await.unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("thinking_budget").is_none());
}

#[tokio::test]
async fn openai_reasoning_effort_opt_out_works_for_streaming() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, request_receiver) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_compatibility(without_reasoning_effort())
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    request.reasoning = Some(reasoning_config(true, None, Some("high"), None));
    jarvis_model_provider::collect_stream(provider.stream(request).await.unwrap())
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("thinking_budget").is_none());
}

#[tokio::test]
async fn openai_reasoning_effort_is_omitted_without_reasoning_config() {
    let (base_url, request_receiver) = fixture(
        "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}".into(),
        "application/json",
    )
    .await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap();

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert!(body.get("reasoning_effort").is_none());
}

#[tokio::test]
async fn anthropic_thinking_and_auto_tool_choice_are_serialized() {
    let (base_url, request_receiver) = fixture(
        "{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}".into(),
        "application/json",
    )
    .await;
    let provider = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    request.max_output_tokens = Some(2048);
    request.tools = vec![ToolSpec {
        name: "lookup".into(),
        description: "lookup".into(),
        input_schema: serde_json::json!({"type": "object"}),
        constraint: None,
    }];
    request.tool_choice = Some(ToolChoice::Auto);
    request.reasoning = Some(ReasoningConfig::enabled(Some(1024)));
    provider.complete(request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["tool_choice"]["type"], "auto");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 1024);
}

#[tokio::test]
async fn openai_rate_limit_captures_retry_after() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base_url = format!("http://{address}/v1");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = tokio::io::AsyncReadExt::read(&mut socket, &mut chunk)
                .await
                .unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let body = r#"{"error":{"message":"slow down"}}"#;
        let response = format!(
            "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\nretry-after: 7\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
            .await
            .unwrap();
    });
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let error = provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiCompletions,
        )))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::RateLimit);
    assert_eq!(error.retry_after, Some(std::time::Duration::from_secs(7)));
}

#[tokio::test]
async fn openai_usage_preserves_cached_tokens() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":2,\"total_tokens\":22,\"prompt_tokens_details\":{\"cached_tokens\":8,\"cache_write_tokens\":3}}}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let completion = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiCompletions,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    let usage = completion.usage.unwrap();
    assert_eq!(usage.cache_read_tokens, Some(8));
    assert_eq!(usage.cache_write_tokens, Some(3));
    assert!(usage.has_consistent_accounting());
}

#[test]
fn builtin_catalog_lists_openai_and_anthropic() {
    let catalog = ModelCatalog::builtin();
    let gpt = catalog.get("openai", "gpt-4o").unwrap();
    assert_eq!(gpt.api, Api::OpenAiCompletions);
    let gpt5 = catalog.get("openai", "gpt-5").unwrap();
    assert_eq!(gpt5.api, Api::OpenAiResponses);
    let gpt5_mini = catalog.get("openai", "gpt-5-mini").unwrap();
    assert_eq!(gpt5_mini.api, Api::OpenAiResponses);
    for id in ["gpt-4.1", "o3", "o4-mini"] {
        assert_eq!(
            catalog.get("openai", id).unwrap().api,
            Api::OpenAiCompletions
        );
    }
    assert!(gpt.cost.unwrap().input > 0.0);
    assert!(catalog.get("anthropic", "claude-sonnet-4-5").is_some());
    assert!(catalog.providers().contains(&"openai"));
    assert!(catalog.providers().contains(&"anthropic"));
}

#[test]
fn models_connect_id_uses_builtin_responses_api_without_dispatch() {
    let models = Models::new().with_api_key("openai", "sk-test").unwrap();
    let (model, provider) = models.connect_id("openai", "gpt-5").unwrap();

    assert_eq!(model.api, Api::OpenAiResponses);
    assert_eq!(provider.api(), &Api::OpenAiResponses);
}

#[test]
fn models_get_then_connect_keeps_builtin_catalog_behavior() {
    let models = Models::new().with_api_key("openai", "sk-test").unwrap();
    let model = models.get("openai", "gpt-5").unwrap();
    let provider = models.connect(&model).unwrap();

    assert_eq!(model.api, Api::OpenAiResponses);
    assert_eq!(provider.api(), &Api::OpenAiResponses);
}

#[test]
fn legacy_model_spec_without_compatibility_metadata_deserializes() {
    let mut legacy = serde_json::to_value(ModelSpec::custom(
        "legacy-gateway-model",
        ProviderId::new("openai-compatible").unwrap(),
        Api::OpenAiCompletions,
    ))
    .unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("openai_completions_compatibility");

    let model: ModelSpec = serde_json::from_value(legacy).unwrap();
    assert_eq!(model.openai_completions_compatibility, None);
    assert!(serde_json::to_value(model)
        .unwrap()
        .get("openai_completions_compatibility")
        .is_none());
}

#[test]
fn models_connect_non_completions_api_bypasses_chat_compatibility() {
    let mut model = ModelSpec::custom(
        "responses-model",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    );
    model.openai_completions_compatibility = Some(OpenAiCompletionsCompatibility {
        system_role: OpenAiSystemRole::Developer,
        max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
        thinking_dialect: OpenAiThinkingDialect::Qwen,
        supports_reasoning_effort: false,
    });
    let models = Models::new().with_api_key("openai", "sk-test").unwrap();

    let provider = models.connect(&model).unwrap();

    assert_eq!(provider.api(), &Api::OpenAiResponses);
}

#[test]
fn catalog_cost_matches_usage() {
    let model = ModelCatalog::builtin()
        .get("openai", "gpt-4o-mini")
        .unwrap()
        .clone();
    let usage = jarvis_model_provider::Usage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        total_tokens: 2_000_000,
        cache_read_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
    };
    let cost = model.cost_for(&usage).unwrap();
    assert!((cost.input - 0.15).abs() < 1e-9);
    assert!((cost.output - 0.60).abs() < 1e-9);
    assert!((cost.total - 0.75).abs() < 1e-9);
    let rates = ModelCost {
        input: 1.0,
        output: 2.0,
        cache_read: 0.1,
        cache_write: 0.2,
    };
    let with_cache = jarvis_model_provider::Usage {
        input_tokens: 1_000_000,
        output_tokens: 0,
        total_tokens: 1_000_000,
        cache_read_tokens: Some(1_000_000),
        cache_write_tokens: Some(1_000_000),
        reasoning_tokens: None,
    };
    let cost = calculate_cost(&rates, &with_cache);
    assert!((cost.total - 0.3).abs() < 1e-9);
}

#[test]
fn usage_accounting_keeps_cache_and_reasoning_as_subledgers() {
    let usage = jarvis_model_provider::Usage {
        input_tokens: 12,
        output_tokens: 9,
        total_tokens: 21,
        cache_read_tokens: Some(8),
        cache_write_tokens: Some(3),
        reasoning_tokens: Some(5),
    };
    assert_eq!(usage.accounted_total_tokens(), 21);
    assert!(usage.has_consistent_accounting());

    let inconsistent = jarvis_model_provider::Usage {
        total_tokens: 26,
        ..usage
    };
    assert!(!inconsistent.has_consistent_accounting());
}

#[test]
fn models_available_requires_credential() {
    let models = Models::new();
    assert!(models.available_snapshot().is_empty());
    let models = Models::new().with_api_key("openai", "sk-test").unwrap();
    assert!(models
        .available_snapshot()
        .iter()
        .all(|model| model.provider.as_str() == "openai"));
    assert!(models.get("openai", "gpt-4o").is_some());
}

#[tokio::test]
async fn responses_non_stream_normalizes_text_reasoning_and_tool_identity() {
    let body = serde_json::json!({
        "id": "resp_1",
        "error": null,
        "status": "completed",
        "output": [
            {"type": "reasoning", "summary": [{"type": "summary_text", "text": "plan"}]},
            {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]},
            {"type": "function_call", "call_id": "call_external", "name": "lookup", "arguments": "{\"q\":1}", "status": "completed"}
        ],
        "usage": {"input_tokens": 4, "output_tokens": 6, "total_tokens": 10}
    });
    let (base_url, request_receiver) = fixture(body.to_string(), "application/json").await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let mut request = request(
        ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiResponses,
        )
        .with_capabilities(ModelCapabilities {
            tools: true,
            reasoning: true,
            ..ModelCapabilities::default()
        }),
    );
    request.tools.push(ToolSpec {
        name: "lookup".into(),
        description: "look up a value".into(),
        input_schema: serde_json::json!({"type":"object"}),
        constraint: None,
    });
    let completion = provider.complete(request).await.unwrap();
    assert_eq!(completion.message.text_value(), "done");
    assert_eq!(completion.message.reasoning_chars(), 4);
    assert_eq!(completion.message.tool_calls()[0].id, "call_external");
    let usage = completion.usage.unwrap();
    assert_eq!(usage.total_tokens, 10);
    assert!(usage.has_consistent_accounting());
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], false);
    assert_eq!(body["tools"][0]["type"], "function");
}

#[tokio::test]
async fn responses_stream_normalizes_text_usage_and_terminal_event() {
    let body = concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
        "event: response.content_part.added\n",
        "data: {\"type\":\"response.content_part.added\",\"output_index\":0,\"content_index\":0,\"part\":{\"type\":\"output_text\",\"text\":\"\"}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"hello\"}\n\n",
        "event: response.output_text.done\n",
        "data: {\"type\":\"response.output_text.done\",\"output_index\":0,\"content_index\":0,\"text\":\"hello\"}\n\n",
        "event: response.content_part.done\n",
        "data: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":0,\"part\":{\"type\":\"output_text\",\"text\":\"hello\"}}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello\"}]}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"id\":\"msg_1\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello\"}]}],\"usage\":{\"input_tokens\":2,\"output_tokens\":3,\"total_tokens\":5}}}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url_and_policy(&base_url, EndpointPolicy::SecureOrLoopback)
        .unwrap();
    let completion = jarvis_model_provider::collect_stream(
        provider
            .stream(request(ModelSpec::custom(
                "gpt-test",
                ProviderId::new("openai").unwrap(),
                Api::OpenAiResponses,
            )))
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(completion.message.text_value(), "hello");
    let usage = completion.usage.unwrap();
    assert_eq!(usage.total_tokens, 5);
    assert!(usage.has_consistent_accounting());
    assert_eq!(completion.stop_reason, StopReason::Stop);
}

#[tokio::test]
async fn responses_redacted_summary_replay_is_opaque_only() {
    let first_body = serde_json::json!({
        "id": "resp_opaque",
        "status": "completed",
        "output": [
            {
                "type": "reasoning",
                "summary": [{"type": "summary_text", "text": "provider summary"}],
                "encrypted_content": "opaque"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "answer"}]
            }
        ],
        "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
    });
    let (first_url, _) = fixture(first_body.to_string(), "application/json").await;
    let first_provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&first_url)
        .unwrap();
    let first = first_provider
        .complete(request(ModelSpec::custom(
            "gpt-test",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiResponses,
        )))
        .await
        .unwrap();
    let redacted = first
        .message
        .content
        .iter()
        .find_map(|part| match part {
            AssistantContent::Reasoning(reasoning) => Some(reasoning),
            _ => None,
        })
        .expect("Responses reasoning should be normalized");
    assert!(redacted.redacted);
    assert_eq!(redacted.text, "provider summary");
    assert_eq!(redacted.portability, ReasoningPortability::ProviderBound);
    let continuation = first
        .continuation
        .as_ref()
        .and_then(|value| value.openai_responses())
        .expect("Responses encrypted reasoning belongs to the continuation sidecar");
    assert_eq!(continuation.replay_item_count(), 2);

    let (chat_url, chat_receiver) = fixture(
        r#"{"choices":[{"message":{"role":"assistant","content":"continued"},"finish_reason":"stop"}]}"#.into(),
        "application/json",
    )
    .await;
    let chat = OpenAiCompatibleProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&chat_url)
        .unwrap();
    let mut chat_request = request(ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiCompletions,
    ));
    chat_request
        .messages
        .push(Message::Assistant(first.message.clone()));
    chat.complete(chat_request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&chat_receiver.await.unwrap()).unwrap();
    let assistant = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .unwrap();
    assert_eq!(assistant["content"], "answer");
    assert!(assistant.get("reasoning_content").is_none());

    let (anthropic_url, anthropic_receiver) = fixture(
        r#"{"content":[{"type":"text","text":"continued"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let anthropic = AnthropicProvider::new(ProviderId::new("anthropic").unwrap(), "secret")
        .unwrap()
        .with_base_url(&anthropic_url)
        .unwrap();
    let mut anthropic_request = request(ModelSpec::custom(
        "claude-test",
        ProviderId::new("anthropic").unwrap(),
        Api::AnthropicMessages,
    ));
    anthropic_request
        .messages
        .push(Message::Assistant(first.message));
    anthropic.complete(anthropic_request).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&anthropic_receiver.await.unwrap()).unwrap();
    let blocks = body["messages"][1]["content"].as_array().unwrap();
    assert_eq!(blocks[0]["type"], "text");
    assert_eq!(blocks[0]["text"], "answer");
    assert!(!blocks
        .iter()
        .any(|block| block["text"] == "provider summary"));
}

#[tokio::test]
async fn models_refreshes_oauth_and_uses_anthropic_bearer_auth() {
    let (base_url, headers_receiver) = header_fixture(
        r#"{"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = ProviderId::new("anthropic").unwrap();
    let model = ModelSpec::custom("claude-test", provider.clone(), Api::AnthropicMessages);
    let profile = ProviderProfile::new(
        provider.clone(),
        Api::AnthropicMessages,
        ModelCatalog::new([model.clone()]),
    )
    .with_base_url(base_url)
    .with_auth(AuthRequirement::Required);
    let credentials = Arc::new(MemoryCredentialStore::new());
    credentials
        .set_oauth(OAuthCredential::new(
            provider.clone(),
            "expired-wif-access-token",
            Some("old-wif-refresh-token"),
            Some(SystemTime::UNIX_EPOCH),
        ))
        .unwrap();
    let models = Models::new()
        .with_credential_store(credentials)
        .with_credential_refresher(Arc::new(OAuthTestRefresher))
        .with_profile(profile)
        .unwrap();

    models
        .connect(&models.get("anthropic", "claude-test").unwrap())
        .unwrap()
        .complete(request(model))
        .await
        .unwrap();

    let headers = headers_receiver.await.unwrap();
    assert_eq!(
        headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.as_str()),
        Some("Bearer fresh-wif-access-token")
    );
    assert!(!headers.iter().any(|(name, _)| name == "x-api-key"));
}

#[tokio::test]
async fn models_uses_oauth_bearer_for_openai_responses() {
    let (base_url, headers_receiver) = header_fixture(
        r#"{"id":"resp_1","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}"#.into(),
        "application/json",
    )
    .await;
    let provider = ProviderId::new("openai").unwrap();
    let model = ModelSpec::custom("gpt-test", provider.clone(), Api::OpenAiResponses);
    let profile = ProviderProfile::new(
        provider.clone(),
        Api::OpenAiResponses,
        ModelCatalog::new([model.clone()]),
    )
    .with_base_url(base_url)
    .with_auth(AuthRequirement::Required);
    let credentials = Arc::new(MemoryCredentialStore::new());
    credentials
        .set_oauth(OAuthCredential::new(
            provider.clone(),
            "expired-openai-wif-token",
            None::<String>,
            Some(SystemTime::UNIX_EPOCH),
        ))
        .unwrap();
    let models = Models::new()
        .with_credential_store(credentials)
        .with_credential_refresher(Arc::new(OAuthTestRefresher))
        .with_profile(profile)
        .unwrap();

    models
        .connect(&models.get("openai", "gpt-test").unwrap())
        .unwrap()
        .complete(request(model))
        .await
        .unwrap();

    let headers = headers_receiver.await.unwrap();
    assert_eq!(
        headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.as_str()),
        Some("Bearer fresh-wif-access-token")
    );
    assert!(!headers.iter().any(|(name, _)| name == "x-api-key"));
}

#[tokio::test]
async fn openai_image_generation_serializes_options_and_decodes_images() {
    let (base_url, request_receiver) = fixture(
        r#"{"created":1700000000,"background":"transparent","data":[{"b64_json":"aW1hZ2U=","revised_prompt":"revised"}],"usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7}}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiImageProvider::new(ProviderId::new("openai").unwrap(), "image-secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let model = ModelSpec::custom(
        "gpt-image-2",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiImages,
    );
    let request = ImageGenerationRequest::new(model, "a red kite over a blue lake")
        .with_n(2)
        .with_size(ImageSize::Landscape)
        .with_quality(ImageQuality::High)
        .with_background(ImageBackground::Transparent)
        .with_output_format(ImageOutputFormat::Png)
        .with_response_format(ImageResponseFormat::B64Json);

    let response = provider
        .generate_with(request, RequestOptions::default())
        .await
        .unwrap();

    assert_eq!(response.created, 1_700_000_000);
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].b64_json.as_deref(), Some("aW1hZ2U="));
    assert_eq!(response.data[0].revised_prompt.as_deref(), Some("revised"));
    assert_eq!(response.usage.unwrap().total_tokens, Some(7));

    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["model"], "gpt-image-2");
    assert_eq!(body["prompt"], "a red kite over a blue lake");
    assert_eq!(body["n"], 2);
    assert_eq!(body["size"], "1536x1024");
    assert_eq!(body["quality"], "high");
    assert_eq!(body["background"], "transparent");
    assert_eq!(body["output_format"], "png");
    assert_eq!(body["response_format"], "b64_json");
}

#[tokio::test]
async fn openai_image_generation_uses_images_generations_endpoint() {
    let (base_url, request_line_receiver) = request_line_fixture(
        r#"{"created":1700000002,"data":[{"b64_json":"aW1hZ2U="}]}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiImageProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let model = ModelSpec::custom(
        "gpt-image-2",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiImages,
    );

    provider
        .generate(ImageGenerationRequest::new(model, "an endpoint test"))
        .await
        .unwrap();

    assert_eq!(
        request_line_receiver.await.unwrap(),
        "POST /v1/images/generations HTTP/1.1"
    );
}

#[tokio::test]
async fn provider_factory_builds_standalone_image_generator() {
    let (base_url, request_receiver) = fixture(
        r#"{"created":1700000001,"data":[{"url":"https://cdn.example.test/image.png"}]}"#.into(),
        "application/json",
    )
    .await;
    let provider = ProviderFactory::build_image_generator(ProviderConfig {
        provider_id: ProviderId::new("openai-images").unwrap(),
        api: Api::OpenAiImages,
        api_key: "factory-image-secret".into(),
        base_url: Some(base_url),
        endpoint_policy: EndpointPolicy::SecureOrLoopback,
        request_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let model = ModelSpec::custom(
        "gpt-image-2",
        ProviderId::new("openai-images").unwrap(),
        Api::OpenAiImages,
    );

    let response = provider
        .generate(
            ImageGenerationRequest::new(model, "a factory image")
                .with_response_format(ImageResponseFormat::Url),
        )
        .await
        .unwrap();

    assert_eq!(response.created, 1_700_000_001);
    assert_eq!(
        response.data[0].url.as_deref(),
        Some("https://cdn.example.test/image.png")
    );
    let body: serde_json::Value = serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
    assert_eq!(body["model"], "gpt-image-2");
    assert_eq!(body["prompt"], "a factory image");
    assert_eq!(body["response_format"], "url");
}

#[tokio::test]
async fn models_connect_image_uses_profile_authority_and_bearer_auth() {
    let (base_url, headers_receiver) = header_fixture(
        r#"{"created":1700000000,"data":[{"b64_json":"aW1hZ2U="}]}"#.into(),
        "application/json",
    )
    .await;
    let provider = ProviderId::new("openai-images").unwrap();
    let model = ModelSpec::custom("gpt-image-2", provider.clone(), Api::OpenAiImages);
    let profile = ProviderProfile::new(
        provider.clone(),
        Api::OpenAiImages,
        ModelCatalog::new([model.clone()]),
    )
    .with_base_url(base_url)
    .with_auth(AuthRequirement::Required);
    let models = Models::new()
        .with_api_key(provider.as_str(), "profile-image-secret")
        .unwrap()
        .with_profile(profile)
        .unwrap();

    let connection = models
        .connect_image(&models.get("openai-images", "gpt-image-2").unwrap())
        .unwrap();
    let response = connection
        .generate(ImageGenerationRequest::new(model, "a blue square"))
        .await
        .unwrap();

    assert_eq!(response.data[0].b64_json.as_deref(), Some("aW1hZ2U="));
    let headers = headers_receiver.await.unwrap();
    assert_eq!(
        headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.as_str()),
        Some("Bearer profile-image-secret")
    );
}

#[tokio::test]
async fn unauthenticated_image_profile_rejects_caller_credential_headers() {
    let (base_url, _headers_receiver) = header_fixture(
        r#"{"created":1700000000,"data":[{"b64_json":"aW1hZ2U="}]}"#.into(),
        "application/json",
    )
    .await;
    let provider = ProviderId::new("local-images").unwrap();
    let model = ModelSpec::custom("local-image", provider.clone(), Api::OpenAiImages);
    let profile = ProviderProfile::new(
        provider.clone(),
        Api::OpenAiImages,
        ModelCatalog::new([model.clone()]),
    )
    .with_base_url(base_url)
    .with_auth(AuthRequirement::None);
    let models = Models::new().with_profile(profile).unwrap();
    let connection = models.connect_image(&model).unwrap();

    let error = connection
        .generate_with(
            ImageGenerationRequest::new(model, "a local image"),
            RequestOptions {
                abort: None,
                headers: vec![("Authorization".into(), "Bearer caller-secret".into())],
            },
        )
        .await
        .unwrap_err();

    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
}

#[tokio::test]
async fn openai_image_generation_honors_abort_before_dispatch() {
    let (base_url, _request_receiver) = fixture(
        r#"{"created":1700000000,"data":[{"b64_json":"aW1hZ2U="}]}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiImageProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(base_url)
        .unwrap();
    let abort = AbortSignal::new();
    abort.abort();
    let model = ModelSpec::custom(
        "gpt-image-2",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiImages,
    );

    let error = provider
        .generate_with(
            ImageGenerationRequest::new(model, "will not dispatch"),
            RequestOptions {
                abort: Some(abort),
                headers: Vec::new(),
            },
        )
        .await
        .unwrap_err();

    assert_eq!(error.kind, ProviderErrorKind::Aborted);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
}

#[tokio::test]
async fn image_generation_rejects_empty_prompt_before_dispatch() {
    let provider = OpenAiImageProvider::new(ProviderId::new("openai").unwrap(), "secret").unwrap();
    let model = ModelSpec::custom(
        "gpt-image-2",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiImages,
    );

    let error = provider
        .generate(ImageGenerationRequest::new(model, "   "))
        .await
        .unwrap_err();

    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
    assert!(error.message.contains("prompt"));
}

#[tokio::test]
async fn image_generation_redacts_api_key_in_provider_errors() {
    let (base_url, _request_receiver) = fixture_with_status(
        401,
        r#"{"error":{"message":"image-secret rejected"}}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiImageProvider::new(ProviderId::new("openai").unwrap(), "image-secret")
        .unwrap()
        .with_base_url(base_url)
        .unwrap();
    let model = ModelSpec::custom(
        "gpt-image-2",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiImages,
    );

    let error = provider
        .generate(ImageGenerationRequest::new(model, "a safe prompt"))
        .await
        .unwrap_err();

    assert!(!error.message.contains("image-secret"));
    assert_eq!(error.kind, ProviderErrorKind::Authentication);
    assert_eq!(error.phase, FailurePhase::AfterDispatch);
}

#[test]
fn prepared_request_semantics_match_direct_profile_and_credential_paths() {
    let provider = ProviderId::new("anthropic").unwrap();
    let model = ModelSpec::custom("claude-test", provider.clone(), Api::AnthropicMessages)
        .with_capabilities(ModelCapabilities::default());
    let mut raw = request(model.clone());
    raw.messages.push(Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: "portable plan".into(),
            redacted: false,
            portability: ReasoningPortability::Portable,
            continuation_ref: None,
        })],
    }));

    let direct = AnthropicProvider::new(provider.clone(), "secret").unwrap();
    let direct_prepared = direct.prepare_request(raw.clone()).unwrap();
    direct.validate_prepared_request(&direct_prepared).unwrap();
    assert!(direct_prepared.history().is_lossy());

    let profile = ProviderProfile::new(
        provider.clone(),
        Api::AnthropicMessages,
        ModelCatalog::new([model.clone()]),
    )
    .with_auth(AuthRequirement::None);
    let models = Models::new().with_profile(profile).unwrap();
    let profile_provider = models.connect(&model).unwrap();
    let profile_prepared = profile_provider.prepare_request(raw.clone()).unwrap();
    profile_provider
        .validate_prepared_request(&profile_prepared)
        .unwrap();

    let credential_models = Models::new().with_api_key("anthropic", "secret").unwrap();
    let credential_provider = credential_models.connect(&model).unwrap();
    let credential_prepared = credential_provider.prepare_request(raw.clone()).unwrap();
    credential_provider
        .validate_request(&raw)
        .expect("credential-backed validation must use the same prepared semantics");

    assert_eq!(direct_prepared, profile_prepared);
    assert_eq!(direct_prepared, credential_prepared);
    assert_eq!(
        direct_prepared.request().messages.last().unwrap(),
        profile_prepared.request().messages.last().unwrap()
    );
}

#[tokio::test]
async fn responses_continuation_is_typed_and_replayed_without_message_pollution() {
    let first_body = serde_json::json!({
        "id": "resp_stateless_1",
        "status": "completed",
        "output": [
            {
                "type": "reasoning",
                "summary": [{"type": "summary_text", "text": "plan"}],
                "encrypted_content": "encrypted-plan"
            },
            {
                "type": "function_call",
                "id": "fc_1",
                "call_id": "call-1",
                "name": "lookup",
                "arguments": "{\"q\":1}",
                "status": "completed"
            },
            {
                "type": "reasoning",
                "summary": [{"type": "summary_text", "text": "plan two"}],
                "encrypted_content": "encrypted-plan-two"
            },
            {
                "type": "message",
                "role": "assistant",
                "phase": "final",
                "content": [{"type": "output_text", "text": "answer"}]
            }
        ],
        "usage": {"input_tokens": 2, "output_tokens": 3}
    });
    let (first_url, _) = fixture(first_body.to_string(), "application/json").await;
    let first_provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&first_url)
        .unwrap();
    let model = ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    );
    let first = first_provider
        .complete(request(model.clone()))
        .await
        .unwrap();
    let continuation = first
        .continuation
        .clone()
        .expect("encrypted Responses output must produce a continuation");
    assert_eq!(continuation.provider().as_str(), "openai");
    assert_eq!(continuation.api(), Api::OpenAiResponses);
    assert_eq!(continuation.model(), "gpt-test");
    assert_eq!(
        continuation.durability(),
        jarvis_model_provider::ContinuationDurability::SensitiveNonDurable
    );
    assert_eq!(
        continuation.openai_responses().unwrap().replay_item_count(),
        4
    );
    let serialized = serde_json::to_string(&continuation).unwrap();
    let round_trip: jarvis_model_provider::ProviderContinuation =
        serde_json::from_str(&serialized).unwrap();
    assert_eq!(round_trip, continuation);
    let message_json = serde_json::to_value(&first.message).unwrap();
    assert!(!message_json.to_string().contains("encrypted-plan"));

    let (second_url, second_receiver) = fixture(
        r#"{"id":"resp_stateless_2","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"continued"}]}],"usage":{"input_tokens":3,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let second_provider =
        OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
            .unwrap()
            .with_base_url(&second_url)
            .unwrap();
    let mut follow_up = request(model.clone());
    follow_up.messages.push(Message::Assistant(first.message));
    follow_up.messages.push(Message::tool_result(
        "call-1",
        Some("lookup".into()),
        "tool output",
    ));
    follow_up.messages.push(Message::user("next question"));
    follow_up.continuation = Some(continuation.clone());
    second_provider.complete(follow_up).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&second_receiver.await.unwrap()).unwrap();
    assert_eq!(body.get("previous_response_id"), None);
    // Stateless manual replay retains the complete required history: the
    // original user input stays first, anchored assistant outputs are
    // substituted in place, and user/tool inputs keep their positions.
    assert_eq!(body["input"][0]["role"], "user");
    assert_eq!(body["input"][0]["content"][0]["text"], "hello");
    assert_eq!(body["input"][1]["type"], "reasoning");
    assert_eq!(body["input"][1]["encrypted_content"], "encrypted-plan");
    assert_eq!(body["input"][2]["type"], "function_call");
    assert_eq!(body["input"][2]["call_id"], "call-1");
    assert_eq!(body["input"][2]["id"], "fc_1");
    assert_eq!(body["input"][3]["type"], "reasoning");
    assert_eq!(body["input"][3]["encrypted_content"], "encrypted-plan-two");
    assert_eq!(body["input"][4]["type"], "message");
    assert_eq!(body["input"][4]["phase"], "final");
    assert_eq!(body["input"][4]["content"][0]["text"], "answer");
    assert_eq!(body["input"][5]["type"], "function_call_output");
    assert_eq!(body["input"][5]["call_id"], "call-1");
    assert_eq!(body["input"][6]["role"], "user");
    assert_eq!(body["input"][6]["content"][0]["text"], "next question");

    let wrong_provider =
        OpenAiResponsesProvider::new(ProviderId::new("other").unwrap(), "secret").unwrap();
    let wrong_model = ModelSpec::custom(
        "gpt-test",
        ProviderId::new("other").unwrap(),
        Api::OpenAiResponses,
    );
    let mut wrong_request = request(wrong_model);
    wrong_request.continuation = Some(continuation);
    let error = wrong_provider.validate_request(&wrong_request).unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(error.phase, FailurePhase::BeforeDispatch);
}

#[tokio::test]
async fn stateful_responses_continuation_uses_previous_response_id() {
    let (first_url, _) = fixture(
        r#"{"id":"resp_stateful_1","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"answer"}]}],"usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&first_url)
        .unwrap();
    let model = ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    );
    let mut first_request = request(model.clone());
    first_request.retention = jarvis_model_provider::DataRetentionPolicy::ProviderDefault;
    let first = provider.complete(first_request).await.unwrap();
    let continuation = first.continuation.unwrap();
    assert_eq!(
        continuation.durability(),
        jarvis_model_provider::ContinuationDurability::ProviderBound
    );

    let (second_url, receiver) = fixture(
        r#"{"id":"resp_stateful_2","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"continued"}]}],"usage":{"input_tokens":1,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&second_url)
        .unwrap();
    let mut next = request(model);
    next.retention = jarvis_model_provider::DataRetentionPolicy::ProviderDefault;
    next.messages.push(Message::Assistant(first.message));
    next.messages.push(Message::user("new suffix"));
    next.continuation = Some(continuation);
    provider.complete(next).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(body["previous_response_id"], "resp_stateful_1");
    assert_eq!(body["instructions"], "You are concise.");
    assert_eq!(body["input"].as_array().unwrap().len(), 1);
    assert_eq!(body["input"][0]["role"], "user");
    assert_eq!(body["input"][0]["content"][0]["text"], "new suffix");
}

#[tokio::test]
async fn streamed_responses_completion_exposes_encrypted_continuation_sidecar() {
    let body = concat!(
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\"}}\n\n",
        "event: response.reasoning_summary_text.delta\n",
        "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"plan\"}\n\n",
        "event: response.reasoning_summary_text.done\n",
        "data: {\"type\":\"response.reasoning_summary_text.done\",\"text\":\"plan\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"plan\"}],\"encrypted_content\":\"stream-encrypted-plan\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"answer\"}]}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream_1\",\"status\":\"completed\",\"output\":[{\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"plan\"}],\"encrypted_content\":\"stream-encrypted-plan\"},{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"answer\"}]}],\"usage\":{\"input_tokens\":2,\"output_tokens\":3}}}\n\n"
    );
    let (base_url, _) = fixture(body.into(), "text/event-stream").await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&base_url)
        .unwrap();
    let model = ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    );

    let completion =
        jarvis_model_provider::collect_stream(provider.stream(request(model)).await.unwrap())
            .await
            .unwrap();

    assert_eq!(completion.message.text_value(), "answer");
    let continuation = completion
        .continuation
        .expect("streamed encrypted Responses output must produce a continuation");
    assert_eq!(continuation.model(), "gpt-test");
    assert_eq!(
        continuation.durability(),
        jarvis_model_provider::ContinuationDurability::SensitiveNonDurable
    );
    assert!(!serde_json::to_value(&completion.message)
        .unwrap()
        .to_string()
        .contains("stream-encrypted-plan"));
}

#[test]
fn stateless_responses_continuation_requires_encrypted_replay_state() {
    let error = jarvis_model_provider::OpenAiResponsesContinuation::new(
        ProviderId::new("openai").unwrap(),
        "gpt-test",
        None,
        Vec::new(),
        false,
    )
    .unwrap_err();
    assert!(error.contains("no replay state"));
}

#[test]
fn stateless_responses_rejects_server_coverage_claims() {
    let reference = ContinuationRef::new("reasoning").unwrap();
    let anchor =
        jarvis_model_provider::history_message_ids(&[Message::user("u1")]).unwrap()[0].clone();
    let error = jarvis_model_provider::OpenAiResponsesContinuation::with_segments(
        ProviderId::new("openai").unwrap(),
        "gpt-test",
        Some("resp_1".into()),
        OpenAiResponsesContinuationMode::Stateless,
        ContinuationScope::for_history(&[Message::user("u1")]).unwrap(),
        vec![OpenAiResponsesReplaySegment::new(
            anchor,
            vec![OpenAiResponsesReplayItem::reasoning(
                reference,
                Some("rs_1".into()),
                "encrypted",
                Vec::new(),
            )],
        )],
    )
    .unwrap_err();
    assert!(
        error.contains("must not retain a response id")
            || error.contains("must not claim server coverage"),
        "{error}"
    );
}

#[test]
fn usage_cost_uses_uncached_input_and_rejects_malformed_subdivisions() {
    let usage = jarvis_model_provider::Usage {
        input_tokens: 100,
        output_tokens: 20,
        total_tokens: 120,
        cache_read_tokens: Some(30),
        cache_write_tokens: Some(10),
        reasoning_tokens: Some(5),
    };
    assert_eq!(usage.uncached_input_tokens().unwrap(), 60);
    let rates = jarvis_model_provider::ModelCost {
        input: 1.0,
        output: 2.0,
        cache_read: 3.0,
        cache_write: 4.0,
    };
    let cost = jarvis_model_provider::try_calculate_cost(&rates, &usage).unwrap();
    assert!((cost.input - 0.00006).abs() < 1e-12);
    assert!((cost.output - 0.00004).abs() < 1e-12);
    assert!((cost.cache_read - 0.00009).abs() < 1e-12);
    assert!((cost.cache_write - 0.00004).abs() < 1e-12);

    let malformed = jarvis_model_provider::Usage {
        input_tokens: 10,
        output_tokens: 1,
        total_tokens: 11,
        cache_read_tokens: Some(8),
        cache_write_tokens: Some(3),
        reasoning_tokens: None,
    };
    assert!(matches!(
        jarvis_model_provider::try_calculate_cost(&rates, &malformed),
        Err(jarvis_model_provider::UsageError::CacheTokensExceedInput { .. })
    ));
    assert!(!malformed.has_consistent_accounting());
}

// ========================================================================
// M7.1 stateless Responses test matrix
// ========================================================================

fn responses_test_model() -> ModelSpec {
    ModelSpec::custom(
        "gpt-test",
        ProviderId::new("openai").unwrap(),
        Api::OpenAiResponses,
    )
}

fn reasoning_output(id: &str, encrypted: &str, summary: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "reasoning",
        "id": id,
        "summary": [{"type": "summary_text", "text": summary}],
        "encrypted_content": encrypted
    })
}

fn function_call_output(id: &str, call_id: &str, name: &str, arguments: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "function_call",
        "id": id,
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    })
}

fn message_output(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}]
    })
}

fn wire_input_types(body: &serde_json::Value) -> Vec<String> {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("type")
                        .and_then(serde_json::Value::as_str)
                        .or_else(|| item.get("role").and_then(serde_json::Value::as_str))
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// TEST 1 — the original user input must survive stateless replay.
#[tokio::test]
async fn m71_stateless_replay_retains_original_user_input() {
    let first_body = serde_json::json!({
        "id": "m71_t1_1",
        "status": "completed",
        "output": [
            reasoning_output("rs_1", "enc-plan", "plan"),
            function_call_output("fc_1", "call-1", "lookup", "{\"q\":1}")
        ],
        "usage": {"input_tokens": 2, "output_tokens": 3}
    });
    let (first_url, _) = fixture(first_body.to_string(), "application/json").await;
    let provider = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&first_url)
        .unwrap();
    let model = responses_test_model();
    let first = provider.complete(request(model.clone())).await.unwrap();
    let continuation = first.continuation.expect("encrypted output must continue");

    let (second_url, receiver) = fixture(
        r#"{"id":"m71_t1_2","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}],"usage":{"input_tokens":3,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let second = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&second_url)
        .unwrap();
    let mut follow_up = request(model);
    follow_up.messages.push(Message::Assistant(first.message));
    follow_up.messages.push(Message::tool_result(
        "call-1",
        Some("lookup".into()),
        "result",
    ));
    follow_up.messages.push(Message::user("next"));
    follow_up.continuation = Some(continuation);
    second.complete(follow_up).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(
        wire_input_types(&body),
        [
            "user",
            "reasoning",
            "function_call",
            "function_call_output",
            "user"
        ]
    );
    assert_eq!(body["input"][0]["content"][0]["text"], "hello");
    assert_eq!(body["input"][4]["content"][0]["text"], "next");
}

/// TEST 2 — two assistant segments preserve full conversation order.
#[tokio::test]
async fn m71_stateless_two_segments_preserve_conversation_order() {
    // Turn 1: reasoning + function call.
    let turn1_body = serde_json::json!({
        "id": "m71_t2_1",
        "status": "completed",
        "output": [
            reasoning_output("rs_1", "enc-r1", "plan one"),
            function_call_output("fc_1", "call-1", "lookup", "{\"q\":1}")
        ],
        "usage": {"input_tokens": 2, "output_tokens": 3}
    });
    let (url1, _) = fixture(turn1_body.to_string(), "application/json").await;
    let provider1 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let model = responses_test_model();
    let completion1 = provider1.complete(request(model.clone())).await.unwrap();

    // Turn 2: reasoning + message, replaying turn-1 history with its segment.
    let turn2_body = serde_json::json!({
        "id": "m71_t2_2",
        "status": "completed",
        "output": [
            reasoning_output("rs_2", "enc-r2", "plan two"),
            message_output("final answer")
        ],
        "usage": {"input_tokens": 5, "output_tokens": 4}
    });
    let (url2, _) = fixture(turn2_body.to_string(), "application/json").await;
    let provider2 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url2)
        .unwrap();
    let mut follow_up1 = request(model.clone());
    follow_up1
        .messages
        .push(Message::Assistant(completion1.message.clone()));
    follow_up1.messages.push(Message::tool_result(
        "call-1",
        Some("lookup".into()),
        "result one",
    ));
    follow_up1.messages.push(Message::user("turn two question"));
    follow_up1.continuation = completion1.continuation.clone();
    let completion2 = provider2.complete(follow_up1).await.unwrap();
    assert_eq!(
        completion2
            .continuation
            .as_ref()
            .and_then(ProviderContinuation::openai_responses)
            .map(jarvis_model_provider::OpenAiResponsesContinuation::replay_segment_count),
        Some(2),
        "turn 2 must carry both anchored segments"
    );

    // Turn 3: send full accumulated history with the newest continuation.
    let (url3, receiver) = fixture(
        r#"{"id":"m71_t2_3","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}],"usage":{"input_tokens":8,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider3 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url3)
        .unwrap();
    let mut follow_up2 = request(model);
    follow_up2
        .messages
        .push(Message::Assistant(completion1.message));
    follow_up2.messages.push(Message::tool_result(
        "call-1",
        Some("lookup".into()),
        "result one",
    ));
    follow_up2.messages.push(Message::user("turn two question"));
    follow_up2
        .messages
        .push(Message::Assistant(completion2.message));
    follow_up2
        .messages
        .push(Message::user("turn three question"));
    follow_up2.continuation = completion2.continuation;
    provider3.complete(follow_up2).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(
        wire_input_types(&body),
        [
            "user",
            "reasoning",            // A1 replay R1
            "function_call",        // A1 replay FC1
            "function_call_output", // T1
            "user",
            "reasoning", // A2 replay R2
            "message",   // A2 replay Message2
            "user"
        ]
    );
    assert_eq!(body["input"][0]["content"][0]["text"], "hello");
    assert_eq!(body["input"][6]["content"][0]["text"], "final answer");
}

/// TEST 3 — a portable assistant without a segment encodes normally.
#[tokio::test]
async fn m71_stateless_portable_assistant_encodes_normally() {
    let encrypted_body = serde_json::json!({
        "id": "m71_t3_1",
        "status": "completed",
        "output": [reasoning_output("rs_9", "enc-x", "plan")],
        "usage": {"input_tokens": 2, "output_tokens": 1}
    });
    let (url1, _) = fixture(encrypted_body.to_string(), "application/json").await;
    let provider1 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url1)
        .unwrap();
    let model = responses_test_model();
    let with_reasoning = provider1.complete(request(model)).await.unwrap();
    let continuation = with_reasoning
        .continuation
        .expect("encrypted reasoning must continue");

    let (url2, receiver) = fixture(
        r#"{"id":"m71_t3_2","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}],"usage":{"input_tokens":4,"output_tokens":1}}"#.into(),
        "application/json",
    )
    .await;
    let provider2 = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&url2)
        .unwrap();
    // History keeps the anchored provider-bound assistant and adds an
    // ordinary portable assistant entry that needs no replay segment.
    let portable_assistant = Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Text(
            jarvis_model_provider::TextContent::new("older portable answer"),
        )],
    });
    let mut next = request(responses_test_model());
    next.messages
        .push(Message::Assistant(with_reasoning.message));
    next.messages.push(portable_assistant);
    next.messages.push(Message::user("follow-up"));
    next.continuation = Some(continuation);
    provider2.complete(next).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&receiver.await.unwrap()).unwrap();
    assert_eq!(
        wire_input_types(&body),
        ["user", "reasoning", "message", "user"]
    );
    assert_eq!(
        body["input"][2]["content"][0]["text"],
        "older portable answer"
    );
    assert_eq!(body["input"][3]["content"][0]["text"], "follow-up");
}

/// TEST 4 — an edited anchored assistant fails before dispatch.
#[test]
fn m71_stateless_edited_anchor_fails_closed() {
    let reference = ContinuationRef::new("r-edit").unwrap();
    let message_ref = ContinuationRef::new("m-edit").unwrap();
    let assistant = Message::Assistant(AssistantMessage {
        content: vec![
            AssistantContent::Reasoning(ReasoningContent {
                text: String::new(),
                redacted: true,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: Some(reference.clone()),
            }),
            AssistantContent::Text(jarvis_model_provider::TextContent::new("original")),
        ],
    });
    let history = vec![Message::user("u1"), assistant];
    let anchor = jarvis_model_provider::history_message_ids(&history).unwrap()[1].clone();
    let continuation = ProviderContinuation::OpenAiResponses(
        jarvis_model_provider::OpenAiResponsesContinuation::with_segments(
            ProviderId::new("openai").unwrap(),
            "gpt-test",
            None,
            OpenAiResponsesContinuationMode::Stateless,
            ContinuationScope::empty(),
            vec![OpenAiResponsesReplaySegment::new(
                anchor,
                vec![
                    OpenAiResponsesReplayItem::reasoning(
                        reference,
                        Some("rs_1".into()),
                        "enc",
                        Vec::new(),
                    ),
                    OpenAiResponsesReplayItem::assistant_message(
                        message_ref,
                        Some("msg_1".into()),
                        None,
                        "original",
                    ),
                ],
            )],
        )
        .unwrap(),
    );
    continuation.validate_for_history(&history).unwrap();
    // Edited text changes the anchor identity; validation must fail closed.
    let mut edited_history = history.clone();
    if let Message::Assistant(message) = &mut edited_history[1] {
        message.content[1] =
            AssistantContent::Text(jarvis_model_provider::TextContent::new("edited"));
    }
    let error = continuation
        .validate_for_history(&edited_history)
        .unwrap_err();
    assert!(error.contains("anchor no longer matches"), "{error}");
}

/// TEST 5 — ProviderBound reasoning without any matching segment fails.
#[test]
fn m71_stateless_missing_segment_fails_closed() {
    let reference = ContinuationRef::new("r-missing").unwrap();
    let assistant = Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: String::new(),
            redacted: true,
            portability: ReasoningPortability::ProviderBound,
            continuation_ref: Some(reference),
        })],
    });
    let messages = vec![Message::user("u1"), assistant];
    // Segment bound to a different (nonexistent in this history) anchor.
    let other_anchor =
        jarvis_model_provider::history_message_ids(&[Message::user("other")]).unwrap()[0].clone();
    let continuation = ProviderContinuation::OpenAiResponses(
        jarvis_model_provider::OpenAiResponsesContinuation::with_segments(
            ProviderId::new("openai").unwrap(),
            "gpt-test",
            None,
            OpenAiResponsesContinuationMode::Stateless,
            ContinuationScope::empty(),
            vec![OpenAiResponsesReplaySegment::new(
                other_anchor,
                vec![OpenAiResponsesReplayItem::reasoning(
                    ContinuationRef::new("unused").unwrap(),
                    Some("rs_x".into()),
                    "enc",
                    Vec::new(),
                )],
            )],
        )
        .unwrap(),
    );
    let error = continuation.validate_for_history(&messages).unwrap_err();
    assert!(
        error.contains("missing replay metadata") || error.contains("anchor no longer matches"),
        "{error}"
    );
}

/// TEST 6 — an extra segment whose anchor does not exist fails.
#[test]
fn m71_stateless_extra_segment_fails_closed() {
    let reference = ContinuationRef::new("r-real").unwrap();
    let assistant = Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: String::new(),
            redacted: true,
            portability: ReasoningPortability::ProviderBound,
            continuation_ref: Some(reference.clone()),
        })],
    });
    let messages = vec![Message::user("u1"), assistant];
    let real_anchor = jarvis_model_provider::history_message_ids(&messages).unwrap()[1].clone();
    let stale_anchor =
        jarvis_model_provider::history_message_ids(&[Message::user("stale")]).unwrap()[0].clone();
    let continuation = ProviderContinuation::OpenAiResponses(
        jarvis_model_provider::OpenAiResponsesContinuation::with_segments(
            ProviderId::new("openai").unwrap(),
            "gpt-test",
            None,
            OpenAiResponsesContinuationMode::Stateless,
            ContinuationScope::empty(),
            vec![
                OpenAiResponsesReplaySegment::new(
                    real_anchor,
                    vec![OpenAiResponsesReplayItem::reasoning(
                        reference,
                        Some("rs_a".into()),
                        "enc-a",
                        Vec::new(),
                    )],
                ),
                OpenAiResponsesReplaySegment::new(
                    stale_anchor,
                    vec![OpenAiResponsesReplayItem::reasoning(
                        ContinuationRef::new("r-extra").unwrap(),
                        Some("rs_b".into()),
                        "enc-b",
                        Vec::new(),
                    )],
                ),
            ],
        )
        .unwrap(),
    );
    let error = continuation.validate_for_history(&messages).unwrap_err();
    assert!(error.contains("anchor no longer matches"), "{error}");
}

/// TEST 7 — segment storage order does not affect association.
#[test]
fn m71_stateless_segment_association_is_anchor_based() {
    let ref_a = ContinuationRef::new("ra").unwrap();
    let ref_b = ContinuationRef::new("rb").unwrap();
    let assistant_a = Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: String::new(),
            redacted: true,
            portability: ReasoningPortability::ProviderBound,
            continuation_ref: Some(ref_a.clone()),
        })],
    });
    let assistant_b = Message::Assistant(AssistantMessage {
        content: vec![AssistantContent::Reasoning(ReasoningContent {
            text: String::new(),
            redacted: true,
            portability: ReasoningPortability::ProviderBound,
            continuation_ref: Some(ref_b.clone()),
        })],
    });
    let messages = vec![
        Message::user("u1"),
        assistant_a,
        Message::user("u2"),
        assistant_b,
    ];
    let anchor_a = jarvis_model_provider::history_message_ids(&messages).unwrap()[1].clone();
    let anchor_b = jarvis_model_provider::history_message_ids(&messages).unwrap()[3].clone();
    // Store B before A: association must still resolve by anchor identity.
    let continuation = ProviderContinuation::OpenAiResponses(
        jarvis_model_provider::OpenAiResponsesContinuation::with_segments(
            ProviderId::new("openai").unwrap(),
            "gpt-test",
            None,
            OpenAiResponsesContinuationMode::Stateless,
            ContinuationScope::empty(),
            vec![
                OpenAiResponsesReplaySegment::new(
                    anchor_b,
                    vec![OpenAiResponsesReplayItem::reasoning(
                        ref_b.clone(),
                        Some("rs_b".into()),
                        "enc-b",
                        Vec::new(),
                    )],
                ),
                OpenAiResponsesReplaySegment::new(
                    anchor_a,
                    vec![OpenAiResponsesReplayItem::reasoning(
                        ref_a,
                        Some("rs_a".into()),
                        "enc-a",
                        Vec::new(),
                    )],
                ),
            ],
        )
        .unwrap(),
    );
    continuation.validate_for_history(&messages).unwrap();
}

/// TEST 8 — stream and complete produce equivalent segmentation.
#[tokio::test]
async fn m71_stream_and_complete_produce_equivalent_continuations() {
    let model = responses_test_model();
    let sse_body = concat!(
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\"}}\n\n",
        "event: response.reasoning_summary_text.delta\n",
        "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"plan\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"plan\"}],\"encrypted_content\":\"parity-enc\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call-p\",\"name\":\"lookup\",\"arguments\":\"\"}}\n\n",
        "event: response.function_call_arguments.delta\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"delta\":\"{\\\"q\\\":1}\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"id\":\"fc_p\",\"call_id\":\"call-p\",\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":1}\"}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_parity\",\"status\":\"completed\",\"output\":[{\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"plan\"}],\"encrypted_content\":\"parity-enc\"},{\"type\":\"function_call\",\"id\":\"fc_p\",\"call_id\":\"call-p\",\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":1}\"}],\"usage\":{\"input_tokens\":2,\"output_tokens\":3}}}\n\n"
    );
    let (stream_url, _) = fixture(sse_body.into(), "text/event-stream").await;
    let streaming = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&stream_url)
        .unwrap();
    let streamed = jarvis_model_provider::collect_stream(
        streaming.stream(request(model.clone())).await.unwrap(),
    )
    .await
    .unwrap();

    let complete_body = serde_json::json!({
        "id": "resp_parity",
        "status": "completed",
        "output": [
            {"type":"reasoning","summary":[{"type":"summary_text","text":"plan"}],"encrypted_content":"parity-enc"},
            {"type":"function_call","id":"fc_p","call_id":"call-p","name":"lookup","arguments":"{\"q\":1}"}
        ],
        "usage": {"input_tokens": 2, "output_tokens": 3}
    });
    let (complete_url, _) = fixture(complete_body.to_string(), "application/json").await;
    let non_streaming = OpenAiResponsesProvider::new(ProviderId::new("openai").unwrap(), "secret")
        .unwrap()
        .with_base_url(&complete_url)
        .unwrap();
    let completed = non_streaming.complete(request(model)).await.unwrap();

    // Equivalent normalized messages and segmentation shape. Item references
    // are generated per call, so compare structure and payload sizes.
    let streamed_cont = streamed
        .continuation
        .as_ref()
        .unwrap()
        .openai_responses()
        .unwrap();
    let completed_cont = completed
        .continuation
        .as_ref()
        .unwrap()
        .openai_responses()
        .unwrap();
    assert_eq!(streamed_cont.mode(), completed_cont.mode());
    assert_eq!(
        streamed_cont.replay_segment_count(),
        completed_cont.replay_segment_count()
    );
    assert_eq!(
        streamed_cont.replay_item_count(),
        completed_cont.replay_item_count()
    );
    for (a, b) in streamed_cont
        .replay_segments()
        .iter()
        .zip(completed_cont.replay_segments())
    {
        assert_eq!(a.items().len(), b.items().len());
        for (item_a, item_b) in a.items().iter().zip(b.items()) {
            assert_eq!(item_a.kind(), item_b.kind());
            // Compare redacted payload size through the safe Debug contract.
            let debug_a = format!("{item_a:?}");
            let debug_b = format!("{item_b:?}");
            let bytes = |text: &str| {
                text.split("sensitive_bytes: ")
                    .nth(1)
                    .and_then(|rest| rest.split(',').next())
                    .and_then(|value| value.trim_end_matches(')').trim().parse::<usize>().ok())
            };
            assert_eq!(bytes(&debug_a), bytes(&debug_b));
        }
    }
    // Neither normalized message leaks the encrypted payload.
    let streamed_json = serde_json::to_value(&streamed.message).unwrap().to_string();
    let completed_json = serde_json::to_value(&completed.message)
        .unwrap()
        .to_string();
    assert!(!streamed_json.contains("parity-enc"));
    assert!(!completed_json.contains("parity-enc"));
}
