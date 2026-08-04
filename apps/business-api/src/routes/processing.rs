use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use document_processing::ports::FinalizeReviewCommand;
use document_processing::{
    ProcessingJob, ProcessingJobStatus, ProcessingRepositoryError, ReviewCandidateCommand,
    ReviewDecision,
};
use document_processing_contracts::safe_failure_code;
use serde::{Deserialize, Serialize};
use shared_kernel::TenantContext;
use uuid::Uuid;

use crate::api_error::ApiError;
use crate::api_response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProcessingJobRequest {
    pub content_revision: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequest {
    pub decision: ReviewDecision,
    pub candidate_version: i64,
    #[serde(default)]
    pub patch: Option<serde_json::Value>,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProcessingJobResponse {
    pub job_id: Uuid,
    pub document_id: Uuid,
    pub content_revision: i64,
    pub status: ProcessingJobStatus,
    pub current_step: document_processing::ProcessingStepKind,
    pub attempt_count: i32,
    pub failure_code: Option<String>,
    pub cancel_requested: bool,
    pub candidate_available: bool,
    pub review_available: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CandidateResponse {
    pub candidate_id: Uuid,
    pub job_id: Uuid,
    pub content_revision: i64,
    pub schema_version: String,
    pub payload: document_processing::CandidatePayload,
    pub evidence: Vec<document_processing::CandidateEvidence>,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub version: i64,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    #[serde(flatten)]
    pub review: document_processing::CandidateReview,
    pub replayed: bool,
}

impl From<document_processing::ExtractionCandidate> for CandidateResponse {
    fn from(candidate: document_processing::ExtractionCandidate) -> Self {
        let version = candidate.version();
        let created_at = candidate.created_at();
        Self {
            candidate_id: candidate.id(),
            job_id: candidate.job_id(),
            content_revision: candidate.content_revision(),
            schema_version: candidate.schema_version,
            payload: candidate.payload,
            evidence: candidate.evidence,
            provider: candidate.provider,
            model: candidate.model,
            prompt_version: candidate.prompt_version,
            version,
            created_at,
        }
    }
}

fn context(context: &TenantContext) -> Result<(Uuid, Uuid), ApiError> {
    let tenant_id = Uuid::parse_str(&context.tenant_id)
        .map_err(|_| ApiError::validation("invalid tenant context"))?;
    let user_id = Uuid::parse_str(&context.user_id)
        .map_err(|_| ApiError::validation("invalid user context"))?;
    Ok((tenant_id, user_id))
}

fn trace(mut error: ApiError, headers: &HeaderMap) -> ApiError {
    if let Some(request_id) = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
    {
        error = error.with_trace_id(request_id.to_string());
    }
    error
}

fn processing_services(state: &AppState) -> Result<&crate::state::ProcessingServices, ApiError> {
    state.processing.as_ref().ok_or_else(|| {
        ApiError::from(shared_kernel::error::AppError::ExternalService {
            service: "document processing".to_string(),
            message: "processing is unavailable".to_string(),
        })
    })
}

fn map_error(error: &ProcessingRepositoryError) -> ApiError {
    match error {
        ProcessingRepositoryError::NotFound | ProcessingRepositoryError::TenantMismatch => {
            ApiError::not_found("processing_job", "unknown")
        }
        ProcessingRepositoryError::Conflict | ProcessingRepositoryError::IdempotencyConflict => {
            ApiError::from(shared_kernel::error::AppError::Conflict(
                "processing request conflict".to_string(),
            ))
        }
        ProcessingRepositoryError::LeaseLost => ApiError::from(
            shared_kernel::error::AppError::Conflict("processing lease lost".to_string()),
        ),
        ProcessingRepositoryError::Unavailable => {
            ApiError::from(shared_kernel::error::AppError::Database(
                "processing persistence unavailable".to_string(),
            ))
        }
        ProcessingRepositoryError::Failed => ApiError::from(
            shared_kernel::error::AppError::Internal("processing operation failed".to_string()),
        ),
    }
}

fn response(detail: &document_processing::ports::ProcessingJobDetail) -> ProcessingJobResponse {
    ProcessingJobResponse {
        job_id: detail.job.id(),
        document_id: detail.job.document_id(),
        content_revision: detail.job.document_content_revision(),
        status: detail.job.status(),
        current_step: detail.job.current_step(),
        attempt_count: detail.job.attempt_count(),
        failure_code: detail
            .job
            .failure_code()
            .map(safe_failure_code)
            .map(ToOwned::to_owned),
        cancel_requested: detail.job.cancel_requested_at().is_some(),
        candidate_available: detail.candidate.is_some(),
        review_available: detail.review.is_some(),
        created_at: detail.job.created_at(),
        updated_at: detail.job.updated_at(),
    }
}

pub async fn create_for_document(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(document_id): Path<Uuid>,
    Json(body): Json<CreateProcessingJobRequest>,
) -> Result<Response, ApiError> {
    let (tenant_id, user_id) = context(&auth).map_err(|error| trace(error, &headers))?;
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            trace(
                ApiError::validation("Idempotency-Key is required"),
                &headers,
            )
        })?;
    if body.content_revision <= 0 {
        return Err(trace(
            ApiError::validation("content_revision must be positive"),
            &headers,
        ));
    }
    let document = state
        .documents
        .detail
        .execute(tenant_id, document_id)
        .await
        .map_err(|error| trace(ApiError::from(error), &headers))?
        .ok_or_else(|| trace(ApiError::not_found("document", document_id), &headers))?;
    if document.content_revision != body.content_revision {
        return Err(trace(
            ApiError::from(shared_kernel::error::AppError::Conflict(
                "document content revision changed".to_string(),
            )),
            &headers,
        ));
    }
    let services = processing_services(&state)?;
    let job = ProcessingJob::queue(
        tenant_id,
        document_id,
        body.content_revision,
        key.to_string(),
        user_id,
        3,
        Utc::now(),
    )
    .map_err(|_| {
        trace(
            ApiError::validation("invalid processing job request"),
            &headers,
        )
    })?;
    let stored = services
        .execution
        .create_job(&job)
        .await
        .map_err(|error| trace(map_error(&error), &headers))?;
    let detail = services
        .queries
        .detail(tenant_id, stored.id())
        .await
        .map_err(|error| trace(map_error(&error), &headers))?
        .ok_or_else(|| trace(ApiError::not_found("processing_job", stored.id()), &headers))?;
    let status = if stored.id() == job.id() && stored.aggregate_version() == job.aggregate_version()
    {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ApiResponse::ok(response(&detail)))).into_response())
}

pub async fn get_job(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let (tenant_id, _) = context(&auth).map_err(|error| trace(error, &headers))?;
    let services = processing_services(&state)?;
    let detail = services
        .queries
        .detail(tenant_id, job_id)
        .await
        .map_err(|error| trace(map_error(&error), &headers))?
        .ok_or_else(|| trace(ApiError::not_found("processing_job", job_id), &headers))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(response(&detail)))).into_response())
}

pub async fn cancel_job(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let (tenant_id, user_id) = context(&auth).map_err(|error| trace(error, &headers))?;
    let services = processing_services(&state)?;
    let job = services
        .execution
        .cancel_processing(tenant_id, job_id, user_id, Utc::now())
        .await
        .map_err(|error| trace(map_error(&error), &headers))?;
    let detail = services
        .queries
        .detail(tenant_id, job.id())
        .await
        .map_err(|error| trace(map_error(&error), &headers))?
        .ok_or_else(|| trace(ApiError::not_found("processing_job", job_id), &headers))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(response(&detail)))).into_response())
}

pub async fn get_candidate(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let (tenant_id, _) = context(&auth).map_err(|error| trace(error, &headers))?;
    let services = processing_services(&state)?;
    let candidate = services
        .candidate_queries
        .get_candidate(tenant_id, job_id)
        .await
        .map_err(|error| trace(map_error(&error), &headers))?
        .ok_or_else(|| trace(ApiError::not_found("candidate", job_id), &headers))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(CandidateResponse::from(candidate))),
    )
        .into_response())
}

pub async fn review_candidate(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<Uuid>,
    Json(body): Json<ReviewRequest>,
) -> Result<Response, ApiError> {
    let (tenant_id, reviewer_id) = context(&auth).map_err(|error| trace(error, &headers))?;
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            trace(
                ApiError::validation("Idempotency-Key is required"),
                &headers,
            )
        })?;
    if idempotency_key.len() > 255 {
        return Err(trace(
            ApiError::validation("Idempotency-Key is too long"),
            &headers,
        ));
    }
    let services = processing_services(&state)?;
    let detail = services
        .queries
        .detail(tenant_id, job_id)
        .await
        .map_err(|error| trace(map_error(&error), &headers))?
        .ok_or_else(|| trace(ApiError::not_found("processing_job", job_id), &headers))?;
    let candidate = detail
        .candidate
        .as_ref()
        .ok_or_else(|| trace(ApiError::not_found("candidate", job_id), &headers))?;
    if body.candidate_version != candidate.version() {
        return Err(trace(
            ApiError::from(shared_kernel::error::AppError::Conflict(
                "candidate version conflict".to_string(),
            )),
            &headers,
        ));
    }
    let review_command = ReviewCandidateCommand {
        tenant_id,
        job_id,
        reviewer_id,
        decision: body.decision,
        patch: body.patch,
        comment: body.comment,
        candidate_version: body.candidate_version,
    };
    let request_fingerprint = review_command
        .request_fingerprint(candidate.id())
        .map_err(|_| trace(ApiError::validation("invalid review request"), &headers))?;
    let review = review_command.build_review(candidate.id());
    review
        .validate(candidate)
        .map_err(|_| trace(ApiError::validation("invalid review"), &headers))?;
    let finalized = services
        .execution
        .finalize_review(
            FinalizeReviewCommand {
                tenant_id,
                job_id,
                idempotency_key: idempotency_key.to_string(),
                request_fingerprint,
                review,
            },
            Utc::now(),
        )
        .await
        .map_err(|error| trace(map_error(&error), &headers))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(ReviewResponse {
            review: finalized.review,
            replayed: finalized.replayed,
        })),
    )
        .into_response())
}
