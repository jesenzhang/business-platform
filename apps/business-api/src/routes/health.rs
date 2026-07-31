use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use crate::state::{AppState, ReadinessStatus};

pub async fn liveness() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "business-api"
    }))
}

pub async fn readiness(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let report = state.readiness.check().await;
    if report.status == ReadinessStatus::Ready {
        return Ok(Json(json!({
            "status": "ready",
            "checks": {
                "database": report.database,
                "migrations": report.migrations,
            }
        })));
    }

    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "status": "not_ready",
            "checks": {
                "database": report.database,
                "migrations": report.migrations,
            }
        })),
    ))
}
