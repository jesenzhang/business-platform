use async_trait::async_trait;
use futures::stream;

use crate::{
    Api, AssistantContent, Completion, CompletionRequest, FailurePhase, ModelProvider,
    ProviderCapabilities, ProviderError, ProviderErrorKind, ProviderId, ProviderStream,
    RequestOptions, StreamEvent, ToolCall,
};

/// Deterministic provider for consumers and contract tests.
pub struct MockProvider {
    provider_id: ProviderId,
    api: Api,
    completion: Completion,
}

impl MockProvider {
    pub fn new(completion: Completion) -> Result<Self, ProviderError> {
        let provider_id = ProviderId::new("mock").map_err(|message| {
            ProviderError::new(
                crate::ProviderErrorKind::InvalidRequest,
                crate::FailurePhase::BeforeDispatch,
                message,
            )
        })?;
        Ok(Self {
            provider_id,
            api: Api::Custom("mock".into()),
            completion,
        })
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        &self.api
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

    async fn stream_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<ProviderStream, ProviderError> {
        if options
            .abort
            .as_ref()
            .is_some_and(|abort| abort.is_aborted())
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Aborted,
                FailurePhase::BeforeDispatch,
                "provider request aborted",
            ));
        }
        let prepared = self.prepare_request(request)?;
        self.validate_prepared_request(&prepared)?;
        let request = prepared.into_request();
        let mut events = vec![Ok(StreamEvent::Start {
            model: request.model.id,
        })];
        let mut text_started = false;
        let mut reasoning_started = false;
        for part in &self.completion.message.content {
            match part {
                AssistantContent::Text(text) => {
                    if !text_started {
                        events.push(Ok(StreamEvent::TextStart));
                        text_started = true;
                    }
                    events.push(Ok(StreamEvent::TextDelta {
                        text: text.text.clone(),
                    }));
                }
                AssistantContent::Reasoning(reasoning) => {
                    if !reasoning_started {
                        events.push(Ok(StreamEvent::ReasoningStart));
                        reasoning_started = true;
                    }
                    events.push(Ok(StreamEvent::ReasoningDelta {
                        text: reasoning.text.clone(),
                    }));
                }
                AssistantContent::ToolCall(call) => {
                    let index = events
                        .iter()
                        .filter(|event| matches!(event, Ok(StreamEvent::ToolCallStart { .. })))
                        .count();
                    events.push(Ok(StreamEvent::ToolCallStart {
                        index,
                        id: call.id.clone(),
                        name: call.name.clone(),
                    }));
                    events.push(Ok(StreamEvent::ToolCallDelta {
                        index,
                        arguments_delta: serde_json::to_string(&call.arguments).map_err(|_| {
                            ProviderError::new(
                                crate::ProviderErrorKind::Serialization,
                                crate::FailurePhase::BeforeDispatch,
                                "mock tool arguments are not serializable",
                            )
                        })?,
                    }));
                    events.push(Ok(StreamEvent::ToolCallEnd {
                        index,
                        tool_call: ToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        },
                    }));
                }
            }
        }
        if text_started {
            events.push(Ok(StreamEvent::TextEnd));
        }
        if reasoning_started {
            events.push(Ok(StreamEvent::ReasoningEnd));
        }
        if let Some(usage) = self.completion.usage {
            events.push(Ok(StreamEvent::Usage(usage)));
        }
        if let Some(continuation) = &self.completion.continuation {
            events.push(Ok(StreamEvent::Continuation(continuation.clone())));
        }
        events.push(Ok(StreamEvent::Done {
            stop_reason: self.completion.stop_reason.clone(),
        }));
        Ok(Box::pin(stream::iter(events)))
    }
}

/// A deterministic stream script. It is useful for testing malformed and
/// interrupted provider behavior without a network server.
pub struct ScriptedProvider {
    provider_id: ProviderId,
    api: Api,
    events: Vec<Result<StreamEvent, ProviderError>>,
}

impl ScriptedProvider {
    pub fn new(events: Vec<Result<StreamEvent, ProviderError>>) -> Result<Self, ProviderError> {
        let provider_id = ProviderId::new("scripted").map_err(|message| {
            ProviderError::new(
                crate::ProviderErrorKind::InvalidRequest,
                crate::FailurePhase::BeforeDispatch,
                message,
            )
        })?;
        Ok(Self {
            provider_id,
            api: Api::Custom("test".into()),
            events,
        })
    }

    pub fn with_api(mut self, api: Api) -> Self {
        self.api = api;
        self
    }
}

#[async_trait]
impl ModelProvider for ScriptedProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        &self.api
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

    async fn stream_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<ProviderStream, ProviderError> {
        if options
            .abort
            .as_ref()
            .is_some_and(|abort| abort.is_aborted())
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Aborted,
                FailurePhase::BeforeDispatch,
                "provider request aborted",
            ));
        }
        let prepared = self.prepare_request(request)?;
        self.validate_prepared_request(&prepared)?;
        Ok(Box::pin(stream::iter(self.events.clone())))
    }
}
