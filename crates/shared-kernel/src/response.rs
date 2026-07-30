use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// 统一 API 响应包装
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    /// 是否成功
    pub success: bool,
    /// 响应数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// 提示消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// 成功响应
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    /// 成功响应附带消息
    pub fn ok_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

/// 创建成功响应（201）
pub fn created<T: Serialize>(data: T) -> Response {
    (
        StatusCode::CREATED,
        Json(ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        }),
    )
        .into_response()
}

/// 无内容成功响应（204）
pub fn no_content() -> Response {
    StatusCode::NO_CONTENT.into_response()
}
