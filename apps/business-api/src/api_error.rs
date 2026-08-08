use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use shared_kernel::error::AppError;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct ApiError {
    pub error: AppError,
    pub trace_id: Option<String>,
}

impl ApiError {
    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            error: AppError::Validation(message.into()),
            trace_id: None,
        }
    }

    #[must_use]
    pub fn not_found(resource: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            error: AppError::NotFound {
                resource: resource.into(),
                id: id.into(),
            },
            trace_id: None,
        }
    }

    #[must_use]
    pub fn with_trace_id(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.error {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::ExternalService { .. } => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) | AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let internal = matches!(self.error, AppError::Internal(_) | AppError::Database(_));
        if internal {
            tracing::error!(
                error = %self.error,
                trace_id = ?self.trace_id,
                "internal API error"
            );
        }
        (
            status,
            Json(ErrorBody {
                code: self.error.error_code().to_string(),
                message: if internal {
                    "Internal server error".to_string()
                } else {
                    self.error.to_string()
                },
                request_id: self
                    .trace_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                trace_id: self.trace_id,
                details: None,
            }),
        )
            .into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self {
            error,
            trace_id: None,
        }
    }
}

impl From<document::application::CreateDocumentError> for ApiError {
    fn from(error: document::application::CreateDocumentError) -> Self {
        let app_error = match error {
            document::application::CreateDocumentError::Validation(message) => {
                AppError::Validation(message)
            }
            document::application::CreateDocumentError::IdempotencyConflict => {
                AppError::Conflict("idempotency key conflict".to_string())
            }
            document::application::CreateDocumentError::Unavailable => {
                AppError::Database("document persistence unavailable".to_string())
            }
            document::application::CreateDocumentError::Failed => {
                AppError::Database("document persistence failed".to_string())
            }
        };
        Self::from(app_error)
    }
}

impl From<document::query::QueryError> for ApiError {
    fn from(error: document::query::QueryError) -> Self {
        Self::from(AppError::Database(error.to_string()))
    }
}
