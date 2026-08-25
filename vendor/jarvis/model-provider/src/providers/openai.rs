use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::Stream;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};

use crate::providers::{
    aborted, apply_headers, bounded_error_body, bounded_response_body, client_for_policy, dispatch,
    normalize_base_url, normalize_image, protocol, retry_after_from_headers, stream_error,
    DispatchResult, EndpointPolicy, SseReader, MAX_STREAM_BUFFER_BYTES,
};
use crate::{
    AbortSignal, Api, AssistantContent, AssistantMessage, Completion, CompletionMetadata,
    CompletionRequest, FailurePhase, ImageContent, MaxOutputTokensField, Message, ModelProvider,
    ModelSpec, OpenAiCompletionsCompatibility, OpenAiSystemRole, OutputConstraint,
    ProviderCapabilities, ProviderError, ProviderErrorKind, ProviderId, ProviderStream,
    ReasoningConfig, ReasoningContent, ReasoningPortability, RequestOptions, StopReason,
    StreamEvent, TextContent, ToolCall, ToolChoice, ToolConstraint, ToolResultContent, Usage,
    UserContent,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// OpenAI Chat Completions and compatible gateway transport.
pub struct OpenAiCompatibleProvider {
    provider_id: ProviderId,
    api_key: String,
    base_url: reqwest::Url,
    client: Client,
    request_timeout: Duration,
    compatibility: OpenAiCompletionsCompatibility,
}

impl OpenAiCompatibleProvider {
    pub fn new(provider_id: ProviderId, api_key: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            provider_id,
            api_key: api_key.into(),
            base_url: normalize_base_url(DEFAULT_BASE_URL, EndpointPolicy::SecureOrLoopback)?,
            client: client_for_policy(EndpointPolicy::SecureOrLoopback)?,
            request_timeout: DEFAULT_TIMEOUT,
            compatibility: OpenAiCompletionsCompatibility::default(),
        })
    }

    pub fn with_base_url(self, base_url: impl AsRef<str>) -> Result<Self, ProviderError> {
        self.with_base_url_and_policy(base_url, EndpointPolicy::SecureOrLoopback)
    }

    pub fn with_base_url_and_policy(
        mut self,
        base_url: impl AsRef<str>,
        policy: EndpointPolicy,
    ) -> Result<Self, ProviderError> {
        self.base_url = normalize_base_url(base_url.as_ref(), policy)?;
        self.client = client_for_policy(policy)?;
        Ok(self)
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn with_max_output_tokens_field(mut self, field: MaxOutputTokensField) -> Self {
        self.compatibility.max_output_tokens_field = field;
        self
    }

    pub fn with_compatibility(mut self, compatibility: OpenAiCompletionsCompatibility) -> Self {
        self.compatibility = compatibility;
        self
    }

    fn endpoint(&self) -> Result<reqwest::Url, ProviderError> {
        self.base_url
            .join("chat/completions")
            .map_err(|_| invalid("invalid provider endpoint"))
    }

    fn request(&self, body: &ChatRequest<'_>) -> Result<reqwest::RequestBuilder, ProviderError> {
        let body =
            serde_json::to_vec(body).map_err(|_| serialization("request serialization failed"))?;
        let mut request = self
            .client
            .post(self.endpoint()?)
            .header("content-type", "application/json")
            .timeout(self.request_timeout)
            .body(body);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        Ok(request)
    }

    async fn dispatch_request(
        &self,
        body: &ChatRequest<'_>,
        options: &RequestOptions,
    ) -> Result<Response, ProviderError> {
        let builder = apply_headers(
            self.request(body)?,
            &options.headers,
            (!self.api_key.is_empty()).then_some("authorization"),
        );
        match dispatch(builder, options.abort.as_ref()).await {
            DispatchResult::Aborted(phase) => Err(aborted(phase)),
            DispatchResult::Sent(result) => result.map_err(|error| self.transport_error(error)),
        }
    }

    async fn status_error(&self, response: Response, phase: FailurePhase) -> ProviderError {
        let retry_after = retry_after_from_headers(response.headers());
        let status = response.status();
        let body = match bounded_error_body(response).await {
            Ok(body) => body,
            Err(()) => {
                return ProviderError::new(
                    ProviderErrorKind::StreamInterrupted,
                    FailurePhase::DuringStream,
                    "OpenAI provider error body interrupted or exceeded the limit",
                )
                .with_status(status.as_u16())
            }
        };
        let message = serde_json::from_slice::<ApiErrorResponse>(&body)
            .ok()
            .map(|error| error.error.message)
            .unwrap_or_else(|| format!("HTTP {status} provider error"));
        let message = ProviderError::redacted_message(message, &self.api_key);
        let kind = match status.as_u16() {
            401 | 403 => ProviderErrorKind::Authentication,
            408 => ProviderErrorKind::Timeout,
            429 => ProviderErrorKind::RateLimit,
            400..=499 => ProviderErrorKind::InvalidRequest,
            500..=599 => ProviderErrorKind::Unavailable,
            _ => ProviderErrorKind::Other,
        };
        let mut error = ProviderError::new(kind, phase, message).with_status(status.as_u16());
        if let Some(retry_after) = retry_after {
            error = error.with_retry_after(retry_after);
        }
        error
    }

    fn transport_error(&self, error: reqwest::Error) -> ProviderError {
        let message = ProviderError::redacted_message(error.to_string(), &self.api_key);
        let (kind, phase) = classify_transport_failure(error.is_timeout(), error.is_connect());
        ProviderError::new(kind, phase, message)
    }
}

fn classify_transport_failure(
    is_timeout: bool,
    is_connect: bool,
) -> (ProviderErrorKind, FailurePhase) {
    if is_timeout {
        // A timeout while sending may happen before or after the provider
        // accepted the request. Keep the dispatch outcome ambiguous.
        (ProviderErrorKind::Timeout, FailurePhase::Unknown)
    } else if is_connect {
        (ProviderErrorKind::Unavailable, FailurePhase::BeforeDispatch)
    } else {
        (ProviderErrorKind::Other, FailurePhase::Unknown)
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        static API: Api = Api::OpenAiCompletions;
        &API
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            reasoning: true,
            tools: true,
            tool_streaming: true,
            vision: true,
        }
    }

    async fn complete_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<Completion, ProviderError> {
        let prepared = self.prepare_request(request)?;
        self.validate_prepared_request(&prepared)?;
        let request = prepared.request();
        let started = Instant::now();
        let body = chat_request(request, false, self.compatibility)?;
        let response = self.dispatch_request(&body, &options).await?;
        if !response.status().is_success() {
            return Err(self
                .status_error(response, FailurePhase::AfterDispatch)
                .await);
        }
        let bytes = bounded_response_body(response).await.map_err(|()| {
            ProviderError::new(
                ProviderErrorKind::StreamInterrupted,
                FailurePhase::DuringStream,
                "OpenAI response body interrupted or exceeded the limit",
            )
        })?;
        let response: ChatResponse = serde_json::from_slice(&bytes)
            .map_err(|_| response_serialization("invalid OpenAI completion response"))?;
        completion_from_response(response, started.elapsed())
    }

    async fn stream_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<ProviderStream, ProviderError> {
        let prepared = self.prepare_request(request)?;
        self.validate_prepared_request(&prepared)?;
        let request = prepared.request();
        let body = chat_request(request, true, self.compatibility)?;
        let response = self.dispatch_request(&body, &options).await?;
        if !response.status().is_success() {
            return Err(self
                .status_error(response, FailurePhase::AfterDispatch)
                .await);
        }
        let state = OpenAiStream::new(
            request.model.clone(),
            response.bytes_stream(),
            options.abort,
        );
        Ok(Box::pin(futures::stream::unfold(
            state,
            |mut state| async move { state.next_event().await.map(|event| (event, state)) },
        )))
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ChatReasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ChatThinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_budget: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Serialize)]
struct ChatThinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct ChatReasoning {
    enabled: bool,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct ChatMessage {
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<ChatContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Serialize)]
struct ChatTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunctionSpec,
}

#[derive(Serialize)]
struct ChatFunctionSpec {
    name: String,
    description: String,
    parameters: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum ChatContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatContentPart {
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<ChatImageUrl>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatImageUrl {
    url: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: ChatFunctionCall,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default)]
    completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u64>,
    #[serde(default)]
    cache_write_tokens: Option<u64>,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct CompletionTokensDetails {
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct ApiErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

fn chat_request(
    request: &CompletionRequest,
    stream: bool,
    compatibility: OpenAiCompletionsCompatibility,
) -> Result<ChatRequest<'_>, ProviderError> {
    let messages = request
        .messages
        .iter()
        .map(|message| {
            convert_message(
                message,
                compatibility.system_role,
                compatibility
                    .thinking_dialect
                    .wire_policy(compatibility.supports_reasoning_effort),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tools = (!request.tools.is_empty()).then(|| {
        request
            .tools
            .iter()
            .map(|tool| ChatTool {
                kind: "function",
                function: ChatFunctionSpec {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.input_schema.clone(),
                    strict: matches!(tool.constraint, Some(ToolConstraint::StrictJsonSchema))
                        .then_some(true),
                },
            })
            .collect()
    });
    let (max_tokens, max_completion_tokens) = match compatibility.max_output_tokens_field {
        MaxOutputTokensField::MaxTokens => (request.max_output_tokens, None),
        MaxOutputTokensField::MaxCompletionTokens => (None, request.max_output_tokens),
    };
    let reasoning = compatibility
        .thinking_dialect
        .wire_policy(compatibility.supports_reasoning_effort);
    let reasoning_fields = reasoning_wire_fields(request.reasoning.as_ref(), reasoning);
    Ok(ChatRequest {
        model: &request.model.id,
        messages,
        tools,
        temperature: request.temperature,
        max_tokens,
        max_completion_tokens,
        top_p: request.top_p,
        tool_choice: request.tool_choice.as_ref().map(openai_tool_choice),
        response_format: request
            .output_constraint
            .as_ref()
            .map(openai_output_constraint)
            .transpose()?,
        reasoning_effort: reasoning_fields.reasoning_effort,
        reasoning: reasoning_fields.reasoning,
        thinking: reasoning_fields.thinking,
        enable_thinking: reasoning_fields.enable_thinking,
        thinking_budget: reasoning_fields.thinking_budget,
        stream,
        stream_options: stream.then_some(StreamOptions {
            include_usage: true,
        }),
    })
}

fn openai_output_constraint(
    constraint: &OutputConstraint,
) -> Result<serde_json::Value, ProviderError> {
    match constraint {
        OutputConstraint::JsonSchema {
            name,
            schema,
            strict,
        } => Ok(serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": name,
                "strict": strict,
                "schema": schema,
            }
        })),
        OutputConstraint::Grammar { .. } => Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            FailurePhase::BeforeDispatch,
            "grammar structured output is not supported by OpenAI Chat Completions",
        )),
    }
}

fn openai_tool_choice(choice: &ToolChoice) -> serde_json::Value {
    match choice {
        ToolChoice::Auto => serde_json::json!("auto"),
        ToolChoice::None => serde_json::json!("none"),
        ToolChoice::Required => serde_json::json!("required"),
        ToolChoice::Tool { name } => serde_json::json!({
            "type": "function",
            "function": { "name": name },
        }),
    }
}

struct ReasoningWireFields {
    reasoning_effort: Option<String>,
    reasoning: Option<ChatReasoning>,
    thinking: Option<ChatThinking>,
    enable_thinking: Option<bool>,
    thinking_budget: Option<u32>,
}

fn reasoning_wire_fields(
    reasoning: Option<&ReasoningConfig>,
    policy: crate::ReasoningWirePolicy,
) -> ReasoningWireFields {
    let mut fields = ReasoningWireFields {
        reasoning_effort: None,
        reasoning: None,
        thinking: None,
        enable_thinking: None,
        thinking_budget: None,
    };
    let Some(reasoning) = reasoning else {
        return fields;
    };
    match policy.encoding {
        crate::ReasoningEncoding::OpenAiEffort => {
            if policy.effort_enabled {
                fields.reasoning_effort = if reasoning.enabled {
                    reasoning
                        .effort
                        .clone()
                        .or_else(|| reasoning.budget_tokens.is_none().then(|| "medium".into()))
                } else {
                    Some("none".into())
                };
            }
        }
        crate::ReasoningEncoding::ThinkingObject => {
            fields.thinking = Some(ChatThinking {
                kind: if reasoning.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
            });
            if policy.effort_enabled && reasoning.enabled {
                fields.reasoning_effort = reasoning.effort.clone();
            }
        }
        crate::ReasoningEncoding::TogetherToggle => {
            fields.reasoning = Some(ChatReasoning {
                enabled: reasoning.enabled,
            });
        }
        crate::ReasoningEncoding::QwenToggle => {
            fields.enable_thinking = Some(reasoning.enabled);
            fields.thinking_budget = reasoning
                .enabled
                .then_some(reasoning.budget_tokens)
                .flatten();
        }
    }
    fields
}

fn convert_message(
    message: &Message,
    system_role: OpenAiSystemRole,
    reasoning_policy: crate::ReasoningWirePolicy,
) -> Result<ChatMessage, ProviderError> {
    match message {
        Message::System { content } => Ok(ChatMessage {
            role: match system_role {
                OpenAiSystemRole::System => "system",
                OpenAiSystemRole::Developer => "developer",
            }
            .into(),
            content: Some(ChatContent::Text(content.clone())),
            ..Default::default()
        }),
        Message::User { content } => Ok(ChatMessage {
            role: "user".into(),
            content: Some(user_chat_content(content)?),
            ..Default::default()
        }),
        Message::Assistant(message) => {
            let mut text = String::new();
            let mut reasoning_content = String::new();
            let mut reasoning = String::new();
            let mut tool_calls = Vec::new();
            for part in &message.content {
                match part {
                    AssistantContent::Text(value) => text.push_str(&value.text),
                    AssistantContent::Reasoning(value) => match reasoning_policy.history {
                        crate::ReasoningHistoryEncoding::ReasoningContent => {
                            reasoning_content.push_str(&value.text)
                        }
                        crate::ReasoningHistoryEncoding::Reasoning => {
                            reasoning.push_str(&value.text)
                        }
                    },
                    AssistantContent::ToolCall(call) => tool_calls.push(ChatToolCall {
                        id: call.id.clone(),
                        kind: "function".into(),
                        function: ChatFunctionCall {
                            name: call.name.clone(),
                            arguments: serde_json::to_string(&call.arguments).map_err(|_| {
                                serialization("tool arguments are not serializable")
                            })?,
                        },
                    }),
                }
            }
            Ok(ChatMessage {
                role: "assistant".into(),
                content: (!text.is_empty()).then_some(ChatContent::Text(text)),
                reasoning_content: (!reasoning_content.is_empty()).then_some(reasoning_content),
                reasoning: (!reasoning.is_empty()).then_some(reasoning),
                tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                ..Default::default()
            })
        }
        Message::ToolResult {
            tool_call_id,
            name,
            content,
        } => Ok(ChatMessage {
            role: "tool".into(),
            content: Some(tool_result_chat_content(content)?),
            name: name.clone(),
            tool_call_id: Some(tool_call_id.clone()),
            ..Default::default()
        }),
    }
}

fn user_chat_content(content: &[UserContent]) -> Result<ChatContent, ProviderError> {
    if content
        .iter()
        .all(|part| matches!(part, UserContent::Text(_)))
    {
        return Ok(ChatContent::Text(
            content
                .iter()
                .filter_map(|part| match part {
                    UserContent::Text(value) => Some(value.text.as_str()),
                    UserContent::Image(_) => None,
                })
                .collect(),
        ));
    }
    let mut parts = Vec::new();
    for part in content {
        match part {
            UserContent::Text(value) => parts.push(ChatContentPart {
                kind: "text".into(),
                text: Some(value.text.clone()),
                image_url: None,
            }),
            UserContent::Image(image) => parts.push(image_part(image)?),
        }
    }
    Ok(ChatContent::Parts(parts))
}

fn tool_result_chat_content(content: &[ToolResultContent]) -> Result<ChatContent, ProviderError> {
    if content
        .iter()
        .all(|part| matches!(part, ToolResultContent::Text(_)))
    {
        return Ok(ChatContent::Text(
            content
                .iter()
                .filter_map(|part| match part {
                    ToolResultContent::Text(value) => Some(value.text.as_str()),
                    ToolResultContent::Image(_) => None,
                })
                .collect(),
        ));
    }
    let mut parts = Vec::new();
    for part in content {
        match part {
            ToolResultContent::Text(value) => parts.push(ChatContentPart {
                kind: "text".into(),
                text: Some(value.text.clone()),
                image_url: None,
            }),
            ToolResultContent::Image(image) => parts.push(image_part(image)?),
        }
    }
    Ok(ChatContent::Parts(parts))
}

fn image_part(image: &ImageContent) -> Result<ChatContentPart, ProviderError> {
    let (media_type, data) = normalize_image(image)?;
    Ok(ChatContentPart {
        kind: "image_url".into(),
        text: None,
        image_url: Some(ChatImageUrl {
            url: format!("data:{media_type};base64,{data}"),
        }),
    })
}

fn completion_from_response(
    response: ChatResponse,
    elapsed: Duration,
) -> Result<Completion, ProviderError> {
    let ChatResponse { choices, usage } = response;
    let choice = choices
        .into_iter()
        .next()
        .ok_or_else(|| response_protocol("OpenAI response contained no choices"))?;
    let message = assistant_from_wire(choice.message)?;
    let stop_reason = parse_stop_reason(choice.finish_reason.as_deref());
    let usage = usage.map(usage_from_wire);
    if let Some(usage) = usage {
        usage
            .validate()
            .map_err(|error| response_protocol(error.to_string()))?;
    }
    Ok(Completion {
        metadata: metadata(&message, stop_reason.clone(), true, elapsed),
        message,
        usage,
        continuation: None,
        stop_reason,
    })
}

fn assistant_from_wire(message: ChatMessage) -> Result<AssistantMessage, ProviderError> {
    let ChatMessage {
        content,
        reasoning_content,
        reasoning,
        tool_calls,
        ..
    } = message;
    let mut parts = Vec::new();
    let text = chat_content_text(content);
    if !text.is_empty() {
        parts.push(AssistantContent::Text(TextContent::new(text)));
    }
    let reasoning = reasoning_content.or(reasoning).unwrap_or_default();
    if !reasoning.is_empty() {
        parts.push(AssistantContent::Reasoning(ReasoningContent {
            text: reasoning,
            redacted: false,
            portability: ReasoningPortability::Portable,
            continuation_ref: None,
        }));
    }
    for call in tool_calls.unwrap_or_default() {
        if call.kind != "function" || call.function.name.is_empty() {
            return Err(response_protocol("invalid OpenAI function tool call"));
        }
        let arguments = serde_json::from_str(&call.function.arguments)
            .map_err(|_| response_serialization("invalid tool-call arguments JSON"))?;
        parts.push(AssistantContent::ToolCall(ToolCall {
            id: call.id,
            name: call.function.name,
            arguments,
        }));
    }
    Ok(AssistantMessage { content: parts })
}

fn chat_content_text(content: Option<ChatContent>) -> String {
    match content {
        Some(ChatContent::Text(text)) => text,
        Some(ChatContent::Parts(parts)) => parts.into_iter().filter_map(|part| part.text).collect(),
        None => String::new(),
    }
}

fn usage_from_wire(usage: ChatUsage) -> Usage {
    let input_tokens = usage.prompt_tokens;
    let output_tokens = usage.completion_tokens;
    Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        cache_read_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .filter(|value| *value > 0),
        cache_write_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cache_write_tokens)
            .filter(|value| *value > 0),
        reasoning_tokens: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens),
    }
}

fn parse_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("stop") => StopReason::Stop,
        Some("length") => StopReason::Length,
        Some("tool_calls" | "function_call") => StopReason::ToolUse,
        Some("content_filter") => StopReason::ContentFilter,
        Some(other) => StopReason::Other(other.into()),
        None => StopReason::Other("missing".into()),
    }
}

fn metadata(
    message: &AssistantMessage,
    stop_reason: StopReason,
    completed: bool,
    elapsed: Duration,
) -> CompletionMetadata {
    CompletionMetadata {
        content_chars: message.text_value().chars().count(),
        reasoning_chars: message.reasoning_chars(),
        tool_call_count: message.tool_calls().len(),
        stop_reason: Some(stop_reason),
        stream_completed: completed,
        elapsed_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SseChunk {
    choices: Vec<SseChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SseChoice {
    delta: SseDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SseDelta {
    content: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: FunctionCallDelta,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FunctionCallDelta {
    name: Option<String>,
    arguments: Option<String>,
}

struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    started: bool,
}

struct OpenAiStream {
    reader: SseReader,
    abort: Option<AbortSignal>,
    queue: VecDeque<Result<StreamEvent, ProviderError>>,
    text_started: bool,
    reasoning_started: bool,
    tools: BTreeMap<usize, PartialToolCall>,
    tool_arguments_bytes: usize,
    finish_reason: Option<StopReason>,
    usage: Option<Usage>,
    done: bool,
}

impl OpenAiStream {
    fn new<S>(model: ModelSpec, body: S, abort: Option<AbortSignal>) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    {
        let mut queue = VecDeque::new();
        queue.push_back(Ok(StreamEvent::Start {
            model: model.id.clone(),
        }));
        Self {
            reader: SseReader::new(body),
            abort,
            queue,
            text_started: false,
            reasoning_started: false,
            tools: BTreeMap::new(),
            tool_arguments_bytes: 0,
            finish_reason: None,
            usage: None,
            done: false,
        }
    }

    async fn next_record(&mut self) -> Result<Option<crate::providers::SseRecord>, ProviderError> {
        let Some(abort) = &self.abort else {
            return self.reader.next_record().await;
        };
        if abort.is_aborted() {
            return Err(aborted(FailurePhase::DuringStream));
        }
        tokio::select! {
            _ = abort.cancelled() => Err(aborted(FailurePhase::DuringStream)),
            record = self.reader.next_record() => record,
        }
    }

    async fn next_event(&mut self) -> Option<Result<StreamEvent, ProviderError>> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Some(event);
            }
            if self.done {
                return None;
            }
            let record = match self.next_record().await {
                Ok(Some(record)) => record,
                Ok(None) => {
                    self.done = true;
                    return Some(Err(stream_error("OpenAI SSE ended before [DONE]")));
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            };
            let result = if record.data.trim() == "[DONE]" {
                self.finish()
            } else {
                self.ingest(&record.data)
            };
            if let Err(error) = result {
                self.done = true;
                self.queue.push_back(Err(error));
            }
        }
    }

    fn ingest(&mut self, data: &str) -> Result<(), ProviderError> {
        let chunk: SseChunk = serde_json::from_str(data)
            .map_err(|error| stream_serialization(format!("invalid OpenAI SSE JSON: {error}")))?;
        if let Some(usage) = chunk.usage {
            self.usage = Some(usage_from_wire(usage));
        }
        for choice in chunk.choices {
            if let Some(reason) = choice.finish_reason {
                self.finish_reason = Some(parse_stop_reason(Some(&reason)));
            }
            if let Some(text) = choice.delta.content.filter(|value| !value.is_empty()) {
                if !self.text_started {
                    self.text_started = true;
                    self.queue.push_back(Ok(StreamEvent::TextStart));
                }
                self.queue.push_back(Ok(StreamEvent::TextDelta { text }));
            }
            let reasoning = choice
                .delta
                .reasoning
                .or(choice.delta.reasoning_content)
                .filter(|value| !value.is_empty());
            if let Some(text) = reasoning {
                if !self.reasoning_started {
                    self.reasoning_started = true;
                    self.queue.push_back(Ok(StreamEvent::ReasoningStart));
                }
                self.queue
                    .push_back(Ok(StreamEvent::ReasoningDelta { text }));
            }
            for delta in choice.delta.tool_calls.unwrap_or_default() {
                if delta.kind.as_deref().is_some_and(|kind| kind != "function") {
                    return Err(protocol("unsupported OpenAI tool-call type"));
                }
                let index = match delta.index {
                    Some(index) => index,
                    None => match delta.id.as_deref() {
                        Some(id) => self
                            .tools
                            .iter()
                            .find_map(|(index, call)| {
                                (call.id.as_deref() == Some(id)).then_some(*index)
                            })
                            .unwrap_or_else(|| self.next_tool_index()),
                        None if self.tools.len() == 1 => *self.tools.keys().next().unwrap(),
                        None => {
                            return Err(protocol(
                                "ambiguous OpenAI tool delta without index or id",
                            ));
                        }
                    },
                };
                let argument_bytes = delta.function.arguments.as_ref().map_or(0, String::len);
                self.add_tool_argument_bytes(argument_bytes)?;
                let entry = self.tools.entry(index).or_insert_with(|| PartialToolCall {
                    id: None,
                    name: None,
                    arguments: String::new(),
                    started: false,
                });
                if let Some(id) = delta.id {
                    if let Some(existing) = &entry.id {
                        if existing != &id {
                            return Err(protocol("OpenAI tool call id changed during stream"));
                        }
                    } else {
                        entry.id = Some(id);
                    }
                }
                if let Some(name) = delta.function.name {
                    if let Some(existing) = &entry.name {
                        if existing != &name {
                            return Err(protocol("OpenAI tool call name changed during stream"));
                        }
                    } else {
                        entry.name = Some(name);
                    }
                }
                if !entry.started {
                    if let (Some(id), Some(name)) = (&entry.id, &entry.name) {
                        entry.started = true;
                        self.queue.push_back(Ok(StreamEvent::ToolCallStart {
                            index,
                            id: id.clone(),
                            name: name.clone(),
                        }));
                        if !entry.arguments.is_empty() {
                            self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
                                index,
                                arguments_delta: entry.arguments.clone(),
                            }));
                        }
                    }
                }
                if let Some(arguments) = delta.function.arguments {
                    if entry.arguments.len().saturating_add(arguments.len())
                        > MAX_STREAM_BUFFER_BYTES
                    {
                        return Err(protocol("OpenAI tool arguments exceed the stream limit"));
                    }
                    entry.arguments.push_str(&arguments);
                    if entry.started {
                        self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
                            index,
                            arguments_delta: arguments,
                        }));
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ProviderError> {
        let stop_reason = self
            .finish_reason
            .clone()
            .ok_or_else(|| protocol("OpenAI stream ended without finish_reason"))?;
        if self.text_started {
            self.text_started = false;
            self.queue.push_back(Ok(StreamEvent::TextEnd));
        }
        if self.reasoning_started {
            self.reasoning_started = false;
            self.queue.push_back(Ok(StreamEvent::ReasoningEnd));
        }
        let tools = std::mem::take(&mut self.tools);
        for (index, tool) in tools {
            let id = tool
                .id
                .ok_or_else(|| protocol("OpenAI tool call has no id"))?;
            let name = tool
                .name
                .ok_or_else(|| protocol("OpenAI tool call has no name"))?;
            if !tool.started {
                self.queue.push_back(Ok(StreamEvent::ToolCallStart {
                    index,
                    id: id.clone(),
                    name: name.clone(),
                }));
                if !tool.arguments.is_empty() {
                    self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
                        index,
                        arguments_delta: tool.arguments.clone(),
                    }));
                }
            }
            let arguments = serde_json::from_str(&tool.arguments)
                .map_err(|_| protocol("malformed OpenAI tool-call arguments"))?;
            self.queue.push_back(Ok(StreamEvent::ToolCallEnd {
                index,
                tool_call: ToolCall {
                    id,
                    name,
                    arguments,
                },
            }));
        }
        if let Some(usage) = self.usage.take() {
            self.queue.push_back(Ok(StreamEvent::Usage(usage)));
        }
        self.queue.push_back(Ok(StreamEvent::Done { stop_reason }));
        self.done = true;
        Ok(())
    }

    fn add_tool_argument_bytes(&mut self, bytes: usize) -> Result<(), ProviderError> {
        self.tool_arguments_bytes = self
            .tool_arguments_bytes
            .checked_add(bytes)
            .ok_or_else(|| protocol("OpenAI tool arguments exceed the stream limit"))?;
        if self.tool_arguments_bytes > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("OpenAI tool arguments exceed the stream limit"));
        }
        Ok(())
    }

    fn next_tool_index(&self) -> usize {
        self.tools
            .keys()
            .next_back()
            .copied()
            .map_or(0, |index| index.saturating_add(1))
    }
}

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn serialization(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Serialization,
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn response_protocol(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Protocol,
        FailurePhase::AfterDispatch,
        message,
    )
}

fn response_serialization(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Serialization,
        FailurePhase::AfterDispatch,
        message,
    )
}

fn stream_serialization(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Serialization,
        FailurePhase::DuringStream,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_failure_is_retryable_before_dispatch() {
        assert_eq!(
            classify_transport_failure(false, true),
            (ProviderErrorKind::Unavailable, FailurePhase::BeforeDispatch)
        );
    }

    #[test]
    fn ambiguous_transport_timeout_is_unknown() {
        assert_eq!(
            classify_transport_failure(true, true),
            (ProviderErrorKind::Timeout, FailurePhase::Unknown)
        );
    }
}
