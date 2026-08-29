use futures::StreamExt;
use jarvis_model_provider::providers::EndpointPolicy;
use jarvis_model_provider::{
    Api, CompletionRequest, Message, ModelSpec, ProviderConfig, ProviderFactory, ProviderId,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider_id = ProviderId::new("anthropic")?;
    let provider = ProviderFactory::build(ProviderConfig {
        provider_id: provider_id.clone(),
        api: Api::AnthropicMessages,
        api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
        base_url: None,
        endpoint_policy: EndpointPolicy::SecureOrLoopback,
        request_timeout: std::time::Duration::from_secs(120),
    })?;
    let request = CompletionRequest::new(
        ModelSpec::custom(
            "claude-3-5-sonnet-latest",
            provider_id,
            Api::AnthropicMessages,
        ),
        vec![Message::user("Say hello in one sentence.")],
    )
    .with_max_output_tokens(128);
    let mut stream = provider.stream(request).await?;
    while let Some(event) = stream.next().await {
        println!("{:?}", event?);
    }
    Ok(())
}
