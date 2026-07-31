use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

/// A publisher that records events for test verification.
///
/// Supports a fail mode to simulate broker errors in integration tests.
#[derive(Clone)]
pub struct RecordingPublisher {
    published: Arc<Mutex<Vec<DomainEvent>>>,
    should_fail: Arc<AtomicBool>,
}

impl Default for RecordingPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingPublisher {
    /// Create a new recording publisher in success mode.
    #[must_use]
    pub fn new() -> Self {
        Self {
            published: Arc::new(Mutex::new(Vec::new())),
            should_fail: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Enable or disable failure simulation.
    pub fn set_fail_mode(&self, fail: bool) {
        self.should_fail.store(fail, Ordering::SeqCst);
    }

    /// Return a snapshot of all published events.
    #[must_use]
    pub fn published_events(&self) -> Vec<DomainEvent> {
        self.published
            .lock()
            .map_or_else(|_| Vec::new(), |guard| guard.clone())
    }

    /// Return the number of published events.
    #[must_use]
    pub fn published_count(&self) -> usize {
        self.published.lock().map_or(0, |guard| guard.len())
    }
}

#[async_trait]
impl EventPublisher for RecordingPublisher {
    async fn publish(&self, event: &DomainEvent) -> Result<(), PublishError> {
        if self.should_fail.load(Ordering::SeqCst) {
            return Err(PublishError::Broker("simulated failure".to_string()));
        }

        if let Ok(mut guard) = self.published.lock() {
            guard.push(event.clone());
        }

        Ok(())
    }

    async fn publish_batch(&self, events: &[DomainEvent]) -> Result<(), PublishError> {
        if self.should_fail.load(Ordering::SeqCst) {
            return Err(PublishError::Broker("simulated failure".to_string()));
        }

        if let Ok(mut guard) = self.published.lock() {
            guard.extend_from_slice(events);
        }

        Ok(())
    }
}
