//! Thin Axum handlers for document metadata endpoints.
//!
//! Handlers perform protocol translation only: extract auth context, parse
//! request, delegate to application services, and map to HTTP responses.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use shared_kernel::error::AppError;
use shared_kernel::pagination::{PageRequest, PageResponse};
use shared_kernel::tenant::TenantContext;
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::{
    CreateDocumentCommand, CreateDocumentMetadata, GetDocumentMetadata, ListDocumentMetadata,
};
use crate::domain::DocumentRepository;

use super::request::{CreateDocumentRequest, ListDocumentsParams};
use super::response::DocumentResponse;

/// Shared services for document handlers.
///
/// Holds the repository and database pool needed by application use cases.
#[derive(Clone)]
pub struct DocumentServices {
    /// Document repository (port implementation).
    pub repo: Arc<dyn DocumentRepository>,
    /// Database pool for transaction management.
    pub pool: PgPool,
}

/// Unified API response wrapper (mirrors business-api's `ApiResponse`).
#[derive(Debug, serde::Serialize)]
struct ApiResponse<T: serde::Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl<T: serde::Serialize> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }
}

/// API error wrapper for mapping `AppError` to HTTP responses.
pub struct ApiError(AppError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::ExternalService { .. } => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) | AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let is_internal = matches!(self.0, AppError::Internal(_) | AppError::Database(_));

        if is_internal {
            tracing::error!(error = %self.0, "internal error in document handler");
        }

        let body = serde_json::json!({
            "code": self.0.error_code(),
            "message": if is_internal {
                "Internal server error".to_string()
            } else {
                self.0.to_string()
            },
        });

        (status, Json(body)).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

/// Parse tenant context IDs into UUIDs.
fn parse_tenant_ids(tenant: &TenantContext) -> Result<(Uuid, Uuid), ApiError> {
    let tenant_id = Uuid::parse_str(&tenant.tenant_id).map_err(|_| {
        ApiError(AppError::Validation(format!(
            "invalid tenant_id format: {}",
            tenant.tenant_id
        )))
    })?;
    let user_id = Uuid::parse_str(&tenant.user_id).map_err(|_| {
        ApiError(AppError::Validation(format!(
            "invalid user_id format: {}",
            tenant.user_id
        )))
    })?;
    Ok((tenant_id, user_id))
}

/// POST /api/v1/documents
///
/// Create a new document metadata record.
pub async fn create_document(
    axum::Extension(tenant): axum::Extension<TenantContext>,
    State(services): State<DocumentServices>,
    Json(body): Json<CreateDocumentRequest>,
) -> Result<Response, ApiError> {
    let (tenant_id, user_id) = parse_tenant_ids(&tenant)?;

    let cmd = CreateDocumentCommand {
        tenant_id,
        user_id,
        original_filename: body.original_filename,
        content_type: body.content_type,
        object_key: body.object_key,
        size_bytes: body.size_bytes,
    };

    let use_case = CreateDocumentMetadata::new(services.repo.as_ref(), &services.pool);
    let doc = use_case.execute(cmd).await?;

    let response = ApiResponse::ok(DocumentResponse::from(doc));
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

/// GET /api/v1/documents/:id
///
/// Retrieve a single document by ID.
pub async fn get_document(
    axum::Extension(tenant): axum::Extension<TenantContext>,
    State(services): State<DocumentServices>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let (tenant_id, _user_id) = parse_tenant_ids(&tenant)?;

    let use_case = GetDocumentMetadata::new(services.repo.as_ref());
    let doc = use_case.execute(tenant_id, id).await?;

    match doc {
        Some(doc) => {
            let response = ApiResponse::ok(DocumentResponse::from(doc));
            Ok((StatusCode::OK, Json(response)).into_response())
        }
        None => Err(ApiError(AppError::NotFound {
            resource: "document".to_string(),
            id: id.to_string(),
        })),
    }
}

/// GET /api/v1/documents
///
/// List documents for the current tenant with pagination.
pub async fn list_documents(
    axum::Extension(tenant): axum::Extension<TenantContext>,
    State(services): State<DocumentServices>,
    Query(params): Query<ListDocumentsParams>,
) -> Result<Response, ApiError> {
    let (tenant_id, _user_id) = parse_tenant_ids(&tenant)?;

    let page_request = PageRequest {
        page: params.page,
        page_size: params.page_size,
        sort_by: None,
        sort_order: None,
    };

    let use_case = ListDocumentMetadata::new(services.repo.as_ref());
    let result = use_case.execute(tenant_id, &page_request).await?;

    let items: Vec<DocumentResponse> = result
        .items
        .into_iter()
        .map(DocumentResponse::from)
        .collect();
    let page_response = PageResponse::new(items, result.total, result.page, result.page_size);

    let response = ApiResponse::ok(page_response);
    Ok((StatusCode::OK, Json(response)).into_response())
}
