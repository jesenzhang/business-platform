//! 业务工作流处理器
//!
//! 消费领域事件，执行异步业务逻辑（审批流转、通知发送、数据同步等）。

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::BusinessWorkerConfig::load()?;
    let _guard = observability::init_tracing(
        "business-worker",
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    tracing::info!(environment = ?config.env, "Starting business-worker");
    // TODO: 阶段四/五实现
    Ok(())
}
