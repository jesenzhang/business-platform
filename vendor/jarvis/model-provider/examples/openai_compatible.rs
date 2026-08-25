use futures::StreamExt;
use jarvis_model_provider::providers::EndpointPolicy;
use jarvis_model_provider::{
    Api, CompletionRequest, Message, ModelSpec, ProviderConfig, ProviderFactory, ProviderId,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider_id = ProviderId::new("openai-compatible")?;
    let provider = ProviderFactory::build(ProviderConfig {
        provider_id: provider_id.clone(),
        api: Api::OpenAiCompletions,
        api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
        base_url: None,
        endpoint_policy: EndpointPolicy::SecureOrLoopback,
        request_timeout: std::time::Duration::from_secs(120),
    })?;
    let request = CompletionRequest {
        model: ModelSpec::custom("gpt-4o-mini", provider_id, Api::OpenAiCompletions),
        messages: vec![Message::user("Say hello in one sentence.")],
        tools: Vec::new(),
        temperature: None,
        max_output_tokens: Some(128),
        top_p: None,
        tool_choice: None,
        reasoning: None,
        output_constraint: None,
        retention: jarvis_model_provider::DataRetentionPolicy::Ephemeral,
        continuation: None,
    };
    let mut stream = provider.stream(request).await?;
    while let Some(event) = stream.next().await {
        println!("{:?}", event?);
    }
    Ok(())
}
