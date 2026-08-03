//! Reusable behavior contracts for Document persistence adapters.
//!
//! The contract is deliberately split by invariant so a failing adapter test
//! points at the violated boundary instead of a long scenario function.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use document::domain::{DocumentMetadata, DocumentStatus, RehydrateDocumentMetadata};
use document::ports::{ApplicationPortError, CreateDocumentUnitOfWork, PersistNewDocument};
use document::query::{
    DocumentDetailQuery, DocumentListFilter, DocumentListQuery, DocumentListRequest,
};
use uuid::Uuid;

pub async fn verify_document_persistence_contract(
    unit_of_work: Arc<dyn CreateDocumentUnitOfWork>,
    detail: Arc<dyn DocumentDetailQuery>,
    list: Arc<dyn DocumentListQuery>,
) -> Result<(), String> {
    let context = ContractContext::new()?;
    verify_create(&unit_of_work, &context).await?;
    verify_idempotency(&unit_of_work, &context).await?;
    verify_tenant_isolation(&detail, &list, &context).await?;
    verify_filters(&unit_of_work, &list, &context).await?;
    verify_cursor(&unit_of_work, &list, &context).await?;
    verify_invalid_stored_data();
    Ok(())
}

#[derive(Clone)]
struct ContractContext {
    tenant: Uuid,
    other_tenant: Uuid,
    user: Uuid,
    first: DocumentMetadata,
    command: PersistNewDocument,
}

impl ContractContext {
    fn new() -> Result<Self, String> {
        let tenant = Uuid::now_v7();
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
            request_fingerprint: "a".repeat(64),
            fingerprint_version: 1,
        };
        Ok(Self {
            tenant,
            other_tenant: Uuid::now_v7(),
            user,
            first,
            command,
        })
    }
}

async fn verify_create(
    unit_of_work: &Arc<dyn CreateDocumentUnitOfWork>,
    context: &ContractContext,
) -> Result<(), String> {
    let created = unit_of_work
        .execute(context.command.clone())
        .await
        .map_err(|error| error.to_string())?;
    if created.replayed || created.document.id() != context.first.id() {
        return Err("create did not return the new document".to_string());
    }
    Ok(())
}

async fn verify_idempotency(
    unit_of_work: &Arc<dyn CreateDocumentUnitOfWork>,
    context: &ContractContext,
) -> Result<(), String> {
    let replay = unit_of_work
        .execute(context.command.clone())
        .await
        .map_err(|error| error.to_string())?;
    if !replay.replayed || replay.document.id() != context.first.id() {
        return Err("idempotent replay did not return the original document".to_string());
    }
    let mut conflict = context.command.clone();
    conflict.request_fingerprint = "b".repeat(64);
    if unit_of_work.execute(conflict).await != Err(ApplicationPortError::IdempotencyConflict) {
        return Err("idempotency conflict was not rejected".to_string());
    }
    Ok(())
}

async fn verify_tenant_isolation(
    detail: &Arc<dyn DocumentDetailQuery>,
    list: &Arc<dyn DocumentListQuery>,
    context: &ContractContext,
) -> Result<(), String> {
    let found = detail
        .execute(context.tenant, context.first.id())
        .await
        .map_err(|error| error.to_string())?;
    if found.as_ref().map(|view| view.id) != Some(context.first.id()) {
        return Err("detail query did not return created document".to_string());
    }
    if detail
        .execute(context.other_tenant, context.first.id())
        .await
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("detail query crossed tenant boundary".to_string());
    }
    let foreign_page = list
        .execute(DocumentListRequest {
            tenant_id: context.other_tenant,
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

async fn verify_filters(
    unit_of_work: &Arc<dyn CreateDocumentUnitOfWork>,
    list: &Arc<dyn DocumentListQuery>,
    context: &ContractContext,
) -> Result<(), String> {
    let now = Utc::now();
    let fixtures = [
        (
            "contract-active.txt",
            DocumentStatus::Active,
            now - Duration::seconds(3),
        ),
        (
            "contract-archived.txt",
            DocumentStatus::Archived,
            now - Duration::seconds(2),
        ),
        (
            "contract-deleted.txt",
            DocumentStatus::Deleted,
            now - Duration::seconds(1),
        ),
        ("foo.pdf", DocumentStatus::Active, now),
        ("100%.pdf", DocumentStatus::Active, now),
        ("under_score.pdf", DocumentStatus::Active, now),
        (r"back\slash.pdf", DocumentStatus::Active, now),
        ("中文合同.pdf", DocumentStatus::Active, now),
        ("Report.PDF", DocumentStatus::Active, now),
    ];
    for (index, (filename, status, created_at)) in fixtures.into_iter().enumerate() {
        let document = rehydrated_document(
            context.tenant,
            context.user,
            Uuid::now_v7(),
            filename,
            status,
            created_at,
        )?;
        unit_of_work
            .execute(PersistNewDocument {
                document,
                idempotency_key: format!("contract-filter-{index}"),
                request_fingerprint: format!("filter-{index}").repeat(16),
                fingerprint_version: 1,
            })
            .await
            .map_err(|error| error.to_string())?;
    }

    verify_status_filter(list, context).await?;
    verify_filename_filters(list, context, now).await?;
    verify_limit_clamp(list, context).await
}

async fn verify_status_filter(
    list: &Arc<dyn DocumentListQuery>,
    context: &ContractContext,
) -> Result<(), String> {
    let archived = list
        .execute(DocumentListRequest {
            tenant_id: context.tenant,
            filter: DocumentListFilter {
                status: Some(document::query::DocumentStatusFilter::Archived),
                ..DocumentListFilter::default()
            },
            cursor: None,
            limit: 200,
        })
        .await
        .map_err(|error| error.to_string())?;
    if archived.items.len() != 1
        || archived.items[0].status != document::query::DocumentStatusView::Archived
    {
        return Err("status filter did not isolate archived documents".to_string());
    }
    Ok(())
}

async fn verify_filename_filters(
    list: &Arc<dyn DocumentListQuery>,
    context: &ContractContext,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let name_and_date = list
        .execute(DocumentListRequest {
            tenant_id: context.tenant,
            filter: DocumentListFilter {
                filename_contains: Some("Report".to_string()),
                created_after: Some(now - Duration::seconds(1)),
                created_before: Some(now + Duration::seconds(1)),
                ..DocumentListFilter::default()
            },
            cursor: None,
            limit: 200,
        })
        .await
        .map_err(|error| error.to_string())?;
    if name_and_date.items.len() != 1 || name_and_date.items[0].original_filename != "Report.PDF" {
        return Err("combined filename/date filters did not match exactly".to_string());
    }

    for needle in [
        "foo.pdf",
        "100%.pdf",
        "under_score.pdf",
        r"back\slash.pdf",
        "中文合同.pdf",
        "report.pdf",
    ] {
        let page = list
            .execute(DocumentListRequest {
                tenant_id: context.tenant,
                filter: DocumentListFilter {
                    filename_contains: Some(needle.to_string()),
                    ..DocumentListFilter::default()
                },
                cursor: None,
                limit: 200,
            })
            .await
            .map_err(|error| error.to_string())?;
        if page.items.len() != 1 {
            return Err(format!(
                "LIKE literal filter returned wrong rows for {needle:?}"
            ));
        }
    }

    let no_match = list
        .execute(DocumentListRequest {
            tenant_id: context.tenant,
            filter: DocumentListFilter {
                filename_contains: Some("does-not-exist".to_string()),
                ..DocumentListFilter::default()
            },
            cursor: None,
            limit: 200,
        })
        .await
        .map_err(|error| error.to_string())?;
    if !no_match.items.is_empty() {
        return Err("no-match filter returned rows".to_string());
    }
    Ok(())
}

async fn verify_limit_clamp(
    list: &Arc<dyn DocumentListQuery>,
    context: &ContractContext,
) -> Result<(), String> {
    for limit in [0, 1, 200, 201] {
        let page = list
            .execute(DocumentListRequest {
                tenant_id: context.tenant,
                filter: DocumentListFilter::default(),
                cursor: None,
                limit,
            })
            .await
            .map_err(|error| error.to_string())?;
        if page.items.len() > 200 || (limit == 1 && page.items.len() > 1) {
            return Err("list limit clamp contract failed".to_string());
        }
    }
    Ok(())
}

async fn verify_cursor(
    unit_of_work: &Arc<dyn CreateDocumentUnitOfWork>,
    list: &Arc<dyn DocumentListQuery>,
    context: &ContractContext,
) -> Result<(), String> {
    let timestamp = Utc::now() - Duration::hours(1);
    for index in 0..3 {
        let document = rehydrated_document(
            context.tenant,
            context.user,
            Uuid::now_v7(),
            format!("cursor-{index}.txt"),
            DocumentStatus::Active,
            timestamp,
        )?;
        unit_of_work
            .execute(PersistNewDocument {
                document,
                idempotency_key: format!("contract-cursor-{index}"),
                request_fingerprint: format!("cursor-{index}").repeat(16),
                fingerprint_version: 1,
            })
            .await
            .map_err(|error| error.to_string())?;
    }

    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let page = list
            .execute(DocumentListRequest {
                tenant_id: context.tenant,
                filter: DocumentListFilter {
                    filename_contains: Some("cursor-".to_string()),
                    ..DocumentListFilter::default()
                },
                cursor,
                limit: 2,
            })
            .await
            .map_err(|error| error.to_string())?;
        ids.extend(page.items.iter().map(|item| item.id));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != 3 {
        return Err("stable cursor pagination omitted or duplicated rows".to_string());
    }
    Ok(())
}

fn verify_invalid_stored_data() {
    let result = DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
        id: Uuid::now_v7(),
        tenant_id: Uuid::now_v7(),
        original_filename: "invalid.pdf".to_string(),
        content_type: "application/pdf".to_string(),
        object_key: "uploads/invalid.pdf".to_string(),
        status: DocumentStatus::Active,
        version: 0,
        size_bytes: Some(1),
        created_by: Uuid::now_v7(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    });
    assert!(
        result.is_err(),
        "invalid persisted version must fail closed"
    );
}

fn rehydrated_document(
    tenant_id: Uuid,
    created_by: Uuid,
    id: Uuid,
    filename: impl Into<String>,
    status: DocumentStatus,
    created_at: DateTime<Utc>,
) -> Result<DocumentMetadata, String> {
    DocumentMetadata::rehydrate(RehydrateDocumentMetadata {
        id,
        tenant_id,
        original_filename: filename.into(),
        content_type: "application/octet-stream".to_string(),
        object_key: format!("uploads/{id}.bin"),
        status,
        version: 1,
        size_bytes: Some(1),
        created_by,
        created_at,
        updated_at: created_at,
    })
    .map_err(|error| error.to_string())
}
