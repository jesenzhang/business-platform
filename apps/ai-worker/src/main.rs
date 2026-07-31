//! AI 任务处理器
//!
//! 消费 AI 相关任务（文档摘要、智能分析、向量索引构建等），调用 LLM 完成异步推理。

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::AiWorkerConfig::load()?;
    let _guard = observability::init_tracing(
        "ai-worker",
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    tracing::info!(environment = ?config.env, "Starting ai-worker");
    // TODO: 阶段四/五实现
    Ok(())
}
