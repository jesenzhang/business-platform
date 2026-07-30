use async_trait::async_trait;

use crate::event::DomainEvent;

/// 消息发布者 trait - 具体实现可以是 NATS、Kafka 等
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish a single domain event to the message broker.
    async fn publish(&self, event: &DomainEvent) -> Result<(), PublishError>;

    /// Publish a batch of domain events to the message broker.
    async fn publish_batch(&self, events: &[DomainEvent]) -> Result<(), PublishError>;
}

/// Errors that can occur during event publishing.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Broker error: {0}")]
    Broker(String),
}

/// 空实现 - 开发/测试用，丢弃所有事件。
pub struct NoopPublisher;

#[async_trait]
impl EventPublisher for NoopPublisher {
    async fn publish(&self, _event: &DomainEvent) -> Result<(), PublishError> {
        tracing::debug!("NoopPublisher: event discarded");
        Ok(())
    }

    async fn publish_batch(&self, events: &[DomainEvent]) -> Result<(), PublishError> {
        tracing::debug!("NoopPublisher: {} events discarded", events.len());
        Ok(())
    }
}
