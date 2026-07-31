use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use shared_kernel::pagination::{PageRequest, PageResponse};
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
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

#[derive(Debug, Serialize)]
pub struct DocumentResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub object_key: String,
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
            object_key: document.object_key,
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
        .get
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
    let page = PageRequest {
        page: params.page,
        page_size: params.page_size,
        sort_by: None,
        sort_order: None,
    };
    let result = state
        .documents
        .list
        .execute(tenant_id, &page)
        .await
        .map_err(|error| trace_error(ApiError::from(error), &headers))?;
    let response = PageResponse::new(
        result
            .items
            .into_iter()
            .map(DocumentResponse::from)
            .collect::<Vec<_>>(),
        result.total,
        result.page,
        result.page_size,
    );
    Ok((StatusCode::OK, Json(ApiResponse::ok(response))).into_response())
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}
