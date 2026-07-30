use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

/// 统一应用错误类型
#[derive(Debug, Error)]
pub enum AppError {
    /// 请求参数校验失败
    #[error("Validation failed: {0}")]
    Validation(String),

    /// 资源未找到
    #[error("Resource not found: {resource} {id}")]
    NotFound { resource: String, id: String },

    /// 权限不足
    #[error("Permission denied: {0}")]
    Forbidden(String),

    /// 认证失败
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// 业务规则冲突（如乐观锁版本不一致）
    #[error("Conflict: {0}")]
    Conflict(String),

    /// 请求过于频繁
    #[error("Rate limit exceeded")]
    RateLimited,

    /// 外部服务调用失败
    #[error("External service error: {service}: {message}")]
    ExternalService { service: String, message: String },

    /// 内部错误
    #[error("Internal error: {0}")]
    Internal(String),

    /// 数据库错误
    #[error("Database error: {0}")]
    Database(String),
}

/// 应用结果类型别名
pub type AppResult<T> = Result<T, AppError>;

/// API 错误响应体
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// 错误码，用于客户端程序化处理
    pub code: String,
    /// 人类可读的错误消息
    pub message: String,
    /// 请求追踪 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// 详细错误信息（仅开发环境返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl AppError {
    /// 返回对应的 HTTP 状态码
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::ExternalService { .. } => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 返回错误码字符串
    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "VALIDATION_ERROR",
            AppError::NotFound { .. } => "NOT_FOUND",
            AppError::Forbidden(_) => "FORBIDDEN",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::Conflict(_) => "CONFLICT",
            AppError::RateLimited => "RATE_LIMITED",
            AppError::ExternalService { .. } => "EXTERNAL_SERVICE_ERROR",
            AppError::Internal(_) => "INTERNAL_ERROR",
            AppError::Database(_) => "DATABASE_ERROR",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let is_internal = matches!(self, AppError::Internal(_) | AppError::Database(_));

        // 内部错误记录日志，不向客户端暴露细节
        if is_internal {
            tracing::error!(error = %self, "Internal error occurred");
        }

        let body = ErrorBody {
            code: self.error_code().to_string(),
            message: if is_internal {
                "Internal server error".to_string()
            } else {
                self.to_string()
            },
            trace_id: None,
            details: None,
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound {
                resource: "record".to_string(),
                id: "unknown".to_string(),
            },
            _ => AppError::Database(err.to_string()),
        }
    }
}
