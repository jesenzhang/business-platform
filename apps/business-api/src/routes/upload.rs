use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::api_error::ApiError;
use crate::api_response::ApiResponse;
use crate::routes::documents::DocumentResponse;
use crate::state::AppState;
use object_storage::{ObjectKey, StorageError};
use shared_kernel::TenantContext;

const MAX_UPLOAD_BYTES: u64 = 10 * 1024 * 1024;
const ALLOWED_CONTENT_TYPES: [&str; 4] = [
    "application/pdf",
    "text/plain",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
];

fn trace(mut error: ApiError, headers: &HeaderMap) -> ApiError {
    if let Some(request_id) = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
    {
        error = error.with_trace_id(request_id.to_string());
    }
    error
}

fn deterministic_document_id(tenant_id: Uuid, idempotency_key: &str) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"business-platform-upload:v1\0");
    digest.update(tenant_id.as_bytes());
    digest.update(idempotency_key.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn deterministic_revision_id(
    tenant_id: Uuid,
    idempotency_key: &str,
    content_sha256: &[u8],
) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"business-platform-upload-revision:v1\0");
    digest.update(tenant_id.as_bytes());
    digest.update(idempotency_key.as_bytes());
    digest.update(content_sha256);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn storage_error(_: StorageError) -> ApiError {
    ApiError::from(shared_kernel::error::AppError::ExternalService {
        service: "object storage".to_string(),
        message: "object storage is unavailable".to_string(),
    })
}

#[allow(clippy::too_many_lines)]
pub async fn upload_document(
    axum::Extension(context): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let tenant_id = Uuid::parse_str(&context.tenant_id)
        .map_err(|_| trace(ApiError::validation("invalid tenant context"), &headers))?;
    let user_id = Uuid::parse_str(&context.user_id)
        .map_err(|_| trace(ApiError::validation("invalid user context"), &headers))?;
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
    let storage = state.storage.as_ref().ok_or_else(|| {
        trace(
            ApiError::from(shared_kernel::error::AppError::ExternalService {
                service: "object storage".to_string(),
                message: "object storage is unavailable".to_string(),
            }),
            &headers,
        )
    })?;

    let mut filename = None;
    let mut content_type = None;
    let temporary_path = std::env::temp_dir().join(format!("bp-upload-{}.tmp", Uuid::now_v7()));
    let mut file = tokio::fs::File::create(&temporary_path)
        .await
        .map_err(|_| {
            trace(
                ApiError::from(shared_kernel::error::AppError::Internal(
                    "upload staging failed".to_string(),
                )),
                &headers,
            )
        })?;
    let mut size = 0_u64;
    let mut content_hasher = Sha256::new();
    let mut found_file = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| trace(ApiError::validation("malformed multipart upload"), &headers))?
    {
        if field.name() != Some("file") {
            while field
                .chunk()
                .await
                .map_err(|_| trace(ApiError::validation("malformed multipart upload"), &headers))?
                .is_some()
            {}
            continue;
        }
        if found_file {
            return Err(trace(
                ApiError::validation("only one file is allowed"),
                &headers,
            ));
        }
        found_file = true;
        filename = field.file_name().map(ToOwned::to_owned);
        content_type = field.content_type().map(ToOwned::to_owned);
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| trace(ApiError::validation("malformed multipart upload"), &headers))?
        {
            size = size.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
            if size > MAX_UPLOAD_BYTES {
                let _ = tokio::fs::remove_file(&temporary_path).await;
                return Err(trace(
                    ApiError::validation("upload exceeds the 10 MiB limit"),
                    &headers,
                ));
            }
            content_hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|_| {
                trace(
                    ApiError::from(shared_kernel::error::AppError::Internal(
                        "upload staging failed".to_string(),
                    )),
                    &headers,
                )
            })?;
        }
    }
    file.flush().await.map_err(|_| {
        trace(
            ApiError::from(shared_kernel::error::AppError::Internal(
                "upload staging failed".to_string(),
            )),
            &headers,
        )
    })?;
    drop(file);

    let filename = filename
        .filter(|value| !value.trim().is_empty() && value.len() <= 500)
        .ok_or_else(|| trace(ApiError::validation("file name is required"), &headers))?;
    let content_type = content_type
        .filter(|value| ALLOWED_CONTENT_TYPES.contains(&value.as_str()))
        .ok_or_else(|| trace(ApiError::validation("unsupported content type"), &headers))?;
    if !found_file {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(trace(ApiError::validation("file is required"), &headers));
    }

    let document_id = deterministic_document_id(tenant_id, idempotency_key);
    let content_sha256 = content_hasher.finalize();
    let content_sha256_hex = format!("{content_sha256:x}");
    let revision_id = deterministic_revision_id(tenant_id, idempotency_key, &content_sha256);
    let mut name_digest = Sha256::new();
    name_digest.update(tenant_id.as_bytes());
    name_digest.update(idempotency_key.as_bytes());
    let logical_path = format!("uploads/{:x}.bin", name_digest.finalize());
    let command = document::application::CreateDocumentCommand {
        tenant_id,
        user_id,
        original_filename: filename,
        content_type: content_type.clone(),
        object_key: logical_path,
        size_bytes: Some(i64::try_from(size).unwrap_or(i64::MAX)),
        sha256: Some(content_sha256_hex.clone()),
        revision_id: Some(revision_id),
        idempotency_key: idempotency_key.to_string(),
    };
    let document = document::domain::DocumentMetadata::create_with_revision_id(
        document_id,
        tenant_id,
        command.original_filename.clone(),
        command.content_type.clone(),
        command.object_key.clone(),
        user_id,
        command.size_bytes,
        revision_id,
    )
    .map_err(|_| trace(ApiError::validation("invalid upload metadata"), &headers))?;
    let object_key = ObjectKey::new(document.object_key())
        .map_err(|_| trace(ApiError::validation("invalid upload key"), &headers))?;
    let input = tokio::fs::File::open(&temporary_path).await.map_err(|_| {
        trace(
            ApiError::from(shared_kernel::error::AppError::Internal(
                "upload staging failed".to_string(),
            )),
            &headers,
        )
    })?;
    let body = ReaderStream::new(input)
        .map(|chunk| chunk.map_err(|error| StorageError::Io(error.to_string())));
    let mut storage_metadata = BTreeMap::new();
    storage_metadata.insert("sha256".to_string(), content_sha256_hex);
    if let Err(error) = storage
        .objects
        .put_stream(
            &object_key,
            Box::pin(body),
            size,
            &content_type,
            &storage_metadata,
        )
        .await
    {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(trace(storage_error(error), &headers));
    }

    let result = state
        .documents
        .create
        .execute_with_id(Some(document_id), command)
        .await;
    let _ = tokio::fs::remove_file(&temporary_path).await;
    match result {
        Ok(result) => {
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
        Err(error) => {
            let _ = storage.objects.delete(&object_key).await;
            Err(trace(ApiError::from(error), &headers))
        }
    }
}
