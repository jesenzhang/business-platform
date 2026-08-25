//! Provider-neutral conversation history preparation.
//!
//! A normalized [`Message`] can outlive the provider that produced it.  The
//! wire protocols do not have identical representations for reasoning blocks,
//! so each transport applies the small, deterministic policy in this module
//! immediately before validation and dispatch.

use crate::{
    Api, AssistantContent, AssistantMessage, CapabilityKnowledge, CompletionRequest, Message,
    ProviderContinuation, ReasoningContent, ReasoningPortability, TextContent,
};

/// The result of preparing normalized history for a target wire protocol.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryNormalization {
    /// The messages ready for a target provider transport.
    pub messages: Vec<Message>,
    /// Number of reasoning blocks represented as ordinary assistant text.
    pub downgraded_reasoning_blocks: usize,
    /// Number of opaque reasoning blocks omitted because their payload has no
    /// portable representation for the target protocol.
    pub dropped_reasoning_blocks: usize,
}

impl HistoryNormalization {
    /// Whether preparation changed or discarded provider reasoning content.
    pub const fn is_lossy(&self) -> bool {
        self.downgraded_reasoning_blocks != 0 || self.dropped_reasoning_blocks != 0
    }
}

/// Normalize provider-neutral history for a target protocol.
///
/// The policy is intentionally conservative and deterministic:
///
/// - unsigned reasoning sent to Anthropic is downgraded to assistant text;
/// - provider-bound reasoning is replayed only when the target continuation
///   sidecar supplies matching native metadata; otherwise a textual summary
///   may be explicitly downgraded for a cross-provider target, but it is
///   dropped for Anthropic rather than pretending the signature survived;
/// - redacted reasoning is retained only for target protocols with an opaque
///   reasoning field (Anthropic `redacted_thinking` and Responses
///   `encrypted_content`); Chat Completions drops it because it has no such
///   field;
/// - content order and tool-call identity are otherwise copied exactly.
pub fn normalize_history(messages: &[Message], target: &Api) -> HistoryNormalization {
    normalize_history_with_reasoning(messages, target, true, None)
}

/// Normalize the history using the target API and the model's known
/// reasoning capability, exactly as the provider transport does before local
/// validation.
pub(crate) fn normalize_request_history_for_target(
    request: &CompletionRequest,
    target: &Api,
) -> HistoryNormalization {
    normalize_history_with_reasoning(
        &request.messages,
        target,
        reasoning_allowed_for_model(request),
        request.continuation.as_ref(),
    )
}

/// Normalize a request for the API declared by its model.
pub fn normalize_request_history(request: &CompletionRequest) -> HistoryNormalization {
    normalize_request_history_for_target(request, &request.model.api)
}

fn reasoning_allowed_for_model(request: &CompletionRequest) -> bool {
    match request.model.capability_knowledge {
        CapabilityKnowledge::Known => request.model.capabilities.reasoning,
        CapabilityKnowledge::Unknown => true,
    }
}

fn normalize_history_with_reasoning(
    messages: &[Message],
    target: &Api,
    allow_reasoning: bool,
    continuation: Option<&ProviderContinuation>,
) -> HistoryNormalization {
    let mut result = HistoryNormalization {
        messages: Vec::with_capacity(messages.len()),
        ..HistoryNormalization::default()
    };
    for message in messages {
        match message {
            Message::Assistant(assistant) => {
                let content = normalize_assistant_content(
                    assistant,
                    target,
                    allow_reasoning,
                    continuation,
                    &mut result,
                );
                if !content.is_empty() {
                    result
                        .messages
                        .push(Message::Assistant(AssistantMessage { content }));
                }
            }
            other => result.messages.push(other.clone()),
        }
    }
    result
}

fn normalize_assistant_content(
    message: &AssistantMessage,
    target: &Api,
    allow_reasoning: bool,
    continuation: Option<&ProviderContinuation>,
    result: &mut HistoryNormalization,
) -> Vec<AssistantContent> {
    let mut content = Vec::with_capacity(message.content.len());
    for part in &message.content {
        match part {
            AssistantContent::Text(text) => {
                content.push(AssistantContent::Text(text.clone()));
            }
            AssistantContent::ToolCall(tool_call) => {
                content.push(AssistantContent::ToolCall(tool_call.clone()));
            }
            AssistantContent::Reasoning(reasoning) => {
                let replay_available =
                    if reasoning.portability == ReasoningPortability::ProviderBound {
                        provider_replay_available(target, continuation, reasoning)
                    } else {
                        false
                    };
                normalize_reasoning(
                    reasoning,
                    target,
                    allow_reasoning,
                    replay_available,
                    result,
                    &mut content,
                );
            }
        }
    }
    content
}

fn provider_replay_available(
    target: &Api,
    continuation: Option<&ProviderContinuation>,
    reasoning: &ReasoningContent,
) -> bool {
    let Some(reference) = reasoning.continuation_ref.as_ref() else {
        return false;
    };
    match (target, continuation) {
        (Api::AnthropicMessages, Some(ProviderContinuation::AnthropicMessages(value))) => value
            .replay_for(reference)
            .is_some_and(|block| block.is_redacted() == reasoning.redacted),
        (Api::OpenAiResponses, Some(ProviderContinuation::OpenAiResponses(value))) => {
            value.mode() == crate::OpenAiResponsesContinuationMode::Stateful
                || value
                    .replay_items()
                    .iter()
                    .any(|item| item.reference() == reference)
        }
        _ => false,
    }
}

fn normalize_reasoning(
    reasoning: &ReasoningContent,
    target: &Api,
    allow_reasoning: bool,
    replay_available: bool,
    result: &mut HistoryNormalization,
    content: &mut Vec<AssistantContent>,
) {
    if reasoning.portability == ReasoningPortability::ProviderBound
        && reasoning.continuation_ref.is_none()
    {
        // Legacy/provider-bound content without the new stable identity is
        // preserved as an explicit fail-closed marker. It must not be
        // silently downgraded into replayable text.
        content.push(AssistantContent::Reasoning(reasoning.clone()));
        return;
    }
    if reasoning.redacted {
        if allow_reasoning
            && replay_available
            && matches!(target, Api::AnthropicMessages | Api::OpenAiResponses)
        {
            content.push(AssistantContent::Reasoning(ReasoningContent {
                text: reasoning.text.clone(),
                redacted: true,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: reasoning.continuation_ref.clone(),
            }));
        } else {
            result.dropped_reasoning_blocks += 1;
        }
        return;
    }

    if !allow_reasoning {
        if !reasoning.text.is_empty() {
            result.downgraded_reasoning_blocks += 1;
            content.push(AssistantContent::Text(TextContent::new(
                reasoning.text.clone(),
            )));
        } else {
            result.dropped_reasoning_blocks += 1;
        }
        return;
    }

    match target {
        Api::AnthropicMessages => {
            if replay_available {
                content.push(AssistantContent::Reasoning(reasoning.clone()));
            } else if reasoning.portability == ReasoningPortability::ProviderBound
                || reasoning.text.is_empty()
            {
                result.dropped_reasoning_blocks += 1;
            } else {
                result.downgraded_reasoning_blocks += 1;
                content.push(AssistantContent::Text(TextContent::new(
                    reasoning.text.clone(),
                )));
            }
        }
        Api::OpenAiCompletions | Api::OpenAiResponses => {
            if reasoning.text.is_empty() {
                result.dropped_reasoning_blocks += 1;
            } else {
                if reasoning.portability == ReasoningPortability::ProviderBound && !replay_available
                {
                    result.downgraded_reasoning_blocks += 1;
                    content.push(AssistantContent::Text(TextContent::new(
                        reasoning.text.clone(),
                    )));
                } else if reasoning.portability == ReasoningPortability::ProviderBound {
                    content.push(AssistantContent::Reasoning(ReasoningContent {
                        text: reasoning.text.clone(),
                        redacted: false,
                        portability: ReasoningPortability::ProviderBound,
                        continuation_ref: reasoning.continuation_ref.clone(),
                    }));
                } else {
                    content.push(AssistantContent::Reasoning(ReasoningContent {
                        text: reasoning.text.clone(),
                        redacted: false,
                        portability: ReasoningPortability::Portable,
                        continuation_ref: None,
                    }));
                }
            }
        }
        _ => {
            if reasoning.text.is_empty() {
                result.dropped_reasoning_blocks += 1;
            } else if reasoning.portability == ReasoningPortability::ProviderBound {
                result.downgraded_reasoning_blocks += 1;
                content.push(AssistantContent::Text(TextContent::new(
                    reasoning.text.clone(),
                )));
            } else {
                content.push(AssistantContent::Reasoning(reasoning.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ToolCall, UserContent};

    fn assistant(parts: Vec<AssistantContent>) -> Message {
        Message::Assistant(AssistantMessage { content: parts })
    }

    #[test]
    fn unsigned_reasoning_is_text_when_targeting_anthropic() {
        let normalized = normalize_history(
            &[assistant(vec![
                AssistantContent::Reasoning(ReasoningContent {
                    text: "plan".into(),
                    redacted: false,
                    portability: ReasoningPortability::Portable,
                    continuation_ref: None,
                }),
                AssistantContent::ToolCall(ToolCall {
                    id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: serde_json::json!({"q": 1}),
                }),
            ])],
            &Api::AnthropicMessages,
        );
        assert_eq!(normalized.downgraded_reasoning_blocks, 1);
        assert_eq!(normalized.dropped_reasoning_blocks, 0);
        let Message::Assistant(message) = &normalized.messages[0] else {
            panic!("expected assistant message")
        };
        assert!(matches!(
            message.content.as_slice(),
            [AssistantContent::Text(_), AssistantContent::ToolCall(_)]
        ));
        assert_eq!(message.tool_calls()[0].id, "call-1");
    }

    #[test]
    fn signed_reasoning_is_summary_when_targeting_responses() {
        let normalized = normalize_history(
            &[assistant(vec![AssistantContent::Reasoning(
                ReasoningContent {
                    text: "plan".into(),
                    redacted: false,
                    portability: ReasoningPortability::ProviderBound,
                    continuation_ref: None,
                },
            )])],
            &Api::OpenAiResponses,
        );
        assert_eq!(normalized.downgraded_reasoning_blocks, 0);
        let Message::Assistant(message) = &normalized.messages[0] else {
            panic!("expected assistant message")
        };
        assert!(matches!(
            message.content.as_slice(),
            [AssistantContent::Reasoning(ReasoningContent {
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: None,
                ..
            })]
        ));
    }

    #[test]
    fn redacted_reasoning_is_dropped_for_chat_completions() {
        let normalized = normalize_history(
            &[
                Message::User {
                    content: vec![UserContent::Text(TextContent::new("hello"))],
                },
                assistant(vec![AssistantContent::Reasoning(ReasoningContent {
                    text: String::new(),
                    redacted: true,
                    portability: ReasoningPortability::ProviderBound,
                    continuation_ref: None,
                })]),
            ],
            &Api::OpenAiCompletions,
        );
        assert_eq!(normalized.dropped_reasoning_blocks, 0);
        assert_eq!(normalized.messages.len(), 2);
        assert!(matches!(normalized.messages[0], Message::User { .. }));
        let Message::Assistant(message) = &normalized.messages[1] else {
            panic!("expected provider-bound assistant marker")
        };
        assert!(matches!(
            message.content.as_slice(),
            [AssistantContent::Reasoning(ReasoningContent {
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: None,
                ..
            })]
        ));
    }

    #[test]
    fn request_preview_matches_capability_aware_dispatch_preparation() {
        let request = CompletionRequest {
            model: crate::ModelSpec::custom(
                "no-reasoning",
                "anthropic".parse().unwrap(),
                Api::AnthropicMessages,
            )
            .with_capabilities(crate::ModelCapabilities::default()),
            messages: vec![assistant(vec![AssistantContent::Reasoning(
                ReasoningContent {
                    text: "plan".into(),
                    redacted: false,
                    portability: ReasoningPortability::ProviderBound,
                    continuation_ref: None,
                },
            )])],
            tools: Vec::new(),
            temperature: None,
            max_output_tokens: None,
            top_p: None,
            tool_choice: None,
            reasoning: None,
            output_constraint: None,
            retention: crate::DataRetentionPolicy::Ephemeral,
            continuation: None,
        };
        let normalized = normalize_request_history(&request);
        assert_eq!(normalized.downgraded_reasoning_blocks, 0);
        let Message::Assistant(message) = &normalized.messages[0] else {
            panic!("expected downgraded assistant history")
        };
        assert!(matches!(
            message.content[0],
            AssistantContent::Reasoning(ReasoningContent {
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: None,
                ..
            })
        ));
    }

    #[test]
    fn redacted_summary_is_opaque_only_for_all_targets() {
        let messages = [assistant(vec![AssistantContent::Reasoning(
            ReasoningContent {
                text: "provider summary".into(),
                redacted: true,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: None,
            },
        )])];
        for target in [Api::AnthropicMessages, Api::OpenAiResponses] {
            let normalized = normalize_history(&messages, &target);
            assert_eq!(normalized.dropped_reasoning_blocks, 0);
            assert_eq!(normalized.messages.len(), 1);
            let Message::Assistant(message) = &normalized.messages[0] else {
                panic!("expected provider-bound assistant marker")
            };
            assert!(matches!(
                message.content.as_slice(),
                [AssistantContent::Reasoning(ReasoningContent {
                    portability: ReasoningPortability::ProviderBound,
                    continuation_ref: None,
                    ..
                })]
            ));
        }
        let normalized = normalize_history(&messages, &Api::OpenAiCompletions);
        assert_eq!(normalized.dropped_reasoning_blocks, 0);
        assert_eq!(normalized.messages.len(), 1);
    }
}
