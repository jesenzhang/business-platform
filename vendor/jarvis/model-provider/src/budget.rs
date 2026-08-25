use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{AssistantContent, CompletionRequest, Message, ToolResultContent, UserContent};

const MESSAGE_OVERHEAD_TOKENS: u64 = 4;
const CONTENT_PART_OVERHEAD_TOKENS: u64 = 1;
const TOOL_OVERHEAD_TOKENS: u64 = 8;
const IMAGE_BASE_TOKENS: u64 = 1_024;
const IMAGE_DATA_BYTES_PER_TOKEN: u64 = 8;
const TEXT_BYTES_PER_TOKEN: u64 = 2;

/// How much confidence a token estimate carries.
///
/// The default estimator has no provider tokenizer. Its result is therefore a
/// conservative bounded estimate, not a provider billing value. `Exact` is
/// available to callers that provide a tokenizer-backed [`TokenEstimator`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenEstimatePrecision {
    Exact,
    #[default]
    Bounded,
    Unknown,
}

/// A token count with an explicit precision level.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenEstimate {
    /// For a bounded estimate this is the conservative upper estimate used by
    /// context preflight. It is not an official provider count.
    pub tokens: u64,
    pub precision: TokenEstimatePrecision,
}

impl TokenEstimate {
    pub const fn exact(tokens: u64) -> Self {
        Self {
            tokens,
            precision: TokenEstimatePrecision::Exact,
        }
    }

    pub const fn bounded(tokens: u64) -> Self {
        Self {
            tokens,
            precision: TokenEstimatePrecision::Bounded,
        }
    }

    pub const fn unknown() -> Self {
        Self {
            tokens: 0,
            precision: TokenEstimatePrecision::Unknown,
        }
    }

    pub const fn is_known(self) -> bool {
        !matches!(self.precision, TokenEstimatePrecision::Unknown)
    }

    pub const fn upper_bound(self) -> Option<u64> {
        match self.precision {
            TokenEstimatePrecision::Unknown => None,
            TokenEstimatePrecision::Exact | TokenEstimatePrecision::Bounded => Some(self.tokens),
        }
    }
}

impl Default for TokenEstimate {
    fn default() -> Self {
        Self::unknown()
    }
}

/// A token limit whose absence is meaningful and must not be treated as zero.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenLimit {
    Known(u64),
    #[default]
    Unknown,
}

impl TokenLimit {
    pub const fn known(tokens: u64) -> Self {
        Self::Known(tokens)
    }

    pub const fn unknown() -> Self {
        Self::Unknown
    }

    pub const fn value(self) -> Option<u64> {
        match self {
            Self::Known(tokens) => Some(tokens),
            Self::Unknown => None,
        }
    }
}

/// Provider-neutral input estimate broken down by request component.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputTokenBreakdown {
    /// System/developer, user, assistant, and tool-result message content plus
    /// message framing. Image data is reported separately.
    pub message_tokens: TokenEstimate,
    /// Serialized tool definitions, including descriptions and input schemas.
    pub tool_tokens: TokenEstimate,
    /// Image parts, using a conservative provider-neutral image heuristic.
    pub image_tokens: TokenEstimate,
    pub total_tokens: TokenEstimate,
}

impl InputTokenBreakdown {
    pub fn new(
        message_tokens: TokenEstimate,
        tool_tokens: TokenEstimate,
        image_tokens: TokenEstimate,
    ) -> Self {
        Self {
            message_tokens,
            tool_tokens,
            image_tokens,
            total_tokens: add_estimates(add_estimates(message_tokens, tool_tokens), image_tokens),
        }
    }
}

/// The result of estimating one request's input and declared output budget.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequestTokenBudget {
    pub input_tokens: TokenEstimate,
    pub message_tokens: TokenEstimate,
    pub tool_tokens: TokenEstimate,
    pub image_tokens: TokenEstimate,
    /// The explicitly requested output reservation. An omitted request field
    /// stays `Unknown`; it is not silently replaced with a provider default.
    pub requested_output_tokens: TokenLimit,
    /// The output capacity reserved for context preflight: the explicit
    /// request limit when present, otherwise the model's declared maximum.
    /// This is a safety upper bound, not a provider default request value.
    pub output_budget_tokens: TokenLimit,
    /// Input plus the effective output reservation when both are known. A
    /// bounded input estimate produces a bounded total estimate.
    pub total_tokens: TokenEstimate,
    /// Reasoning is a sub-ledger of output tokens, never an additional total.
    pub reasoning_budget_tokens: TokenLimit,
    pub context_window: TokenLimit,
    /// Conservative output capacity remaining after the estimated input.
    pub available_output_tokens: TokenLimit,
    pub model_max_output_tokens: TokenLimit,
}

impl RequestTokenBudget {
    pub fn context_status(&self) -> ContextBudgetStatus {
        let Some(context_window) = self.context_window.value() else {
            return ContextBudgetStatus::Unknown;
        };

        if let Some(input_tokens) = self.input_tokens.upper_bound() {
            if input_tokens > context_window {
                return ContextBudgetStatus::Exceeded {
                    required_tokens: input_tokens,
                    context_window,
                };
            }
        }

        match self.total_tokens.upper_bound() {
            Some(total_tokens) if total_tokens > context_window => ContextBudgetStatus::Exceeded {
                required_tokens: total_tokens,
                context_window,
            },
            Some(_) => ContextBudgetStatus::Within,
            None => ContextBudgetStatus::Unknown,
        }
    }
}

/// Result of context-window preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBudgetStatus {
    Within,
    Exceeded {
        required_tokens: u64,
        context_window: u64,
    },
    Unknown,
}

/// A token estimator supplies provider-neutral input accounting.
pub trait TokenEstimator: Send + Sync {
    fn estimate_input(&self, request: &CompletionRequest) -> InputTokenBreakdown;
}

/// Default estimator used by dispatch preflight.
///
/// It intentionally does not depend on a provider tokenizer. Text and JSON
/// are estimated from bounded UTF-8/serialized sizes; image parts receive a
/// conservative fixed floor plus a data-size component. This is useful for
/// fail-before-dispatch safety, but must not be presented as billing usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConservativeTokenEstimator;

impl TokenEstimator for ConservativeTokenEstimator {
    fn estimate_input(&self, request: &CompletionRequest) -> InputTokenBreakdown {
        estimate_input_breakdown(request)
    }
}

/// Estimate a request with the crate's conservative provider-neutral estimator.
pub fn estimate_request_budget(request: &CompletionRequest) -> RequestTokenBudget {
    estimate_request_budget_with(request, &ConservativeTokenEstimator)
}

/// Estimate a request with a caller-supplied tokenizer or estimation policy.
pub fn estimate_request_budget_with(
    request: &CompletionRequest,
    estimator: &dyn TokenEstimator,
) -> RequestTokenBudget {
    let breakdown = estimator.estimate_input(request);
    let requested_output_tokens = request
        .max_output_tokens
        .map(u64::from)
        .map_or(TokenLimit::Unknown, TokenLimit::Known);
    let model_max_output_tokens = request
        .model
        .max_output_tokens
        .map(u64::from)
        .map_or(TokenLimit::Unknown, TokenLimit::Known);
    let output_budget_tokens = match (
        requested_output_tokens.value(),
        model_max_output_tokens.value(),
    ) {
        (Some(requested), _) => TokenLimit::Known(requested),
        (None, Some(model_max)) => TokenLimit::Known(model_max),
        (None, None) => TokenLimit::Unknown,
    };
    let total_tokens = match (
        breakdown.total_tokens.upper_bound(),
        output_budget_tokens.value(),
    ) {
        (Some(input_tokens), Some(output_tokens)) => {
            let precision = if matches!(
                breakdown.total_tokens.precision,
                TokenEstimatePrecision::Exact
            ) {
                TokenEstimatePrecision::Exact
            } else {
                TokenEstimatePrecision::Bounded
            };
            TokenEstimate {
                tokens: input_tokens.saturating_add(output_tokens),
                precision,
            }
        }
        _ => TokenEstimate::unknown(),
    };
    let reasoning_budget_tokens = match request.reasoning.as_ref() {
        Some(reasoning) if reasoning.enabled => reasoning
            .budget_tokens
            .map(u64::from)
            .map_or(TokenLimit::Unknown, TokenLimit::Known),
        _ => TokenLimit::Known(0),
    };
    let context_window = request
        .model
        .context_window
        .map(u64::from)
        .map_or(TokenLimit::Unknown, TokenLimit::Known);
    let available_output_tokens =
        match (context_window.value(), breakdown.total_tokens.upper_bound()) {
            (Some(context_window), Some(input_tokens)) => {
                TokenLimit::Known(context_window.saturating_sub(input_tokens))
            }
            _ => TokenLimit::Unknown,
        };

    RequestTokenBudget {
        input_tokens: breakdown.total_tokens,
        message_tokens: breakdown.message_tokens,
        tool_tokens: breakdown.tool_tokens,
        image_tokens: breakdown.image_tokens,
        requested_output_tokens,
        output_budget_tokens,
        total_tokens,
        reasoning_budget_tokens,
        context_window,
        available_output_tokens,
        model_max_output_tokens,
    }
}

/// Validate only facts that are knowable before network dispatch.
pub fn validate_request_budget(
    request: &CompletionRequest,
) -> Result<RequestTokenBudget, BudgetViolation> {
    let budget = estimate_request_budget(request);
    validate_estimated_request_budget(request, &budget)?;
    Ok(budget)
}

/// Validate a budget that was computed during request preparation.
///
/// Keeping this separate from [`validate_request_budget`] prevents dispatch
/// paths from silently estimating and validating different request shapes.
pub(crate) fn validate_estimated_request_budget(
    request: &CompletionRequest,
    budget: &RequestTokenBudget,
) -> Result<(), BudgetViolation> {
    if let (Some(requested), Some(model_max)) = (
        request.max_output_tokens.map(u64::from),
        request.model.max_output_tokens.map(u64::from),
    ) {
        if requested > model_max {
            return Err(BudgetViolation::OutputExceedsModelLimit {
                requested_tokens: requested,
                model_max_output_tokens: model_max,
            });
        }
    }
    if let ContextBudgetStatus::Exceeded {
        required_tokens,
        context_window,
    } = budget.context_status()
    {
        return Err(BudgetViolation::ContextWindowExceeded {
            required_tokens,
            context_window,
        });
    }
    Ok(())
}

/// A known pre-dispatch budget violation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetViolation {
    ContextWindowExceeded {
        required_tokens: u64,
        context_window: u64,
    },
    OutputExceedsModelLimit {
        requested_tokens: u64,
        model_max_output_tokens: u64,
    },
}

impl fmt::Display for BudgetViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextWindowExceeded {
                required_tokens,
                context_window,
            } => write!(
                formatter,
                "request token budget may exceed model context window: required upper estimate {required_tokens}, context window {context_window}"
            ),
            Self::OutputExceedsModelLimit {
                requested_tokens,
                model_max_output_tokens,
            } => write!(
                formatter,
                "requested max_output_tokens {requested_tokens} exceeds model limit {model_max_output_tokens}"
            ),
        }
    }
}

impl std::error::Error for BudgetViolation {}

fn estimate_input_breakdown(request: &CompletionRequest) -> InputTokenBreakdown {
    let mut message_tokens = 0_u64;
    let mut image_tokens = 0_u64;
    for message in &request.messages {
        let (message_estimate, images) = estimate_message(message);
        message_tokens = message_tokens.saturating_add(message_estimate);
        image_tokens = image_tokens.saturating_add(images);
    }

    let tool_tokens = request.tools.iter().fold(0_u64, |total, tool| {
        let serialized = serde_json::to_vec(tool).unwrap_or_default();
        total.saturating_add(TOOL_OVERHEAD_TOKENS.saturating_add(bytes_to_tokens(serialized.len())))
    });

    InputTokenBreakdown::new(
        TokenEstimate::bounded(message_tokens),
        TokenEstimate::bounded(tool_tokens),
        TokenEstimate::bounded(image_tokens),
    )
}

fn estimate_message(message: &Message) -> (u64, u64) {
    let mut tokens = MESSAGE_OVERHEAD_TOKENS;
    let mut images = 0_u64;
    match message {
        Message::System { content } => {
            tokens = tokens.saturating_add(text_to_tokens(content));
        }
        Message::User { content } => {
            for part in content {
                match part {
                    UserContent::Text(text) => {
                        tokens = tokens
                            .saturating_add(CONTENT_PART_OVERHEAD_TOKENS)
                            .saturating_add(text_to_tokens(&text.text));
                    }
                    UserContent::Image(image) => {
                        images =
                            images.saturating_add(image_to_tokens(&image.media_type, &image.data));
                    }
                }
            }
        }
        Message::Assistant(message) => {
            for part in &message.content {
                tokens = tokens.saturating_add(CONTENT_PART_OVERHEAD_TOKENS);
                match part {
                    AssistantContent::Text(text) => {
                        tokens = tokens.saturating_add(text_to_tokens(&text.text));
                    }
                    AssistantContent::Reasoning(reasoning) => {
                        tokens = tokens.saturating_add(text_to_tokens(&reasoning.text));
                    }
                    AssistantContent::ToolCall(call) => {
                        tokens = tokens
                            .saturating_add(text_to_tokens(&call.id))
                            .saturating_add(text_to_tokens(&call.name))
                            .saturating_add(json_to_tokens(&call.arguments));
                    }
                }
            }
        }
        Message::ToolResult {
            tool_call_id,
            name,
            content,
        } => {
            tokens = tokens
                .saturating_add(text_to_tokens(tool_call_id))
                .saturating_add(name.as_deref().map_or(0, text_to_tokens));
            for part in content {
                match part {
                    ToolResultContent::Text(text) => {
                        tokens = tokens
                            .saturating_add(CONTENT_PART_OVERHEAD_TOKENS)
                            .saturating_add(text_to_tokens(&text.text));
                    }
                    ToolResultContent::Image(image) => {
                        images =
                            images.saturating_add(image_to_tokens(&image.media_type, &image.data));
                    }
                }
            }
        }
    }
    (tokens, images)
}

fn add_estimates(left: TokenEstimate, right: TokenEstimate) -> TokenEstimate {
    let Some(left_tokens) = left.upper_bound() else {
        return TokenEstimate::unknown();
    };
    let Some(right_tokens) = right.upper_bound() else {
        return TokenEstimate::unknown();
    };
    let precision = if matches!(left.precision, TokenEstimatePrecision::Exact)
        && matches!(right.precision, TokenEstimatePrecision::Exact)
    {
        TokenEstimatePrecision::Exact
    } else {
        TokenEstimatePrecision::Bounded
    };
    TokenEstimate {
        tokens: left_tokens.saturating_add(right_tokens),
        precision,
    }
}

fn text_to_tokens(value: &str) -> u64 {
    bytes_to_tokens(value.len())
}

fn json_to_tokens(value: &serde_json::Value) -> u64 {
    serde_json::to_vec(value)
        .map(|serialized| bytes_to_tokens(serialized.len()))
        .unwrap_or_default()
}

fn bytes_to_tokens(bytes: usize) -> u64 {
    if bytes == 0 {
        return 0;
    }
    (bytes as u64).saturating_add(TEXT_BYTES_PER_TOKEN - 1) / TEXT_BYTES_PER_TOKEN
}

fn image_to_tokens(media_type: &str, data: &str) -> u64 {
    IMAGE_BASE_TOKENS
        .saturating_add(text_to_tokens(media_type))
        .saturating_add(
            (data.len() as u64).saturating_add(IMAGE_DATA_BYTES_PER_TOKEN - 1)
                / IMAGE_DATA_BYTES_PER_TOKEN,
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Api, ImageContent, ModelSpec, ProviderId, ReasoningConfig, ToolResultContent, ToolSpec,
    };

    fn model() -> ModelSpec {
        ModelSpec::custom(
            "budget-model",
            ProviderId::new("test").unwrap(),
            Api::OpenAiCompletions,
        )
    }

    #[test]
    fn default_estimate_counts_system_tools_and_images() {
        let mut request = CompletionRequest::new(
            model(),
            vec![Message::system("rules"), Message::user("hello")],
        );
        let text_only = estimate_request_budget(&request);
        request
            .messages
            .push(Message::user_parts(vec![UserContent::Image(
                ImageContent::new("image/png", "encoded-image"),
            )]));
        request.messages.push(Message::ToolResult {
            tool_call_id: "call-1".into(),
            name: Some("lookup".into()),
            content: vec![ToolResultContent::Image(ImageContent::new(
                "image/jpeg",
                "encoded-result-image",
            ))],
        });
        request.tools.push(ToolSpec {
            name: "lookup".into(),
            description: "look up a value".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"q":{"type":"string"}}}),
            constraint: None,
        });
        let budget = estimate_request_budget(&request);
        assert!(budget.image_tokens.tokens >= IMAGE_BASE_TOKENS * 2);
        assert!(budget.tool_tokens.tokens > 0);
        assert!(budget.input_tokens.tokens > text_only.input_tokens.tokens);
        assert_eq!(
            budget.input_tokens.precision,
            TokenEstimatePrecision::Bounded
        );
    }

    #[test]
    fn missing_metadata_is_explicitly_unknown() {
        let request = CompletionRequest::new(model(), vec![Message::user("hello")]);
        let budget = estimate_request_budget(&request);
        assert_eq!(budget.context_window, TokenLimit::Unknown);
        assert_eq!(budget.requested_output_tokens, TokenLimit::Unknown);
        assert_eq!(budget.total_tokens, TokenEstimate::unknown());
        assert_eq!(budget.context_status(), ContextBudgetStatus::Unknown);
        assert!(validate_request_budget(&request).is_ok());
    }

    #[test]
    fn reasoning_budget_is_a_subledger_of_output() {
        let mut request = CompletionRequest::new(model(), vec![Message::user("hello")]);
        request.max_output_tokens = Some(64);
        request.reasoning = Some(ReasoningConfig::enabled(Some(32)));
        let budget = estimate_request_budget(&request);
        assert_eq!(budget.reasoning_budget_tokens, TokenLimit::Known(32));
        assert_eq!(
            budget.total_tokens.tokens,
            budget.input_tokens.tokens.saturating_add(64)
        );
    }

    #[test]
    fn known_context_overflow_is_rejected_but_unknown_context_is_not() {
        let mut request = CompletionRequest::new(model(), vec![Message::user("x".repeat(200))]);
        request.max_output_tokens = Some(16);
        request.model.context_window = Some(32);
        assert_eq!(
            estimate_request_budget(&request).available_output_tokens,
            TokenLimit::Known(0)
        );
        assert!(matches!(
            validate_request_budget(&request),
            Err(BudgetViolation::ContextWindowExceeded { .. })
        ));

        request.model.context_window = None;
        assert!(validate_request_budget(&request).is_ok());
    }

    #[test]
    fn explicit_output_budget_respects_model_maximum() {
        let mut request = CompletionRequest::new(model(), vec![Message::user("hello")]);
        request.max_output_tokens = Some(128);
        request.model.max_output_tokens = Some(64);
        assert!(matches!(
            validate_request_budget(&request),
            Err(BudgetViolation::OutputExceedsModelLimit { .. })
        ));
    }

    #[test]
    fn model_maximum_is_a_safety_reservation_when_request_limit_is_missing() {
        let mut request = CompletionRequest::new(model(), vec![Message::user("hello")]);
        request.model.context_window = Some(20);
        request.model.max_output_tokens = Some(5);
        let budget = estimate_request_budget(&request);
        assert_eq!(budget.requested_output_tokens, TokenLimit::Unknown);
        assert_eq!(budget.output_budget_tokens, TokenLimit::Known(5));
        assert_eq!(budget.total_tokens.tokens, budget.input_tokens.tokens + 5);
        assert_eq!(budget.context_status(), ContextBudgetStatus::Within);
    }

    struct ExactEstimator;

    impl TokenEstimator for ExactEstimator {
        fn estimate_input(&self, _request: &CompletionRequest) -> InputTokenBreakdown {
            InputTokenBreakdown::new(
                TokenEstimate::exact(10),
                TokenEstimate::exact(2),
                TokenEstimate::exact(0),
            )
        }
    }

    #[test]
    fn caller_estimator_can_publish_exact_budget_precision() {
        let mut request = CompletionRequest::new(model(), vec![Message::user("hello")]);
        request.max_output_tokens = Some(5);
        request.model.context_window = Some(20);
        let budget = estimate_request_budget_with(&request, &ExactEstimator);
        assert_eq!(budget.input_tokens, TokenEstimate::exact(12));
        assert_eq!(budget.total_tokens, TokenEstimate::exact(17));
        assert_eq!(budget.context_status(), ContextBudgetStatus::Within);
    }
}
