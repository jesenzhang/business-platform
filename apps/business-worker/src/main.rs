//! 业务工作流处理器
//!
//! 消费领域事件，执行异步业务逻辑（审批流转、通知发送、数据同步等）。

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing::info!("Starting business-worker");
    // TODO: 阶段四/五实现
    Ok(())
}
