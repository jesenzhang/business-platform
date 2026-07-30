use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use crate::state::AppState;

/// Liveness probe - 服务是否存活
pub async fn liveness() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "business-api"
    }))
}

/// Readiness probe - 服务是否就绪（含数据库连接检查）
pub async fn readiness(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => Ok(Json(json!({
            "status": "ready",
            "database": "connected"
        }))),
        Err(e) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "database": format!("error: {e}")
            })),
        )),
    }
}
