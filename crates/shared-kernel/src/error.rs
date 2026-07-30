use thiserror::Error;

/// Protocol-agnostic error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// 4xx-class: validation, not found, forbidden, unauthorized, conflict, rate limit.
    Client,
    /// External service failure.
    Upstream,
    /// Server-side bug.
    Internal,
}

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

impl AppError {
    /// Returns the protocol-agnostic error category.
    #[must_use]
    pub fn category(&self) -> ErrorCategory {
        match self {
            AppError::Validation(_)
            | AppError::NotFound { .. }
            | AppError::Forbidden(_)
            | AppError::Unauthorized(_)
            | AppError::Conflict(_)
            | AppError::RateLimited => ErrorCategory::Client,
            AppError::ExternalService { .. } => ErrorCategory::Upstream,
            AppError::Internal(_) | AppError::Database(_) => ErrorCategory::Internal,
        }
    }

    /// 返回错误码字符串
    #[must_use]
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
