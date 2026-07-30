//! AI 任务处理器
//!
//! 消费 AI 相关任务（文档摘要、智能分析、向量索引构建等），调用 LLM 完成异步推理。

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing::info!("Starting ai-worker");
    // TODO: 阶段四/五实现
    Ok(())
}
