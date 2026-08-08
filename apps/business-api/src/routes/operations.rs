use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, Utc};
use document::query::DocumentListFilter;
use document_processing::ports::ProcessingJobListRequest;
use public_api_contracts::{OperationsOverview, ProcessingStatusCounts};
use shared_kernel::TenantContext;
use uuid::Uuid;

use crate::api_error::ApiError;
use crate::api_response::ApiResponse;
use crate::routes::public_dto;
use crate::state::AppState;

const RECENT_JOB_LIMIT: u32 = 20;

fn tenant(context: &TenantContext) -> Result<Uuid, ApiError> {
    Uuid::parse_str(&context.tenant_id).map_err(|_| ApiError::validation("invalid tenant context"))
}

#[allow(clippy::too_many_lines)]
pub async fn overview(
    axum::Extension(context): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, ApiError> {
    let tenant_id = tenant(&context)?;
    let document_total = state
        .documents
        .list
        .count(tenant_id, DocumentListFilter::default())
        .await
        .map_err(ApiError::from)?;
    let start_of_day = Utc::now() - Duration::days(1);
    let document_created_today = state
        .documents
        .list
        .count(
            tenant_id,
            DocumentListFilter {
                created_after: Some(start_of_day),
                ..Default::default()
            },
        )
        .await
        .map_err(ApiError::from)?;

    let processing = state.processing.as_ref().ok_or_else(|| {
        ApiError::from(shared_kernel::error::AppError::ExternalService {
            service: "document processing".to_string(),
            message: "processing is unavailable".to_string(),
        })
    })?;
    let jobs = processing
        .queries
        .list(ProcessingJobListRequest {
            tenant_id,
            document_id: None,
            cursor: None,
            limit: RECENT_JOB_LIMIT,
        })
        .await
        .map_err(|_| {
            ApiError::from(shared_kernel::error::AppError::Database(
                "processing query failed".to_string(),
            ))
        })?;
    let processing_counts = processing
        .queries
        .status_counts(tenant_id)
        .await
        .map_err(|_| {
            ApiError::from(shared_kernel::error::AppError::Database(
                "processing query failed".to_string(),
            ))
        })?;
    let counts = ProcessingStatusCounts {
        queued: processing_counts.queued,
        running: processing_counts.running,
        waiting_for_ai: processing_counts.waiting_for_ai,
        waiting_for_review: processing_counts.waiting_for_review,
        succeeded: processing_counts.succeeded,
        failed: processing_counts.failed,
        cancelled: processing_counts.cancelled,
        rejected: processing_counts.rejected,
    };

    let (unresolved_findings, audit_events) = if let Some(governance) = &state.governance {
        let unresolved = governance
            .integrity_queries
            .count_unresolved(tenant_id)
            .await
            .map_err(|_| {
                ApiError::from(shared_kernel::error::AppError::Database(
                    "governance query failed".to_string(),
                ))
            })?;
        let audit = governance
            .audit_queries
            .list(audit::AuditQueryRequest {
                tenant_id,
                limit: 10,
                ..Default::default()
            })
            .await
            .map_err(|_| {
                ApiError::from(shared_kernel::error::AppError::Database(
                    "audit query failed".to_string(),
                ))
            })?;
        (
            unresolved,
            audit.items.iter().map(public_dto::audit).collect(),
        )
    } else {
        (0, Vec::new())
    };

    let response = OperationsOverview {
        document_total,
        document_created_today,
        review_pending: counts.waiting_for_review,
        processing_by_status: counts,
        unresolved_findings,
        recent_jobs: jobs.items.iter().map(public_dto::processing_job).collect(),
        recent_audit_events: audit_events,
    };
    Ok((StatusCode::OK, Json(ApiResponse::ok(response))).into_response())
}
