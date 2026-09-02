use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::continuation::ProviderContinuation;

/// Open-ended provider identity. Provider names are data, not a closed enum.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("provider id must not be empty".into());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ProviderId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<&str> for ProviderId {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Wire protocol. A provider may expose one or more APIs.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Api {
    OpenAiCompletions,
    OpenAiResponses,
    /// Standalone OpenAI Images API generation endpoint.
    OpenAiImages,
    AnthropicMessages,
    Custom(String),
}

/// Selects the output token field for OpenAI Chat Completions-compatible requests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxOutputTokensField {
    #[default]
    MaxTokens,
    MaxCompletionTokens,
}

/// Selects the role emitted for provider-neutral system messages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiSystemRole {
    #[default]
    System,
    Developer,
}

/// Selects the request fields used for OpenAI-compatible reasoning/thinking.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiThinkingDialect {
    /// Send OpenAI's top-level `reasoning_effort` field.
    #[default]
    OpenAi,
    /// Send `thinking: {"type": "enabled"|"disabled"}`.
    ThinkingObject,
    /// Send Together's `reasoning: {"enabled": true|false}` toggle.
    Together,
    /// Send Qwen/DashScope's top-level `enable_thinking` boolean.
    Qwen,
    /// Send Qwen thinking toggles through `chat_template_kwargs` (vLLM /
    /// SGLang served Qwen models ignore the top-level `enable_thinking`
    /// field but honor their chat-template switches).
    QwenChatTemplate,
}

/// The reasoning wire field family selected by an OpenAI-compatible dialect.
///
/// This is a dialect decision, not a model capability. Keeping it as one
/// structured value prevents request encoding from growing an unrelated set
/// of compatibility booleans.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReasoningEncoding {
    OpenAiEffort,
    ThinkingObject,
    TogetherToggle,
    QwenToggle,
    QwenChatTemplateToggle,
}

/// The history field family paired with a reasoning request encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReasoningHistoryEncoding {
    ReasoningContent,
    Reasoning,
}

/// Concentrated reasoning request/replay policy for one OpenAI-compatible
/// dialect. Wire codecs own the concrete JSON shape; callers select only the
/// dialect and do not combine independent field flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasoningWirePolicy {
    pub encoding: ReasoningEncoding,
    pub history: ReasoningHistoryEncoding,
    pub effort_enabled: bool,
}

impl OpenAiThinkingDialect {
    pub fn wire_policy(self, supports_reasoning_effort: bool) -> ReasoningWirePolicy {
        match self {
            Self::OpenAi => ReasoningWirePolicy {
                encoding: ReasoningEncoding::OpenAiEffort,
                history: ReasoningHistoryEncoding::ReasoningContent,
                effort_enabled: supports_reasoning_effort,
            },
            Self::ThinkingObject => ReasoningWirePolicy {
                encoding: ReasoningEncoding::ThinkingObject,
                history: ReasoningHistoryEncoding::ReasoningContent,
                effort_enabled: supports_reasoning_effort,
            },
            Self::Together => ReasoningWirePolicy {
                encoding: ReasoningEncoding::TogetherToggle,
                history: ReasoningHistoryEncoding::Reasoning,
                effort_enabled: false,
            },
            Self::Qwen => ReasoningWirePolicy {
                encoding: ReasoningEncoding::QwenToggle,
                history: ReasoningHistoryEncoding::ReasoningContent,
                effort_enabled: false,
            },
            Self::QwenChatTemplate => ReasoningWirePolicy {
                encoding: ReasoningEncoding::QwenChatTemplateToggle,
                history: ReasoningHistoryEncoding::ReasoningContent,
                effort_enabled: false,
            },
        }
    }
}

fn default_supports_reasoning_effort() -> bool {
    true
}

/// Compatibility choices for OpenAI Chat Completions-compatible endpoints.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiCompletionsCompatibility {
    pub max_output_tokens_field: MaxOutputTokensField,
    /// Legacy opt-out retained for source and profile compatibility. New
    /// dialects must encode their own supported fields through
    /// [`ReasoningWirePolicy`]; this flag cannot enable foreign dialect
    /// fields.
    #[serde(default = "default_supports_reasoning_effort")]
    pub supports_reasoning_effort: bool,
    pub system_role: OpenAiSystemRole,
    pub thinking_dialect: OpenAiThinkingDialect,
}

impl Default for OpenAiCompletionsCompatibility {
    fn default() -> Self {
        Self {
            max_output_tokens_field: MaxOutputTokensField::default(),
            supports_reasoning_effort: true,
            system_role: OpenAiSystemRole::default(),
            thinking_dialect: OpenAiThinkingDialect::default(),
        }
    }
}

/// Controls whether a provider may retain request/response state remotely.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DataRetentionPolicy {
    /// Do not retain the response at the provider.
    #[default]
    Ephemeral,
    /// Use the provider's documented default.
    ProviderDefault,
}

/// How an opaque provider continuation may be handled by the owning runtime.
/// The provider library reports this classification; it does not decide
/// whether the runtime must persist or retry it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContinuationDurability {
    Portable,
    ProviderBound,
    ProcessLocal,
    SensitiveNonDurable,
}

/// Whether a reasoning block can be replayed as normalized content alone.
///
/// `ProviderBound` is a normalized semantic marker only. The provider-native
/// replay payload is carried by [`ProviderContinuation`], never by a message.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReasoningPortability {
    #[default]
    Portable,
    ProviderBound,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub reasoning: bool,
    pub tools: bool,
    pub vision: bool,
    pub structured_output: bool,
}

/// Provider-declared constraint features for the supported wire protocols.
///
/// These are protocol capabilities, not a claim that every model behind an
/// OpenAI-compatible endpoint supports every feature. A model with known
/// metadata may further narrow the structured-output path through
/// [`ModelCapabilities::structured_output`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConstraintCapabilities {
    /// The provider accepts strict JSON Schema on function tools and/or
    /// structured output, depending on the request constraint.
    pub strict_json_schema: bool,
    /// The provider accepts a JSON Schema structured-output request.
    pub structured_output: bool,
    /// The provider accepts a grammar-constrained request.
    pub grammar: bool,
}

/// Return the conservative constraint capability matrix for a wire protocol.
///
/// The matrix only advertises fields implemented by this crate. OpenAI Chat
/// Completions, Responses, and Anthropic Messages have native strict
/// function-tool and JSON Schema output fields. Standalone image generation
/// has no conversational constraint surface. Arbitrary custom protocols do
/// not receive an inferred constraint capability.
pub fn protocol_constraint_capabilities(api: &Api) -> ConstraintCapabilities {
    match api {
        Api::OpenAiCompletions | Api::OpenAiResponses => ConstraintCapabilities {
            strict_json_schema: true,
            structured_output: true,
            grammar: false,
        },
        Api::OpenAiImages => ConstraintCapabilities::default(),
        Api::AnthropicMessages => ConstraintCapabilities {
            strict_json_schema: true,
            structured_output: true,
            grammar: false,
        },
        Api::Custom(_) => ConstraintCapabilities::default(),
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum CapabilityKnowledge {
    Known,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub reasoning: bool,
    pub tools: bool,
    pub tool_streaming: bool,
    pub vision: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub name: Option<String>,
    pub provider: ProviderId,
    pub api: Api,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_completions_compatibility: Option<OpenAiCompletionsCompatibility>,
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub capability_knowledge: CapabilityKnowledge,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub cost: Option<ModelCost>,
}

impl ModelSpec {
    pub fn custom(id: impl Into<String>, provider: ProviderId, api: Api) -> Self {
        Self {
            id: id.into(),
            name: None,
            provider,
            api,
            openai_completions_compatibility: None,
            // A custom model has no catalog entry. Unknown capabilities remain
            // representable, but feature-bearing requests must opt into an
            // explicit capability assertion via `with_capabilities`.
            capabilities: ModelCapabilities::default(),
            capability_knowledge: CapabilityKnowledge::Unknown,
            context_window: None,
            max_output_tokens: None,
            cost: None,
        }
    }

    pub fn cost_for(&self, usage: &Usage) -> Option<Cost> {
        self.cost.as_ref().map(|rates| calculate_cost(rates, usage))
    }

    pub fn with_capabilities(mut self, capabilities: ModelCapabilities) -> Self {
        self.capabilities = capabilities;
        self.capability_knowledge = CapabilityKnowledge::Known;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageContent {
    pub media_type: String,
    pub data: String,
}

impl ImageContent {
    pub fn new(media_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            media_type: media_type.into(),
            data: data.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReasoningContent {
    pub text: String,
    pub redacted: bool,
    /// Provider-native replay state is carried by [`ProviderContinuation`].
    /// This marker prevents a missing sidecar from being mistaken for
    /// ordinary portable reasoning history.
    #[serde(default)]
    pub portability: ReasoningPortability,
    /// Stable identity for provider-bound replay state. Portable reasoning
    /// must leave this unset; interrupted partial blocks may also be unset
    /// until a completed continuation sidecar exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_ref: Option<crate::ContinuationRef>,
}

impl<'de> Deserialize<'de> for ReasoningContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireReasoningContent {
            text: String,
            #[serde(default)]
            redacted: bool,
            #[serde(default)]
            portability: ReasoningPortability,
            // Read the removed fields only to fail closed for persisted V1
            // messages. Their payload never re-enters normalized content.
            #[serde(default, rename = "signature")]
            signature: Option<String>,
            #[serde(default, rename = "redacted_data")]
            redacted_data: Option<String>,
        }

        let value = WireReasoningContent::deserialize(deserializer)?;
        let legacy_provider_state = value.signature.is_some() || value.redacted_data.is_some();
        Ok(Self {
            text: value.text,
            redacted: value.redacted || value.redacted_data.is_some(),
            portability: if legacy_provider_state {
                ReasoningPortability::ProviderBound
            } else {
                value.portability
            },
            continuation_ref: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UserContent {
    Text(TextContent),
    Image(ImageContent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AssistantContent {
    Text(TextContent),
    Reasoning(ReasoningContent),
    ToolCall(ToolCall),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToolResultContent {
    Text(TextContent),
    Image(ImageContent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<AssistantContent>,
}

impl AssistantMessage {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![AssistantContent::Text(TextContent::new(text))],
        }
    }

    pub fn text_value(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                AssistantContent::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn reasoning_chars(&self) -> usize {
        self.content
            .iter()
            .filter_map(|part| match part {
                AssistantContent::Reasoning(reasoning) => Some(reasoning.text.chars().count()),
                _ => None,
            })
            .sum()
    }

    pub fn tool_calls(&self) -> Vec<&ToolCall> {
        self.content
            .iter()
            .filter_map(|part| match part {
                AssistantContent::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: Vec<UserContent>,
    },
    Assistant(AssistantMessage),
    ToolResult {
        tool_call_id: String,
        name: Option<String>,
        content: Vec<ToolResultContent>,
    },
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: vec![UserContent::Text(TextContent::new(content))],
        }
    }

    pub fn user_parts(content: Vec<UserContent>) -> Self {
        Self::User { content }
    }

    pub fn assistant(content: AssistantMessage) -> Self {
        Self::Assistant(content)
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: Option<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            name,
            content: vec![ToolResultContent::Text(TextContent::new(content))],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// Optional provider-neutral constraint for this tool's input schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<ToolConstraint>,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            constraint: None,
        }
    }

    pub fn with_constraint(mut self, constraint: ToolConstraint) -> Self {
        self.constraint = Some(constraint);
        self
    }
}

/// Constraint applied to a tool's input schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToolConstraint {
    /// Require the provider's strict JSON Schema function-tool mode.
    StrictJsonSchema,
    /// Request a grammar-constrained tool argument. No current transport
    /// advertises this capability; it is represented so callers receive a
    /// deterministic before-dispatch error instead of a silent downgrade.
    Grammar { grammar: String },
}

/// Constraint applied to the model's final structured output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OutputConstraint {
    /// Emit a strict or non-strict provider-native JSON Schema output format.
    JsonSchema {
        name: String,
        schema: serde_json::Value,
        strict: bool,
    },
    /// Request a grammar-constrained output. No current transport advertises
    /// this capability.
    Grammar { grammar: String },
}

/// Provider-neutral tool selection. Transports map this onto their wire field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Tool { name: String },
}

/// Request-side reasoning control. Response reasoning blocks remain separate.
///
/// This V1 surface is `#[non_exhaustive]`: future optional provider reasoning
/// controls are added without becoming an immediate source-breaking change
/// for external consumers. Construct through [`ReasoningConfig::enabled`],
/// [`ReasoningConfig::disabled`], or the builder methods; serialized data
/// keeps decoding because wire compatibility is unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReasoningConfig {
    pub enabled: bool,
    pub budget_tokens: Option<u32>,
    /// OpenAI-compatible effort string such as `low`, `medium`, or `high`.
    pub effort: Option<String>,
    /// Responses API reasoning summary: `auto`, `concise`, or `detailed`.
    pub summary: Option<String>,
}

impl ReasoningConfig {
    pub fn enabled(budget_tokens: Option<u32>) -> Self {
        Self {
            enabled: true,
            budget_tokens,
            effort: None,
            summary: None,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            budget_tokens: None,
            effort: None,
            summary: None,
        }
    }

    pub fn with_budget_tokens(mut self, budget_tokens: u32) -> Self {
        self.budget_tokens = Some(budget_tokens);
        self
    }

    pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
        self.effort = Some(effort.into());
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Provider-reported usage in the same normalized shape for streaming and
/// non-streaming completions.
///
/// `input_tokens` is total logical input processed. When cache subdivisions are
/// reported, `input_tokens = uncached + cache_read + cache_write`; absent
/// subdivisions are treated as unknown rather than changing that meaning.
/// `total_tokens = input_tokens + output_tokens`; reasoning tokens are a
/// sub-ledger of `output_tokens` and are not added again.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

/// A malformed provider usage payload that cannot satisfy the normalized
/// accounting contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UsageError {
    CacheTokensExceedInput {
        input_tokens: u64,
        cache_tokens: u64,
    },
    TotalTokensMismatch {
        expected: u64,
        actual: u64,
    },
    ReasoningTokensExceedOutput {
        output_tokens: u64,
        reasoning_tokens: u64,
    },
}

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheTokensExceedInput {
                input_tokens,
                cache_tokens,
            } => write!(
                formatter,
                "cache token subdivisions {cache_tokens} exceed logical input tokens {input_tokens}"
            ),
            Self::TotalTokensMismatch { expected, actual } => write!(
                formatter,
                "usage total_tokens {actual} does not equal normalized input plus output {expected}"
            ),
            Self::ReasoningTokensExceedOutput {
                output_tokens,
                reasoning_tokens,
            } => write!(
                formatter,
                "reasoning tokens {reasoning_tokens} exceed output tokens {output_tokens}"
            ),
        }
    }
}

impl std::error::Error for UsageError {}

impl Usage {
    /// Compute the normalized primary ledger without double-counting cache or
    /// reasoning dimensions.
    pub const fn accounted_total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    /// Return the input tokens that must use the normal input rate.
    pub fn uncached_input_tokens(&self) -> Result<u64, UsageError> {
        let cache_tokens = self
            .cache_read_tokens
            .unwrap_or_default()
            .checked_add(self.cache_write_tokens.unwrap_or_default())
            .ok_or(UsageError::CacheTokensExceedInput {
                input_tokens: self.input_tokens,
                cache_tokens: u64::MAX,
            })?;
        self.input_tokens
            .checked_sub(cache_tokens)
            .ok_or(UsageError::CacheTokensExceedInput {
                input_tokens: self.input_tokens,
                cache_tokens,
            })
    }

    /// Validate the cross-provider usage invariant.
    pub fn validate(&self) -> Result<(), UsageError> {
        let expected = self.accounted_total_tokens();
        if self.total_tokens != expected {
            return Err(UsageError::TotalTokensMismatch {
                expected,
                actual: self.total_tokens,
            });
        }
        self.uncached_input_tokens()?;
        if let Some(reasoning_tokens) = self.reasoning_tokens {
            if reasoning_tokens > self.output_tokens {
                return Err(UsageError::ReasoningTokensExceedOutput {
                    output_tokens: self.output_tokens,
                    reasoning_tokens,
                });
            }
        }
        Ok(())
    }

    /// Whether the provider-reported total and reasoning sub-ledger are
    /// internally consistent, including cache subdivisions within logical
    /// input tokens.
    pub const fn has_consistent_accounting(&self) -> bool {
        let cache_read_tokens = match self.cache_read_tokens {
            Some(value) => value,
            None => 0,
        };
        let cache_write_tokens = match self.cache_write_tokens {
            Some(value) => value,
            None => 0,
        };
        let cache_tokens = match cache_read_tokens.checked_add(cache_write_tokens) {
            Some(value) => value,
            None => return false,
        };
        self.total_tokens == self.accounted_total_tokens()
            && cache_tokens <= self.input_tokens
            && match self.reasoning_tokens {
                Some(reasoning_tokens) => reasoning_tokens <= self.output_tokens,
                None => true,
            }
    }
}

/// USD rates per million tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelCost {
    /// Rate for uncached logical input tokens, in USD per million tokens.
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

/// USD cost for one completion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

pub fn calculate_cost(rates: &ModelCost, usage: &Usage) -> Cost {
    let uncached_input_tokens = usage.uncached_input_tokens().unwrap_or_default();
    calculate_cost_for_input(rates, usage, uncached_input_tokens)
}

/// Calculate cost only when the usage payload satisfies the normalized
/// accounting contract.
pub fn try_calculate_cost(rates: &ModelCost, usage: &Usage) -> Result<Cost, UsageError> {
    usage.validate()?;
    Ok(calculate_cost_for_input(
        rates,
        usage,
        usage.uncached_input_tokens()?,
    ))
}

fn calculate_cost_for_input(rates: &ModelCost, usage: &Usage, uncached_input_tokens: u64) -> Cost {
    let million = 1_000_000.0;
    let input = rates.input / million * uncached_input_tokens as f64;
    let output = rates.output / million * usage.output_tokens as f64;
    let cache_read =
        rates.cache_read / million * usage.cache_read_tokens.unwrap_or_default() as f64;
    let cache_write =
        rates.cache_write / million * usage.cache_write_tokens.unwrap_or_default() as f64;
    Cost {
        input,
        output,
        cache_read,
        cache_write,
        total: input + output + cache_read + cache_write,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    ContentFilter,
    Other(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionMetadata {
    pub content_chars: usize,
    pub reasoning_chars: usize,
    pub tool_call_count: usize,
    pub stop_reason: Option<StopReason>,
    pub stream_completed: bool,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub message: AssistantMessage,
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ProviderContinuation>,
    pub stop_reason: StopReason,
    pub metadata: CompletionMetadata,
}

/// One provider-neutral completion request.
///
/// This V1 surface is `#[non_exhaustive]`: future optional request fields do
/// not become an immediate source-breaking change for external consumers.
/// Construct through [`CompletionRequest::new`] plus the `with_*` builder
/// methods; serialized data keeps decoding because wire compatibility is
/// unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CompletionRequest {
    pub model: ModelSpec,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_constraint: Option<OutputConstraint>,
    #[serde(default)]
    pub retention: DataRetentionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ProviderContinuation>,
}

impl CompletionRequest {
    pub fn new(model: ModelSpec, messages: Vec<Message>) -> Self {
        Self {
            model,
            messages,
            tools: Vec::new(),
            temperature: None,
            max_output_tokens: None,
            top_p: None,
            tool_choice: None,
            reasoning: None,
            output_constraint: None,
            retention: DataRetentionPolicy::Ephemeral,
            continuation: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the temperature directly from an optional value.
    pub fn with_temperature_opt(mut self, temperature: Option<f32>) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    /// Set the output-token limit directly from an optional value.
    pub fn with_max_output_tokens_opt(mut self, max_output_tokens: Option<u32>) -> Self {
        self.max_output_tokens = max_output_tokens;
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Set the top-p value directly from an optional value.
    pub fn with_top_p_opt(mut self, top_p: Option<f32>) -> Self {
        self.top_p = top_p;
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    pub fn with_reasoning(mut self, reasoning: ReasoningConfig) -> Self {
        self.reasoning = Some(reasoning);
        self
    }

    /// Set reasoning controls directly from an optional value.
    pub fn with_reasoning_opt(mut self, reasoning: Option<ReasoningConfig>) -> Self {
        self.reasoning = reasoning;
        self
    }

    pub fn with_retention(mut self, retention: DataRetentionPolicy) -> Self {
        self.retention = retention;
        self
    }

    pub fn with_output_constraint(mut self, constraint: OutputConstraint) -> Self {
        self.output_constraint = Some(constraint);
        self
    }

    pub fn with_continuation(mut self, continuation: ProviderContinuation) -> Self {
        self.continuation = Some(continuation);
        self
    }

    /// Set the continuation sidecar directly from an optional value.
    pub fn with_continuation_opt(mut self, continuation: Option<ProviderContinuation>) -> Self {
        self.continuation = continuation;
        self
    }
}
