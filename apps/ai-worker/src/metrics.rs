//! Bounded-label runtime metrics for the AI worker (PLAN-0012 T4.2).
//!
//! Every label value is a `&'static str` chosen in code below. Tenant ids,
//! task ids, correlation ids, model names, prompts, and model output must
//! never appear as label values — see
//! `docs/architecture/OBSERVABILITY_ARCHITECTURE.md` on label cardinality.

use document_processing::ports::ProcessingFailureDisposition;

/// Records how long an AI task waited between enqueue and this claim, in
/// milliseconds; anything beyond ~24.8 days is clamped to the representable max.
pub fn record_queue_wait(millis: i64) {
    let seconds = f64::from(i32::try_from(millis.max(0)).unwrap_or(i32::MAX)) / 1000.0;
    metrics::histogram!("ai_task_queue_wait_seconds").record(seconds);
}

/// Records the wall time of processing one claimed AI task, in seconds.
pub fn record_task_duration(seconds: f64) {
    metrics::histogram!("ai_task_duration_seconds").record(seconds);
}

/// Counts one finished AI task by its bounded outcome.
pub fn record_task_outcome(outcome: TaskOutcome) {
    metrics::counter!("ai_tasks_total", "outcome" => outcome.as_str()).increment(1);
}

/// Counts an AI task failure under exactly one bounded disposition class.
pub fn record_disposition(disposition: &ProcessingFailureDisposition) {
    let disposition = match disposition {
        ProcessingFailureDisposition::Retry { .. } => "retry",
        ProcessingFailureDisposition::Permanent => "permanent",
        ProcessingFailureDisposition::Cancelled => "cancelled",
        ProcessingFailureDisposition::LeaseLost => "lease_lost",
    };
    metrics::counter!("ai_task_dispositions_total", "disposition" => disposition).increment(1);
}

/// Records one provider round trip with a bounded success/error label.
pub fn record_provider_request(seconds: f64, succeeded: bool) {
    metrics::histogram!(
        "ai_provider_request_seconds",
        "outcome" => if succeeded { "ok" } else { "error" },
    )
    .record(seconds);
}

/// Counts a provider 429 / rate-limit rejection.
pub fn record_provider_rate_limited() {
    metrics::counter!("ai_provider_rate_limited_total").increment(1);
}

/// Counts a provider 5xx server-side failure.
pub fn record_provider_server_error() {
    metrics::counter!("ai_provider_server_error_total").increment(1);
}

/// Counts expired AI leases handed back to the queue by the reclaim sweep.
pub fn record_leases_reclaimed(count: u64) {
    metrics::counter!("ai_leases_reclaimed_total").increment(count);
}

/// Counts lease ownership losses detected locally (fence or heartbeat).
pub fn record_lease_lost() {
    metrics::counter!("ai_lease_lost_total").increment(1);
}

#[derive(Clone, Copy)]
pub enum TaskOutcome {
    Succeeded,
    Failed,
    LeaseUnproven,
}

impl TaskOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::LeaseUnproven => "lease_unproven",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_and_disposition_labels_are_bounded() {
        assert_eq!(TaskOutcome::Succeeded.as_str(), "succeeded");
        assert_eq!(TaskOutcome::Failed.as_str(), "failed");
        assert_eq!(TaskOutcome::LeaseUnproven.as_str(), "lease_unproven");
        for disposition in [
            ProcessingFailureDisposition::Retry {
                backoff: chrono::Duration::seconds(1),
            },
            ProcessingFailureDisposition::Permanent,
            ProcessingFailureDisposition::Cancelled,
            ProcessingFailureDisposition::LeaseLost,
        ] {
            // Must not panic and must not carry dynamic label values.
            record_disposition(&disposition);
        }
    }
}
