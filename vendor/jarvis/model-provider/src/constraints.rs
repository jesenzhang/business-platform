use crate::{
    Api, CompletionRequest, ConstraintCapabilities, FailurePhase, OutputConstraint, ProviderError,
    ProviderErrorKind, ToolChoice, ToolConstraint,
};

/// Maximum compact JSON size accepted for a tool or structured-output schema.
pub const MAX_CONSTRAINT_SCHEMA_BYTES: usize = 64 * 1024;
/// Maximum JSON nesting depth accepted for a tool or structured-output schema.
pub const MAX_CONSTRAINT_SCHEMA_DEPTH: usize = 32;
/// Maximum UTF-8 size accepted for a grammar expression.
pub const MAX_CONSTRAINT_GRAMMAR_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 size accepted for a structured-output schema name.
pub const MAX_CONSTRAINT_NAME_BYTES: usize = 128;

/// Validate all provider-neutral constraint semantics before a request body is
/// built or a transport is dispatched.
pub(crate) fn validate_request_constraints(
    request: &CompletionRequest,
    api: &Api,
    capabilities: ConstraintCapabilities,
) -> Result<(), ProviderError> {
    for tool in &request.tools {
        validate_schema("tool input schema", &tool.input_schema)?;
        if let Some(constraint) = &tool.constraint {
            match constraint {
                ToolConstraint::StrictJsonSchema => {
                    if !capabilities.strict_json_schema {
                        return Err(unsupported(format!(
                            "strict JSON Schema tool constraint is not supported by {api:?}"
                        )));
                    }
                }
                ToolConstraint::Grammar { grammar } => {
                    validate_grammar(grammar, "tool grammar")?;
                    if !capabilities.grammar {
                        return Err(unsupported(format!(
                            "grammar tool constraint is not supported by {api:?}"
                        )));
                    }
                }
            }
        }
    }

    if matches!(api, Api::AnthropicMessages)
        && request
            .reasoning
            .as_ref()
            .is_some_and(|reasoning| reasoning.enabled)
        && matches!(
            request.tool_choice,
            Some(ToolChoice::Required | ToolChoice::Tool { .. })
        )
    {
        return Err(invalid(
            "Anthropic manual thinking conflicts with forced tool choice; use auto or none",
        ));
    }

    if let Some(constraint) = &request.output_constraint {
        validate_output_constraint(constraint, api, capabilities)?;
        if output_constraint_conflicts_with_tools(request) {
            return Err(invalid(
                "structured output constraint conflicts with tool definitions or tool_choice",
            ));
        }
    }

    Ok(())
}

fn validate_output_constraint(
    constraint: &OutputConstraint,
    api: &Api,
    capabilities: ConstraintCapabilities,
) -> Result<(), ProviderError> {
    match constraint {
        OutputConstraint::JsonSchema {
            name,
            schema,
            strict,
        } => {
            validate_name(name)?;
            validate_schema("structured output schema", schema)?;
            if !*strict && matches!(api, Api::AnthropicMessages) {
                return Err(unsupported(
                    "non-strict JSON Schema structured output is not representable by Anthropic Messages",
                ));
            }
            if *strict && !capabilities.strict_json_schema {
                return Err(unsupported(format!(
                    "strict JSON Schema structured output is not supported by {api:?}"
                )));
            }
            if !*strict && !capabilities.structured_output {
                return Err(unsupported(format!(
                    "JSON Schema structured output is not supported by {api:?}"
                )));
            }
        }
        OutputConstraint::Grammar { grammar } => {
            validate_grammar(grammar, "output grammar")?;
            if !capabilities.grammar {
                return Err(unsupported(format!(
                    "grammar structured output is not supported by {api:?}"
                )));
            }
        }
    }
    Ok(())
}

fn output_constraint_conflicts_with_tools(request: &CompletionRequest) -> bool {
    match request.tool_choice.as_ref() {
        Some(ToolChoice::Required | ToolChoice::Tool { .. }) => true,
        Some(ToolChoice::Auto | ToolChoice::None) | None => false,
    }
}

fn validate_name(name: &str) -> Result<(), ProviderError> {
    if name.trim().is_empty() {
        return Err(invalid("structured output schema name must not be empty"));
    }
    if name.len() > MAX_CONSTRAINT_NAME_BYTES {
        return Err(invalid(format!(
            "structured output schema name exceeds {MAX_CONSTRAINT_NAME_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_grammar(grammar: &str, label: &str) -> Result<(), ProviderError> {
    if grammar.trim().is_empty() {
        return Err(invalid(format!("{label} must not be empty")));
    }
    if grammar.len() > MAX_CONSTRAINT_GRAMMAR_BYTES {
        return Err(invalid(format!(
            "{label} exceeds {MAX_CONSTRAINT_GRAMMAR_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_schema(label: &str, schema: &serde_json::Value) -> Result<(), ProviderError> {
    if !schema.is_object() {
        return Err(invalid(format!("{label} must be a JSON object")));
    }
    let serialized = serde_json::to_vec(schema)
        .map_err(|_| invalid(format!("{label} could not be serialized")))?;
    if serialized.len() > MAX_CONSTRAINT_SCHEMA_BYTES {
        return Err(invalid(format!(
            "{label} exceeds {MAX_CONSTRAINT_SCHEMA_BYTES} serialized bytes"
        )));
    }
    let depth = json_depth(schema);
    if depth > MAX_CONSTRAINT_SCHEMA_DEPTH {
        return Err(invalid(format!(
            "{label} exceeds maximum nesting depth {MAX_CONSTRAINT_SCHEMA_DEPTH}"
        )));
    }
    Ok(())
}

fn json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Array(values) => {
            1 + values.iter().map(json_depth).max().unwrap_or_default()
        }
        serde_json::Value::Object(values) => {
            1 + values.values().map(json_depth).max().unwrap_or_default()
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => 1,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol_constraint_capabilities, DataRetentionPolicy, Message, ModelSpec, ProviderId,
        ToolSpec,
    };

    fn request() -> CompletionRequest {
        CompletionRequest {
            model: ModelSpec::custom(
                "test",
                ProviderId::new("test").unwrap(),
                Api::OpenAiCompletions,
            ),
            messages: vec![Message::user("hello")],
            tools: Vec::new(),
            temperature: None,
            max_output_tokens: Some(32),
            top_p: None,
            tool_choice: None,
            reasoning: None,
            output_constraint: None,
            retention: DataRetentionPolicy::Ephemeral,
            continuation: None,
        }
    }

    #[test]
    fn ordinary_tool_schema_is_subject_to_size_and_depth_bounds() {
        let mut request = request();
        let mut schema = serde_json::json!({"type": "string"});
        for _ in 0..MAX_CONSTRAINT_SCHEMA_DEPTH {
            schema = serde_json::json!({"nested": schema});
        }
        request.tools.push(ToolSpec {
            name: "lookup".into(),
            description: "lookup".into(),
            input_schema: schema,
            constraint: None,
        });
        let error = validate_request_constraints(
            &request,
            &Api::OpenAiCompletions,
            protocol_constraint_capabilities(&Api::OpenAiCompletions),
        )
        .unwrap_err();
        assert!(error.message.contains("nesting depth"));
    }
}
