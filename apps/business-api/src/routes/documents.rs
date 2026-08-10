use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};
use shared_kernel::TenantContext;
use uuid::Uuid;

use public_api_contracts as contracts;

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
#[serde(deny_unknown_fields)]
pub struct ListDocumentsParams {
    #[serde(default = "default_page_size", alias = "page_size")]
    pub limit: u32,
    pub cursor: Option<String>,
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
    pub status: String,
    pub version: i64,
    pub content_revision: i64,
    pub revision_id: Uuid,
    pub revision_no: i64,
    pub is_current: bool,
    pub size_bytes: Option<i64>,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<document::domain::DocumentMetadata> for DocumentResponse {
    fn from(document: document::domain::DocumentMetadata) -> Self {
        Self {
            id: document.id(),
            tenant_id: document.tenant_id(),
            original_filename: document.original_filename().to_string(),
            content_type: document.content_type().to_string(),
            status: document.status().as_str().to_string(),
            version: document.version(),
            content_revision: document.content_revision().value(),
            revision_id: document.current_revision_id(),
            revision_no: document.content_revision().value(),
            is_current: true,
            size_bytes: document.size_bytes(),
            created_by: document.created_by(),
            created_at: document.created_at(),
            updated_at: document.updated_at(),
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
            status: document.status.as_str().to_string(),
            version: document.version,
            content_revision: document.content_revision,
            revision_id: document.revision_id,
            revision_no: document.revision_no,
            is_current: document.is_current,
            size_bytes: document.size_bytes,
            created_by: document.created_by,
            created_at: document.created_at,
            updated_at: document.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct DocumentListResponse {
    items: Vec<contracts::Document>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CursorToken {
    version: u8,
    created_at: chrono::DateTime<chrono::Utc>,
    id: Uuid,
}

fn encode_cursor(cursor: document::query::DocumentListCursor) -> String {
    let token = CursorToken {
        version: 1,
        created_at: cursor.created_at,
        id: cursor.id,
    };
    let payload = serde_json::to_vec(&token).unwrap_or_default();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload)
}

fn decode_cursor(value: &str) -> Result<document::query::DocumentListCursor, ApiError> {
    if value.is_empty() || value.len() > 512 {
        return Err(ApiError::validation("invalid cursor"));
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::validation("invalid cursor"))?;
    let token: CursorToken =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::validation("invalid cursor"))?;
    if token.version != 1 || token.id.is_nil() {
        return Err(ApiError::validation("invalid cursor"));
    }
    Ok(document::query::DocumentListCursor {
        created_at: token.created_at,
        id: token.id,
    })
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/",
            axum::routing::post(create_document).get(list_documents),
        )
        .route(
            "/upload",
            axum::routing::post(crate::routes::upload::upload_document),
        )
        .route("/{id}", axum::routing::get(get_document))
        .route(
            "/{id}/processing-jobs",
            axum::routing::post(crate::routes::processing::create_for_document)
                .get(crate::routes::processing::list_for_document),
        )
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
        sha256: None,
        revision_id: None,
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
        Json(ApiResponse::ok(crate::routes::public_dto::document(result))),
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
    let cursor = params
        .cursor
        .as_deref()
        .map(decode_cursor)
        .transpose()
        .map_err(|error| trace_error(error, &headers))?;
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
    let response = DocumentListResponse {
        items: result
            .items
            .into_iter()
            .map(crate::routes::public_dto::document_list_item)
            .collect(),
        next_cursor: result.next_cursor.map(encode_cursor),
    };
    Ok((StatusCode::OK, Json(ApiResponse::ok(response))).into_response())
}

fn default_page_size() -> u32 {
    20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_an_opaque_versioned_token() {
        let cursor = document::query::DocumentListCursor {
            created_at: chrono::Utc::now(),
            id: Uuid::now_v7(),
        };
        let encoded = encode_cursor(cursor);
        assert!(!encoded.contains('|'));
        assert!(!encoded.contains("created_at"));
        let decoded = decode_cursor(&encoded);
        assert_eq!(decoded.ok(), Some(cursor));
    }

    #[test]
    fn cursor_rejects_malformed_or_unknown_versions() {
        assert!(decode_cursor("not-base64").is_err());
        let payload = serde_json::json!({
            "version": 2,
            "created_at": chrono::Utc::now(),
            "id": Uuid::now_v7(),
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap_or_default());
        assert!(decode_cursor(&encoded).is_err());
        assert!(decode_cursor(&"a".repeat(513)).is_err());
    }
}
