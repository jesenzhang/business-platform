use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use shared_kernel::TenantContext;
use uuid::Uuid;

use crate::api_error::ApiError;
use crate::api_response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateDocumentRequest {
    pub original_filename: String,
    pub content_type: String,
    pub object_key: String,
    #[serde(default)]
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListDocumentsParams {
    #[serde(default = "default_page_size", alias = "page_size")]
    pub limit: u32,
    pub cursor_created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub cursor_id: Option<Uuid>,
    pub status: Option<String>,
    pub filename_contains: Option<String>,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct DocumentResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_key: Option<String>,
    pub status: String,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<document::domain::DocumentMetadata> for DocumentResponse {
    fn from(document: document::domain::DocumentMetadata) -> Self {
        Self {
            id: document.id,
            tenant_id: document.tenant_id,
            original_filename: document.original_filename,
            content_type: document.content_type,
            object_key: Some(document.object_key),
            status: document.status.as_str().to_string(),
            version: document.version,
            size_bytes: document.size_bytes,
            created_by: document.created_by,
            created_at: document.created_at,
            updated_at: document.updated_at,
        }
    }
}

impl From<document::query::DocumentDetailView> for DocumentResponse {
    fn from(document: document::query::DocumentDetailView) -> Self {
        Self {
            id: document.id,
            tenant_id: document.tenant_id,
            original_filename: document.original_filename,
            content_type: document.content_type,
            object_key: None,
            status: document.status.as_str().to_string(),
            version: document.version,
            size_bytes: document.size_bytes,
            created_by: document.created_by,
            created_at: document.created_at,
            updated_at: document.updated_at,
        }
    }
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/",
            axum::routing::post(create_document).get(list_documents),
        )
        .route("/{id}", axum::routing::get(get_document))
}

use std::sync::Arc;

fn parse_context(context: &TenantContext) -> Result<(Uuid, Uuid), ApiError> {
    let tenant_id = Uuid::parse_str(&context.tenant_id)
        .map_err(|_| ApiError::validation("invalid tenant context"))?;
    let user_id = Uuid::parse_str(&context.user_id)
        .map_err(|_| ApiError::validation("invalid user context"))?;
    Ok((tenant_id, user_id))
}

fn trace_error(mut error: ApiError, headers: &HeaderMap) -> ApiError {
    if let Some(value) = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
    {
        error = error.with_trace_id(value.to_string());
    }
    error
}

pub async fn create_document(
    axum::Extension(context): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateDocumentRequest>,
) -> Result<Response, ApiError> {
    let (tenant_id, user_id) =
        parse_context(&context).map_err(|error| trace_error(error, &headers))?;
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let command = document::application::CreateDocumentCommand {
        tenant_id,
        user_id,
        original_filename: body.original_filename,
        content_type: body.content_type,
        object_key: body.object_key,
        size_bytes: body.size_bytes,
        idempotency_key,
    };
    let result = state
        .documents
        .create
        .execute(command)
        .await
        .map_err(|error| trace_error(ApiError::from(error), &headers))?;
    let status = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ApiResponse::ok(DocumentResponse::from(result.document))),
    )
        .into_response())
}

pub async fn get_document(
    axum::Extension(context): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(document_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let (tenant_id, _) = parse_context(&context).map_err(|error| trace_error(error, &headers))?;
    let result = state
        .documents
        .detail
        .execute(tenant_id, document_id)
        .await
        .map_err(|error| trace_error(ApiError::from(error), &headers))?
        .ok_or_else(|| trace_error(ApiError::not_found("document", document_id), &headers))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(DocumentResponse::from(result))),
    )
        .into_response())
}

pub async fn list_documents(
    axum::Extension(context): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ListDocumentsParams>,
) -> Result<Response, ApiError> {
    let (tenant_id, _) = parse_context(&context).map_err(|error| trace_error(error, &headers))?;
    let status = match params.status.as_deref() {
        None => None,
        Some("active") => Some(document::query::DocumentStatusFilter::Active),
        Some("archived") => Some(document::query::DocumentStatusFilter::Archived),
        Some("deleted") => Some(document::query::DocumentStatusFilter::Deleted),
        Some(_) => {
            return Err(trace_error(
                ApiError::validation("invalid document status"),
                &headers,
            ))
        }
    };
    let cursor = match (params.cursor_created_at, params.cursor_id) {
        (Some(created_at), Some(id)) => {
            Some(document::query::DocumentListCursor { created_at, id })
        }
        (None, None) => None,
        _ => {
            return Err(trace_error(
                ApiError::validation("cursor_created_at and cursor_id must be provided together"),
                &headers,
            ))
        }
    };
    let result = state
        .documents
        .list
        .execute(document::query::DocumentListRequest {
            tenant_id,
            filter: document::query::DocumentListFilter {
                status,
                filename_contains: params.filename_contains,
                created_after: params.created_after,
                created_before: params.created_before,
            },
            cursor,
            limit: params.limit,
        })
        .await
        .map_err(|error| trace_error(ApiError::from(error), &headers))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(result))).into_response())
}

fn default_page_size() -> u32 {
    20
}
