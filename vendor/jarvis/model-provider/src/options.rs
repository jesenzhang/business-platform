use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

/// Cooperative cancellation for one provider request.
///
/// Abort does not imply retry. Callers observe [`crate::ProviderErrorKind::Aborted`]
/// plus a [`crate::FailurePhase`] that says whether dispatch or streaming was
/// already observed.
#[derive(Clone, Debug)]
pub struct AbortSignal {
    inner: Arc<AbortInner>,
}

#[derive(Debug)]
struct AbortInner {
    aborted: AtomicBool,
    notify: Notify,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AbortInner {
                aborted: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    pub fn abort(&self) {
        self.inner.aborted.store(true, Ordering::Release);
        self.inner.notify.notify_waiters();
    }

    pub fn is_aborted(&self) -> bool {
        self.inner.aborted.load(Ordering::Acquire)
    }

    pub async fn cancelled(&self) {
        let notified = self.inner.notify.notified();
        if self.is_aborted() {
            return;
        }
        notified.await;
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// Transport options for one request. These are not part of the serializable
/// conversation payload.
#[derive(Clone, Debug, Default)]
pub struct RequestOptions {
    pub abort: Option<AbortSignal>,
    /// Extra HTTP headers merged with provider defaults case-insensitively.
    /// The last caller value wins for non-credential headers. A configured
    /// provider credential remains provider-owned; when no credential is
    /// configured, a caller credential header is retained.
    pub headers: Vec<(String, String)>,
}
