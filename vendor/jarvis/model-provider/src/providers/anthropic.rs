use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::Stream;
use reqwest::{Client, Response};
use serde_json::{json, Value};

use crate::providers::{
    aborted, apply_headers, bounded_error_body, bounded_response_body, client_for_policy, dispatch,
    normalize_base_url, normalize_image, protocol, retry_after_from_headers, stream_error,
    DispatchResult, EndpointPolicy, SseReader, MAX_STREAM_BUFFER_BYTES,
};
use crate::{
    AbortSignal, AnthropicMessagesContinuation, AnthropicReasoningReplay,
    AnthropicReasoningReplayEntry, Api, AssistantContent, AssistantMessage, Completion,
    CompletionMetadata, CompletionRequest, ContinuationRef, ContinuationScope, FailurePhase,
    Message, ModelProvider, ModelSpec, OutputConstraint, ProviderCapabilities,
    ProviderContinuation, ProviderError, ProviderErrorKind, ProviderId, ProviderStream,
    ReasoningConfig, ReasoningContent, ReasoningPortability, RequestOptions, StopReason,
    StreamEvent, TextContent, ToolCall, ToolChoice, ToolConstraint, ToolResultContent, Usage,
    UserContent,
};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Anthropic Messages API transport with normalized model messages/events.
pub struct AnthropicProvider {
    provider_id: ProviderId,
    api_key: String,
    bearer_auth: bool,
    base_url: reqwest::Url,
    client: Client,
    request_timeout: Duration,
}

impl AnthropicProvider {
    pub fn new(provider_id: ProviderId, api_key: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            provider_id,
            api_key: api_key.into(),
            bearer_auth: false,
            base_url: normalize_base_url(DEFAULT_BASE_URL, EndpointPolicy::SecureOrLoopback)?,
            client: client_for_policy(EndpointPolicy::SecureOrLoopback)?,
            request_timeout: DEFAULT_TIMEOUT,
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

    pub fn with_bearer_auth(mut self, bearer_auth: bool) -> Self {
        self.bearer_auth = bearer_auth;
        self
    }

    fn endpoint(&self) -> Result<reqwest::Url, ProviderError> {
        self.base_url
            .join("messages")
            .map_err(|_| invalid("invalid provider endpoint"))
    }

    async fn dispatch_request(
        &self,
        body: Value,
        options: &RequestOptions,
    ) -> Result<Response, ProviderError> {
        let builder = apply_headers(
            self.request(body)?,
            &options.headers,
            (!self.api_key.is_empty()).then_some(if self.bearer_auth {
                "authorization"
            } else {
                "x-api-key"
            }),
        );
        match dispatch(builder, options.abort.as_ref()).await {
            DispatchResult::Aborted(phase) => Err(aborted(phase)),
            DispatchResult::Sent(result) => result.map_err(|error| self.transport_error(error)),
        }
    }

    fn request(&self, body: Value) -> Result<reqwest::RequestBuilder, ProviderError> {
        let mut request = self
            .client
            .post(self.endpoint()?)
            .header("content-type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .timeout(self.request_timeout)
            .json(&body);
        if !self.api_key.is_empty() {
            request = if self.bearer_auth {
                request.bearer_auth(&self.api_key)
            } else {
                request.header("x-api-key", &self.api_key)
            };
        }
        Ok(request)
    }

    async fn status_error(&self, response: Response, phase: FailurePhase) -> ProviderError {
        let retry_after = retry_after_from_headers(response.headers());
        let status = response.status();
        let body = match bounded_error_body(response).await {
            Ok(body) => body,
            Err(_) => {
                return ProviderError::new(
                    ProviderErrorKind::StreamInterrupted,
                    FailurePhase::DuringStream,
                    "Anthropic provider error body interrupted or exceeded the limit",
                )
                .with_status(status.as_u16())
            }
        };
        let message = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|body| {
                body.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("HTTP {status} provider error"));
        let mut error = ProviderError::new(
            match status.as_u16() {
                401 | 403 => ProviderErrorKind::Authentication,
                408 => ProviderErrorKind::Timeout,
                429 => ProviderErrorKind::RateLimit,
                400..=499 => ProviderErrorKind::InvalidRequest,
                500..=599 => ProviderErrorKind::Unavailable,
                _ => ProviderErrorKind::Other,
            },
            phase,
            ProviderError::redacted_message(message, &self.api_key),
        )
        .with_status(status.as_u16());
        if let Some(retry_after) = retry_after {
            error = error.with_retry_after(retry_after);
        }
        error
    }

    fn transport_error(&self, error: reqwest::Error) -> ProviderError {
        let (kind, phase) = classify_transport_failure(error.is_timeout(), error.is_connect());
        ProviderError::new(
            kind,
            phase,
            ProviderError::redacted_message(error.to_string(), &self.api_key),
        )
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
impl ModelProvider for AnthropicProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        static API: Api = Api::AnthropicMessages;
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
        let response = self
            .dispatch_request(request_body(request, false)?, &options)
            .await?;
        if !response.status().is_success() {
            return Err(self
                .status_error(response, FailurePhase::AfterDispatch)
                .await);
        }
        let body = bounded_response_body(response).await.map_err(|()| {
            stream_error("Anthropic response body interrupted or exceeded the limit")
        })?;
        let body = serde_json::from_slice::<Value>(&body)
            .map_err(|_| response_serialization("invalid Anthropic completion response"))?;
        completion_from_response(
            body,
            started.elapsed(),
            self.provider_id.clone(),
            request.model.id.clone(),
            &request.messages,
            request.continuation.as_ref(),
        )
    }

    async fn stream_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<ProviderStream, ProviderError> {
        let prepared = self.prepare_request(request)?;
        self.validate_prepared_request(&prepared)?;
        let request = prepared.request();
        let response = self
            .dispatch_request(request_body(request, true)?, &options)
            .await?;
        if !response.status().is_success() {
            return Err(self
                .status_error(response, FailurePhase::AfterDispatch)
                .await);
        }
        let state = AnthropicStream::new(
            self.provider_id.clone(),
            request.model.clone(),
            request.messages.clone(),
            request
                .continuation
                .as_ref()
                .and_then(ProviderContinuation::anthropic_messages)
                .map(|continuation| continuation.reasoning_entries().to_vec())
                .unwrap_or_default(),
            response.bytes_stream(),
            options.abort,
        );
        Ok(Box::pin(futures::stream::unfold(
            state,
            |mut state| async move { state.next_event().await.map(|event| (event, state)) },
        )))
    }
}

fn request_body(request: &CompletionRequest, stream: bool) -> Result<Value, ProviderError> {
    let mut system = Vec::new();
    let mut messages = Vec::new();
    let mut replay = AnthropicReplayCursor::new(request.continuation.as_ref());
    for message in &request.messages {
        match message {
            Message::System { content } => system.push(content.clone()),
            Message::User { content } => messages.push(json!({
                "role": "user",
                "content": user_blocks(content)?,
            })),
            Message::Assistant(message) => messages.push(json!({
                "role": "assistant",
                "content": assistant_blocks(message, &mut replay)?,
            })),
            Message::ToolResult {
                tool_call_id,
                content,
                ..
            } => messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": tool_result_content(content)?,
                }],
            })),
        }
    }
    replay.finish()?;
    if messages.is_empty() {
        return Err(invalid("Anthropic request requires at least one message"));
    }
    let mut body = json!({
        "model": request.model.id,
        "max_tokens": request.max_output_tokens.unwrap_or(4096),
        "messages": messages,
        "stream": stream,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system.join("\n\n"));
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(top_p) = request.top_p {
        body["top_p"] = json!(top_p);
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    let mut value = json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.input_schema,
                    });
                    if matches!(&tool.constraint, Some(ToolConstraint::StrictJsonSchema)) {
                        value["strict"] = json!(true);
                    }
                    value
                })
                .collect(),
        );
    }
    if let Some(choice) = &request.tool_choice {
        body["tool_choice"] = anthropic_tool_choice(choice);
    }
    if let Some(reasoning) = &request.reasoning {
        body["thinking"] = anthropic_thinking(reasoning, request.max_output_tokens)?;
    }
    if let Some(constraint) = &request.output_constraint {
        body["output_config"] = anthropic_output_constraint(constraint)?;
    }
    Ok(body)
}

fn anthropic_output_constraint(constraint: &OutputConstraint) -> Result<Value, ProviderError> {
    match constraint {
        OutputConstraint::JsonSchema {
            schema,
            strict: true,
            ..
        } => Ok(json!({
            "format": {
                "type": "json_schema",
                "schema": schema,
            }
        })),
        OutputConstraint::JsonSchema { .. } => Err(unsupported(
            "non-strict JSON Schema structured output is not supported by Anthropic Messages",
        )),
        OutputConstraint::Grammar { .. } => Err(unsupported(
            "grammar structured output is not supported by Anthropic Messages",
        )),
    }
}

fn anthropic_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!({"type": "auto"}),
        ToolChoice::None => json!({"type": "none"}),
        ToolChoice::Required => json!({"type": "any"}),
        ToolChoice::Tool { name } => json!({"type": "tool", "name": name}),
    }
}

fn anthropic_thinking(
    reasoning: &ReasoningConfig,
    max_output_tokens: Option<u32>,
) -> Result<Value, ProviderError> {
    if !reasoning.enabled {
        return Ok(json!({"type": "disabled"}));
    }
    let budget = reasoning.budget_tokens.unwrap_or(1_024);
    let max_tokens = max_output_tokens.unwrap_or(4_096);
    if max_tokens <= budget {
        return Err(invalid(
            "max_output_tokens must exceed thinking budget_tokens",
        ));
    }
    Ok(json!({"type": "enabled", "budget_tokens": budget}))
}

fn user_blocks(content: &[UserContent]) -> Result<Vec<Value>, ProviderError> {
    content
        .iter()
        .map(|content| match content {
            UserContent::Text(text) => Ok(json!({"type": "text", "text": text.text})),
            UserContent::Image(image) => {
                let (media_type, data) = normalize_image(image)?;
                Ok(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    }
                }))
            }
        })
        .collect()
}

struct AnthropicReplayCursor<'a> {
    continuation: Option<&'a AnthropicMessagesContinuation>,
    used: HashSet<ContinuationRef>,
}

impl<'a> AnthropicReplayCursor<'a> {
    fn new(continuation: Option<&'a ProviderContinuation>) -> Self {
        Self {
            continuation: continuation.and_then(ProviderContinuation::anthropic_messages),
            used: HashSet::new(),
        }
    }

    fn next(
        &mut self,
        reasoning: &ReasoningContent,
    ) -> Result<&'a AnthropicReasoningReplay, ProviderError> {
        if reasoning.portability != ReasoningPortability::ProviderBound {
            return Err(unsupported(
                "Anthropic provider-bound reasoning is missing continuation state",
            ));
        }
        let reference = reasoning.continuation_ref.as_ref().ok_or_else(|| {
            unsupported("Anthropic provider-bound reasoning is missing continuation reference")
        })?;
        let continuation = self.continuation.ok_or_else(|| {
            unsupported("Anthropic provider-bound reasoning is missing replay metadata")
        })?;
        if !self.used.insert(reference.clone()) {
            return Err(unsupported(
                "Anthropic reasoning continuation reference was replayed twice",
            ));
        }
        let replay = self
            .continuation
            .and_then(|_| continuation.replay_for(reference))
            .ok_or_else(|| {
                unsupported("Anthropic provider-bound reasoning is missing replay metadata")
            })?;
        Ok(replay)
    }

    fn finish(&self) -> Result<(), ProviderError> {
        if self
            .continuation
            .is_some_and(|continuation| self.used.len() != continuation.reasoning_entry_count())
        {
            return Err(unsupported(
                "Anthropic continuation has unused reasoning replay metadata",
            ));
        }
        Ok(())
    }
}

fn assistant_blocks(
    message: &AssistantMessage,
    replay: &mut AnthropicReplayCursor<'_>,
) -> Result<Vec<Value>, ProviderError> {
    message
        .content
        .iter()
        .map(|content| match content {
            AssistantContent::Text(text) => Ok(json!({"type": "text", "text": text.text})),
            AssistantContent::Reasoning(reasoning) => match replay.next(reasoning)? {
                AnthropicReasoningReplay::Redacted { data } if reasoning.redacted => Ok(json!({
                    "type": "redacted_thinking",
                    "data": data,
                })),
                AnthropicReasoningReplay::Thinking { signature } if !reasoning.redacted => {
                    let mut block = json!({"type": "thinking", "thinking": reasoning.text});
                    block["signature"] = json!(signature);
                    Ok(block)
                }
                AnthropicReasoningReplay::Redacted { .. } => Err(unsupported(
                    "Anthropic thinking replay metadata does not match normalized content",
                )),
                AnthropicReasoningReplay::Thinking { .. } => Err(unsupported(
                    "Anthropic redacted replay metadata does not match normalized content",
                )),
            },
            AssistantContent::ToolCall(call) => Ok(json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.arguments,
            })),
        })
        .collect()
}

fn tool_result_content(content: &[ToolResultContent]) -> Result<Value, ProviderError> {
    if content
        .iter()
        .all(|part| matches!(part, ToolResultContent::Text(_)))
    {
        return Ok(Value::String(
            content
                .iter()
                .filter_map(|part| match part {
                    ToolResultContent::Text(value) => Some(value.text.as_str()),
                    ToolResultContent::Image(_) => None,
                })
                .collect(),
        ));
    }
    let mut blocks = Vec::new();
    for part in content {
        match part {
            ToolResultContent::Text(value) => {
                blocks.push(json!({"type": "text", "text": value.text}))
            }
            ToolResultContent::Image(image) => {
                let (media_type, data) = normalize_image(image)?;
                blocks.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    }
                }));
            }
        }
    }
    Ok(Value::Array(blocks))
}

fn completion_from_response(
    body: Value,
    elapsed: Duration,
    provider: ProviderId,
    model: String,
    history: &[Message],
    previous_continuation: Option<&ProviderContinuation>,
) -> Result<Completion, ProviderError> {
    let (message, reasoning) = assistant_from_blocks(
        body.get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| protocol("Anthropic response has no content"))?,
    )?;
    let stop_reason = parse_stop_reason(body.get("stop_reason").and_then(Value::as_str))?;
    let usage = body.get("usage").map(usage_from_value);
    if let Some(usage) = usage {
        usage
            .validate()
            .map_err(|error| protocol(error.to_string()))?;
    }
    let mut replay = previous_continuation
        .and_then(ProviderContinuation::anthropic_messages)
        .map(|continuation| continuation.reasoning_entries().to_vec())
        .unwrap_or_default();
    replay.extend(reasoning);
    let continuation = if replay.is_empty() {
        None
    } else {
        let mut covered_history = history.to_vec();
        covered_history.push(Message::Assistant(message.clone()));
        let scope = ContinuationScope::for_history(&covered_history).map_err(protocol)?;
        Some(ProviderContinuation::AnthropicMessages(
            AnthropicMessagesContinuation::with_scope(provider, model, scope, replay)
                .map_err(protocol)?,
        ))
    };
    Ok(Completion {
        metadata: metadata(&message, stop_reason.clone(), true, elapsed),
        message,
        usage,
        continuation,
        stop_reason,
    })
}

fn assistant_from_blocks(
    blocks: &[Value],
) -> Result<(AssistantMessage, Vec<AnthropicReasoningReplayEntry>), ProviderError> {
    let mut content = Vec::new();
    let mut reasoning = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Anthropic text block has no text"))?;
                content.push(AssistantContent::Text(TextContent::new(text)));
            }
            Some("thinking") => {
                let text = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Anthropic thinking block has no text"))?;
                let signature = block
                    .get("signature")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| protocol("Anthropic thinking block has no signature"))?;
                let reference = ContinuationRef::generated();
                content.push(AssistantContent::Reasoning(ReasoningContent {
                    text: text.into(),
                    redacted: false,
                    portability: ReasoningPortability::ProviderBound,
                    continuation_ref: Some(reference.clone()),
                }));
                reasoning.push(AnthropicReasoningReplayEntry::new(
                    reference,
                    AnthropicReasoningReplay::thinking(signature),
                ));
            }
            Some("redacted_thinking") => {
                let data = block
                    .get("data")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| protocol("Anthropic redacted thinking has no data"))?;
                let reference = ContinuationRef::generated();
                content.push(AssistantContent::Reasoning(ReasoningContent {
                    text: String::new(),
                    redacted: true,
                    portability: ReasoningPortability::ProviderBound,
                    continuation_ref: Some(reference.clone()),
                }));
                reasoning.push(AnthropicReasoningReplayEntry::new(
                    reference,
                    AnthropicReasoningReplay::redacted(data),
                ));
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Anthropic tool_use block has no id"))?;
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Anthropic tool_use block has no name"))?;
                let arguments = block.get("input").cloned().unwrap_or_else(|| json!({}));
                content.push(AssistantContent::ToolCall(ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                }));
            }
            Some(other) => {
                return Err(protocol(format!(
                    "unsupported Anthropic block type {other}"
                )))
            }
            None => return Err(protocol("Anthropic content block has no type")),
        }
    }
    Ok((AssistantMessage { content }, reasoning))
}

fn usage_from_value(value: &Value) -> Usage {
    let raw_input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let cache_read_tokens = value.get("cache_read_input_tokens").and_then(Value::as_u64);
    let cache_write_tokens = value
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64);
    let input_tokens = raw_input_tokens
        .saturating_add(cache_read_tokens.unwrap_or_default())
        .saturating_add(cache_write_tokens.unwrap_or_default());
    let output_tokens = value
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens: None,
    }
}

fn parse_stop_reason(value: Option<&str>) -> Result<StopReason, ProviderError> {
    match value {
        Some("end_turn" | "stop_sequence") => Ok(StopReason::Stop),
        Some("max_tokens") => Ok(StopReason::Length),
        Some("tool_use") => Ok(StopReason::ToolUse),
        Some("refusal" | "content_filter") => Ok(StopReason::ContentFilter),
        Some(other) => Ok(StopReason::Other(other.into())),
        None => Err(protocol("Anthropic response has no stop_reason")),
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

struct BlockState {
    kind: String,
    id: Option<String>,
    name: Option<String>,
    text: String,
    arguments: String,
    signature: Option<String>,
    redacted_data: Option<String>,
    continuation_ref: Option<ContinuationRef>,
}

struct AnthropicStream {
    provider: ProviderId,
    model: String,
    history: Vec<Message>,
    prior_reasoning_replay: Vec<AnthropicReasoningReplayEntry>,
    reader: SseReader,
    abort: Option<AbortSignal>,
    queue: VecDeque<Result<StreamEvent, ProviderError>>,
    blocks: BTreeMap<usize, BlockState>,
    tool_arguments_bytes: usize,
    stop_reason: Option<StopReason>,
    usage: Option<Usage>,
    reasoning_replay: Vec<AnthropicReasoningReplayEntry>,
    completed_content: BTreeMap<usize, AssistantContent>,
    done: bool,
}

impl AnthropicStream {
    fn new<S>(
        provider: ProviderId,
        model: ModelSpec,
        history: Vec<Message>,
        prior_reasoning_replay: Vec<AnthropicReasoningReplayEntry>,
        body: S,
        abort: Option<AbortSignal>,
    ) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    {
        let mut queue = VecDeque::new();
        queue.push_back(Ok(StreamEvent::Start {
            model: model.id.clone(),
        }));
        Self {
            provider,
            model: model.id.clone(),
            history,
            prior_reasoning_replay,
            reader: SseReader::new(body),
            abort,
            queue,
            blocks: BTreeMap::new(),
            tool_arguments_bytes: 0,
            stop_reason: None,
            usage: None,
            reasoning_replay: Vec::new(),
            completed_content: BTreeMap::new(),
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
                    return Some(Err(stream_error("Anthropic SSE ended before message_stop")));
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            };
            let result = self.ingest(record.event.as_deref().unwrap_or_default(), &record.data);
            if let Err(error) = result {
                self.done = true;
                self.queue.push_back(Err(error));
            }
        }
    }

    fn ingest(&mut self, event: &str, data: &str) -> Result<(), ProviderError> {
        let body: Value =
            serde_json::from_str(data).map_err(|_| protocol("invalid Anthropic SSE JSON"))?;
        match event {
            "message_start" => {
                if let Some(usage) = body.pointer("/message/usage") {
                    self.usage = Some(usage_from_value(usage));
                }
            }
            "content_block_start" => {
                let index = body
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| protocol("Anthropic block start has no index"))?
                    as usize;
                let block = body
                    .get("content_block")
                    .ok_or_else(|| protocol("Anthropic block start has no content_block"))?;
                let kind = block
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Anthropic block start has no type"))?
                    .to_owned();
                let continuation_ref = matches!(kind.as_str(), "thinking" | "redacted_thinking")
                    .then(ContinuationRef::generated);
                let state = BlockState {
                    kind: kind.clone(),
                    id: block.get("id").and_then(Value::as_str).map(str::to_owned),
                    name: block.get("name").and_then(Value::as_str).map(str::to_owned),
                    text: String::new(),
                    arguments: String::new(),
                    signature: None,
                    redacted_data: block
                        .get("data")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    continuation_ref: continuation_ref.clone(),
                };
                if self.blocks.insert(index, state).is_some() {
                    return Err(protocol("duplicate Anthropic content block"));
                }
                match kind.as_str() {
                    "text" => self.queue.push_back(Ok(StreamEvent::TextStart)),
                    "thinking" => {
                        self.queue.push_back(Ok(StreamEvent::ReasoningStart));
                        self.queue.push_back(Ok(StreamEvent::ReasoningReference {
                            reference: continuation_ref
                                .clone()
                                .expect("thinking continuation reference is present"),
                        }));
                    }
                    "redacted_thinking" => {
                        let data = block
                            .get("data")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| protocol("Anthropic redacted thinking has no data"))?;
                        self.queue.push_back(Ok(StreamEvent::ReasoningStart));
                        self.queue.push_back(Ok(StreamEvent::ReasoningReference {
                            reference: continuation_ref
                                .clone()
                                .expect("redacted continuation reference is present"),
                        }));
                        self.queue.push_back(Ok(StreamEvent::ReasoningRedacted {
                            data: Some(data.into()),
                        }));
                    }
                    "tool_use" => {
                        self.queue.push_back(Ok(StreamEvent::ToolCallStart {
                            index,
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .ok_or_else(|| protocol("Anthropic tool_use has no id"))?
                                .into(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .ok_or_else(|| protocol("Anthropic tool_use has no name"))?
                                .into(),
                        }));
                    }
                    other => return Err(protocol(format!("unsupported Anthropic block {other}"))),
                }
            }
            "content_block_delta" => {
                let index = body
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| protocol("Anthropic block delta has no index"))?
                    as usize;
                let delta = body
                    .get("delta")
                    .ok_or_else(|| protocol("Anthropic block delta has no delta"))?;
                if delta.get("type").and_then(Value::as_str) == Some("input_json_delta") {
                    let bytes = delta
                        .get("partial_json")
                        .and_then(Value::as_str)
                        .ok_or_else(|| protocol("Anthropic tool delta has no partial_json"))?
                        .len();
                    self.add_tool_argument_bytes(bytes)?;
                }
                let Some(block) = self.blocks.get_mut(&index) else {
                    return Err(protocol("Anthropic block delta before block start"));
                };
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = delta
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| protocol("Anthropic text delta has no text"))?;
                        block.text.push_str(text);
                        self.queue
                            .push_back(Ok(StreamEvent::TextDelta { text: text.into() }));
                    }
                    Some("thinking_delta") => {
                        let text = delta
                            .get("thinking")
                            .and_then(Value::as_str)
                            .ok_or_else(|| protocol("Anthropic thinking delta has no text"))?;
                        block.text.push_str(text);
                        self.queue
                            .push_back(Ok(StreamEvent::ReasoningDelta { text: text.into() }));
                    }
                    Some("signature_delta") => {
                        if block.kind != "thinking" {
                            return Err(protocol("Anthropic signature delta is not for thinking"));
                        }
                        let signature_delta = delta
                            .get("signature")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                protocol("Anthropic signature delta has no signature")
                            })?;
                        let signature = block.signature.get_or_insert_with(String::new);
                        signature.push_str(signature_delta);
                        self.queue.push_back(Ok(StreamEvent::ReasoningSignature {
                            signature: signature_delta.into(),
                        }));
                    }
                    Some("input_json_delta") => {
                        let text = delta
                            .get("partial_json")
                            .and_then(Value::as_str)
                            .ok_or_else(|| protocol("Anthropic tool delta has no partial_json"))?;
                        if block.arguments.len().saturating_add(text.len())
                            > MAX_STREAM_BUFFER_BYTES
                        {
                            return Err(protocol(
                                "Anthropic tool arguments exceed the stream limit",
                            ));
                        }
                        block.arguments.push_str(text);
                        self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
                            index,
                            arguments_delta: text.into(),
                        }));
                    }
                    Some(other) => {
                        return Err(protocol(format!("unsupported Anthropic delta {other}")))
                    }
                    None => return Err(protocol("Anthropic block delta has no type")),
                }
            }
            "content_block_stop" => {
                let index = body
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| protocol("Anthropic block stop has no index"))?
                    as usize;
                let block = self
                    .blocks
                    .remove(&index)
                    .ok_or_else(|| protocol("Anthropic block stop before block start"))?;
                match block.kind.as_str() {
                    "text" => {
                        self.completed_content
                            .insert(index, AssistantContent::Text(TextContent::new(block.text)));
                        self.queue.push_back(Ok(StreamEvent::TextEnd));
                    }
                    "thinking" => {
                        let signature = block
                            .signature
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| protocol("Anthropic thinking block has no signature"))?;
                        let continuation_ref = block.continuation_ref.ok_or_else(|| {
                            protocol("Anthropic thinking block has no continuation reference")
                        })?;
                        self.reasoning_replay
                            .push(AnthropicReasoningReplayEntry::new(
                                continuation_ref.clone(),
                                AnthropicReasoningReplay::thinking(signature),
                            ));
                        self.completed_content.insert(
                            index,
                            AssistantContent::Reasoning(ReasoningContent {
                                text: block.text,
                                redacted: false,
                                portability: ReasoningPortability::ProviderBound,
                                continuation_ref: Some(continuation_ref),
                            }),
                        );
                        self.queue.push_back(Ok(StreamEvent::ReasoningEnd));
                    }
                    "redacted_thinking" => {
                        let data = block
                            .redacted_data
                            .ok_or_else(|| protocol("Anthropic redacted thinking has no data"))?;
                        let continuation_ref = block.continuation_ref.ok_or_else(|| {
                            protocol("Anthropic redacted thinking has no continuation reference")
                        })?;
                        self.reasoning_replay
                            .push(AnthropicReasoningReplayEntry::new(
                                continuation_ref.clone(),
                                AnthropicReasoningReplay::redacted(data),
                            ));
                        self.completed_content.insert(
                            index,
                            AssistantContent::Reasoning(ReasoningContent {
                                text: String::new(),
                                redacted: true,
                                portability: ReasoningPortability::ProviderBound,
                                continuation_ref: Some(continuation_ref),
                            }),
                        );
                        self.queue.push_back(Ok(StreamEvent::ReasoningEnd));
                    }
                    "tool_use" => {
                        let id = block
                            .id
                            .ok_or_else(|| protocol("Anthropic tool has no id"))?;
                        let name = block
                            .name
                            .ok_or_else(|| protocol("Anthropic tool has no name"))?;
                        let arguments: Value = serde_json::from_str(&block.arguments)
                            .map_err(|_| protocol("malformed Anthropic tool arguments"))?;
                        self.completed_content.insert(
                            index,
                            AssistantContent::ToolCall(ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                arguments: arguments.clone(),
                            }),
                        );
                        self.queue.push_back(Ok(StreamEvent::ToolCallEnd {
                            index,
                            tool_call: ToolCall {
                                id,
                                name,
                                arguments,
                            },
                        }));
                    }
                    other => return Err(protocol(format!("unsupported Anthropic block {other}"))),
                }
            }
            "message_delta" => {
                if let Some(reason) = body.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(parse_stop_reason(Some(reason))?);
                }
                if let Some(usage) = body.get("usage") {
                    let value = usage_from_value(usage);
                    let previous = self.usage.unwrap_or_default();
                    self.usage = Some(Usage {
                        input_tokens: previous.input_tokens,
                        output_tokens: value.output_tokens,
                        total_tokens: previous.input_tokens + value.output_tokens,
                        cache_read_tokens: value.cache_read_tokens.or(previous.cache_read_tokens),
                        cache_write_tokens: value
                            .cache_write_tokens
                            .or(previous.cache_write_tokens),
                        reasoning_tokens: value.reasoning_tokens,
                    });
                }
            }
            "message_stop" => {
                if !self.blocks.is_empty() {
                    return Err(protocol("Anthropic message_stop has open content blocks"));
                }
                let stop_reason = self
                    .stop_reason
                    .clone()
                    .ok_or_else(|| protocol("Anthropic stream has no stop_reason"))?;
                let mut replay = std::mem::take(&mut self.prior_reasoning_replay);
                replay.append(&mut self.reasoning_replay);
                if !replay.is_empty() {
                    let message = AssistantMessage {
                        content: self.completed_content.values().cloned().collect(),
                    };
                    let mut covered_history = self.history.clone();
                    covered_history.push(Message::Assistant(message));
                    let scope =
                        ContinuationScope::for_history(&covered_history).map_err(protocol)?;
                    let continuation = AnthropicMessagesContinuation::with_scope(
                        self.provider.clone(),
                        self.model.clone(),
                        scope,
                        replay,
                    )
                    .map_err(|error| protocol(&error))?;
                    self.queue.push_back(Ok(StreamEvent::Continuation(
                        ProviderContinuation::AnthropicMessages(continuation),
                    )));
                }
                if let Some(usage) = self.usage.take() {
                    self.queue.push_back(Ok(StreamEvent::Usage(usage)));
                }
                self.queue.push_back(Ok(StreamEvent::Done { stop_reason }));
                self.done = true;
            }
            "ping" => {}
            _ => return Err(protocol(format!("unsupported Anthropic SSE event {event}"))),
        }
        Ok(())
    }

    fn add_tool_argument_bytes(&mut self, bytes: usize) -> Result<(), ProviderError> {
        self.tool_arguments_bytes = self
            .tool_arguments_bytes
            .checked_add(bytes)
            .ok_or_else(|| protocol("Anthropic tool arguments exceed the stream limit"))?;
        if self.tool_arguments_bytes > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("Anthropic tool arguments exceed the stream limit"));
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn unsupported(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unsupported,
        FailurePhase::BeforeDispatch,
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
