//! AI-task failure → retry disposition policy (PLAN-0012 release hardening).
//!
//! The policy is deliberately isolated in one testable module so the whole
//! `ProviderError → ExtractionError → ProcessingFailureDisposition` chain can
//! be exercised end to end without a live worker loop:
//!
//! - `AiProviderRateLimited` honours the provider's `Retry-After` hint,
//!   clamped to `document_processing::ports::MAX_PROVIDER_RETRY_AFTER_SECS`
//!   so a hostile or misconfigured upstream cannot park a task indefinitely;
//!   without a hint it falls back to the platform backoff ladder.
//! - `AiProviderUnavailable` (timeouts, 5xx) and `Internal` use the platform
//!   backoff ladder.
//! - `AiProviderRejected` (credential/authorization rejection) and malformed
//!   responses are permanent: retrying an identical request cannot succeed,
//!   so the task fails closed instead of burning its attempt budget.

use chrono::Duration as ChronoDuration;
use document_processing::ports::{
    capped_provider_retry_after, ClassifiedProcessingFailure, ProcessingFailureDisposition,
};
use document_processing::ExtractionError;

fn platform_backoff(attempt_count: i32) -> ChronoDuration {
    ChronoDuration::seconds(match attempt_count {
        1 => 1,
        2 => 5,
        _ => 30,
    })
}

pub(super) fn classify_failure(
    error: &ExtractionError,
    attempt_count: i32,
) -> ClassifiedProcessingFailure {
    let disposition = match error {
        ExtractionError::AiProviderRateLimited { retry_after } => {
            ProcessingFailureDisposition::Retry {
                backoff: match retry_after {
                    Some(hint) => capped_provider_retry_after(*hint),
                    None => platform_backoff(attempt_count),
                },
            }
        }
        ExtractionError::AiProviderUnavailable | ExtractionError::Internal => {
            ProcessingFailureDisposition::Retry {
                backoff: platform_backoff(attempt_count),
            }
        }
        ExtractionError::LeaseLost => ProcessingFailureDisposition::LeaseLost,
        ExtractionError::Cancelled => ProcessingFailureDisposition::Cancelled,
        _ => ProcessingFailureDisposition::Permanent,
    };
    ClassifiedProcessingFailure {
        code: error.code().to_string(),
        message: None,
        disposition,
    }
}

#[cfg(test)]
mod tests {
    // Deterministic fixtures: `expect` documents the invariant and panics
    // with a message when a fixture itself is broken.
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::extractor::ModelBackedExtractor;
    use document_processing::ports::MAX_PROVIDER_RETRY_AFTER_SECS;
    use document_processing::{DocumentFieldExtractor, ExtractionRequest};
    use jarvis_model_provider::providers::ScriptedProvider;
    use jarvis_model_provider::{FailurePhase, ProviderError, ProviderErrorKind, StreamEvent};
    use std::sync::Arc;
    use std::time::Duration;
    use uuid::Uuid;

    fn request() -> ExtractionRequest {
        let text = "Quarterly results.".to_string();
        ExtractionRequest {
            tenant_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            content_revision: 1,
            content_type: "text/plain".to_string(),
            line_count: 1,
            character_count: u64::try_from(text.chars().count()).unwrap_or(u64::MAX),
            text,
        }
    }

    fn scripted_provider(error: ProviderError) -> Arc<ScriptedProvider> {
        Arc::new(
            ScriptedProvider::new(vec![
                Ok(StreamEvent::Start { model: "m".into() }),
                Err(error),
            ])
            .expect("scripted provider must build"),
        )
    }

    fn backoff_secs(disposition: &ClassifiedProcessingFailure) -> Option<i64> {
        match disposition.disposition {
            ProcessingFailureDisposition::Retry { backoff } => Some(backoff.num_seconds()),
            _ => None,
        }
    }

    #[test]
    fn rate_limited_with_hint_uses_capped_provider_retry_after() {
        let failure = classify_failure(
            &ExtractionError::AiProviderRateLimited {
                retry_after: Some(Duration::from_secs(7)),
            },
            1,
        );
        assert_eq!(failure.code, "ai_provider_rate_limited");
        assert_eq!(backoff_secs(&failure), Some(7));
    }

    #[test]
    fn rate_limited_hint_is_clamped_to_the_platform_cap() {
        let failure = classify_failure(
            &ExtractionError::AiProviderRateLimited {
                retry_after: Some(Duration::from_secs(
                    MAX_PROVIDER_RETRY_AFTER_SECS as u64 * 12,
                )),
            },
            1,
        );
        assert_eq!(backoff_secs(&failure), Some(MAX_PROVIDER_RETRY_AFTER_SECS));
    }

    #[test]
    fn sub_second_hint_is_never_zero() {
        let failure = classify_failure(
            &ExtractionError::AiProviderRateLimited {
                retry_after: Some(Duration::from_millis(200)),
            },
            1,
        );
        assert_eq!(backoff_secs(&failure), Some(1));
    }

    #[test]
    fn rate_limited_without_hint_falls_back_to_the_platform_ladder() {
        let no_hint = || ExtractionError::AiProviderRateLimited { retry_after: None };
        assert_eq!(backoff_secs(&classify_failure(&no_hint(), 1)), Some(1));
        assert_eq!(backoff_secs(&classify_failure(&no_hint(), 2)), Some(5));
        assert_eq!(backoff_secs(&classify_failure(&no_hint(), 9)), Some(30));
    }

    #[test]
    fn unavailable_uses_the_platform_ladder() {
        assert_eq!(
            backoff_secs(&classify_failure(
                &ExtractionError::AiProviderUnavailable,
                1
            )),
            Some(1)
        );
        assert_eq!(
            backoff_secs(&classify_failure(&ExtractionError::Internal, 3)),
            Some(30)
        );
    }

    #[test]
    fn credential_rejection_and_invalid_requests_are_permanent() {
        for error in [
            ExtractionError::AiProviderRejected,
            ExtractionError::AiInvalidResponse,
            ExtractionError::CandidateValidationFailed,
        ] {
            let failure = classify_failure(&error, 1);
            assert_eq!(
                failure.disposition,
                ProcessingFailureDisposition::Permanent,
                "must not retry {error}"
            );
        }
    }

    async fn extract_and_classify(
        error: ProviderError,
        attempt_count: i32,
    ) -> ClassifiedProcessingFailure {
        let extractor = ModelBackedExtractor::new(scripted_provider(error), "test-model".into());
        let extraction_error = extractor
            .extract(request())
            .await
            .expect_err("scripted provider must fail");
        classify_failure(&extraction_error, attempt_count)
    }

    #[tokio::test]
    async fn rate_limited_provider_error_retries_with_server_hint() {
        let failure = extract_and_classify(
            ProviderError::new(
                ProviderErrorKind::RateLimit,
                FailurePhase::AfterDispatch,
                "429 slow down",
            )
            .with_status(429)
            .with_retry_after(Duration::from_secs(7)),
            1,
        )
        .await;
        assert_eq!(failure.code, "ai_provider_rate_limited");
        assert_eq!(backoff_secs(&failure), Some(7));
    }

    #[tokio::test]
    async fn timeout_provider_error_retries_on_the_platform_ladder() {
        let failure = extract_and_classify(
            ProviderError::new(
                ProviderErrorKind::Timeout,
                FailurePhase::AfterDispatch,
                "deadline exceeded",
            ),
            2,
        )
        .await;
        assert_eq!(failure.code, "ai_provider_unavailable");
        assert_eq!(backoff_secs(&failure), Some(5));
    }

    #[tokio::test]
    async fn authentication_provider_error_fails_permanently() {
        let failure = extract_and_classify(
            ProviderError::new(
                ProviderErrorKind::Authentication,
                FailurePhase::BeforeDispatch,
                "invalid api key",
            )
            .with_status(401),
            1,
        )
        .await;
        assert_eq!(failure.code, "ai_provider_rejected");
        assert_eq!(failure.disposition, ProcessingFailureDisposition::Permanent);
    }

    #[tokio::test]
    async fn invalid_request_provider_error_fails_permanently() {
        let failure = extract_and_classify(
            ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                FailurePhase::BeforeDispatch,
                "context length exceeded",
            )
            .with_status(400),
            1,
        )
        .await;
        assert_eq!(failure.code, "ai_invalid_response");
        assert_eq!(failure.disposition, ProcessingFailureDisposition::Permanent);
    }
}
