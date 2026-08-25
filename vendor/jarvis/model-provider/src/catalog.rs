use serde::{Deserialize, Serialize};

use crate::{Api, CapabilityKnowledge, ModelCapabilities, ModelCost, ModelSpec, ProviderId};

/// Static catalog of first-party OpenAI and Anthropic models.
/// Custom/gateway models remain representable via [`ModelSpec::custom`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelCatalog {
    models: Vec<ModelSpec>,
}

impl ModelCatalog {
    pub fn new(models: impl IntoIterator<Item = ModelSpec>) -> Self {
        let mut catalog = Self::default();
        for model in models {
            catalog.insert(model);
        }
        catalog
    }

    pub fn builtin() -> Self {
        Self {
            models: builtin_models(),
        }
    }

    pub fn get(&self, provider: &str, id: &str) -> Option<&ModelSpec> {
        self.models
            .iter()
            .find(|model| model.provider.as_str() == provider && model.id == id)
    }

    pub fn list(&self) -> &[ModelSpec] {
        &self.models
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn list_provider(&self, provider: &str) -> Vec<&ModelSpec> {
        self.models
            .iter()
            .filter(|model| model.provider.as_str() == provider)
            .collect()
    }

    pub fn into_models(self) -> Vec<ModelSpec> {
        self.models
    }

    pub fn replace(&mut self, models: impl IntoIterator<Item = ModelSpec>) {
        *self = Self::new(models);
    }

    pub fn providers(&self) -> Vec<&str> {
        let mut ids = Vec::new();
        for model in &self.models {
            if !ids.contains(&model.provider.as_str()) {
                ids.push(model.provider.as_str());
            }
        }
        ids
    }

    pub fn insert(&mut self, model: ModelSpec) {
        if let Some(existing) = self
            .models
            .iter_mut()
            .find(|entry| entry.provider == model.provider && entry.id == model.id)
        {
            *existing = model;
        } else {
            self.models.push(model);
        }
    }
}

fn builtin_models() -> Vec<ModelSpec> {
    vec![
        openai(
            "gpt-4o", "GPT-4o", false, true, 128_000, 16_384, 2.50, 10.00, 1.25, 2.50,
        ),
        openai(
            "gpt-4o-mini",
            "GPT-4o mini",
            false,
            true,
            128_000,
            16_384,
            0.15,
            0.60,
            0.075,
            0.15,
        ),
        openai(
            "gpt-4.1", "GPT-4.1", false, true, 1_047_576, 32_768, 2.00, 8.00, 0.50, 2.00,
        ),
        openai(
            "gpt-4.1-mini",
            "GPT-4.1 mini",
            false,
            true,
            1_047_576,
            32_768,
            0.40,
            1.60,
            0.10,
            0.40,
        ),
        openai(
            "o3", "o3", true, true, 200_000, 100_000, 2.00, 8.00, 0.50, 2.00,
        ),
        openai(
            "o4-mini", "o4-mini", true, true, 200_000, 100_000, 1.10, 4.40, 0.275, 1.10,
        ),
        openai_responses(
            "gpt-5", "GPT-5", true, true, 400_000, 128_000, 1.25, 10.00, 0.125, 1.25,
        ),
        openai_responses(
            "gpt-5-mini",
            "GPT-5 mini",
            true,
            true,
            400_000,
            128_000,
            0.25,
            2.00,
            0.025,
            0.25,
        ),
        anthropic(
            "claude-opus-4-1",
            "Claude Opus 4.1",
            true,
            true,
            200_000,
            32_000,
            15.00,
            75.00,
            1.50,
            18.75,
        ),
        anthropic(
            "claude-sonnet-4-5",
            "Claude Sonnet 4.5",
            true,
            true,
            200_000,
            64_000,
            3.00,
            15.00,
            0.30,
            3.75,
        ),
        anthropic(
            "claude-haiku-4-5",
            "Claude Haiku 4.5",
            true,
            true,
            200_000,
            64_000,
            1.00,
            5.00,
            0.10,
            1.25,
        ),
        anthropic(
            "claude-3-5-sonnet-latest",
            "Claude 3.5 Sonnet",
            true,
            true,
            200_000,
            8_192,
            3.00,
            15.00,
            0.30,
            3.75,
        ),
        anthropic(
            "claude-3-5-haiku-latest",
            "Claude 3.5 Haiku",
            false,
            true,
            200_000,
            8_192,
            0.80,
            4.00,
            0.08,
            1.00,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn openai(
    id: &str,
    name: &str,
    reasoning: bool,
    vision: bool,
    context: u32,
    max_out: u32,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
) -> ModelSpec {
    spec(
        id,
        name,
        "openai",
        Api::OpenAiCompletions,
        reasoning,
        vision,
        context,
        max_out,
        input,
        output,
        cache_read,
        cache_write,
    )
}

#[allow(clippy::too_many_arguments)]
fn openai_responses(
    id: &str,
    name: &str,
    reasoning: bool,
    vision: bool,
    context: u32,
    max_out: u32,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
) -> ModelSpec {
    let mut model = openai(
        id,
        name,
        reasoning,
        vision,
        context,
        max_out,
        input,
        output,
        cache_read,
        cache_write,
    );
    model.api = Api::OpenAiResponses;
    model
}

#[allow(clippy::too_many_arguments)]
fn anthropic(
    id: &str,
    name: &str,
    reasoning: bool,
    vision: bool,
    context: u32,
    max_out: u32,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
) -> ModelSpec {
    spec(
        id,
        name,
        "anthropic",
        Api::AnthropicMessages,
        reasoning,
        vision,
        context,
        max_out,
        input,
        output,
        cache_read,
        cache_write,
    )
}

#[allow(clippy::too_many_arguments)]
fn spec(
    id: &str,
    name: &str,
    provider: &str,
    api: Api,
    reasoning: bool,
    vision: bool,
    context: u32,
    max_out: u32,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
) -> ModelSpec {
    ModelSpec {
        id: id.into(),
        name: Some(name.into()),
        provider: ProviderId::new(provider).expect("builtin provider id"),
        api,
        openai_completions_compatibility: None,
        capabilities: ModelCapabilities {
            reasoning,
            tools: true,
            vision,
            structured_output: true,
        },
        capability_knowledge: CapabilityKnowledge::Known,
        context_window: Some(context),
        max_output_tokens: Some(max_out),
        cost: Some(ModelCost {
            input,
            output,
            cache_read,
            cache_write,
        }),
    }
}
