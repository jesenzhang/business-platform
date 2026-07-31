//! Agent 协议适配器
//!
//! 提供 MCP/A2A 协议适配，将外部 AI Agent 请求转换为平台内部调用。

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::AgentAdapterConfig::load()?;
    let _guard = observability::init_tracing(
        "agent-adapter",
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    tracing::info!(environment = ?config.env, "Starting agent-adapter");
    // TODO: 阶段四/五实现
    Ok(())
}
