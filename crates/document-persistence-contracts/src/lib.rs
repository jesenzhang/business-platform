//! Reusable behavior contracts for Document persistence adapters.
//! This crate is consumed only as a development dependency by adapters.

use std::sync::Arc;

use document::domain::DocumentMetadata;
use document::ports::{ApplicationPortError, CreateDocumentUnitOfWork, PersistNewDocument};
use document::query::{
    DocumentDetailQuery, DocumentListFilter, DocumentListQuery, DocumentListRequest,
};
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub async fn verify_document_persistence_contract(
    unit_of_work: Arc<dyn CreateDocumentUnitOfWork>,
    detail: Arc<dyn DocumentDetailQuery>,
    list: Arc<dyn DocumentListQuery>,
) -> Result<(), String> {
    let tenant = Uuid::now_v7();
    let other_tenant = Uuid::now_v7();
    let user = Uuid::now_v7();
    let first = DocumentMetadata::create(
        tenant,
        "contract-a.pdf".to_string(),
        "application/pdf".to_string(),
        "uploads/a.pdf".to_string(),
        user,
        Some(12),
    )
    .map_err(|error| error.to_string())?;
    let command = PersistNewDocument {
        document: first.clone(),
        idempotency_key: "contract-key-a".to_string(),
        request_fingerprint: "fingerprint-a".to_string(),
        fingerprint_version: 1,
    };
    let created = unit_of_work
        .execute(command.clone())
        .await
        .map_err(|error| error.to_string())?;
    if created.replayed || created.document.id != first.id {
        return Err("create did not return the new document".to_string());
    }
    let replay = unit_of_work
        .execute(command.clone())
        .await
        .map_err(|error| error.to_string())?;
    if !replay.replayed || replay.document.id != first.id {
        return Err("idempotent replay did not return the original document".to_string());
    }
    let mut conflict = command;
    conflict.request_fingerprint = "different".to_string();
    if unit_of_work.execute(conflict).await != Err(ApplicationPortError::IdempotencyConflict) {
        return Err("idempotency conflict was not rejected".to_string());
    }
    let found = detail
        .execute(tenant, first.id)
        .await
        .map_err(|error| error.to_string())?;
    if found.as_ref().map(|view| view.id) != Some(first.id) {
        return Err("detail query did not return created document".to_string());
    }
    if detail
        .execute(other_tenant, first.id)
        .await
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("detail query crossed tenant boundary".to_string());
    }

    let mut invalid = first.clone();
    invalid.id = Uuid::now_v7();
    invalid.size_bytes = Some(-1);
    let invalid_result = unit_of_work
        .execute(PersistNewDocument {
            document: invalid.clone(),
            idempotency_key: "contract-negative-size".to_string(),
            request_fingerprint: "contract-negative-size".to_string(),
            fingerprint_version: 1,
        })
        .await;
    if invalid_result.is_ok()
        || detail
            .execute(tenant, invalid.id)
            .await
            .map_err(|error| error.to_string())?
            .is_some()
    {
        return Err("negative size crossed the persistence seam".to_string());
    }

    for index in 0..3 {
        let document = DocumentMetadata::create(
            tenant,
            format!("contract-{index}.txt"),
            "text/plain".to_string(),
            format!("uploads/{index}.txt"),
            user,
            Some(index),
        )
        .map_err(|error| error.to_string())?;
        unit_of_work
            .execute(PersistNewDocument {
                document,
                idempotency_key: format!("contract-key-{index}"),
                request_fingerprint: format!("fingerprint-{index}"),
                fingerprint_version: 1,
            })
            .await
            .map_err(|error| error.to_string())?;
    }
    let first_page = list
        .execute(DocumentListRequest {
            tenant_id: tenant,
            filter: DocumentListFilter::default(),
            cursor: None,
            limit: 2,
        })
        .await
        .map_err(|error| error.to_string())?;
    if first_page.items.len() != 2 || first_page.next_cursor.is_none() {
        return Err("first cursor page is invalid".to_string());
    }
    let second_page = list
        .execute(DocumentListRequest {
            tenant_id: tenant,
            filter: DocumentListFilter {
                filename_contains: Some("contract".to_string()),
                ..DocumentListFilter::default()
            },
            cursor: first_page.next_cursor,
            limit: 2,
        })
        .await
        .map_err(|error| error.to_string())?;
    if second_page.items.is_empty()
        || first_page
            .items
            .iter()
            .any(|left| second_page.items.iter().any(|right| left.id == right.id))
    {
        return Err("stable cursor pagination returned duplicates".to_string());
    }
    let foreign_page = list
        .execute(DocumentListRequest {
            tenant_id: other_tenant,
            filter: DocumentListFilter::default(),
            cursor: None,
            limit: 20,
        })
        .await
        .map_err(|error| error.to_string())?;
    if !foreign_page.items.is_empty() {
        return Err("list query crossed tenant boundary".to_string());
    }
    Ok(())
}
