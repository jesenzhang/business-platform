use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::types::{Api, AssistantContent, AssistantMessage, Message, ReasoningPortability};

/// Stable, provider-neutral identity for one provider-native replay fact.
///
/// The reference is deliberately separate from signatures, encrypted payloads,
/// provider output ids, and collection positions. It is safe to persist and
/// log, but carries no provider-native payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContinuationRef(String);

impl ContinuationRef {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("continuation reference must not be empty".into());
        }
        if value.len() > 256 {
            return Err("continuation reference exceeds the 256-byte limit".into());
        }
        Ok(Self(value))
    }

    pub fn generated() -> Self {
        Self(format!("cr-{}", Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContinuationRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Domain separator for the conversation history identity chain. The chain
/// digest must never collide with digests computed for unrelated purposes.
const HISTORY_IDENTITY_DOMAIN: &str = "jarvis-model-provider/history-message/v1";

/// Sequence-sensitive identity for one occurrence in normalized history.
///
/// The identity is a domain-separated chain digest:
/// `id(n) = Hash(domain, id(n-1), canonical(message(n)))` over the
/// non-System conversation prefix ending at that message. Identical content
/// at different positions therefore receives different identities, and any
/// earlier edit, insert, delete, or reorder changes every downstream
/// identity. The digest derives only from safe normalized content, never
/// from provider-native payloads, and is deterministic across serialization
/// and recovery.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HistoryMessageId(String);

impl HistoryMessageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn unbound() -> Self {
        // Intentionally never equals a digest computed from real history, so
        // compatibility-built continuations cannot bind to any message.
        Self("hm:unbound".into())
    }
}

/// Compute one chain identity from the previous conversation identity and
/// the canonical normalized message. Length prefixes keep the three fields
/// unambiguously separated.
fn chained_history_identity(
    previous: Option<&HistoryMessageId>,
    message: &Message,
) -> Result<HistoryMessageId, String> {
    let message_bytes = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update((HISTORY_IDENTITY_DOMAIN.len() as u64).to_le_bytes());
    hasher.update(HISTORY_IDENTITY_DOMAIN.as_bytes());
    match previous {
        None => hasher.update(0u64.to_le_bytes()),
        Some(previous) => {
            let previous = previous.0.as_bytes();
            hasher.update((previous.len() as u64).to_le_bytes());
            hasher.update(previous);
        }
    }
    hasher.update((message_bytes.len() as u64).to_le_bytes());
    hasher.update(&message_bytes);
    Ok(HistoryMessageId(format!("hm:{:x}", hasher.finalize())))
}

/// Explicit boundary describing the conversation prefix represented by a
/// continuation. System messages are request-level instructions and are not
/// part of this conversation coverage boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContinuationScope {
    covered_message_count: usize,
    covered_prefix_digest: String,
    covered_through: Option<HistoryMessageId>,
}

impl ContinuationScope {
    pub fn for_history(messages: &[Message]) -> Result<Self, String> {
        let ids = history_message_ids(messages)?;
        Self::from_ids(&ids)
    }

    pub fn empty() -> Self {
        Self::from_ids(&[]).expect("empty history digest is infallible")
    }

    pub fn covered_message_count(&self) -> usize {
        self.covered_message_count
    }

    pub fn covered_prefix_digest(&self) -> &str {
        &self.covered_prefix_digest
    }

    pub fn covered_through(&self) -> Option<&HistoryMessageId> {
        self.covered_through.as_ref()
    }

    /// Validate that the current history still has the exact covered prefix.
    /// Returns the validated prefix length; callers must use that boundary
    /// rather than inferring it from the current message count.
    pub fn validate_history(&self, messages: &[Message]) -> Result<usize, String> {
        let ids = history_message_ids(messages)?;
        if ids.len() < self.covered_message_count {
            return Err("continuation history is shorter than its covered boundary".into());
        }
        let prefix = &ids[..self.covered_message_count];
        let expected = Self::digest_ids(prefix)?;
        if expected != self.covered_prefix_digest {
            return Err("continuation covered history prefix no longer matches".into());
        }
        if self.covered_through.as_ref() != prefix.last() {
            return Err("continuation covered history boundary identity does not match".into());
        }
        Ok(self.covered_message_count)
    }

    /// Return only the uncovered conversation suffix after validating the
    /// explicit boundary. System messages are intentionally omitted.
    pub fn uncovered_history(&self, messages: &[Message]) -> Result<Vec<Message>, String> {
        let covered = self.validate_history(messages)?;
        Ok(messages
            .iter()
            .filter(|message| !matches!(message, Message::System { .. }))
            .skip(covered)
            .cloned()
            .collect())
    }

    fn from_ids(ids: &[HistoryMessageId]) -> Result<Self, String> {
        Ok(Self {
            covered_message_count: ids.len(),
            covered_prefix_digest: Self::digest_ids(ids)?,
            covered_through: ids.last().cloned(),
        })
    }

    fn digest_ids(ids: &[HistoryMessageId]) -> Result<String, String> {
        let bytes = serde_json::to_vec(ids).map_err(|error| error.to_string())?;
        Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
    }
}

/// Return sequence-sensitive chain identities for conversation messages.
/// System/instruction messages are excluded because they are request-level
/// configuration, not inherited conversation content for continuation
/// purposes. The chain runs over the non-System messages in order, so the
/// identity of each entry depends on the exact preceding prefix.
///
/// This function is the single authoritative source of history identity.
/// Replay validation, wire encoding, and completion decoding must all derive
/// anchors through it (or through [`ContinuationScope`], which wraps it);
/// computing message digests independently elsewhere is not supported.
pub fn history_message_ids(messages: &[Message]) -> Result<Vec<HistoryMessageId>, String> {
    let mut ids = Vec::with_capacity(messages.len());
    for message in messages {
        if matches!(message, Message::System { .. }) {
            continue;
        }
        ids.push(chained_history_identity(ids.last(), message)?);
    }
    Ok(ids)
}

/// Compute the authoritative anchor for a new assistant output produced on
/// top of a request history. The anchor is the chain identity of that
/// assistant as the final entry of `history + [assistant]`, so identical
/// content produced at different conversation points binds to different
/// segments.
pub(crate) fn assistant_history_anchor(
    history: &[Message],
    assistant: &AssistantMessage,
) -> Result<HistoryMessageId, String> {
    let mut last: Option<HistoryMessageId> = None;
    for message in history {
        if matches!(message, Message::System { .. }) {
            continue;
        }
        last = Some(chained_history_identity(last.as_ref(), message)?);
    }
    chained_history_identity(last.as_ref(), &Message::Assistant(assistant.clone()))
}

/// Provider-native replay state for one Anthropic reasoning block.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnthropicReasoningReplay {
    Thinking { signature: String },
    Redacted { data: String },
}

impl fmt::Debug for AnthropicReasoningReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("AnthropicReasoningReplay");
        match self {
            Self::Thinking { signature } => {
                debug.field("kind", &"thinking");
                debug.field("sensitive_bytes", &signature.len());
            }
            Self::Redacted { data } => {
                debug.field("kind", &"redacted");
                debug.field("sensitive_bytes", &data.len());
            }
        }
        debug.finish()
    }
}

impl AnthropicReasoningReplay {
    pub fn thinking(signature: impl Into<String>) -> Self {
        Self::Thinking {
            signature: signature.into(),
        }
    }

    pub fn redacted(data: impl Into<String>) -> Self {
        Self::Redacted { data: data.into() }
    }

    pub fn is_redacted(&self) -> bool {
        matches!(self, Self::Redacted { .. })
    }

    pub(crate) fn sensitive_len(&self) -> usize {
        match self {
            Self::Thinking { signature } => signature.len(),
            Self::Redacted { data } => data.len(),
        }
    }
}

/// Reference-bound Anthropic replay state. Entry ordering is not used for
/// association; it is only serialization order.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicReasoningReplayEntry {
    reference: ContinuationRef,
    state: AnthropicReasoningReplay,
}

impl fmt::Debug for AnthropicReasoningReplayEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicReasoningReplayEntry")
            .field("reference", &self.reference)
            .field("state", &self.state)
            .finish()
    }
}

impl AnthropicReasoningReplayEntry {
    pub fn new(reference: ContinuationRef, state: AnthropicReasoningReplay) -> Self {
        Self { reference, state }
    }

    pub fn reference(&self) -> &ContinuationRef {
        &self.reference
    }

    pub(crate) fn state(&self) -> &AnthropicReasoningReplay {
        &self.state
    }
}

/// Provider-bound Anthropic Messages reasoning replay state.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnthropicMessagesContinuation {
    provider: crate::types::ProviderId,
    model: String,
    scope: ContinuationScope,
    reasoning: Vec<AnthropicReasoningReplayEntry>,
    durability: crate::types::ContinuationDurability,
}

impl fmt::Debug for AnthropicMessagesContinuation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicMessagesContinuation")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("scope", &self.scope)
            .field("reasoning_entries", &self.reasoning.len())
            .field(
                "sensitive_bytes",
                &self
                    .reasoning
                    .iter()
                    .map(|entry| entry.state.sensitive_len())
                    .sum::<usize>(),
            )
            .field("durability", &self.durability)
            .finish()
    }
}

impl AnthropicMessagesContinuation {
    /// Compatibility constructor for callers that have not yet bound
    /// normalized reasoning refs. The generated refs and empty scope are
    /// intentionally not replayable against non-empty history; callers that
    /// need exact replay must use [`Self::with_scope`].
    pub fn new(
        provider: crate::types::ProviderId,
        model: impl Into<String>,
        reasoning: Vec<AnthropicReasoningReplay>,
    ) -> Result<Self, String> {
        let entries = reasoning
            .into_iter()
            .map(|state| AnthropicReasoningReplayEntry::new(ContinuationRef::generated(), state))
            .collect();
        Self::with_scope(provider, model, ContinuationScope::empty(), entries)
    }

    pub fn with_scope(
        provider: crate::types::ProviderId,
        model: impl Into<String>,
        scope: ContinuationScope,
        reasoning: Vec<AnthropicReasoningReplayEntry>,
    ) -> Result<Self, String> {
        let model = model.into();
        if model.trim().is_empty() {
            return Err("Anthropic continuation model must not be empty".into());
        }
        if reasoning.is_empty() {
            return Err("Anthropic continuation has no reasoning replay state".into());
        }
        let durability = if reasoning.iter().any(|entry| entry.state.is_redacted()) {
            crate::types::ContinuationDurability::SensitiveNonDurable
        } else {
            crate::types::ContinuationDurability::ProviderBound
        };
        let continuation = Self {
            provider,
            model,
            scope,
            reasoning,
            durability,
        };
        continuation.validate()?;
        Ok(continuation)
    }

    pub fn provider(&self) -> &crate::types::ProviderId {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn scope(&self) -> &ContinuationScope {
        &self.scope
    }

    pub fn durability(&self) -> crate::types::ContinuationDurability {
        self.durability
    }

    pub fn reasoning_entry_count(&self) -> usize {
        self.reasoning.len()
    }

    pub fn reasoning_entries(&self) -> &[AnthropicReasoningReplayEntry] {
        &self.reasoning
    }

    pub(crate) fn replay_for(
        &self,
        reference: &ContinuationRef,
    ) -> Option<&AnthropicReasoningReplay> {
        self.reasoning
            .iter()
            .find(|entry| entry.reference == *reference)
            .map(AnthropicReasoningReplayEntry::state)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("Anthropic continuation model must not be empty".into());
        }
        if self.reasoning.is_empty() {
            return Err("Anthropic continuation has no reasoning replay state".into());
        }
        let mut references = HashSet::new();
        for entry in &self.reasoning {
            if !references.insert(entry.reference.clone()) {
                return Err("Anthropic continuation has duplicate reasoning references".into());
            }
            match entry.state() {
                AnthropicReasoningReplay::Thinking { signature } if signature.trim().is_empty() => {
                    return Err("Anthropic thinking signature must not be empty".into());
                }
                AnthropicReasoningReplay::Redacted { data } if data.trim().is_empty() => {
                    return Err("Anthropic redacted reasoning data must not be empty".into());
                }
                _ => {}
            }
        }
        let expected = if self.reasoning.iter().any(|entry| entry.state.is_redacted()) {
            crate::types::ContinuationDurability::SensitiveNonDurable
        } else {
            crate::types::ContinuationDurability::ProviderBound
        };
        if self.durability != expected {
            return Err("Anthropic continuation durability does not match its state".into());
        }
        Ok(())
    }
}

/// Explicit Responses replay mode. Stateful mode delegates covered
/// conversation state to `previous_response_id`; stateless mode reconstructs
/// the typed replay prefix locally.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OpenAiResponsesContinuationMode {
    Stateful,
    Stateless,
}

/// Minimum typed provider-native output-item state needed for stateless
/// Responses replay. This intentionally is not a mirror of the Responses
/// JSON schema.
///
/// The segment holding these items is the complete ordered projection of one
/// provider output: reasoning items appear whether or not they carry
/// provider-native encrypted state, so portable summary reasoning survives
/// stateless manual replay. `encrypted_content` distinguishes encrypted
/// (provider-bound, sensitive) reasoning from portable reasoning; the
/// continuation reference is required for encrypted items and absent for
/// portable ones.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenAiResponsesReplayItem {
    Reasoning {
        /// Present only for encrypted (provider-bound) reasoning.
        reference: Option<ContinuationRef>,
        item_id: Option<String>,
        /// Present only for encrypted reasoning; absent for portable
        /// reasoning summaries.
        encrypted_content: Option<String>,
        #[serde(default)]
        summary: Vec<String>,
    },
    AssistantMessage {
        reference: ContinuationRef,
        item_id: Option<String>,
        phase: Option<String>,
        text: String,
    },
    FunctionCall {
        reference: ContinuationRef,
        item_id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
}

impl fmt::Debug for OpenAiResponsesReplayItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("OpenAiResponsesReplayItem");
        match self {
            Self::Reasoning {
                reference,
                item_id,
                encrypted_content,
                summary,
            } => {
                debug.field("kind", &"reasoning");
                debug.field("reference", reference);
                debug.field("item_id", item_id);
                debug.field("encrypted_present", &encrypted_content.is_some());
                debug.field("summary_items", &summary.len());
                debug.field(
                    "sensitive_bytes",
                    &(encrypted_content.as_deref().map_or(0, str::len)
                        + summary.iter().map(String::len).sum::<usize>()),
                );
            }
            Self::AssistantMessage {
                reference,
                item_id,
                phase,
                text,
            } => {
                debug.field("kind", &"assistant_message");
                debug.field("reference", reference);
                debug.field("item_id", item_id);
                debug.field("phase", phase);
                debug.field("sensitive_bytes", &text.len());
            }
            Self::FunctionCall {
                reference,
                item_id,
                call_id,
                name,
                arguments,
            } => {
                debug.field("kind", &"function_call");
                debug.field("reference", reference);
                debug.field("item_id", item_id);
                debug.field("call_id", call_id);
                debug.field("name", name);
                debug.field("sensitive_bytes", &arguments.len());
            }
        }
        debug.finish()
    }
}

impl OpenAiResponsesReplayItem {
    /// Encrypted (provider-bound) reasoning replay item. The continuation
    /// reference and encrypted payload are both required; the payload stays
    /// in the sidecar under `SensitiveNonDurable` implications.
    pub fn reasoning(
        reference: ContinuationRef,
        item_id: Option<String>,
        encrypted_content: impl Into<String>,
        summary: Vec<String>,
    ) -> Self {
        Self::Reasoning {
            reference: Some(reference),
            item_id,
            encrypted_content: Some(encrypted_content.into()),
            summary,
        }
    }

    /// Portable reasoning summary replay item. No provider-native secret
    /// state exists, so no continuation reference is carried.
    pub fn portable_reasoning(summary: Vec<String>, item_id: Option<String>) -> Self {
        Self::Reasoning {
            reference: None,
            item_id,
            encrypted_content: None,
            summary,
        }
    }

    pub fn assistant_message(
        reference: ContinuationRef,
        item_id: Option<String>,
        phase: Option<String>,
        text: impl Into<String>,
    ) -> Self {
        Self::AssistantMessage {
            reference,
            item_id,
            phase,
            text: text.into(),
        }
    }

    pub fn function_call(
        reference: ContinuationRef,
        item_id: Option<String>,
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self::FunctionCall {
            reference,
            item_id,
            call_id: call_id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }

    /// The provider-bound continuation reference, present only when this
    /// item carries sensitive native state (encrypted reasoning).
    pub fn reference(&self) -> Option<&ContinuationRef> {
        match self {
            Self::Reasoning { reference, .. } => reference.as_ref(),
            Self::AssistantMessage { reference, .. } | Self::FunctionCall { reference, .. } => {
                Some(reference)
            }
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Reasoning { .. } => "reasoning",
            Self::AssistantMessage { .. } => "assistant_message",
            Self::FunctionCall { .. } => "function_call",
        }
    }

    /// Whether this reasoning item carries provider-native encrypted state.
    pub fn is_encrypted_reasoning(&self) -> bool {
        matches!(
            self,
            Self::Reasoning {
                encrypted_content: Some(_),
                ..
            }
        )
    }

    pub(crate) fn sensitive_len(&self) -> usize {
        match self {
            Self::Reasoning {
                encrypted_content,
                summary,
                ..
            } => {
                encrypted_content.as_deref().map_or(0, str::len)
                    + summary.iter().map(String::len).sum::<usize>()
            }
            Self::AssistantMessage { text, .. } => text.len(),
            Self::FunctionCall { arguments, .. } => arguments.len(),
        }
    }
}

/// One assistant history entry's worth of ordered provider-native replay
/// items. The anchor is the stable [`HistoryMessageId`] of the normalized
/// assistant message the items reproduce; association is by anchor identity,
/// never by collection position. Item order preserves provider output order.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenAiResponsesReplaySegment {
    anchor: HistoryMessageId,
    items: Vec<OpenAiResponsesReplayItem>,
}

impl fmt::Debug for OpenAiResponsesReplaySegment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesReplaySegment")
            .field("anchor", &self.anchor)
            .field("items", &self.items.len())
            .field(
                "sensitive_bytes",
                &self
                    .items
                    .iter()
                    .map(|item| item.sensitive_len())
                    .sum::<usize>(),
            )
            .finish()
    }
}

impl OpenAiResponsesReplaySegment {
    pub fn new(anchor: HistoryMessageId, items: Vec<OpenAiResponsesReplayItem>) -> Self {
        Self { anchor, items }
    }

    pub fn anchor(&self) -> &HistoryMessageId {
        &self.anchor
    }

    pub fn items(&self) -> &[OpenAiResponsesReplayItem] {
        &self.items
    }
}

/// Provider-native continuation state for the OpenAI Responses protocol.
///
/// Stateful mode means the provider retains a verified conversation prefix:
/// `previous_response_id` names that server-side coverage, and only the
/// uncovered suffix is transmitted. Stateless mode means the provider retains
/// nothing: every required historical user/tool input stays in the wire input,
/// and provider-native assistant outputs are substituted through anchored
/// replay segments.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenAiResponsesContinuation {
    provider: crate::types::ProviderId,
    model: String,
    response_id: Option<String>,
    mode: OpenAiResponsesContinuationMode,
    scope: ContinuationScope,
    /// Stateful mode keeps no segments (server coverage applies); stateless
    /// mode keeps one anchored segment per covered assistant output.
    replay_segments: Vec<OpenAiResponsesReplaySegment>,
    durability: crate::types::ContinuationDurability,
}

impl fmt::Debug for OpenAiResponsesContinuation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesContinuation")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("response_id_present", &self.response_id.is_some())
            .field("mode", &self.mode)
            .field("scope", &self.scope)
            .field(
                "replay_segments",
                &(self.replay_segments.len(), self.replay_item_count()),
            )
            .field(
                "sensitive_bytes",
                &self
                    .replay_segments
                    .iter()
                    .map(|segment| {
                        segment
                            .items
                            .iter()
                            .map(OpenAiResponsesReplayItem::sensitive_len)
                            .sum::<usize>()
                    })
                    .sum::<usize>(),
            )
            .field("durability", &self.durability)
            .finish()
    }
}

impl OpenAiResponsesContinuation {
    /// Compatibility constructor for old encrypted-reasoning-only callers.
    /// The generated refs and empty scope intentionally cannot replay a
    /// non-empty history; exact replay uses [`Self::with_replay`].
    pub fn new(
        provider: crate::types::ProviderId,
        model: impl Into<String>,
        response_id: Option<String>,
        encrypted_reasoning: Vec<String>,
        stateful: bool,
    ) -> Result<Self, String> {
        let replay_items = encrypted_reasoning
            .into_iter()
            .map(|data| {
                OpenAiResponsesReplayItem::reasoning(
                    ContinuationRef::generated(),
                    None,
                    data,
                    Vec::new(),
                )
            })
            .collect();
        Self::with_replay(
            provider,
            model,
            response_id,
            if stateful {
                OpenAiResponsesContinuationMode::Stateful
            } else {
                OpenAiResponsesContinuationMode::Stateless
            },
            ContinuationScope::empty(),
            replay_items,
        )
    }

    /// Compatibility constructor that accepts the pre-M7.1 flat item list.
    /// Items are stored as one unanchored legacy segment; such a continuation
    /// can never satisfy exact anchored replay of a non-empty history, so
    /// production construction uses [`Self::with_segments`].
    pub fn with_replay(
        provider: crate::types::ProviderId,
        model: impl Into<String>,
        response_id: Option<String>,
        mode: OpenAiResponsesContinuationMode,
        scope: ContinuationScope,
        replay_items: Vec<OpenAiResponsesReplayItem>,
    ) -> Result<Self, String> {
        let anchor = HistoryMessageId::unbound();
        let segments = if replay_items.is_empty() {
            Vec::new()
        } else {
            vec![OpenAiResponsesReplaySegment::new(anchor, replay_items)]
        };
        Self::with_segments(provider, model, response_id, mode, scope, segments)
    }

    /// Production constructor. Stateful continuations keep server coverage in
    /// `scope` and carry no segments; stateless continuations keep one
    /// anchored segment per covered assistant output and an empty scope.
    pub fn with_segments(
        provider: crate::types::ProviderId,
        model: impl Into<String>,
        response_id: Option<String>,
        mode: OpenAiResponsesContinuationMode,
        scope: ContinuationScope,
        replay_segments: Vec<OpenAiResponsesReplaySegment>,
    ) -> Result<Self, String> {
        let model = model.into();
        let durability = match mode {
            OpenAiResponsesContinuationMode::Stateful => {
                crate::types::ContinuationDurability::ProviderBound
            }
            OpenAiResponsesContinuationMode::Stateless => {
                crate::types::ContinuationDurability::SensitiveNonDurable
            }
        };
        let continuation = Self {
            provider,
            model,
            response_id,
            mode,
            scope,
            replay_segments,
            durability,
        };
        continuation.validate()?;
        Ok(continuation)
    }

    pub fn provider(&self) -> &crate::types::ProviderId {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn response_id(&self) -> Option<&str> {
        self.response_id.as_deref()
    }

    pub fn mode(&self) -> OpenAiResponsesContinuationMode {
        self.mode
    }

    pub fn scope(&self) -> &ContinuationScope {
        &self.scope
    }

    pub fn durability(&self) -> crate::types::ContinuationDurability {
        self.durability
    }

    pub fn replay_item_count(&self) -> usize {
        self.replay_segments
            .iter()
            .map(|segment| segment.items.len())
            .sum()
    }

    pub fn replay_segment_count(&self) -> usize {
        self.replay_segments.len()
    }

    /// Anchored stateless replay segments. Association is by segment anchor;
    /// storage order is serialization order only.
    pub fn replay_segments(&self) -> &[OpenAiResponsesReplaySegment] {
        &self.replay_segments
    }

    pub fn previous_response_id(&self) -> Option<&str> {
        (self.mode == OpenAiResponsesContinuationMode::Stateful)
            .then_some(self.response_id.as_deref())
            .flatten()
    }

    pub(crate) fn replay_items(&self) -> Vec<&OpenAiResponsesReplayItem> {
        self.replay_segments
            .iter()
            .flat_map(|segment| segment.items.iter())
            .collect()
    }

    /// Resolve the exact replay items bound to one assistant history entry.
    /// Returns `None` when no segment carries that anchor; association is by
    /// anchor identity only.
    pub(crate) fn segment_for_anchor(
        &self,
        anchor: &HistoryMessageId,
    ) -> Option<&OpenAiResponsesReplaySegment> {
        self.replay_segments
            .iter()
            .find(|segment| &segment.anchor == anchor)
    }

    /// Stateless wire substitution for one request-history assistant message.
    ///
    /// Returns the provider-native replay items when an exact segment is
    /// bound to that message. A stateless continuation whose history contains
    /// a `ProviderBound` reasoning block with no matching segment fails
    /// closed: the encrypted reasoning could not be reproduced and silently
    /// downgrading it would change what the provider believes happened.
    pub(crate) fn replay_segment_for_message(
        &self,
        messages: &[Message],
        index: usize,
    ) -> Result<Option<Vec<OpenAiResponsesReplayItem>>, String> {
        if self.mode != OpenAiResponsesContinuationMode::Stateless {
            return Ok(None);
        }
        let Message::Assistant(assistant) = &messages[index] else {
            return Ok(None);
        };
        // The wire encoder consumes identities from the authoritative chain,
        // so a per-message digest here observes exactly the same prefix the
        // validator saw.
        let ids = history_message_ids(&messages[..=index])?;
        let anchor = ids.last().ok_or_else(|| {
            "stateless Responses replay requires at least one conversation message".to_string()
        })?;
        match self.segment_for_anchor(anchor) {
            Some(segment) => Ok(Some(segment.items.clone())),
            None => {
                let requires_replay = assistant.content.iter().any(|part| {
                    matches!(
                        part,
                        AssistantContent::Reasoning(value)
                            if value.portability == ReasoningPortability::ProviderBound
                    )
                });
                if requires_replay {
                    return Err(format!(
                        "stateless Responses replay has no segment for the provider-bound assistant message at conversation position {}",
                        ids.len() - 1
                    ));
                }
                Ok(None)
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("Responses continuation model must not be empty".into());
        }
        if self
            .response_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err("Responses continuation response id must not be empty".into());
        }
        let mut references = HashSet::new();
        for segment in &self.replay_segments {
            for item in &segment.items {
                match item.reference() {
                    Some(reference) => {
                        if !references.insert(reference.clone()) {
                            return Err(
                                "Responses continuation has duplicate replay references".into()
                            );
                        }
                    }
                    None => {
                        // Only portable reasoning may lack a provider-bound
                        // reference; every other kind binds one.
                        if !matches!(item, OpenAiResponsesReplayItem::Reasoning { .. } if !item.is_encrypted_reasoning())
                        {
                            return Err(
                                "Responses replay item is missing its continuation reference"
                                    .into(),
                            );
                        }
                    }
                }
                match item {
                    OpenAiResponsesReplayItem::Reasoning {
                        reference,
                        encrypted_content,
                        item_id,
                        ..
                    } => {
                        let encrypted = encrypted_content.as_deref().unwrap_or_default();
                        if !encrypted_content.is_some() && reference.is_some() {
                            return Err(
                                "portable Responses reasoning must not carry a continuation reference"
                                    .into(),
                            );
                        }
                        if encrypted_content.is_some() && encrypted.trim().is_empty() {
                            return Err(
                                "Responses reasoning encrypted content must not be empty".into()
                            );
                        }
                        if item_id.as_deref().is_some_and(|value| value.is_empty()) {
                            return Err("Responses reasoning item id must not be empty".into());
                        }
                    }
                    OpenAiResponsesReplayItem::AssistantMessage { item_id, .. } => {
                        if item_id.as_deref().is_some_and(|value| value.is_empty()) {
                            return Err("Responses assistant item id must not be empty".into());
                        }
                    }
                    OpenAiResponsesReplayItem::FunctionCall {
                        item_id,
                        call_id,
                        name,
                        arguments,
                        ..
                    } => {
                        if item_id.as_deref().is_some_and(|value| value.is_empty())
                            || call_id.trim().is_empty()
                            || name.trim().is_empty()
                            || arguments.trim().is_empty()
                        {
                            return Err(
                                "Responses function-call replay identity is incomplete".into()
                            );
                        }
                    }
                }
            }
        }
        match self.mode {
            OpenAiResponsesContinuationMode::Stateful => {
                if self.response_id.is_none() {
                    return Err("stateful Responses continuation requires a response id".into());
                }
                if !self.replay_segments.is_empty() {
                    return Err(
                        "stateful Responses continuation must not carry replay segments".into(),
                    );
                }
            }
            OpenAiResponsesContinuationMode::Stateless => {
                if self.response_id.is_some() {
                    return Err(
                        "stateless Responses continuation must not retain a response id".into(),
                    );
                }
                if self.scope.covered_message_count() != 0 || self.scope.covered_through().is_some()
                {
                    return Err(
                        "stateless Responses continuation must not claim server coverage".into(),
                    );
                }
                if self.replay_segments.is_empty() {
                    return Err("stateless Responses continuation has no replay state".into());
                }
                let mut anchors = HashSet::new();
                for segment in &self.replay_segments {
                    if segment.items.is_empty() {
                        return Err("Responses replay segment has no items".into());
                    }
                    if !anchors.insert(segment.anchor.clone()) {
                        return Err("Responses continuation has duplicate replay anchors".into());
                    }
                }
            }
        }
        let expected = match self.mode {
            OpenAiResponsesContinuationMode::Stateful => {
                crate::types::ContinuationDurability::ProviderBound
            }
            OpenAiResponsesContinuationMode::Stateless => {
                crate::types::ContinuationDurability::SensitiveNonDurable
            }
        };
        if self.durability != expected {
            return Err("Responses continuation durability does not match its mode".into());
        }
        Ok(())
    }
}

/// Deterministic provider-neutral projection of one stateless replay
/// segment onto the normalized assistant shape it must reproduce. The
/// projection ignores provider-native-only fields (`item_id`,
/// `encrypted_content`) and compares only normalized semantics: reasoning
/// summary text with its redaction/portability class, tool-call
/// identity/name/arguments, and assistant text, in provider output order.
///
/// A segment whose projection differs from its anchored history assistant
/// means the sidecar no longer describes that output; validation fails
/// closed instead of replaying foreign provider state.
fn responses_replay_projection_mismatches(
    items: &[OpenAiResponsesReplayItem],
    assistant: &AssistantMessage,
) -> Vec<String> {
    #[derive(PartialEq)]
    enum ProjectedPart {
        Reasoning {
            text: String,
            redacted: bool,
            provider_bound: bool,
        },
        ToolCall {
            id: String,
            name: String,
            arguments: String,
        },
        Text(String),
    }
    let project = |items: &[OpenAiResponsesReplayItem]| -> Vec<ProjectedPart> {
        items
            .iter()
            .map(|item| match item {
                OpenAiResponsesReplayItem::Reasoning {
                    encrypted_content,
                    summary,
                    ..
                } => ProjectedPart::Reasoning {
                    text: summary.join(""),
                    redacted: encrypted_content.is_some(),
                    provider_bound: true,
                },
                OpenAiResponsesReplayItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                    ..
                } => ProjectedPart::ToolCall {
                    id: call_id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                OpenAiResponsesReplayItem::AssistantMessage { text, .. } => {
                    ProjectedPart::Text(text.clone())
                }
            })
            .collect()
    };
    // Normalized reasoning text for a redacted block is an empty marker by
    // contract; the projection therefore compares the same way.
    let normalize_reasoning = |parts: Vec<ProjectedPart>| -> Vec<ProjectedPart> {
        parts
            .into_iter()
            .map(|part| match part {
                ProjectedPart::Reasoning {
                    text,
                    redacted,
                    provider_bound,
                } => ProjectedPart::Reasoning {
                    text: if redacted { String::new() } else { text },
                    redacted,
                    provider_bound,
                },
                other => other,
            })
            .collect()
    };
    let projected = normalize_reasoning(project(items));
    let mut actual = Vec::with_capacity(assistant.content.len());
    for part in &assistant.content {
        match part {
            AssistantContent::Text(value) => actual.push(ProjectedPart::Text(value.text.clone())),
            AssistantContent::ToolCall(call) => actual.push(ProjectedPart::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.to_string(),
            }),
            AssistantContent::Reasoning(reasoning) => actual.push(ProjectedPart::Reasoning {
                text: if reasoning.redacted {
                    String::new()
                } else {
                    reasoning.text.clone()
                },
                redacted: reasoning.redacted,
                provider_bound: reasoning.portability == ReasoningPortability::ProviderBound,
            }),
        }
    }
    if projected.len() != actual.len() {
        return vec![format!(
            "replay segment projects {} content parts but the anchored assistant has {}",
            projected.len(),
            actual.len()
        )];
    }
    let mut mismatches = Vec::new();
    for (position, (expected, found)) in projected.iter().zip(&actual).enumerate() {
        let mismatch = match (expected, found) {
            (
                ProjectedPart::Reasoning {
                    text: expected_text,
                    redacted: expected_redacted,
                    ..
                },
                ProjectedPart::Reasoning {
                    text: found_text,
                    redacted: found_redacted,
                    ..
                },
            ) => expected_text != found_text || expected_redacted != found_redacted,
            (
                ProjectedPart::ToolCall {
                    id: expected_id,
                    name: expected_name,
                    arguments: expected_arguments,
                },
                ProjectedPart::ToolCall {
                    id: found_id,
                    name: found_name,
                    arguments: found_arguments,
                },
            ) => {
                // Arguments are compared as canonical JSON so key order and
                // whitespace cannot fail a semantically equal payload.
                let arguments_match = serde_json::from_str::<serde_json::Value>(expected_arguments)
                    .ok()
                    .zip(serde_json::from_str::<serde_json::Value>(found_arguments).ok())
                    .is_some_and(|(a, b)| a == b)
                    || expected_arguments == found_arguments;
                expected_id != found_id || expected_name != found_name || !arguments_match
            }
            (ProjectedPart::Text(expected_text), ProjectedPart::Text(found_text)) => {
                expected_text != found_text
            }
            _ => true,
        };
        if mismatch {
            mismatches.push(format!(
                "replay item {position} does not project onto the anchored assistant"
            ));
        }
    }
    mismatches
}

/// Typed provider continuation sidecar. Additional protocols can add a new
/// variant without changing normalized message content.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProviderContinuation {
    AnthropicMessages(AnthropicMessagesContinuation),
    OpenAiResponses(OpenAiResponsesContinuation),
}

impl fmt::Debug for ProviderContinuation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnthropicMessages(value) => formatter
                .debug_tuple("AnthropicMessages")
                .field(value)
                .finish(),
            Self::OpenAiResponses(value) => formatter
                .debug_tuple("OpenAiResponses")
                .field(value)
                .finish(),
        }
    }
}

impl ProviderContinuation {
    pub fn provider(&self) -> &crate::types::ProviderId {
        match self {
            Self::AnthropicMessages(value) => value.provider(),
            Self::OpenAiResponses(value) => value.provider(),
        }
    }

    pub fn api(&self) -> Api {
        match self {
            Self::AnthropicMessages(_) => Api::AnthropicMessages,
            Self::OpenAiResponses(_) => Api::OpenAiResponses,
        }
    }

    pub fn model(&self) -> &str {
        match self {
            Self::AnthropicMessages(value) => value.model(),
            Self::OpenAiResponses(value) => value.model(),
        }
    }

    pub fn scope(&self) -> &ContinuationScope {
        match self {
            Self::AnthropicMessages(value) => value.scope(),
            Self::OpenAiResponses(value) => value.scope(),
        }
    }

    pub fn durability(&self) -> crate::types::ContinuationDurability {
        match self {
            Self::AnthropicMessages(value) => value.durability(),
            Self::OpenAiResponses(value) => value.durability(),
        }
    }

    pub fn anthropic_messages(&self) -> Option<&AnthropicMessagesContinuation> {
        match self {
            Self::AnthropicMessages(value) => Some(value),
            Self::OpenAiResponses(_) => None,
        }
    }

    pub fn openai_responses(&self) -> Option<&OpenAiResponsesContinuation> {
        match self {
            Self::AnthropicMessages(_) => None,
            Self::OpenAiResponses(value) => Some(value),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::AnthropicMessages(value) => value.validate(),
            Self::OpenAiResponses(value) => value.validate(),
        }
    }

    /// Validate all normalized reasoning associations without relying on
    /// vector position. This is called at the provider dispatch boundary.
    pub fn validate_for_history(&self, messages: &[Message]) -> Result<(), String> {
        self.validate()?;
        self.scope().validate_history(messages)?;
        let mut references = HashSet::new();
        let mut reasoning = Vec::new();
        for message in messages {
            let Message::Assistant(assistant) = message else {
                continue;
            };
            for part in &assistant.content {
                let AssistantContent::Reasoning(value) = part else {
                    continue;
                };
                match value.portability {
                    ReasoningPortability::Portable if value.continuation_ref.is_some() => {
                        return Err(
                            "portable reasoning must not carry a continuation reference".into()
                        )
                    }
                    ReasoningPortability::ProviderBound => {
                        let reference = value.continuation_ref.as_ref().ok_or_else(|| {
                            "provider-bound reasoning is missing a continuation reference"
                                .to_string()
                        })?;
                        if !references.insert(reference.clone()) {
                            return Err("duplicate continuation reference in history".into());
                        }
                        reasoning.push((reference.clone(), value.redacted));
                    }
                    ReasoningPortability::Portable => {}
                }
            }
        }
        match self {
            Self::AnthropicMessages(value) => {
                for (reference, redacted) in &reasoning {
                    let state = value.replay_for(reference).ok_or_else(|| {
                        "Anthropic reasoning references missing replay metadata".to_string()
                    })?;
                    if state.is_redacted() != *redacted {
                        return Err(
                            "Anthropic reasoning kind does not match replay metadata".into()
                        );
                    }
                }
                if value.reasoning.len() != reasoning.len() {
                    return Err(
                        "Anthropic continuation has unused reasoning replay metadata".into(),
                    );
                }
            }
            Self::OpenAiResponses(value)
                if value.mode == OpenAiResponsesContinuationMode::Stateless =>
            {
                for (reference, _) in &reasoning {
                    if !value.replay_segments.iter().any(|segment| {
                        segment.items.iter().any(|item| {
                            matches!(
                                item,
                                OpenAiResponsesReplayItem::Reasoning {
                                    reference: Some(item_reference),
                                    ..
                                } if item_reference == reference
                            )
                        })
                    }) {
                        return Err(
                            "Responses reasoning reference is missing replay metadata".into()
                        );
                    }
                }
                // Every stateless segment must bind to an assistant entry in
                // the current history through its sequence-sensitive chain
                // identity. A stale anchor means the anchored assistant output
                // was edited, removed, inserted before, or reordered. The
                // anchor-to-message mapping is by identity, never position.
                let mut consumed = HashSet::new();
                for segment in &value.replay_segments {
                    if !consumed.insert(segment.anchor.clone()) {
                        return Err("Responses replay segment anchor is duplicated".into());
                    }
                    let assistant =
                        resolve_segment_anchor(messages, &segment.anchor)?.ok_or_else(|| {
                            "Responses replay segment anchor does not bind an assistant message"
                                .to_string()
                        })?;
                    if let Some(first) =
                        responses_replay_projection_mismatches(segment.items(), assistant).first()
                    {
                        return Err(first.clone());
                    }
                }
            }
            Self::OpenAiResponses(_) => {}
        }
        Ok(())
    }

    pub(crate) fn validate_for_message(&self, message: &AssistantMessage) -> Result<(), String> {
        self.validate()?;
        let mut references = HashSet::new();
        let mut reasoning = Vec::new();
        for part in &message.content {
            let AssistantContent::Reasoning(value) = part else {
                continue;
            };
            if value.portability == ReasoningPortability::Portable {
                if value.continuation_ref.is_some() {
                    return Err("portable reasoning must not carry a continuation reference".into());
                }
                continue;
            }
            let reference = value.continuation_ref.as_ref().ok_or_else(|| {
                "provider-bound reasoning is missing a continuation reference".to_string()
            })?;
            if !references.insert(reference.clone()) {
                return Err("duplicate continuation reference in completion".into());
            }
            reasoning.push((reference, value.redacted));
        }
        match self {
            Self::AnthropicMessages(value) => {
                for (reference, redacted) in &reasoning {
                    let state = value.replay_for(reference).ok_or_else(|| {
                        "Anthropic completion reasoning is missing replay metadata".to_string()
                    })?;
                    if state.is_redacted() != *redacted {
                        return Err("Anthropic completion reasoning kind mismatch".into());
                    }
                }
            }
            Self::OpenAiResponses(value)
                if value.mode == OpenAiResponsesContinuationMode::Stateless =>
            {
                for (reference, _) in &reasoning {
                    if !value.replay_segments.iter().any(|segment| {
                        segment.items.iter().any(|item| {
                            matches!(
                                item,
                                OpenAiResponsesReplayItem::Reasoning {
                                    reference: Some(item_reference),
                                    ..
                                } if item_reference == *reference
                            )
                        })
                    }) {
                        return Err(
                            "Responses completion reasoning is missing replay metadata".into()
                        );
                    }
                }
            }
            Self::OpenAiResponses(_) => {}
        }
        Ok(())
    }
}

/// Locate the assistant message anchored by a stateless replay segment in the
/// request history, using chain identities only. Returns `Err` when no entry
/// carries that identity, `Ok(None)` when the identity binds a non-assistant
/// entry.
pub(crate) fn resolve_segment_anchor<'m>(
    messages: &'m [Message],
    anchor: &HistoryMessageId,
) -> Result<Option<&'m AssistantMessage>, String> {
    let conversation: Vec<&Message> = messages
        .iter()
        .filter(|message| !matches!(message, Message::System { .. }))
        .collect();
    let ids = history_message_ids(messages)?;
    for (message, id) in conversation.into_iter().zip(&ids) {
        if id == anchor {
            return match message {
                Message::Assistant(assistant) => Ok(Some(assistant)),
                _ => Ok(None),
            };
        }
    }
    Err("Responses replay segment anchor no longer matches the history".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(content: &str) -> Message {
        Message::Assistant(AssistantMessage {
            content: vec![AssistantContent::Text(crate::TextContent::new(content))],
        })
    }

    fn provider_bound_reasoning(reference: &ContinuationRef, redacted: bool) -> Message {
        Message::Assistant(AssistantMessage {
            content: vec![AssistantContent::Reasoning(crate::ReasoningContent {
                text: if redacted {
                    String::new()
                } else {
                    "summary".into()
                },
                redacted,
                portability: ReasoningPortability::ProviderBound,
                continuation_ref: Some(reference.clone()),
            })],
        })
    }

    #[test]
    fn scope_validates_exact_non_system_prefix() {
        let history = vec![
            Message::system("instructions"),
            Message::user("first"),
            assistant("answer"),
        ];
        let scope = ContinuationScope::for_history(&history).unwrap();
        assert_eq!(scope.covered_message_count(), 2);

        let mut extended = history.clone();
        extended.push(Message::user("next"));
        assert_eq!(scope.validate_history(&extended), Ok(2));
        assert_eq!(scope.uncovered_history(&extended).unwrap().len(), 1);

        let mut forked = extended.clone();
        forked[1] = Message::user("edited");
        assert!(scope.validate_history(&forked).is_err());

        let mut inserted = history.clone();
        inserted.insert(1, Message::user("inserted before boundary"));
        assert!(scope.validate_history(&inserted).is_err());
        let deleted = vec![Message::system("instructions"), Message::user("first")];
        assert!(scope.validate_history(&deleted).is_err());

        let reasoning_ref = ContinuationRef::new("reasoning-id").unwrap();
        let reasoning_history = vec![
            Message::user("first"),
            provider_bound_reasoning(&reasoning_ref, false),
        ];
        let reasoning_scope = ContinuationScope::for_history(&reasoning_history).unwrap();
        let mut changed_reasoning = reasoning_history;
        let Message::Assistant(message) = &mut changed_reasoning[1] else {
            panic!("expected assistant reasoning")
        };
        let AssistantContent::Reasoning(reasoning) = &mut message.content[0] else {
            panic!("expected reasoning content")
        };
        reasoning.text = "changed summary".into();
        assert!(reasoning_scope
            .validate_history(&changed_reasoning)
            .is_err());

        // Instructions are deliberately outside the covered conversation
        // boundary and may change independently on the next request.
        forked[0] = Message::system("new instructions");
        assert!(scope.validate_history(&forked).is_err());
        let mut only_system_changed = extended;
        only_system_changed[0] = Message::system("new instructions");
        assert_eq!(scope.validate_history(&only_system_changed), Ok(2));
    }

    #[test]
    fn anthropic_replay_resolution_is_reference_bound_not_positional() {
        let first = ContinuationRef::new("reasoning-a").unwrap();
        let second = ContinuationRef::new("reasoning-b").unwrap();
        let messages = vec![
            Message::user("question"),
            provider_bound_reasoning(&first, false),
            provider_bound_reasoning(&second, true),
        ];
        let continuation = AnthropicMessagesContinuation::with_scope(
            crate::ProviderId::new("anthropic").unwrap(),
            "claude-test",
            ContinuationScope::for_history(&messages).unwrap(),
            vec![
                AnthropicReasoningReplayEntry::new(
                    second.clone(),
                    AnthropicReasoningReplay::redacted("opaque-b"),
                ),
                AnthropicReasoningReplayEntry::new(
                    first.clone(),
                    AnthropicReasoningReplay::thinking("signature-a"),
                ),
            ],
        )
        .unwrap();
        let continuation = ProviderContinuation::AnthropicMessages(continuation);
        continuation.validate_for_history(&messages).unwrap();

        let anthropic = continuation.anthropic_messages().unwrap();
        assert!(matches!(
            anthropic.replay_for(&first),
            Some(AnthropicReasoningReplay::Thinking { signature })
                if signature == "signature-a"
        ));
        assert!(matches!(
            anthropic.replay_for(&second),
            Some(AnthropicReasoningReplay::Redacted { data }) if data == "opaque-b"
        ));

        let missing = ProviderContinuation::AnthropicMessages(
            AnthropicMessagesContinuation::with_scope(
                crate::ProviderId::new("anthropic").unwrap(),
                "claude-test",
                ContinuationScope::for_history(&messages).unwrap(),
                vec![AnthropicReasoningReplayEntry::new(
                    first.clone(),
                    AnthropicReasoningReplay::thinking("signature-a"),
                )],
            )
            .unwrap(),
        );
        assert!(missing.validate_for_history(&messages).is_err());

        let extra_reference = ContinuationRef::new("unused").unwrap();
        let extra = ProviderContinuation::AnthropicMessages(
            AnthropicMessagesContinuation::with_scope(
                crate::ProviderId::new("anthropic").unwrap(),
                "claude-test",
                ContinuationScope::for_history(&messages).unwrap(),
                vec![
                    AnthropicReasoningReplayEntry::new(
                        first,
                        AnthropicReasoningReplay::thinking("signature-a"),
                    ),
                    AnthropicReasoningReplayEntry::new(
                        second,
                        AnthropicReasoningReplay::redacted("opaque-b"),
                    ),
                    AnthropicReasoningReplayEntry::new(
                        extra_reference,
                        AnthropicReasoningReplay::thinking("unused-signature"),
                    ),
                ],
            )
            .unwrap(),
        );
        assert!(extra.validate_for_history(&messages).is_err());
    }

    #[test]
    fn responses_reasoning_must_bind_to_a_reasoning_replay_item() {
        let reference = ContinuationRef::new("reasoning").unwrap();
        let messages = vec![provider_bound_reasoning(&reference, true)];
        let ids = history_message_ids(&messages).unwrap();
        let wrong_kind = ProviderContinuation::OpenAiResponses(
            OpenAiResponsesContinuation::with_segments(
                crate::ProviderId::new("openai").unwrap(),
                "gpt-test",
                None,
                OpenAiResponsesContinuationMode::Stateless,
                ContinuationScope::empty(),
                vec![OpenAiResponsesReplaySegment::new(
                    ids[0].clone(),
                    vec![OpenAiResponsesReplayItem::function_call(
                        reference,
                        Some("fc_1".into()),
                        "call_1",
                        "lookup",
                        "{}",
                    )],
                )],
            )
            .unwrap(),
        );
        assert!(wrong_kind.validate_for_history(&messages).is_err());
    }

    #[test]
    fn continuation_debug_redacts_provider_native_payloads() {
        let reference = ContinuationRef::new("safe-ref").unwrap();
        let anthropic = ProviderContinuation::AnthropicMessages(
            AnthropicMessagesContinuation::with_scope(
                crate::ProviderId::new("anthropic").unwrap(),
                "claude-test",
                ContinuationScope::empty(),
                vec![AnthropicReasoningReplayEntry::new(
                    reference.clone(),
                    AnthropicReasoningReplay::thinking("anthropic-secret-signature"),
                )],
            )
            .unwrap(),
        );
        let responses = ProviderContinuation::OpenAiResponses(
            OpenAiResponsesContinuation::with_replay(
                crate::ProviderId::new("openai").unwrap(),
                "gpt-test",
                None,
                OpenAiResponsesContinuationMode::Stateless,
                ContinuationScope::empty(),
                vec![OpenAiResponsesReplayItem::reasoning(
                    reference,
                    Some("rs_1".into()),
                    "encrypted-reasoning-secret",
                    vec!["summary-secret".into()],
                )],
            )
            .unwrap(),
        );
        let debug = format!("{anthropic:?} {responses:?}");
        for secret in [
            "anthropic-secret-signature",
            "encrypted-reasoning-secret",
            "summary-secret",
        ] {
            assert!(!debug.contains(secret), "Debug leaked {secret}");
        }
        assert!(debug.contains("sensitive_bytes"));
    }

    #[test]
    fn history_identity_is_sequence_sensitive_not_content_only() {
        // H1: identical assistant content at different positions.
        let history = vec![
            Message::user("u1"),
            assistant("same"),
            Message::user("u2"),
            assistant("same"),
        ];
        let ids = history_message_ids(&history).unwrap();
        assert_ne!(ids[1], ids[3], "identical content must not share identity");

        // H2: the same exact history produces identical identities.
        let rebuilt = history_message_ids(&history).unwrap();
        assert_eq!(ids, rebuilt);

        // H3-H6: any earlier edit/insert/delete/reorder changes downstream
        // identities.
        let mutate = |mutate: &dyn Fn(Vec<Message>) -> Vec<Message>| {
            history_message_ids(&mutate(history.clone())).unwrap()
        };
        let edited = mutate(&|mut messages| {
            if let Message::User { content } = &mut messages[0] {
                if let Some(crate::UserContent::Text(text)) = content.first_mut() {
                    text.text = "edited".into();
                }
            }
            messages
        });
        assert_ne!(ids[1], edited[1]);
        assert_ne!(ids[3], edited[3]);

        let inserted = mutate(&|mut messages| {
            messages.insert(0, Message::user("inserted"));
            messages
        });
        assert_ne!(ids[1], inserted[2]);
        assert_ne!(ids[3], inserted[4]);

        let deleted = mutate(&|mut messages| {
            messages.remove(0);
            messages
        });
        assert_ne!(ids[1], deleted[0]);

        let reordered = mutate(&|mut messages| {
            messages.swap(0, 2);
            messages
        });
        assert_ne!(ids[1], reordered[3]);
        assert_ne!(ids[3], reordered[1]);

        // H7: a repeated User/Assistant pattern yields a distinct chain
        // identity per occurrence while remaining deterministic.
        let pattern: Vec<Message> = (0..3)
            .flat_map(|round| vec![Message::user(format!("u{round}")), assistant("answer")])
            .collect();
        let pattern_ids = history_message_ids(&pattern).unwrap();
        for pair in pattern_ids.chunks(2).map(|chunk| &chunk[1]) {
            for other in pattern_ids.chunks(2).map(|chunk| &chunk[1]) {
                if !std::ptr::eq(pair, other) {
                    assert_ne!(pair, other);
                }
            }
        }
    }

    #[test]
    fn assistant_anchor_uses_full_history_prefix() {
        let history = vec![Message::user("u1"), assistant("a1")];
        let standalone = assistant_history_anchor(
            &[],
            &AssistantMessage {
                content: vec![AssistantContent::Text(crate::TextContent::new("a1"))],
            },
        )
        .unwrap();
        let chained = assistant_history_anchor(
            &history,
            &AssistantMessage {
                content: vec![AssistantContent::Text(crate::TextContent::new("a1"))],
            },
        )
        .unwrap();
        assert_ne!(
            standalone, chained,
            "the same content on top of different prefixes cannot share an anchor"
        );
        // The chained anchor equals the last id of the extended history.
        let mut extended = history;
        extended.push(Message::Assistant(AssistantMessage {
            content: vec![AssistantContent::Text(crate::TextContent::new("a1"))],
        }));
        assert_eq!(
            chained,
            *history_message_ids(&extended).unwrap().last().unwrap()
        );
    }
}
