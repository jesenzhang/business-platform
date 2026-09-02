//! Bounded-label runtime metrics for the business worker (PLAN-0012 T4.2).
//!
//! Every label value is a `&'static str` chosen in code below. Tenant ids,
//! document ids, correlation ids, storage paths, failure messages, and model
//! output must never appear as label values — see
//! `docs/architecture/OBSERVABILITY_ARCHITECTURE.md` on label cardinality.

use document_processing::ports::ProcessingFailureDisposition;

/// Records how long a job waited between creation and this claim, in
/// milliseconds; anything beyond ~24.8 days is clamped to the representable max.
pub fn record_queue_wait(millis: i64) {
    let seconds = f64::from(i32::try_from(millis.max(0)).unwrap_or(i32::MAX)) / 1000.0;
    metrics::histogram!("processing_job_queue_wait_seconds").record(seconds);
}

/// Counts one finished processing attempt by its bounded outcome.
pub fn record_attempt(outcome: AttemptOutcome) {
    metrics::counter!("processing_job_attempts_total", "outcome" => outcome.as_str()).increment(1);
}

/// Counts a step failure under exactly one bounded disposition class.
pub fn record_disposition(disposition: &ProcessingFailureDisposition) {
    let disposition = match disposition {
        ProcessingFailureDisposition::Retry { .. } => "retry",
        ProcessingFailureDisposition::Permanent => "permanent",
        ProcessingFailureDisposition::Cancelled => "cancelled",
        ProcessingFailureDisposition::LeaseLost => "lease_lost",
    };
    metrics::counter!("processing_step_dispositions_total", "disposition" => disposition)
        .increment(1);
}

/// Counts expired job leases handed back to the queue by the reclaim sweep.
pub fn record_leases_reclaimed(count: u64) {
    metrics::counter!("processing_leases_reclaimed_total").increment(count);
}

/// Counts lease ownership losses detected locally (fence or heartbeat).
pub fn record_lease_lost() {
    metrics::counter!("processing_lease_lost_total").increment(1);
}

#[derive(Clone, Copy)]
pub enum AttemptOutcome {
    Succeeded,
    Failed,
    LeaseUnproven,
}

impl AttemptOutcome {
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
        assert_eq!(AttemptOutcome::Succeeded.as_str(), "succeeded");
        assert_eq!(AttemptOutcome::Failed.as_str(), "failed");
        assert_eq!(AttemptOutcome::LeaseUnproven.as_str(), "lease_unproven");
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
