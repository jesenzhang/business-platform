use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use futures::{Stream, StreamExt};

use crate::{
    AssistantContent, AssistantMessage, Completion, CompletionMetadata, ContinuationRef,
    ProviderContinuation, ProviderError, ProviderErrorKind, ReasoningContent, ReasoningPortability,
    StopReason, ToolCall, Usage,
};

const MAX_ACCUMULATED_COMPLETION_BYTES: usize = 8 * 1024 * 1024;
const MAX_ACCUMULATED_CONTENT_BLOCKS: usize = 4096;

struct AccumulationLimits {
    bytes: usize,
    content_blocks: usize,
}

#[derive(Clone, PartialEq)]
pub enum StreamEvent {
    Start {
        model: String,
    },
    TextStart,
    TextDelta {
        text: String,
    },
    TextEnd,
    ReasoningStart,
    ReasoningDelta {
        text: String,
    },
    ReasoningSignature {
        signature: String,
    },
    ReasoningReference {
        reference: ContinuationRef,
    },
    ReasoningRedacted {
        /// `None` marks opaque reasoning whose payload is carried by a
        /// provider continuation sidecar rather than normalized message
        /// content.
        data: Option<String>,
    },
    ReasoningEnd,
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallDelta {
        index: usize,
        arguments_delta: String,
    },
    ToolCallEnd {
        index: usize,
        tool_call: ToolCall,
    },
    Usage(Usage),
    /// Provider-native continuation sidecar. It is kept separate from the
    /// normalized assistant message and must appear before `Done`.
    Continuation(ProviderContinuation),
    Done {
        stop_reason: StopReason,
    },
}

impl fmt::Debug for StreamEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start { model } => formatter
                .debug_struct("Start")
                .field("model", model)
                .finish(),
            Self::TextStart => formatter.write_str("TextStart"),
            Self::TextDelta { text } => formatter
                .debug_struct("TextDelta")
                .field("bytes", &text.len())
                .finish(),
            Self::TextEnd => formatter.write_str("TextEnd"),
            Self::ReasoningStart => formatter.write_str("ReasoningStart"),
            Self::ReasoningDelta { text } => formatter
                .debug_struct("ReasoningDelta")
                .field("bytes", &text.len())
                .finish(),
            Self::ReasoningSignature { signature } => formatter
                .debug_struct("ReasoningSignature")
                .field("sensitive_bytes", &signature.len())
                .finish(),
            Self::ReasoningReference { reference } => formatter
                .debug_struct("ReasoningReference")
                .field("reference", reference)
                .finish(),
            Self::ReasoningRedacted { data } => formatter
                .debug_struct("ReasoningRedacted")
                .field("payload_present", &data.is_some())
                .field("sensitive_bytes", &data.as_ref().map_or(0, String::len))
                .finish(),
            Self::ReasoningEnd => formatter.write_str("ReasoningEnd"),
            Self::ToolCallStart { index, id, name } => formatter
                .debug_struct("ToolCallStart")
                .field("index", index)
                .field("id", id)
                .field("name", name)
                .finish(),
            Self::ToolCallDelta {
                index,
                arguments_delta,
            } => formatter
                .debug_struct("ToolCallDelta")
                .field("index", index)
                .field("sensitive_bytes", &arguments_delta.len())
                .finish(),
            Self::ToolCallEnd { index, tool_call } => formatter
                .debug_struct("ToolCallEnd")
                .field("index", index)
                .field("id", &tool_call.id)
                .field("name", &tool_call.name)
                .field("argument_bytes", &tool_call.arguments.to_string().len())
                .finish(),
            Self::Usage(value) => formatter.debug_tuple("Usage").field(value).finish(),
            Self::Continuation(value) => {
                formatter.debug_tuple("Continuation").field(value).finish()
            }
            Self::Done { stop_reason } => formatter
                .debug_struct("Done")
                .field("stop_reason", stop_reason)
                .finish(),
        }
    }
}

struct OpenReasoning {
    text: String,
    signature: Option<String>,
    redacted: bool,
    redacted_data: Option<String>,
    continuation_ref: Option<ContinuationRef>,
}

/// The single authoritative normalized stream state machine.
pub struct StreamAccumulator {
    started_at: Instant,
    forced_elapsed: Option<Duration>,
    accumulation_limits: AccumulationLimits,
    retained_bytes: usize,
    content_blocks_started: usize,
    // A slot is reserved when a block starts, so completion order cannot
    // reorder blocks that were interleaved on the wire.
    content: Vec<Option<AssistantContent>>,
    open_text: Option<String>,
    text_slot: Option<usize>,
    open_reasoning: Option<OpenReasoning>,
    reasoning_slot: Option<usize>,
    reasoning_references: HashSet<ContinuationRef>,
    tool_calls: BTreeMap<usize, (usize, String, String, String)>,
    completed_tool_indexes: HashSet<usize>,
    usage: Option<Usage>,
    continuation: Option<ProviderContinuation>,
    stop_reason: Option<StopReason>,
    started: bool,
    reasoning_started: bool,
    done: bool,
}

impl Default for StreamAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::with_accumulation_limits(
            MAX_ACCUMULATED_COMPLETION_BYTES,
            MAX_ACCUMULATED_CONTENT_BLOCKS,
        )
    }

    fn with_accumulation_limits(bytes: usize, content_blocks: usize) -> Self {
        Self {
            started_at: Instant::now(),
            forced_elapsed: None,
            accumulation_limits: AccumulationLimits {
                bytes,
                content_blocks,
            },
            retained_bytes: 0,
            content_blocks_started: 0,
            content: Vec::new(),
            open_text: None,
            text_slot: None,
            open_reasoning: None,
            reasoning_slot: None,
            reasoning_references: HashSet::new(),
            tool_calls: BTreeMap::new(),
            completed_tool_indexes: HashSet::new(),
            usage: None,
            continuation: None,
            stop_reason: None,
            started: false,
            reasoning_started: false,
            done: false,
        }
    }

    #[cfg(test)]
    fn with_test_limits(bytes: usize, content_blocks: usize) -> Self {
        Self::with_accumulation_limits(bytes, content_blocks)
    }

    fn next_retained_bytes(&self, additional: usize) -> Result<usize, ProviderError> {
        let next = self
            .retained_bytes
            .checked_add(additional)
            .ok_or_else(accumulated_completion_byte_limit)?;
        if next > self.accumulation_limits.bytes {
            return Err(accumulated_completion_byte_limit());
        }
        Ok(next)
    }

    fn next_replaced_bytes(
        &self,
        replaced: usize,
        replacement: usize,
    ) -> Result<usize, ProviderError> {
        let retained_without_replaced = self
            .retained_bytes
            .checked_sub(replaced)
            .ok_or_else(accumulated_completion_byte_limit)?;
        let next = retained_without_replaced
            .checked_add(replacement)
            .ok_or_else(accumulated_completion_byte_limit)?;
        if next > self.accumulation_limits.bytes {
            return Err(accumulated_completion_byte_limit());
        }
        Ok(next)
    }

    fn next_content_block_count(&self) -> Result<usize, ProviderError> {
        let next = self
            .content_blocks_started
            .checked_add(1)
            .ok_or_else(accumulated_completion_block_limit)?;
        if next > self.accumulation_limits.content_blocks {
            return Err(accumulated_completion_block_limit());
        }
        Ok(next)
    }

    /// Deterministic timing hook for tests and callers with an external clock.
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.forced_elapsed = Some(elapsed);
        self
    }

    /// Start elapsed-time accounting at the caller's dispatch boundary.
    /// Providers and adapters can create this before awaiting HTTP dispatch.
    pub fn with_started_at(mut self, started_at: Instant) -> Self {
        self.started_at = started_at;
        self
    }

    /// Return the history collected so far, including currently buffered text
    /// and reasoning blocks.
    ///
    /// This is the continuation boundary for an aborted stream. Completed
    /// tool calls are retained by the accumulator; an in-flight tool call is
    /// included only when its buffered arguments already form valid JSON.
    /// The returned message is intentionally not a completed [`Completion`]
    /// and must not be treated as one.
    pub fn partial_message(&self) -> AssistantMessage {
        let mut content = Vec::with_capacity(self.content.len());
        for (slot, part) in self.content.iter().enumerate() {
            if let Some(part) = part {
                content.push(part.clone());
            } else if self.text_slot == Some(slot) {
                if let Some(text) = &self.open_text {
                    content.push(AssistantContent::Text(crate::TextContent::new(text)));
                }
            } else if self.reasoning_slot == Some(slot) {
                if let Some(reasoning) = &self.open_reasoning {
                    content.push(AssistantContent::Reasoning(ReasoningContent {
                        text: reasoning.text.clone(),
                        redacted: reasoning.redacted,
                        portability: if reasoning.signature.is_some() || reasoning.redacted {
                            ReasoningPortability::ProviderBound
                        } else {
                            ReasoningPortability::Portable
                        },
                        continuation_ref: None,
                    }));
                }
            } else if let Some((_, id, name, arguments)) = self
                .tool_calls
                .values()
                .find(|(candidate_slot, ..)| *candidate_slot == slot)
            {
                let Ok(arguments) = serde_json::from_str(arguments) else {
                    continue;
                };
                content.push(AssistantContent::ToolCall(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments,
                }));
            }
        }
        AssistantMessage { content }
    }

    pub fn push(&mut self, event: StreamEvent) -> Result<(), ProviderError> {
        if self.done {
            return Err(protocol("stream emitted data after terminal Done"));
        }
        if !self.started && !matches!(event, StreamEvent::Start { .. }) {
            return Err(protocol("stream emitted data before Start"));
        }
        match event {
            StreamEvent::Start { .. } => {
                if self.started {
                    return Err(protocol("duplicate stream Start"));
                }
                self.started = true;
            }
            StreamEvent::TextStart => {
                if !self.started || self.open_text.is_some() {
                    return Err(protocol("invalid TextStart"));
                }
                let next_blocks = self.next_content_block_count()?;
                let slot = self.content.len();
                self.open_text = Some(String::new());
                self.text_slot = Some(slot);
                self.content.push(None);
                self.content_blocks_started = next_blocks;
            }
            StreamEvent::TextDelta { text } => {
                if self.open_text.is_none() {
                    return Err(protocol("text delta before TextStart"));
                }
                let next_bytes = self.next_retained_bytes(text.len())?;
                let open_text = self.open_text.as_mut().expect("text state was checked");
                open_text.push_str(&text);
                self.retained_bytes = next_bytes;
            }
            StreamEvent::TextEnd => {
                let Some(text) = self.open_text.take() else {
                    return Err(protocol("text end before TextStart"));
                };
                let Some(slot) = self.text_slot.take() else {
                    return Err(protocol("text slot is missing"));
                };
                self.content[slot] = Some(AssistantContent::Text(crate::TextContent::new(text)));
            }
            StreamEvent::ReasoningStart => {
                if !self.started || self.reasoning_started {
                    return Err(protocol("invalid ReasoningStart"));
                }
                let next_blocks = self.next_content_block_count()?;
                let slot = self.content.len();
                self.reasoning_started = true;
                self.reasoning_slot = Some(slot);
                self.open_reasoning = Some(OpenReasoning {
                    text: String::new(),
                    signature: None,
                    redacted: false,
                    redacted_data: None,
                    continuation_ref: None,
                });
                self.content.push(None);
                self.content_blocks_started = next_blocks;
            }
            StreamEvent::ReasoningDelta { text } => {
                if self.open_reasoning.is_none() {
                    return Err(protocol("reasoning delta before ReasoningStart"));
                }
                let next_bytes = self.next_retained_bytes(text.len())?;
                let reasoning = self
                    .open_reasoning
                    .as_mut()
                    .expect("reasoning state was checked");
                reasoning.text.push_str(&text);
                self.retained_bytes = next_bytes;
            }
            StreamEvent::ReasoningSignature { signature } => {
                if self.open_reasoning.is_none() {
                    return Err(protocol("reasoning signature before ReasoningStart"));
                }
                let next_bytes = self.next_retained_bytes(signature.len())?;
                let reasoning = self
                    .open_reasoning
                    .as_mut()
                    .expect("reasoning state was checked");
                reasoning
                    .signature
                    .get_or_insert_with(String::new)
                    .push_str(&signature);
                self.retained_bytes = next_bytes;
            }
            StreamEvent::ReasoningReference { reference } => {
                if self.open_reasoning.is_none() {
                    return Err(protocol("reasoning reference before ReasoningStart"));
                }
                if reference.as_str().trim().is_empty() {
                    return Err(protocol("reasoning reference must not be empty"));
                }
                if !self.reasoning_references.insert(reference.clone()) {
                    return Err(protocol(
                        "duplicate reasoning continuation reference in stream",
                    ));
                }
                let reasoning = self
                    .open_reasoning
                    .as_mut()
                    .expect("reasoning state was checked");
                if reasoning.continuation_ref.replace(reference).is_some() {
                    return Err(protocol("duplicate reasoning continuation reference"));
                }
            }
            StreamEvent::ReasoningRedacted { data } => {
                if self.open_reasoning.is_none() {
                    return Err(protocol("redacted reasoning before ReasoningStart"));
                }
                let data = data.filter(|value| !value.is_empty());
                let replaced_bytes = self
                    .open_reasoning
                    .as_ref()
                    .and_then(|reasoning| reasoning.redacted_data.as_ref())
                    .map_or(0, String::len);
                let next_bytes =
                    self.next_replaced_bytes(replaced_bytes, data.as_ref().map_or(0, String::len))?;
                let reasoning = self
                    .open_reasoning
                    .as_mut()
                    .expect("reasoning state was checked");
                reasoning.redacted = true;
                reasoning.redacted_data = data;
                self.retained_bytes = next_bytes;
            }
            StreamEvent::ReasoningEnd => {
                if !self.reasoning_started {
                    return Err(protocol("reasoning end before ReasoningStart"));
                }
                self.reasoning_started = false;
                let reasoning = self
                    .open_reasoning
                    .take()
                    .ok_or_else(|| protocol("reasoning state is missing"))?;
                let Some(slot) = self.reasoning_slot.take() else {
                    return Err(protocol("reasoning slot is missing"));
                };
                self.content[slot] = Some(AssistantContent::Reasoning(ReasoningContent {
                    text: reasoning.text,
                    redacted: reasoning.redacted,
                    portability: if reasoning.signature.is_some() || reasoning.redacted {
                        ReasoningPortability::ProviderBound
                    } else {
                        ReasoningPortability::Portable
                    },
                    continuation_ref: reasoning.continuation_ref,
                }));
            }
            StreamEvent::ToolCallStart { index, id, name } => {
                if self.completed_tool_indexes.contains(&index)
                    || self.tool_calls.contains_key(&index)
                {
                    return Err(protocol("duplicate tool call start"));
                }
                let next_blocks = self.next_content_block_count()?;
                let start_bytes = id
                    .len()
                    .checked_add(name.len())
                    .ok_or_else(accumulated_completion_byte_limit)?;
                let next_bytes = self.next_retained_bytes(start_bytes)?;
                let slot = self.content.len();
                self.content.push(None);
                self.tool_calls
                    .insert(index, (slot, id, name, String::new()));
                self.content_blocks_started = next_blocks;
                self.retained_bytes = next_bytes;
            }
            StreamEvent::ToolCallDelta {
                index,
                arguments_delta,
            } => {
                if !self.tool_calls.contains_key(&index) {
                    return Err(protocol("tool call delta before ToolCallStart"));
                }
                let next_bytes = self.next_retained_bytes(arguments_delta.len())?;
                let (_, _, _, arguments) = self
                    .tool_calls
                    .get_mut(&index)
                    .ok_or_else(|| protocol("tool call delta before ToolCallStart"))?;
                arguments.push_str(&arguments_delta);
                self.retained_bytes = next_bytes;
            }
            StreamEvent::ToolCallEnd { index, tool_call } => {
                let Some((_, id, name, arguments)) = self.tool_calls.get(&index) else {
                    return Err(protocol("tool call end before ToolCallStart"));
                };
                if id != &tool_call.id || name != &tool_call.name {
                    return Err(protocol("tool call end identity mismatch"));
                }
                let assembled: serde_json::Value = serde_json::from_str(arguments)
                    .map_err(|_| protocol("malformed tool call argument delta"))?;
                if assembled != tool_call.arguments {
                    return Err(protocol("tool call end arguments disagree with deltas"));
                }
                let (slot, id, name, _) = self
                    .tool_calls
                    .remove(&index)
                    .ok_or_else(|| protocol("tool call end before ToolCallStart"))?;
                if !self.completed_tool_indexes.insert(index) {
                    return Err(protocol("duplicate tool call end"));
                }
                let ToolCall {
                    arguments: final_arguments,
                    ..
                } = tool_call;
                self.content[slot] = Some(AssistantContent::ToolCall(ToolCall {
                    id,
                    name,
                    arguments: final_arguments,
                }));
            }
            StreamEvent::Usage(value) => {
                value.validate().map_err(|error| {
                    ProviderError::new(
                        ProviderErrorKind::Protocol,
                        crate::FailurePhase::DuringStream,
                        error.to_string(),
                    )
                })?;
                self.usage = Some(value);
            }
            StreamEvent::Continuation(value) => {
                value.validate().map_err(|message| protocol(&message))?;
                if self.continuation.replace(value).is_some() {
                    return Err(protocol("stream emitted duplicate provider continuation"));
                }
            }
            StreamEvent::Done { stop_reason } => {
                if !self.started {
                    return Err(protocol("stream Done before Start"));
                }
                if self.open_text.is_some() || self.reasoning_started || !self.tool_calls.is_empty()
                {
                    return Err(protocol("stream Done with an open content block"));
                }
                let stop_reason_bytes = match &stop_reason {
                    StopReason::Other(value) => value.len(),
                    _ => 0,
                };
                let next_bytes = self.next_retained_bytes(stop_reason_bytes)?;
                self.retained_bytes = next_bytes;
                self.stop_reason = Some(stop_reason);
                self.done = true;
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Completion, ProviderError> {
        if !self.started || !self.done {
            return Err(ProviderError::new(
                ProviderErrorKind::StreamInterrupted,
                crate::FailurePhase::DuringStream,
                "provider stream ended before exactly one terminal Done",
            ));
        }
        if self.open_text.is_some() || self.reasoning_started || !self.tool_calls.is_empty() {
            return Err(protocol("stream ended with an incomplete content block"));
        }
        let stop_reason = self
            .stop_reason
            .clone()
            .ok_or_else(|| protocol("stream has no stop reason"))?;
        let content = self
            .content
            .into_iter()
            .map(|part| part.ok_or_else(|| protocol("stream has an incomplete content block")))
            .collect::<Result<Vec<_>, _>>()?;
        let message = AssistantMessage { content };
        if message.content.iter().any(|part| {
            matches!(
                part,
                AssistantContent::Reasoning(reasoning)
                    if reasoning.portability == ReasoningPortability::ProviderBound
            )
        }) && self.continuation.is_none()
        {
            return Err(protocol(
                "provider-bound reasoning is missing a continuation sidecar",
            ));
        }
        if let Some(continuation) = &self.continuation {
            continuation
                .validate_for_message(&message)
                .map_err(|message| protocol(&message))?;
        }
        let elapsed = self
            .forced_elapsed
            .unwrap_or_else(|| self.started_at.elapsed())
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        Ok(Completion {
            metadata: CompletionMetadata {
                content_chars: message.text_value().chars().count(),
                reasoning_chars: message.reasoning_chars(),
                tool_call_count: message.tool_calls().len(),
                stop_reason: Some(stop_reason.clone()),
                stream_completed: true,
                elapsed_ms: elapsed,
            },
            message,
            usage: self.usage,
            continuation: self.continuation,
            stop_reason,
        })
    }
}

/// Convenience collector over the authoritative accumulator.
pub async fn collect_stream<S>(mut stream: S) -> Result<Completion, ProviderError>
where
    S: Stream<Item = Result<StreamEvent, ProviderError>> + Unpin,
{
    collect_stream_with_started_at(&mut stream, Instant::now()).await
}

/// Collect a provider stream while preserving a dispatch-boundary timestamp.
pub async fn collect_stream_with_started_at<S>(
    stream: &mut S,
    started_at: Instant,
) -> Result<Completion, ProviderError>
where
    S: Stream<Item = Result<StreamEvent, ProviderError>> + Unpin,
{
    let mut accumulator = StreamAccumulator::new().with_started_at(started_at);
    while let Some(event) = stream.next().await {
        accumulator.push(event?)?;
    }
    accumulator.finish()
}

fn protocol(message: &str) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Protocol,
        crate::FailurePhase::DuringStream,
        message,
    )
}

fn accumulated_completion_byte_limit() -> ProviderError {
    protocol("normalized stream exceeds accumulated completion byte limit")
}

fn accumulated_completion_block_limit() -> ProviderError {
    protocol("normalized stream exceeds accumulated completion block limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started_accumulator(bytes: usize, content_blocks: usize) -> StreamAccumulator {
        let mut accumulator = StreamAccumulator::with_test_limits(bytes, content_blocks);
        accumulator
            .push(StreamEvent::Start {
                model: "test-model".into(),
            })
            .unwrap();
        accumulator
    }

    fn assert_budget_error(error: ProviderError, message: &str) {
        assert_eq!(error.kind, ProviderErrorKind::Protocol);
        assert_eq!(error.phase, crate::FailurePhase::DuringStream);
        assert_eq!(error.message, message);
    }

    fn minimal_tool(index: usize) -> [StreamEvent; 3] {
        [
            StreamEvent::ToolCallStart {
                index,
                id: String::new(),
                name: String::new(),
            },
            StreamEvent::ToolCallDelta {
                index,
                arguments_delta: "{}".into(),
            },
            StreamEvent::ToolCallEnd {
                index,
                tool_call: ToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: serde_json::json!({}),
                },
            },
        ]
    }

    #[test]
    fn many_small_text_deltas_share_one_byte_budget() {
        let mut accumulator = started_accumulator(3, 16);
        accumulator.push(StreamEvent::TextStart).unwrap();
        for text in ["a", "b", "c"] {
            accumulator
                .push(StreamEvent::TextDelta { text: text.into() })
                .unwrap();
        }

        let error = accumulator
            .push(StreamEvent::TextDelta { text: "d".into() })
            .unwrap_err();
        assert_budget_error(
            error,
            "normalized stream exceeds accumulated completion byte limit",
        );
        assert_eq!(accumulator.open_text.as_deref(), Some("abc"));
        assert_eq!(accumulator.retained_bytes, 3);
    }

    #[test]
    fn reasoning_payloads_share_one_byte_budget() {
        let mut accumulator = started_accumulator(8, 16);
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        accumulator
            .push(StreamEvent::ReasoningDelta { text: "ab".into() })
            .unwrap();
        accumulator
            .push(StreamEvent::ReasoningSignature {
                signature: "cd".into(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::ReasoningRedacted {
                data: Some("efgh".into()),
            })
            .unwrap();

        let error = accumulator
            .push(StreamEvent::ReasoningDelta { text: "i".into() })
            .unwrap_err();
        assert_budget_error(
            error,
            "normalized stream exceeds accumulated completion byte limit",
        );
        assert_eq!(accumulator.retained_bytes, 8);
    }

    #[test]
    fn text_reasoning_and_tool_payloads_share_one_byte_budget() {
        let mut accumulator = started_accumulator(9, 16);
        accumulator.push(StreamEvent::TextStart).unwrap();
        accumulator
            .push(StreamEvent::TextDelta { text: "abc".into() })
            .unwrap();
        accumulator.push(StreamEvent::TextEnd).unwrap();
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        accumulator
            .push(StreamEvent::ReasoningDelta { text: "de".into() })
            .unwrap();
        accumulator.push(StreamEvent::ReasoningEnd).unwrap();
        accumulator
            .push(StreamEvent::ToolCallStart {
                index: 0,
                id: "i".into(),
                name: "n".into(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "{}".into(),
            })
            .unwrap();

        let error = accumulator
            .push(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "x".into(),
            })
            .unwrap_err();
        assert_budget_error(
            error,
            "normalized stream exceeds accumulated completion byte limit",
        );
        assert_eq!(accumulator.retained_bytes, 9);
    }

    #[test]
    fn empty_text_and_reasoning_blocks_consume_structure_budget() {
        let mut accumulator = started_accumulator(1, 3);
        for event in [
            StreamEvent::TextStart,
            StreamEvent::TextEnd,
            StreamEvent::ReasoningStart,
            StreamEvent::ReasoningEnd,
            StreamEvent::TextStart,
            StreamEvent::TextEnd,
        ] {
            accumulator.push(event).unwrap();
        }

        let error = accumulator.push(StreamEvent::ReasoningStart).unwrap_err();
        assert_budget_error(
            error,
            "normalized stream exceeds accumulated completion block limit",
        );
        assert_eq!(accumulator.content_blocks_started, 3);
        assert_eq!(accumulator.retained_bytes, 0);
    }

    #[test]
    fn minimal_tool_blocks_consume_structure_budget() {
        let mut accumulator = started_accumulator(16, 2);
        for event in minimal_tool(0).into_iter() {
            accumulator.push(event).unwrap();
        }
        for event in minimal_tool(1).into_iter() {
            accumulator.push(event).unwrap();
        }

        let error = accumulator
            .push(StreamEvent::ToolCallStart {
                index: 2,
                id: String::new(),
                name: String::new(),
            })
            .unwrap_err();
        assert_budget_error(
            error,
            "normalized stream exceeds accumulated completion block limit",
        );
        assert_eq!(accumulator.content_blocks_started, 2);
        assert_eq!(accumulator.retained_bytes, 4);
    }

    #[test]
    fn completion_just_below_both_accumulation_limits_succeeds() {
        let mut accumulator = started_accumulator(4, 3);
        accumulator.push(StreamEvent::TextStart).unwrap();
        accumulator
            .push(StreamEvent::TextDelta { text: "a".into() })
            .unwrap();
        accumulator.push(StreamEvent::TextEnd).unwrap();
        accumulator
            .push(StreamEvent::ToolCallStart {
                index: 0,
                id: String::new(),
                name: String::new(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::ToolCallDelta {
                index: 0,
                arguments_delta: "{}".into(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::ToolCallEnd {
                index: 0,
                tool_call: ToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: serde_json::json!({}),
                },
            })
            .unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            })
            .unwrap();

        let completion = accumulator.finish().unwrap();
        assert_eq!(completion.message.text_value(), "a");
        assert_eq!(completion.message.tool_calls().len(), 1);
        assert_eq!(completion.metadata.content_chars, 1);
        assert_eq!(completion.metadata.tool_call_count, 1);
    }

    #[test]
    fn stop_reason_other_shares_completion_byte_budget() {
        let mut accumulator = started_accumulator(4, 4);
        accumulator.push(StreamEvent::TextStart).unwrap();
        accumulator
            .push(StreamEvent::TextDelta { text: "abc".into() })
            .unwrap();
        accumulator.push(StreamEvent::TextEnd).unwrap();

        let error = accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Other("de".into()),
            })
            .unwrap_err();

        assert_budget_error(
            error,
            "normalized stream exceeds accumulated completion byte limit",
        );
        assert_eq!(accumulator.retained_bytes, 3);
        assert_eq!(accumulator.stop_reason, None);
        assert!(!accumulator.done);
    }

    #[test]
    fn stop_reason_other_at_remaining_boundary_succeeds() {
        let mut accumulator = started_accumulator(5, 4);
        accumulator.push(StreamEvent::TextStart).unwrap();
        accumulator
            .push(StreamEvent::TextDelta { text: "abc".into() })
            .unwrap();
        accumulator.push(StreamEvent::TextEnd).unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Other("de".into()),
            })
            .unwrap();

        assert_eq!(accumulator.retained_bytes, 5);
        assert_eq!(
            accumulator.stop_reason,
            Some(StopReason::Other("de".into()))
        );
        assert!(accumulator.done);
    }

    #[test]
    fn fixed_stop_reason_does_not_consume_variable_byte_budget() {
        let mut accumulator = started_accumulator(3, 4);
        accumulator.push(StreamEvent::TextStart).unwrap();
        accumulator
            .push(StreamEvent::TextDelta { text: "abc".into() })
            .unwrap();
        accumulator.push(StreamEvent::TextEnd).unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .unwrap();

        assert_eq!(accumulator.retained_bytes, 3);
        assert_eq!(accumulator.stop_reason, Some(StopReason::Stop));
        assert!(accumulator.done);
    }

    fn anthropic_continuation(reference: ContinuationRef) -> ProviderContinuation {
        ProviderContinuation::AnthropicMessages(
            crate::AnthropicMessagesContinuation::with_scope(
                crate::ProviderId::new("anthropic").unwrap(),
                "claude-test",
                crate::ContinuationScope::empty(),
                vec![crate::AnthropicReasoningReplayEntry::new(
                    reference,
                    crate::AnthropicReasoningReplay::thinking("signature-secret"),
                )],
            )
            .unwrap(),
        )
    }

    #[test]
    fn provider_bound_reasoning_requires_a_completed_reference_and_sidecar() {
        let mut accumulator = started_accumulator(128, 8);
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        accumulator
            .push(StreamEvent::ReasoningSignature {
                signature: "signature".into(),
            })
            .unwrap();
        accumulator.push(StreamEvent::ReasoningEnd).unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .unwrap();

        let error = accumulator.finish().unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Protocol);
        assert!(error.message.contains("continuation sidecar"));
    }

    #[test]
    fn provider_bound_stream_reference_is_preserved_and_validated() {
        let reference = ContinuationRef::new("stream-reasoning").unwrap();
        let mut accumulator = started_accumulator(128, 8);
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        accumulator
            .push(StreamEvent::ReasoningReference {
                reference: reference.clone(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::ReasoningDelta {
                text: "plan".into(),
            })
            .unwrap();
        accumulator
            .push(StreamEvent::ReasoningSignature {
                signature: "signature".into(),
            })
            .unwrap();
        accumulator.push(StreamEvent::ReasoningEnd).unwrap();
        accumulator
            .push(StreamEvent::Continuation(anthropic_continuation(
                reference.clone(),
            )))
            .unwrap();
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .unwrap();

        let completion = accumulator.finish().unwrap();
        let AssistantContent::Reasoning(reasoning) = &completion.message.content[0] else {
            panic!("expected reasoning content")
        };
        assert_eq!(reasoning.continuation_ref.as_ref(), Some(&reference));
        assert_eq!(reasoning.portability, ReasoningPortability::ProviderBound);
    }

    #[test]
    fn stream_rejects_duplicate_reasoning_reference_across_blocks() {
        let reference = ContinuationRef::new("duplicate-ref").unwrap();
        let mut accumulator = started_accumulator(128, 8);
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        accumulator
            .push(StreamEvent::ReasoningReference {
                reference: reference.clone(),
            })
            .unwrap();
        accumulator.push(StreamEvent::ReasoningEnd).unwrap();
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        let error = accumulator
            .push(StreamEvent::ReasoningReference { reference })
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Protocol);
        assert!(error.message.contains("duplicate"));
    }

    #[test]
    fn interrupted_reasoning_partial_message_has_no_replay_reference() {
        let reference = ContinuationRef::new("interrupted-ref").unwrap();
        let mut accumulator = started_accumulator(128, 8);
        accumulator.push(StreamEvent::ReasoningStart).unwrap();
        accumulator
            .push(StreamEvent::ReasoningReference { reference })
            .unwrap();
        accumulator
            .push(StreamEvent::ReasoningDelta {
                text: "partial".into(),
            })
            .unwrap();
        let message = accumulator.partial_message();
        let AssistantContent::Reasoning(reasoning) = &message.content[0] else {
            panic!("expected partial reasoning content")
        };
        assert_eq!(reasoning.portability, ReasoningPortability::Portable);
        assert!(reasoning.continuation_ref.is_none());
    }

    #[test]
    fn stream_continuation_must_precede_done_and_debug_is_redacted() {
        let secret = "signature-secret";
        let reference = ContinuationRef::new("safe-ref").unwrap();
        let event = StreamEvent::ReasoningSignature {
            signature: secret.into(),
        };
        assert!(!format!("{event:?}").contains(secret));
        let continuation = anthropic_continuation(reference);
        assert!(!format!("{continuation:?}").contains(secret));

        let mut accumulator = started_accumulator(128, 8);
        accumulator
            .push(StreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .unwrap();
        let error = accumulator
            .push(StreamEvent::Continuation(continuation))
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Protocol);
        assert!(error.message.contains("after terminal Done"));
    }
}
