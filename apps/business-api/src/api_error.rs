//! HTTP error mapping layer.
//!
//! Converts application errors into HTTP responses with stable error codes.
//! Internal errors are logged but never expose details to clients.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use shared_kernel::error::AppError;

/// API error response body.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// 错误码，用于客户端程序化处理
    pub code: String,
    /// 人类可读的错误消息
    pub message: String,
    /// 请求追踪 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

/// Wrapper that implements `IntoResponse` for `AppError`.
pub struct ApiError(pub AppError);

impl ApiError {
    fn status_code(&self) -> StatusCode {
        match &self.0 {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::ExternalService { .. } => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) | AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let is_internal = matches!(self.0, AppError::Internal(_) | AppError::Database(_));

        if is_internal {
            tracing::error!(error = %self.0, "internal error");
        }

        let body = ErrorBody {
            code: self.0.error_code().to_string(),
            message: if is_internal {
                "Internal server error".to_string()
            } else {
                self.0.to_string()
            },
            trace_id: None,
        };

        (status, Json(body)).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self(AppError::NotFound {
                resource: "record".to_string(),
                id: "unknown".to_string(),
            }),
            _ => Self(AppError::Database(err.to_string())),
        }
    }
}
