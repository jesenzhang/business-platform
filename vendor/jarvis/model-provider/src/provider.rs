use std::pin::Pin;
use std::time::Instant;

use async_trait::async_trait;
use futures::Stream;

use crate::{
    collect_stream_with_started_at, protocol_constraint_capabilities,
    validate_estimated_request_budget, Api, AssistantContent, AssistantMessage, Completion,
    CompletionRequest, ConstraintCapabilities, Message, PreparedRequest, ProviderCapabilities,
    ProviderError, ProviderErrorKind, ProviderId, ReasoningPortability, RequestOptions,
    StreamEvent, ToolChoice, ToolResultContent, UserContent,
};

pub type ProviderStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ProviderError>> + Send>>;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn api(&self) -> &Api;
    fn capabilities(&self) -> ProviderCapabilities;

    /// Declare the provider's implemented constraint fields. Implementors
    /// inherit the conservative protocol matrix unless an endpoint has a
    /// narrower or broader, explicitly known contract.
    fn constraint_capabilities(&self) -> ConstraintCapabilities {
        protocol_constraint_capabilities(self.api())
    }

    /// Prepare a raw request for this provider's target API. This is the one
    /// normalization seam used by direct, profile-backed, streaming, and
    /// non-streaming calls.
    fn prepare_request(
        &self,
        request: CompletionRequest,
    ) -> Result<PreparedRequest, ProviderError> {
        Ok(crate::prepare_request(self.api(), request))
    }

    /// Validate the model binding and locally-known feature requirements after
    /// deterministic preparation and before wire encoding or dispatch.
    fn validate_prepared_request(&self, prepared: &PreparedRequest) -> Result<(), ProviderError> {
        let request = prepared.request();
        if request.model.id.trim().is_empty() {
            return Err(before_dispatch("model id must not be empty"));
        }
        if request.model.provider != *self.provider_id() {
            return Err(before_dispatch(format!(
                "model/provider binding mismatch: model belongs to {}, provider is {}",
                request.model.provider,
                self.provider_id()
            )));
        }
        if request.model.api != *self.api() {
            return Err(before_dispatch(format!(
                "model/API binding mismatch: model uses {:?}, provider uses {:?}",
                request.model.api,
                self.api()
            )));
        }
        if let Some(continuation) = &request.continuation {
            if let Err(message) = continuation.validate() {
                return Err(before_dispatch(message));
            }
            if continuation.provider() != self.provider_id()
                || continuation.api() != *self.api()
                || continuation.model() != request.model.id
            {
                return Err(before_dispatch(
                    "provider continuation identity does not match the target provider, API, or model",
                ));
            }
            if let Err(message) = continuation.validate_for_history(&request.messages) {
                return Err(before_dispatch(message));
            }
        }
        if let Err(message) = validate_reasoning_reference_contract(&request.messages) {
            return Err(before_dispatch(message));
        }
        let provider_capabilities = self.capabilities();
        if !request.tools.is_empty() && !provider_capabilities.tools {
            return Err(unsupported("provider tool capability is not supported"));
        }
        if !request.tools.is_empty()
            && matches!(
                request.model.capability_knowledge,
                crate::CapabilityKnowledge::Known
            )
            && !request.model.capabilities.tools
        {
            return Err(unsupported("model tool capability is not supported"));
        }
        let needs_reasoning = request.messages.iter().any(|message| {
            matches!(
                message,
                Message::Assistant(AssistantMessage { content })
                    if content.iter().any(|part| matches!(part, AssistantContent::Reasoning(_)))
            )
        });
        if needs_reasoning && !provider_capabilities.reasoning {
            return Err(unsupported(
                "provider reasoning capability is not supported",
            ));
        }
        if needs_reasoning
            && matches!(
                request.model.capability_knowledge,
                crate::CapabilityKnowledge::Known
            )
            && !request.model.capabilities.reasoning
        {
            return Err(unsupported("model reasoning capability is not supported"));
        }
        let needs_vision = request.messages.iter().any(|message| match message {
            Message::User { content } => content
                .iter()
                .any(|part| matches!(part, UserContent::Image(_))),
            Message::ToolResult { content, .. } => content
                .iter()
                .any(|part| matches!(part, ToolResultContent::Image(_))),
            _ => false,
        });
        if needs_vision && !provider_capabilities.vision {
            return Err(unsupported("provider vision capability is not supported"));
        }
        if needs_vision
            && matches!(
                request.model.capability_knowledge,
                crate::CapabilityKnowledge::Known
            )
            && !request.model.capabilities.vision
        {
            return Err(unsupported("model vision capability is not supported"));
        }
        if request.output_constraint.is_some()
            && matches!(
                request.model.capability_knowledge,
                crate::CapabilityKnowledge::Known
            )
            && !request.model.capabilities.structured_output
        {
            return Err(unsupported(
                "model structured output capability is not supported",
            ));
        }
        crate::constraints::validate_request_constraints(
            request,
            self.api(),
            self.constraint_capabilities(),
        )?;
        if request
            .reasoning
            .as_ref()
            .is_some_and(|value| value.enabled)
            && !provider_capabilities.reasoning
        {
            return Err(unsupported(
                "provider reasoning capability is not supported",
            ));
        }
        if matches!(
            request.tool_choice,
            Some(ToolChoice::Required | ToolChoice::Tool { .. })
        ) && request.tools.is_empty()
        {
            return Err(before_dispatch(
                "tool_choice requires at least one tool definition",
            ));
        }
        if let Err(error) = validate_estimated_request_budget(request, prepared.budget()) {
            return Err(before_dispatch(error.to_string()));
        }
        Ok(())
    }

    /// Prepare and validate a raw request using the same semantics as
    /// dispatch. Callers that need normalized diagnostics should call
    /// [`ModelProvider::prepare_request`] and then
    /// [`ModelProvider::validate_prepared_request`] separately.
    fn validate_request(&self, request: &CompletionRequest) -> Result<(), ProviderError> {
        let prepared = self.prepare_request(request.clone())?;
        self.validate_prepared_request(&prepared)
    }

    async fn stream(&self, request: CompletionRequest) -> Result<ProviderStream, ProviderError> {
        self.stream_with(request, RequestOptions::default()).await
    }

    async fn stream_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<ProviderStream, ProviderError>;

    async fn complete(&self, request: CompletionRequest) -> Result<Completion, ProviderError> {
        self.complete_with(request, RequestOptions::default()).await
    }

    async fn complete_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<Completion, ProviderError> {
        let started_at = Instant::now();
        let mut stream = self.stream_with(request, options).await?;
        collect_stream_with_started_at(&mut stream, started_at).await
    }
}

fn before_dispatch(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        crate::FailurePhase::BeforeDispatch,
        message,
    )
}

fn unsupported(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unsupported,
        crate::FailurePhase::BeforeDispatch,
        message,
    )
}

fn validate_reasoning_reference_contract(messages: &[Message]) -> Result<(), String> {
    let mut references = std::collections::HashSet::new();
    for message in messages {
        let Message::Assistant(assistant) = message else {
            continue;
        };
        for part in &assistant.content {
            let crate::AssistantContent::Reasoning(reasoning) = part else {
                continue;
            };
            match reasoning.portability {
                ReasoningPortability::Portable if reasoning.continuation_ref.is_some() => {
                    return Err("portable reasoning must not carry a continuation reference".into())
                }
                ReasoningPortability::ProviderBound => {
                    let reference = reasoning.continuation_ref.as_ref().ok_or_else(|| {
                        "provider-bound reasoning is missing a continuation reference".to_string()
                    })?;
                    if !references.insert(reference.clone()) {
                        return Err("duplicate continuation reference in history".into());
                    }
                }
                ReasoningPortability::Portable => {}
            }
        }
    }
    Ok(())
}
