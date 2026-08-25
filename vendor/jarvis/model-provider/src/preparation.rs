//! The single provider-neutral request preparation seam.
//!
//! Preparation owns deterministic transformations that are independent of
//! credentials and network I/O. Provider implementations validate the
//! resulting [`PreparedRequest`] and then encode it for their wire protocol.

use crate::{
    estimate_request_budget, Api, CompletionRequest, HistoryNormalization, RequestTokenBudget,
};

/// A completion request after target-aware normalization and budget
/// estimation, but before provider-specific validation or wire encoding.
///
/// The request body remains provider-neutral. In particular, this type does
/// not contain credentials, HTTP state, or protocol-specific JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRequest {
    request: CompletionRequest,
    history: HistoryNormalization,
    budget: RequestTokenBudget,
}

impl PreparedRequest {
    pub(crate) fn new(
        request: CompletionRequest,
        history: HistoryNormalization,
        budget: RequestTokenBudget,
    ) -> Self {
        Self {
            request,
            history,
            budget,
        }
    }

    /// Return the normalized request without exposing any wire representation.
    pub fn request(&self) -> &CompletionRequest {
        &self.request
    }

    /// Consume the preparation result and return its normalized request.
    pub fn into_request(self) -> CompletionRequest {
        self.request
    }

    /// Diagnostics for history transformations performed during preparation.
    pub fn history(&self) -> &HistoryNormalization {
        &self.history
    }

    /// The deterministic provider-neutral budget estimate for the prepared
    /// request.
    pub fn budget(&self) -> &RequestTokenBudget {
        &self.budget
    }
}

/// Normalize one raw request for a target API and compute its deterministic
/// pre-dispatch budget.
pub fn prepare_request(target: &Api, request: CompletionRequest) -> PreparedRequest {
    let history = crate::history::normalize_request_history_for_target(&request, target);
    let mut request = request;
    request.messages = history.messages.clone();
    let budget = estimate_request_budget(&request);
    PreparedRequest::new(request, history, budget)
}
