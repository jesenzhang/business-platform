use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::Stream;
use reqwest::{Client, Response};
use serde_json::{json, Value};

use super::{
    aborted, apply_headers, bounded_error_body, bounded_response_body, client_for_policy, dispatch,
    normalize_base_url, normalize_image, protocol, retry_after_from_headers, stream_error,
    DispatchResult, EndpointPolicy, SseReader, MAX_STREAM_BUFFER_BYTES,
};
use crate::{
    AbortSignal, Api, AssistantContent, AssistantMessage, Completion, CompletionMetadata,
    CompletionRequest, ContinuationRef, ContinuationScope, DataRetentionPolicy, FailurePhase,
    Message, ModelProvider, OpenAiResponsesContinuation, OpenAiResponsesContinuationMode,
    OpenAiResponsesReplayItem, OpenAiResponsesReplaySegment, OutputConstraint,
    ProviderCapabilities, ProviderContinuation, ProviderError, ProviderErrorKind, ProviderId,
    ProviderStream, ReasoningContent, ReasoningPortability, RequestOptions, StopReason,
    StreamEvent, TextContent, ToolCall, ToolChoice, ToolConstraint, ToolResultContent, Usage,
    UserContent,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// OpenAI Responses API transport. Its wire model is intentionally separate
/// from the Chat Completions-compatible transport.
pub struct OpenAiResponsesProvider {
    provider_id: ProviderId,
    api_key: String,
    base_url: reqwest::Url,
    client: Client,
    request_timeout: Duration,
}

impl OpenAiResponsesProvider {
    pub fn new(provider_id: ProviderId, api_key: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            provider_id,
            api_key: api_key.into(),
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

    fn request(&self, body: Value) -> Result<reqwest::RequestBuilder, ProviderError> {
        let endpoint = self
            .base_url
            .join("responses")
            .map_err(|_| invalid("invalid Responses API endpoint"))?;
        let body = serde_json::to_vec(&body)
            .map_err(|_| serialization("Responses request serialization failed"))?;
        let mut request = self
            .client
            .post(endpoint)
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
        body: Value,
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

    async fn status_error(&self, response: Response) -> ProviderError {
        let retry_after = retry_after_from_headers(response.headers());
        let status = response.status();
        let body = match bounded_error_body(response).await {
            Ok(body) => body,
            Err(_) => {
                return ProviderError::new(
                    ProviderErrorKind::StreamInterrupted,
                    FailurePhase::DuringStream,
                    "Responses provider error body interrupted",
                )
                .with_status(status.as_u16())
            }
        };
        let message = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("HTTP {status} Responses provider error"));
        let kind = match status.as_u16() {
            401 | 403 => ProviderErrorKind::Authentication,
            408 => ProviderErrorKind::Timeout,
            429 => ProviderErrorKind::RateLimit,
            400..=499 => ProviderErrorKind::InvalidRequest,
            500..=599 => ProviderErrorKind::Unavailable,
            _ => ProviderErrorKind::Other,
        };
        let mut error = ProviderError::new(
            kind,
            FailurePhase::AfterDispatch,
            ProviderError::redacted_message(message, &self.api_key),
        )
        .with_status(status.as_u16());
        if let Some(retry_after) = retry_after {
            error = error.with_retry_after(retry_after);
        }
        error
    }

    fn transport_error(&self, error: reqwest::Error) -> ProviderError {
        let kind = if error.is_timeout() {
            ProviderErrorKind::Timeout
        } else if error.is_connect() {
            ProviderErrorKind::Unavailable
        } else {
            ProviderErrorKind::StreamInterrupted
        };
        ProviderError::new(
            kind,
            FailurePhase::Unknown,
            ProviderError::redacted_message(error.to_string(), &self.api_key),
        )
    }
}

#[async_trait]
impl ModelProvider for OpenAiResponsesProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        static API: Api = Api::OpenAiResponses;
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
            .dispatch_request(responses_request(request, false)?, &options)
            .await?;
        if !response.status().is_success() {
            return Err(self.status_error(response).await);
        }
        let body = bounded_response_body(response).await.map_err(|()| {
            stream_error("Responses response body interrupted or exceeded the limit")
        })?;
        let body: Value = serde_json::from_slice(&body)
            .map_err(|_| response_protocol("invalid OpenAI Responses response JSON"))?;
        completion_from_response(
            body,
            started.elapsed(),
            &self.api_key,
            &self.provider_id,
            &request.model.id,
            request.retention,
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
            .dispatch_request(responses_request(request, true)?, &options)
            .await?;
        if !response.status().is_success() {
            return Err(self.status_error(response).await);
        }
        let state = ResponsesStream::new(
            request.model.id.clone(),
            self.provider_id.clone(),
            request.retention,
            request.messages.clone(),
            request
                .continuation
                .as_ref()
                .and_then(ProviderContinuation::openai_responses)
                .filter(|continuation| {
                    continuation.mode() == OpenAiResponsesContinuationMode::Stateless
                })
                .map(|continuation| continuation.replay_segments().to_vec())
                .unwrap_or_default(),
            response.bytes_stream(),
            self.api_key.clone(),
            options.abort,
        );
        Ok(Box::pin(futures::stream::unfold(
            state,
            |mut state| async move { state.next_event().await.map(|event| (event, state)) },
        )))
    }
}

fn responses_request(request: &CompletionRequest, stream: bool) -> Result<Value, ProviderError> {
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    let continuation = request
        .continuation
        .as_ref()
        .map(|continuation| {
            continuation.openai_responses().ok_or_else(|| {
                unsupported("continuation protocol is not supported by OpenAI Responses")
            })
        })
        .transpose()?;
    // Stateful mode means the provider retains the covered conversation
    // prefix, so only the validated uncovered suffix is encoded. Stateless
    // mode means the provider retains nothing: every required historical
    // user/tool input is encoded in place, and anchored assistant outputs are
    // substituted with their provider-native replay segments. Stateless replay
    // must never apply server-coverage prefix elision.
    let covered_boundary = match continuation {
        Some(value) if value.mode() == OpenAiResponsesContinuationMode::Stateful => Some(
            value
                .scope()
                .validate_history(&request.messages)
                .map_err(invalid)?,
        ),
        _ => None,
    };
    // Non-system message index (system messages are request-level
    // instructions and sit outside the coverage boundary).
    let mut conversation_index = 0usize;
    for (message_index, message) in request.messages.iter().enumerate() {
        if let Message::System { content } = message {
            instructions.push(content.clone());
            continue;
        }
        let is_covered = covered_boundary.is_some_and(|covered| conversation_index < covered);
        conversation_index += 1;
        if is_covered {
            continue;
        }
        match message {
            Message::System { .. } => unreachable!("system messages were handled above"),
            Message::User { content } => input.push(json!({
                "role": "user",
                "content": user_content(content)?,
            })),
            Message::Assistant(message) => {
                let items = continuation
                    .and_then(|value| {
                        value
                            .replay_segment_for_message(&request.messages, message_index)
                            .transpose()
                    })
                    .transpose()
                    .map_err(invalid)?;
                if let Some(items) = items {
                    input.extend(items.iter().map(responses_replay_item));
                } else {
                    input.extend(assistant_input(message)?);
                }
            }
            Message::ToolResult {
                tool_call_id,
                content,
                ..
            } => input.push(json!({
                "type": "function_call_output",
                "call_id": tool_call_id,
                "output": tool_result_text(content)?,
            })),
        }
    }
    let mut body = json!({
        "model": request.model.id,
        "input": input,
        "stream": stream,
    });
    if !instructions.is_empty() {
        body["instructions"] = Value::String(instructions.join("\n\n"));
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    let mut value = json!({
                        "type": "function",
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.input_schema,
                    });
                    if matches!(&tool.constraint, Some(ToolConstraint::StrictJsonSchema)) {
                        value["strict"] = json!(true);
                    }
                    value
                })
                .collect(),
        );
        body["parallel_tool_calls"] = json!(true);
    }
    if let Some(constraint) = &request.output_constraint {
        body["text"] = responses_output_constraint(constraint)?;
    }
    if let Some(value) = request.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = request.top_p {
        body["top_p"] = json!(value);
    }
    if let Some(value) = request.max_output_tokens {
        body["max_output_tokens"] = json!(value);
    }
    if let Some(choice) = &request.tool_choice {
        body["tool_choice"] = responses_tool_choice(choice);
    }
    if let Some(reasoning) = &request.reasoning {
        if reasoning.enabled {
            let mut value = serde_json::Map::new();
            if let Some(effort) = &reasoning.effort {
                value.insert("effort".into(), json!(effort));
            } else {
                value.insert("effort".into(), json!("medium"));
            }
            if let Some(summary) = &reasoning.summary {
                value.insert("summary".into(), json!(summary));
            }
            body["reasoning"] = Value::Object(value);
        }
    }
    if request.retention == DataRetentionPolicy::Ephemeral {
        body["store"] = json!(false);
        body["include"] = json!(["reasoning.encrypted_content"]);
    }
    apply_continuation(&mut body, request.continuation.as_ref())?;
    Ok(body)
}

fn apply_continuation(
    body: &mut Value,
    continuation: Option<&ProviderContinuation>,
) -> Result<(), ProviderError> {
    let Some(continuation) = continuation else {
        return Ok(());
    };
    let Some(continuation) = continuation.openai_responses() else {
        return Err(unsupported(
            "continuation protocol is not supported by OpenAI Responses",
        ));
    };
    if continuation.mode() == OpenAiResponsesContinuationMode::Stateful {
        let response_id = continuation.previous_response_id().ok_or_else(|| {
            unsupported("stateful Responses continuation has no previous response id")
        })?;
        body["previous_response_id"] = json!(response_id);
    }
    // Stateless replay items are already substituted at their anchored history
    // positions during conversation encoding; nothing is prepended here.
    Ok(())
}

fn responses_replay_item(item: &OpenAiResponsesReplayItem) -> Value {
    match item {
        OpenAiResponsesReplayItem::Reasoning {
            item_id,
            encrypted_content,
            summary,
            ..
        } => {
            let mut value = serde_json::Map::new();
            value.insert("type".into(), json!("reasoning"));
            if let Some(item_id) = item_id {
                value.insert("id".into(), json!(item_id));
            }
            if let Some(encrypted_content) = encrypted_content {
                value.insert("encrypted_content".into(), json!(encrypted_content));
            }
            if !summary.is_empty() {
                value.insert(
                    "summary".into(),
                    Value::Array(
                        summary
                            .iter()
                            .map(|text| json!({"type": "summary_text", "text": text}))
                            .collect(),
                    ),
                );
            }
            Value::Object(value)
        }
        OpenAiResponsesReplayItem::AssistantMessage {
            item_id,
            phase,
            text,
            ..
        } => {
            let mut value = json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": text}],
            });
            if let Some(item_id) = item_id {
                value["id"] = json!(item_id);
            }
            if let Some(phase) = phase {
                value["phase"] = json!(phase);
            }
            value
        }
        OpenAiResponsesReplayItem::FunctionCall {
            item_id,
            call_id,
            name,
            arguments,
            ..
        } => {
            let mut value = json!({
                "type": "function_call",
                "call_id": call_id,
                "name": name,
                "arguments": arguments,
            });
            if let Some(item_id) = item_id {
                value["id"] = json!(item_id);
            }
            value
        }
    }
}

fn responses_output_constraint(constraint: &OutputConstraint) -> Result<Value, ProviderError> {
    match constraint {
        OutputConstraint::JsonSchema {
            name,
            schema,
            strict,
        } => Ok(json!({
            "format": {
                "type": "json_schema",
                "name": name,
                "strict": strict,
                "schema": schema,
            }
        })),
        OutputConstraint::Grammar { .. } => Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            FailurePhase::BeforeDispatch,
            "grammar structured output is not supported by OpenAI Responses",
        )),
    }
}

fn responses_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Tool { name } => json!({"type": "function", "name": name}),
    }
}

fn user_content(content: &[UserContent]) -> Result<Vec<Value>, ProviderError> {
    content
        .iter()
        .map(|part| match part {
            UserContent::Text(text) => Ok(json!({"type": "input_text", "text": text.text})),
            UserContent::Image(image) => {
                let (media_type, data) = normalize_image(image)?;
                Ok(json!({
                    "type": "input_image",
                    "image_url": format!("data:{media_type};base64,{data}"),
                }))
            }
        })
        .collect()
}

fn assistant_input(message: &AssistantMessage) -> Result<Vec<Value>, ProviderError> {
    let mut result = Vec::new();
    for part in &message.content {
        match part {
            AssistantContent::Text(value) => result.push(json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": value.text}],
            })),
            AssistantContent::Reasoning(reasoning) => {
                if reasoning.portability == ReasoningPortability::ProviderBound {
                    // The continuation sidecar is applied once at the request
                    // boundary. Re-emitting the normalized marker would either
                    // duplicate encrypted state or leak provider detail.
                    continue;
                }
                if reasoning.redacted {
                    return Err(unsupported(
                        "redacted reasoning has no provider continuation",
                    ));
                }
                if !reasoning.text.is_empty() {
                    result.push(json!({
                        "type": "reasoning",
                        "summary": [{"type": "summary_text", "text": reasoning.text}],
                    }));
                }
            }
            AssistantContent::ToolCall(call) => {
                result.push(json!({
                    "type": "function_call",
                    "call_id": call.id,
                    "name": call.name,
                    "arguments": serde_json::to_string(&call.arguments)
                        .map_err(|_| serialization("tool arguments are not serializable"))?,
                }));
            }
        }
    }
    Ok(result)
}

fn tool_result_text(content: &[ToolResultContent]) -> Result<String, ProviderError> {
    let mut text = String::new();
    for part in content {
        match part {
            ToolResultContent::Text(value) => text.push_str(&value.text),
            ToolResultContent::Image(_) => {
                return Err(unsupported(
                    "Responses tool result image is not implemented",
                ))
            }
        }
    }
    Ok(text)
}

#[allow(clippy::too_many_arguments)]
fn completion_from_response(
    body: Value,
    elapsed: Duration,
    api_key: &str,
    provider: &ProviderId,
    model: &str,
    retention: DataRetentionPolicy,
    history: &[Message],
    previous_continuation: Option<&ProviderContinuation>,
) -> Result<Completion, ProviderError> {
    if body.get("error").is_some_and(|error| !error.is_null()) {
        return Err(response_protocol_redacted(
            response_error_message(&body, "Responses response returned an error"),
            api_key,
        ));
    }
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| response_protocol("Responses response has no status"))?;
    if status == "failed" {
        return Err(response_protocol_redacted(
            response_error_message(&body, "Responses response failed"),
            api_key,
        ));
    }
    let output = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| response_protocol("Responses response has no output"))?;
    let mut content = Vec::new();
    let mut replay_items = Vec::new();
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                if let Some(replay) = parse_message_output(item, &mut content)? {
                    replay_items.push(replay);
                }
            }
            Some("reasoning") => {
                if let Some(replay) = parse_reasoning_output(item, &mut content)? {
                    replay_items.push(replay);
                }
            }
            Some("function_call") => {
                let (call, replay) = parse_tool_call(item)?;
                content.push(AssistantContent::ToolCall(call));
                replay_items.push(replay);
            }
            Some(other) => {
                return Err(unsupported_after_dispatch(format!(
                    "unsupported Responses output item {other}"
                )))
            }
            None => return Err(response_protocol("Responses output item has no type")),
        }
    }
    let message = AssistantMessage { content };
    // The replay items collected above reproduce exactly this normalized
    // assistant message, so the new segment anchors to its sequence-sensitive
    // identity within the full request history.
    let current_anchor =
        crate::assistant_history_anchor(history, &message).map_err(response_protocol)?;
    let has_tools = !message.tool_calls().is_empty();
    let stop_reason = match status {
        "completed" if has_tools => StopReason::ToolUse,
        "completed" => StopReason::Stop,
        "incomplete" => incomplete_stop_reason(&body),
        other => {
            return Err(unsupported_after_dispatch(format!(
                "unsupported Responses response status {other}"
            )))
        }
    };
    let usage = body.get("usage").map(usage_from_value);
    if let Some(usage) = usage {
        usage
            .validate()
            .map_err(|error| response_protocol(error.to_string()))?;
    }
    let response_id = body
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    // Carry forward the earlier still-required stateless segments, then add
    // exactly one segment for the assistant output this response produced.
    let mut segments = previous_continuation
        .and_then(ProviderContinuation::openai_responses)
        .filter(|continuation| continuation.mode() == OpenAiResponsesContinuationMode::Stateless)
        .map(|continuation| continuation.replay_segments().to_vec())
        .unwrap_or_default();
    if !replay_items.is_empty() {
        segments.push(OpenAiResponsesReplaySegment::new(
            current_anchor,
            std::mem::take(&mut replay_items),
        ));
    }
    // Stateful mode records the server-retained coverage through the new
    // assistant output; stateless mode ignores it.
    let mut covered_history = history.to_vec();
    covered_history.push(Message::Assistant(message.clone()));
    let covered_scope =
        ContinuationScope::for_history(&covered_history).map_err(response_protocol)?;
    let continuation = responses_continuation(
        provider,
        model,
        response_id,
        segments,
        retention,
        covered_scope,
    )?;
    Ok(Completion {
        metadata: metadata(&message, stop_reason.clone(), true, elapsed),
        message,
        usage,
        continuation,
        stop_reason,
    })
}

fn parse_message_output(
    item: &Value,
    content: &mut Vec<AssistantContent>,
) -> Result<Option<OpenAiResponsesReplayItem>, ProviderError> {
    let parts = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| response_protocol("Responses message has no content"))?;
    let mut text = String::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("output_text") => {
                if let Some(annotations) = part.get("annotations") {
                    let annotations = annotations.as_array().ok_or_else(|| {
                        response_protocol("Responses output_text annotations are invalid")
                    })?;
                    if !annotations.is_empty() {
                        return Err(unsupported_after_dispatch(
                            "Responses output annotations are not normalized",
                        ));
                    }
                }
                let value = part
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| response_protocol("Responses output_text has no text"))?;
                text.push_str(value);
            }
            Some("refusal") => {
                return Err(unsupported_after_dispatch(
                    "Responses refusal output is not normalized",
                ))
            }
            Some(other) => {
                return Err(unsupported_after_dispatch(format!(
                    "unsupported Responses message content {other}"
                )))
            }
            None => return Err(response_protocol("Responses message content has no type")),
        }
    }
    if !text.is_empty() {
        content.push(AssistantContent::Text(TextContent::new(text.clone())));
    }
    let reference = ContinuationRef::generated();
    Ok(Some(OpenAiResponsesReplayItem::assistant_message(
        reference,
        item.get("id").and_then(Value::as_str).map(str::to_owned),
        item.get("phase").and_then(Value::as_str).map(str::to_owned),
        text,
    )))
}

fn parse_reasoning_output(
    item: &Value,
    content: &mut Vec<AssistantContent>,
) -> Result<Option<OpenAiResponsesReplayItem>, ProviderError> {
    let summary = item
        .get("summary")
        .and_then(Value::as_array)
        .map_or(&[][..], |value| value.as_slice());
    let encrypted = item
        .get("encrypted_content")
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    response_protocol("Responses reasoning encrypted content is invalid")
                })
        })
        .transpose()?;
    let mut summary_text = String::new();
    for part in summary {
        if part.get("type").and_then(Value::as_str) != Some("summary_text") {
            return Err(unsupported_after_dispatch(
                "unsupported Responses reasoning summary",
            ));
        }
        summary_text.push_str(
            part.get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| response_protocol("Responses reasoning summary has no text"))?,
        );
    }
    let had_encrypted = encrypted.is_some();
    let reference = had_encrypted.then(ContinuationRef::generated);
    if had_encrypted || !summary_text.is_empty() {
        content.push(AssistantContent::Reasoning(ReasoningContent {
            text: summary_text.clone(),
            redacted: had_encrypted,
            portability: if had_encrypted {
                ReasoningPortability::ProviderBound
            } else {
                ReasoningPortability::Portable
            },
            continuation_ref: reference.clone(),
        }));
    }
    // Every reasoning output item enters the replay segment. Encrypted items
    // carry their provider-native payload and reference; portable summaries
    // replay as summary-only items so stateless manual replay reproduces the
    // complete ordered provider output.
    let item_id = item.get("id").and_then(Value::as_str).map(str::to_owned);
    let summary_texts = summary
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    Ok(match encrypted {
        Some(encrypted_content) => Some(OpenAiResponsesReplayItem::reasoning(
            reference.expect("encrypted Responses reasoning has a reference"),
            item_id,
            encrypted_content,
            summary_texts,
        )),
        None => (!summary_text.is_empty())
            .then(|| OpenAiResponsesReplayItem::portable_reasoning(summary_texts, item_id)),
    })
}

fn parse_tool_call(item: &Value) -> Result<(ToolCall, OpenAiResponsesReplayItem), ProviderError> {
    let id = item
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or_else(|| response_protocol("Responses function call has no call_id"))?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| response_protocol("Responses function call has no name"))?;
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .ok_or_else(|| response_protocol("Responses function call has no arguments"))?;
    let call = ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: serde_json::from_str(arguments)
            .map_err(|_| response_protocol("invalid Responses function-call arguments"))?,
    };
    Ok((
        call,
        OpenAiResponsesReplayItem::function_call(
            ContinuationRef::generated(),
            item.get("id").and_then(Value::as_str).map(str::to_owned),
            id,
            name,
            arguments,
        ),
    ))
}

fn responses_continuation(
    provider: &ProviderId,
    model: &str,
    response_id: Option<String>,
    replay_segments: Vec<OpenAiResponsesReplaySegment>,
    retention: DataRetentionPolicy,
    covered_scope: ContinuationScope,
) -> Result<Option<ProviderContinuation>, ProviderError> {
    let mode = if retention != DataRetentionPolicy::Ephemeral {
        OpenAiResponsesContinuationMode::Stateful
    } else {
        OpenAiResponsesContinuationMode::Stateless
    };
    if (mode == OpenAiResponsesContinuationMode::Stateful && response_id.is_none())
        || (mode == OpenAiResponsesContinuationMode::Stateless && replay_segments.is_empty())
    {
        return Ok(None);
    }
    // Stateful mode keeps the server-retained coverage boundary so the next
    // request can trim only that verified prefix. Stateless mode claims no
    // server coverage: full history is re-sent with anchored substitution.
    let scope = match mode {
        OpenAiResponsesContinuationMode::Stateful => covered_scope,
        OpenAiResponsesContinuationMode::Stateless => ContinuationScope::empty(),
    };
    Ok(Some(ProviderContinuation::OpenAiResponses(
        OpenAiResponsesContinuation::with_segments(
            provider.clone(),
            model,
            if mode == OpenAiResponsesContinuationMode::Stateful {
                response_id
            } else {
                None
            },
            mode,
            scope,
            if mode == OpenAiResponsesContinuationMode::Stateful {
                Vec::new()
            } else {
                replay_segments
            },
        )
        .map_err(response_protocol)?,
    )))
}

fn usage_from_value(value: &Value) -> Usage {
    let input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let output_tokens = value
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        cache_read_tokens: value
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64)
            .filter(|value| *value > 0),
        cache_write_tokens: value
            .get("input_tokens_details")
            .and_then(|details| details.get("cache_write_tokens"))
            .and_then(Value::as_u64)
            .or_else(|| value.get("cache_write_tokens").and_then(Value::as_u64)),
        reasoning_tokens: value
            .get("output_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_u64)
            .filter(|value| *value > 0),
    }
}

fn incomplete_stop_reason(response: &Value) -> StopReason {
    match response
        .pointer("/incomplete_details/reason")
        .and_then(Value::as_str)
    {
        Some("content_filter") => StopReason::ContentFilter,
        Some(reason) => StopReason::Other(reason.into()),
        None => StopReason::Length,
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

struct PartialTool {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReasoningSnapshot {
    text: String,
    encrypted_content: Option<String>,
}

struct ResponsesStream {
    reader: SseReader,
    abort: Option<AbortSignal>,
    api_key: String,
    provider: ProviderId,
    model: String,
    retention: DataRetentionPolicy,
    history: Vec<Message>,
    prior_replay_segments: Vec<OpenAiResponsesReplaySegment>,
    queue: VecDeque<Result<StreamEvent, ProviderError>>,
    tools: BTreeMap<usize, PartialTool>,
    tool_arguments_bytes: usize,
    item_kinds: BTreeMap<usize, String>,
    open_content_parts: HashSet<(usize, usize)>,
    completed_item_indexes: HashSet<usize>,
    item_indexes: BTreeMap<String, usize>,
    text_open: bool,
    text_buffer: String,
    last_text: Option<String>,
    text_history: Vec<String>,
    reasoning_open: bool,
    reasoning_item_index: Option<usize>,
    reasoning_buffer: String,
    reasoning_encrypted_content: Option<String>,
    reasoning_history: Vec<ReasoningSnapshot>,
    tool_history: Vec<ToolCall>,
    item_references: BTreeMap<usize, ContinuationRef>,
    completed_content: BTreeMap<usize, AssistantContent>,
    saw_tool_call: bool,
    terminal_seen: bool,
    done: bool,
}

impl ResponsesStream {
    #[allow(clippy::too_many_arguments)]
    fn new<S>(
        model: String,
        provider: ProviderId,
        retention: DataRetentionPolicy,
        history: Vec<Message>,
        prior_replay_segments: Vec<OpenAiResponsesReplaySegment>,
        body: S,
        api_key: String,
        abort: Option<AbortSignal>,
    ) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    {
        let mut queue = VecDeque::new();
        queue.push_back(Ok(StreamEvent::Start {
            model: model.clone(),
        }));
        Self {
            reader: SseReader::new(body),
            abort,
            api_key,
            provider,
            model,
            retention,
            history,
            prior_replay_segments,
            queue,
            tools: BTreeMap::new(),
            tool_arguments_bytes: 0,
            item_kinds: BTreeMap::new(),
            open_content_parts: HashSet::new(),
            completed_item_indexes: HashSet::new(),
            item_indexes: BTreeMap::new(),
            text_open: false,
            text_buffer: String::new(),
            last_text: None,
            text_history: Vec::new(),
            reasoning_open: false,
            reasoning_item_index: None,
            reasoning_buffer: String::new(),
            reasoning_encrypted_content: None,
            reasoning_history: Vec::new(),
            tool_history: Vec::new(),
            item_references: BTreeMap::new(),
            completed_content: BTreeMap::new(),
            saw_tool_call: false,
            terminal_seen: false,
            done: false,
        }
    }

    async fn next_record(&mut self) -> Result<Option<super::SseRecord>, ProviderError> {
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
                    if self.terminal_seen {
                        return None;
                    }
                    return Some(Err(stream_error(
                        "Responses SSE ended before response.completed",
                    )));
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            };
            if self.terminal_seen {
                self.done = true;
                return Some(Err(protocol(
                    "Responses SSE emitted data after response.completed",
                )));
            }
            let result = self.ingest(record.event.as_deref().unwrap_or_default(), &record.data);
            if let Err(error) = result {
                self.done = true;
                self.queue.push_back(Err(error));
            }
        }
    }

    fn ingest(&mut self, event: &str, data: &str) -> Result<(), ProviderError> {
        let body: Value = serde_json::from_str(data)
            .map_err(|_| protocol("invalid OpenAI Responses SSE JSON"))?;
        let kind = body.get("type").and_then(Value::as_str).unwrap_or(event);
        match kind {
            "response.created"
            | "response.in_progress"
            | "response.queued"
            | "response.content_part.added"
            | "response.content_part.done"
            | "response.reasoning_summary_part.added" => {
                self.ingest_lifecycle(kind, &body)?;
            }
            "response.output_text.annotation.added" => {
                return Err(stream_unsupported(
                    "Responses output annotations are not normalized",
                ))
            }
            "response.output_item.added" => self.output_item_added(&body)?,
            "response.output_item.done" => self.output_item_done(&body)?,
            "response.output_text.delta" => {
                self.require_open_output_text(&body, "Responses output_text.delta")?;
                if !self.text_open {
                    self.text_open = true;
                    self.last_text = None;
                    self.queue.push_back(Ok(StreamEvent::TextStart));
                }
                let text = body
                    .get("delta")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses text delta has no delta"))?;
                self.text_buffer.push_str(text);
                if self.text_buffer.len() > MAX_STREAM_BUFFER_BYTES {
                    return Err(protocol("Responses text exceeds the stream limit"));
                }
                self.queue
                    .push_back(Ok(StreamEvent::TextDelta { text: text.into() }));
            }
            "response.output_text.done" => {
                self.require_open_output_text(&body, "Responses output_text.done")?;
                let text = body
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses output_text.done has no text"))?;
                self.finish_text(Some(text))?;
            }
            "response.reasoning_summary_text.delta" => {
                self.reasoning_delta(&body)?;
            }
            "response.reasoning_text.delta" => self.reasoning_delta(&body)?,
            "response.reasoning_summary_text.done" | "response.reasoning_text.done" => {
                let text = body
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses reasoning done has no text"))?;
                self.validate_reasoning_text(text)?;
            }
            "response.reasoning_summary_part.done" => {}
            "response.function_call_arguments.delta" => self.function_arguments_delta(&body)?,
            "response.function_call_arguments.done" => self.function_arguments_done(&body)?,
            "response.completed" => self.response_completed(&body, "completed")?,
            "response.incomplete" => self.response_completed(&body, "incomplete")?,
            "response.failed" | "error" => {
                return Err(stream_protocol_redacted(
                    response_error_message(&body, "Responses response failed"),
                    &self.api_key,
                ))
            }
            other => {
                return Err(stream_unsupported(format!(
                    "unsupported Responses stream event {other}"
                )))
            }
        }
        Ok(())
    }

    fn ingest_lifecycle(&mut self, kind: &str, body: &Value) -> Result<(), ProviderError> {
        if matches!(
            kind,
            "response.content_part.added" | "response.content_part.done"
        ) {
            let output_index = self.message_output_index(body, "Responses content part event")?;
            let content_index = body
                .get("content_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .ok_or_else(|| protocol("Responses content part event has no content_index"))?;
            let part = body
                .get("part")
                .ok_or_else(|| protocol("Responses content part event has no part"))?;
            let key = (output_index, content_index);
            if kind == "response.content_part.added" {
                if !self.open_content_parts.insert(key) {
                    return Err(protocol("duplicate Responses content part"));
                }
            } else if !self.open_content_parts.remove(&key) {
                return Err(protocol(
                    "Responses content part done before content part start",
                ));
            }
            match part.get("type").and_then(Value::as_str) {
                Some("output_text") => {
                    if let Some(annotations) = part.get("annotations") {
                        let annotations = annotations.as_array().ok_or_else(|| {
                            protocol("Responses output_text annotations are invalid")
                        })?;
                        if !annotations.is_empty() {
                            return Err(stream_unsupported(
                                "Responses output annotations are not normalized",
                            ));
                        }
                    }
                    let text = part
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or_else(|| protocol("Responses output_text part has no text"))?;
                    if kind == "response.content_part.done" {
                        self.finish_text(Some(text))?;
                    }
                }
                Some("refusal") => {
                    return Err(stream_unsupported(
                        "Responses refusal output is not normalized",
                    ))
                }
                Some(other) => {
                    return Err(stream_unsupported(format!(
                        "unsupported Responses message content {other}"
                    )))
                }
                None => return Err(protocol("Responses content part has no type")),
            }
        }
        Ok(())
    }

    fn output_item_added(&mut self, body: &Value) -> Result<(), ProviderError> {
        let index = output_index(body)?;
        let item = body
            .get("item")
            .ok_or_else(|| protocol("Responses output item added has no item"))?;
        let kind = item
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("Responses output item has no type"))?;
        if self.completed_item_indexes.contains(&index)
            || self.item_kinds.insert(index, kind.to_owned()).is_some()
        {
            return Err(protocol("duplicate Responses output item"));
        }
        match kind {
            "message" => Ok(()),
            "reasoning" => {
                if self.reasoning_open {
                    return Err(protocol("duplicate open Responses reasoning item"));
                }
                self.reasoning_open = true;
                self.reasoning_item_index = Some(index);
                self.reasoning_encrypted_content = None;
                self.queue.push_back(Ok(StreamEvent::ReasoningStart));
                Ok(())
            }
            "function_call" => {
                self.saw_tool_call = true;
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses function call has no call_id"))?;
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses function call has no name"))?;
                let initial_arguments = item
                    .get("arguments")
                    .map_or(Ok(""), |value| {
                        value.as_str().ok_or_else(|| {
                            protocol("Responses function call arguments are not text")
                        })
                    })?
                    .to_owned();
                if initial_arguments.len() > MAX_STREAM_BUFFER_BYTES {
                    return Err(protocol("Responses tool arguments exceed the stream limit"));
                }
                self.add_tool_argument_bytes(initial_arguments.len())?;
                if self
                    .tools
                    .insert(
                        index,
                        PartialTool {
                            id: call_id.into(),
                            name: name.into(),
                            arguments: initial_arguments.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(protocol("duplicate Responses function call"));
                }
                if let Some(item_id) = item.get("id").and_then(Value::as_str) {
                    self.item_indexes.insert(item_id.into(), index);
                }
                self.queue.push_back(Ok(StreamEvent::ToolCallStart {
                    index,
                    id: call_id.into(),
                    name: name.into(),
                }));
                if !initial_arguments.is_empty() {
                    self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
                        index,
                        arguments_delta: initial_arguments,
                    }));
                }
                Ok(())
            }
            other => Err(stream_unsupported(format!(
                "unsupported Responses output item {other}"
            ))),
        }
    }

    fn output_item_done(&mut self, body: &Value) -> Result<(), ProviderError> {
        let index = output_index(body)?;
        if self
            .open_content_parts
            .iter()
            .any(|(output_index, _)| *output_index == index)
        {
            return Err(protocol(
                "Responses output item done with an open content part",
            ));
        }
        let item = body
            .get("item")
            .ok_or_else(|| protocol("Responses output item done has no item"))?;
        let kind = item
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("Responses output item has no type"))?;
        let expected_kind = self
            .item_kinds
            .remove(&index)
            .ok_or_else(|| protocol("Responses output item done before output item start"))?;
        if expected_kind != kind {
            return Err(protocol("Responses output item type disagrees with start"));
        }
        self.completed_item_indexes.insert(index);
        match kind {
            "message" => {
                let text = stream_message_text(item)?;
                self.finish_text(Some(&text))?;
                if !text.is_empty() {
                    self.completed_content
                        .insert(index, AssistantContent::Text(TextContent::new(text)));
                }
                Ok(())
            }
            "reasoning" => {
                let snapshot = stream_reasoning_snapshot(item)?;
                if !self.reasoning_open {
                    self.reasoning_open = true;
                    self.reasoning_item_index = Some(index);
                    self.reasoning_encrypted_content = None;
                    self.queue.push_back(Ok(StreamEvent::ReasoningStart));
                    if !snapshot.text.is_empty() {
                        self.reasoning_buffer.push_str(&snapshot.text);
                        self.queue.push_back(Ok(StreamEvent::ReasoningDelta {
                            text: snapshot.text.clone(),
                        }));
                    }
                }
                if let Some(data) = &snapshot.encrypted_content {
                    self.ensure_reasoning_reference(index);
                    self.reasoning_encrypted_content = Some(data.clone());
                    // The encrypted payload belongs to the continuation
                    // sidecar, not to normalized Message content. Preserve
                    // only the redacted marker in the shared accumulator.
                    self.queue
                        .push_back(Ok(StreamEvent::ReasoningRedacted { data: None }));
                }
                self.finish_reasoning(Some(&snapshot.text))?;
                let continuation_ref = snapshot
                    .encrypted_content
                    .as_ref()
                    .and_then(|_| self.item_references.get(&index).cloned());
                self.completed_content.insert(
                    index,
                    AssistantContent::Reasoning(ReasoningContent {
                        text: snapshot.text,
                        redacted: snapshot.encrypted_content.is_some(),
                        portability: if snapshot.encrypted_content.is_some() {
                            ReasoningPortability::ProviderBound
                        } else {
                            ReasoningPortability::Portable
                        },
                        continuation_ref,
                    }),
                );
                Ok(())
            }
            "function_call" => {
                let expected = stream_tool_call(item)?;
                let tool = self
                    .tools
                    .get(&index)
                    .ok_or_else(|| protocol("Responses function call has no open state"))?;
                if tool.id != expected.id || tool.name != expected.name {
                    return Err(protocol(
                        "Responses function call identity disagrees with start",
                    ));
                }
                let arguments = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses function call has no arguments"))?;
                self.set_complete_arguments(index, arguments)?;
                let tool = self
                    .tools
                    .remove(&index)
                    .ok_or_else(|| protocol("Responses function call ended twice"))?;
                let arguments = serde_json::from_str(&tool.arguments)
                    .map_err(|_| protocol("incomplete Responses function-call arguments"))?;
                let tool_call = ToolCall {
                    id: tool.id,
                    name: tool.name,
                    arguments,
                };
                self.completed_content
                    .insert(index, AssistantContent::ToolCall(tool_call.clone()));
                self.tool_history.push(tool_call.clone());
                self.queue
                    .push_back(Ok(StreamEvent::ToolCallEnd { index, tool_call }));
                Ok(())
            }
            other => Err(stream_unsupported(format!(
                "unsupported Responses output item {other}"
            ))),
        }
    }

    fn function_arguments_delta(&mut self, body: &Value) -> Result<(), ProviderError> {
        let index = self.tool_index(body)?;
        let delta = body
            .get("delta")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("Responses function arguments delta has no delta"))?;
        self.add_tool_argument_bytes(delta.len())?;
        let tool = self
            .tools
            .get_mut(&index)
            .ok_or_else(|| protocol("Responses function arguments delta before call start"))?;
        tool.arguments.push_str(delta);
        if tool.arguments.len() > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("Responses tool arguments exceed the stream limit"));
        }
        self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
            index,
            arguments_delta: delta.into(),
        }));
        Ok(())
    }

    fn function_arguments_done(&mut self, body: &Value) -> Result<(), ProviderError> {
        let index = self.tool_index(body)?;
        if let Some(arguments) = body.get("arguments").and_then(Value::as_str) {
            self.set_complete_arguments(index, arguments)?;
        }
        Ok(())
    }

    fn set_complete_arguments(
        &mut self,
        index: usize,
        arguments: &str,
    ) -> Result<(), ProviderError> {
        let has_no_arguments = self
            .tools
            .get(&index)
            .ok_or_else(|| protocol("Responses function arguments completed before call start"))?
            .arguments
            .is_empty();
        if arguments.len() > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("Responses tool arguments exceed the stream limit"));
        }
        if has_no_arguments {
            self.add_tool_argument_bytes(arguments.len())?;
            let tool = self.tools.get_mut(&index).ok_or_else(|| {
                protocol("Responses function arguments completed before call start")
            })?;
            tool.arguments = arguments.into();
            if !arguments.is_empty() {
                self.queue.push_back(Ok(StreamEvent::ToolCallDelta {
                    index,
                    arguments_delta: arguments.into(),
                }));
            }
        } else if self
            .tools
            .get(&index)
            .is_some_and(|tool| tool.arguments != arguments)
        {
            return Err(protocol(
                "Responses function arguments done disagrees with deltas",
            ));
        }
        Ok(())
    }

    fn add_tool_argument_bytes(&mut self, bytes: usize) -> Result<(), ProviderError> {
        self.tool_arguments_bytes = self
            .tool_arguments_bytes
            .checked_add(bytes)
            .ok_or_else(|| protocol("Responses tool arguments exceed the stream limit"))?;
        if self.tool_arguments_bytes > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("Responses tool arguments exceed the stream limit"));
        }
        Ok(())
    }

    fn tool_index(&self, body: &Value) -> Result<usize, ProviderError> {
        if let Some(index) = body.get("output_index").and_then(Value::as_u64) {
            return Ok(index as usize);
        }
        if let Some(item_id) = body.get("item_id").and_then(Value::as_str) {
            return self
                .item_indexes
                .get(item_id)
                .copied()
                .ok_or_else(|| protocol("Responses function event has unknown item_id"));
        }
        if self.tools.len() == 1 {
            return self
                .tools
                .keys()
                .next()
                .copied()
                .ok_or_else(|| protocol("missing tool index"));
        }
        Err(protocol("Responses function event has no output_index"))
    }

    fn finish_text(&mut self, final_text: Option<&str>) -> Result<(), ProviderError> {
        if !self.text_open {
            if let Some(final_text) = final_text {
                if self.last_text.as_deref() == Some(final_text) {
                    return Ok(());
                }
                if final_text.len() > MAX_STREAM_BUFFER_BYTES {
                    return Err(protocol("Responses text exceeds the stream limit"));
                }
                self.text_open = true;
                self.last_text = None;
                self.queue.push_back(Ok(StreamEvent::TextStart));
                if !final_text.is_empty() {
                    self.text_buffer.push_str(final_text);
                    self.queue.push_back(Ok(StreamEvent::TextDelta {
                        text: final_text.into(),
                    }));
                }
            }
        } else if let Some(final_text) = final_text {
            if final_text.len() > MAX_STREAM_BUFFER_BYTES {
                return Err(protocol("Responses text exceeds the stream limit"));
            }
            if self.text_buffer != final_text {
                return Err(protocol(
                    "Responses output_text.done disagrees with text deltas",
                ));
            }
        }
        if self.text_open {
            self.last_text = Some(self.text_buffer.clone());
            self.text_history.push(self.text_buffer.clone());
            self.text_open = false;
            self.text_buffer.clear();
            self.queue.push_back(Ok(StreamEvent::TextEnd));
        }
        Ok(())
    }

    fn reasoning_delta(&mut self, body: &Value) -> Result<(), ProviderError> {
        if !self.reasoning_open {
            self.reasoning_open = true;
            self.reasoning_item_index = body
                .get("output_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            self.reasoning_encrypted_content = None;
            self.queue.push_back(Ok(StreamEvent::ReasoningStart));
        }
        let text = body
            .get("delta")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("Responses reasoning delta has no delta"))?;
        self.reasoning_buffer.push_str(text);
        if self.reasoning_buffer.len() > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("Responses reasoning exceeds the stream limit"));
        }
        self.queue
            .push_back(Ok(StreamEvent::ReasoningDelta { text: text.into() }));
        Ok(())
    }

    fn validate_reasoning_text(&self, final_text: &str) -> Result<(), ProviderError> {
        if final_text.len() > MAX_STREAM_BUFFER_BYTES {
            return Err(protocol("Responses reasoning exceeds the stream limit"));
        }
        if self.reasoning_buffer != final_text {
            return Err(protocol(
                "Responses reasoning done disagrees with reasoning deltas",
            ));
        }
        Ok(())
    }

    fn finish_reasoning(&mut self, final_text: Option<&str>) -> Result<(), ProviderError> {
        if !self.reasoning_open {
            if let Some(final_text) = final_text {
                if final_text.len() > MAX_STREAM_BUFFER_BYTES {
                    return Err(protocol("Responses reasoning exceeds the stream limit"));
                }
                self.reasoning_open = true;
                self.reasoning_item_index = None;
                self.reasoning_encrypted_content = None;
                self.queue.push_back(Ok(StreamEvent::ReasoningStart));
                if !final_text.is_empty() {
                    self.reasoning_buffer.push_str(final_text);
                    self.queue.push_back(Ok(StreamEvent::ReasoningDelta {
                        text: final_text.into(),
                    }));
                }
            }
        } else if let Some(final_text) = final_text {
            self.validate_reasoning_text(final_text)?;
        }
        if self.reasoning_open {
            let item_index = self.reasoning_item_index.take();
            if self.reasoning_encrypted_content.is_some() {
                if let Some(index) = item_index {
                    self.ensure_reasoning_reference(index);
                }
            }
            self.reasoning_history.push(ReasoningSnapshot {
                text: self.reasoning_buffer.clone(),
                encrypted_content: self.reasoning_encrypted_content.take(),
            });
            self.reasoning_open = false;
            self.reasoning_buffer.clear();
            self.queue.push_back(Ok(StreamEvent::ReasoningEnd));
        }
        Ok(())
    }

    fn ensure_reasoning_reference(&mut self, index: usize) -> ContinuationRef {
        if let Some(reference) = self.item_references.get(&index) {
            return reference.clone();
        }
        let reference = ContinuationRef::generated();
        self.item_references.insert(index, reference.clone());
        self.queue.push_back(Ok(StreamEvent::ReasoningReference {
            reference: reference.clone(),
        }));
        reference
    }

    fn response_completed(
        &mut self,
        body: &Value,
        expected_status: &str,
    ) -> Result<(), ProviderError> {
        if self.terminal_seen {
            return Err(protocol("duplicate Responses response.completed"));
        }
        if self.text_open || self.reasoning_open || !self.tools.is_empty() {
            return Err(protocol("Responses completed with an open content item"));
        }
        if !self.item_kinds.is_empty() {
            return Err(protocol("Responses completed with an open output item"));
        }
        if !self.open_content_parts.is_empty() {
            return Err(protocol("Responses completed with an open content part"));
        }
        let response = body.get("response").unwrap_or(body);
        let status = response
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("Responses completed has no status"))?;
        if status == "failed" {
            return Err(stream_protocol_redacted(
                response_error_message(response, "Responses response failed"),
                &self.api_key,
            ));
        }
        if response.get("error").is_some_and(|error| !error.is_null()) {
            return Err(stream_protocol_redacted(
                response_error_message(response, "Responses response returned an error"),
                &self.api_key,
            ));
        }
        if !matches!(status, "completed" | "incomplete") {
            return Err(stream_unsupported(format!(
                "unsupported Responses terminal status {status}"
            )));
        }
        if status != expected_status {
            return Err(protocol(
                "Responses terminal event disagrees with response status",
            ));
        }
        let output = response
            .get("output")
            .and_then(Value::as_array)
            .ok_or_else(|| protocol("Responses completed has no output"))?;
        let mut terminal_text = String::new();
        let mut terminal_reasoning = Vec::new();
        let mut terminal_tools = Vec::new();
        for item in output {
            match item.get("type").and_then(Value::as_str) {
                Some("message") => terminal_text.push_str(&stream_message_text(item)?),
                Some("reasoning") => terminal_reasoning.push(stream_reasoning_snapshot(item)?),
                Some("function_call") => terminal_tools.push(stream_tool_call(item)?),
                Some(other) => {
                    return Err(stream_unsupported(format!(
                        "unsupported Responses terminal output item {other}"
                    )))
                }
                None => return Err(protocol("Responses terminal output item has no type")),
            }
        }
        let emitted_text: String = self.text_history.concat();
        if emitted_text != terminal_text {
            return Err(protocol(
                "Responses completed text disagrees with stream events",
            ));
        }
        if self.reasoning_history != terminal_reasoning {
            return Err(protocol(
                "Responses completed reasoning disagrees with stream events",
            ));
        }
        if self.tool_history != terminal_tools {
            return Err(protocol(
                "Responses completed tool calls disagree with stream events",
            ));
        }
        let response_id = response
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let new_items = self.replay_items_from_output(output)?;
        let message = AssistantMessage {
            content: self.completed_content.values().cloned().collect(),
        };
        // The segment anchor is computed only now that the final normalized
        // assistant message is known. Interrupted streams never reach this
        // point, so no partial projection can become a valid anchor.
        let current_anchor =
            crate::assistant_history_anchor(&self.history, &message).map_err(protocol)?;
        let mut segments = std::mem::take(&mut self.prior_replay_segments);
        if !new_items.is_empty() {
            segments.push(OpenAiResponsesReplaySegment::new(current_anchor, new_items));
        }
        let mut covered_history = self.history.clone();
        covered_history.push(Message::Assistant(message));
        let covered_scope = ContinuationScope::for_history(&covered_history).map_err(protocol)?;
        if let Some(continuation) = responses_continuation(
            &self.provider,
            &self.model,
            response_id,
            segments,
            self.retention,
            covered_scope,
        )? {
            self.queue
                .push_back(Ok(StreamEvent::Continuation(continuation)));
        }
        if let Some(usage) = response.get("usage") {
            self.queue
                .push_back(Ok(StreamEvent::Usage(usage_from_value(usage))));
        }
        let stop_reason = if status == "incomplete" {
            incomplete_stop_reason(response)
        } else if self.saw_tool_call || !terminal_tools.is_empty() {
            StopReason::ToolUse
        } else if status == "completed" {
            StopReason::Stop
        } else {
            StopReason::Other(status.into())
        };
        self.queue.push_back(Ok(StreamEvent::Done { stop_reason }));
        self.terminal_seen = true;
        Ok(())
    }

    fn replay_items_from_output(
        &self,
        output: &[Value],
    ) -> Result<Vec<OpenAiResponsesReplayItem>, ProviderError> {
        let mut replay_items = Vec::new();
        for (index, item) in output.iter().enumerate() {
            let reference = self
                .item_references
                .get(&index)
                .cloned()
                .unwrap_or_else(ContinuationRef::generated);
            match item.get("type").and_then(Value::as_str) {
                Some("message") => {
                    let text = stream_message_text(item)?;
                    replay_items.push(OpenAiResponsesReplayItem::assistant_message(
                        reference,
                        item.get("id").and_then(Value::as_str).map(str::to_owned),
                        item.get("phase").and_then(Value::as_str).map(str::to_owned),
                        text,
                    ));
                }
                Some("reasoning") => {
                    let snapshot = stream_reasoning_snapshot(item)?;
                    let item_id = item.get("id").and_then(Value::as_str).map(str::to_owned);
                    let summary = item.get("summary").and_then(Value::as_array).map_or_else(
                        Vec::new,
                        |summary| {
                            summary
                                .iter()
                                .filter_map(|part| {
                                    part.get("text").and_then(Value::as_str).map(str::to_owned)
                                })
                                .collect()
                        },
                    );
                    match snapshot.encrypted_content {
                        Some(encrypted_content) => {
                            replay_items.push(OpenAiResponsesReplayItem::reasoning(
                                reference,
                                item_id,
                                encrypted_content,
                                summary,
                            ))
                        }
                        // Portable reasoning stays in the replay segment so
                        // stateless replay preserves the full output shape.
                        None => {
                            if !summary.is_empty() {
                                replay_items.push(OpenAiResponsesReplayItem::portable_reasoning(
                                    summary, item_id,
                                ));
                            }
                        }
                    }
                }
                Some("function_call") => {
                    let call = stream_tool_call(item)?;
                    let arguments = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .ok_or_else(|| protocol("Responses function call has no arguments"))?;
                    replay_items.push(OpenAiResponsesReplayItem::function_call(
                        reference,
                        item.get("id").and_then(Value::as_str).map(str::to_owned),
                        call.id,
                        call.name,
                        arguments,
                    ));
                }
                Some(other) => {
                    return Err(stream_unsupported(format!(
                        "unsupported Responses terminal output item {other}"
                    )))
                }
                None => return Err(protocol("Responses terminal output item has no type")),
            }
        }
        Ok(replay_items)
    }

    fn require_open_output_text(&self, body: &Value, event: &str) -> Result<(), ProviderError> {
        let output_index = self.message_output_index(body, event)?;
        let content_index = body
            .get("content_index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .ok_or_else(|| protocol(format!("{event} has no content_index")))?;
        if !self
            .open_content_parts
            .contains(&(output_index, content_index))
        {
            return Err(protocol(format!("{event} before content part start")));
        }
        Ok(())
    }

    fn message_output_index(&self, body: &Value, event: &str) -> Result<usize, ProviderError> {
        let index = output_index(body)?;
        match self.item_kinds.get(&index).map(String::as_str) {
            Some("message") => Ok(index),
            Some(kind) => Err(protocol(format!(
                "{event} belongs to unsupported output item {kind}"
            ))),
            None => Err(protocol(format!("{event} before output item start"))),
        }
    }
}

fn output_index(body: &Value) -> Result<usize, ProviderError> {
    body.get("output_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| protocol("Responses output event has no output_index"))
}

fn stream_message_text(item: &Value) -> Result<String, ProviderError> {
    let parts = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol("Responses message has no content"))?;
    let mut text = String::new();
    let mut saw_text = false;
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("output_text") => {
                if let Some(annotations) = part.get("annotations") {
                    let annotations = annotations
                        .as_array()
                        .ok_or_else(|| protocol("Responses output_text annotations are invalid"))?;
                    if !annotations.is_empty() {
                        return Err(stream_unsupported(
                            "Responses output annotations are not normalized",
                        ));
                    }
                }
                let value = part
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Responses output_text has no text"))?;
                text.push_str(value);
                saw_text = true;
            }
            Some("refusal") => {
                return Err(stream_unsupported(
                    "Responses refusal output is not normalized",
                ))
            }
            Some(other) => {
                return Err(stream_unsupported(format!(
                    "unsupported Responses message content {other}"
                )))
            }
            None => return Err(protocol("Responses message content has no type")),
        }
    }
    if !saw_text {
        return Err(protocol("Responses message has no output text"));
    }
    Ok(text)
}

fn stream_reasoning_snapshot(item: &Value) -> Result<ReasoningSnapshot, ProviderError> {
    let summary = item
        .get("summary")
        .and_then(Value::as_array)
        .map_or(&[][..], |value| value.as_slice());
    let mut text = String::new();
    for part in summary {
        if part.get("type").and_then(Value::as_str) != Some("summary_text") {
            return Err(stream_unsupported(
                "unsupported Responses reasoning summary",
            ));
        }
        text.push_str(
            part.get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| protocol("Responses reasoning summary has no text"))?,
        );
    }
    let encrypted_content = item
        .get("encrypted_content")
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| protocol("Responses reasoning encrypted content is invalid"))
        })
        .transpose()?;
    if text.is_empty() && encrypted_content.is_none() {
        return Err(protocol(
            "Responses reasoning has neither summary nor encrypted content",
        ));
    }
    Ok(ReasoningSnapshot {
        text,
        encrypted_content,
    })
}

fn stream_tool_call(item: &Value) -> Result<ToolCall, ProviderError> {
    let id = item
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol("Responses function call has no call_id"))?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol("Responses function call has no name"))?;
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol("Responses function call has no arguments"))?;
    Ok(ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: serde_json::from_str(arguments)
            .map_err(|_| protocol("invalid Responses function-call arguments"))?,
    })
}

fn response_error_message(body: &Value, fallback: &str) -> String {
    body.pointer("/response/error/message")
        .or_else(|| body.pointer("/error/message"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
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

fn unsupported(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unsupported,
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

fn response_protocol_redacted(message: impl Into<String>, secret: &str) -> ProviderError {
    response_protocol(ProviderError::redacted_message(message.into(), secret))
}

fn stream_protocol_redacted(message: impl Into<String>, secret: &str) -> ProviderError {
    stream_protocol(ProviderError::redacted_message(message.into(), secret))
}

fn stream_protocol(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Protocol,
        FailurePhase::DuringStream,
        message,
    )
}

fn stream_unsupported(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unsupported,
        FailurePhase::DuringStream,
        message,
    )
}

fn unsupported_after_dispatch(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unsupported,
        FailurePhase::AfterDispatch,
        message,
    )
}
