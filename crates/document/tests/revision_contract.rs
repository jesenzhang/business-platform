use document::domain::{
    DocumentDeletionState, DocumentDomainError, DocumentLifecycleState, DocumentMetadata,
    DocumentRevision, DocumentRevisionError,
};
use uuid::Uuid;

fn document() -> DocumentMetadata {
    DocumentMetadata::create(
        Uuid::now_v7(),
        "contract.pdf".to_string(),
        "application/pdf".to_string(),
        "uploads/contract.pdf".to_string(),
        Uuid::now_v7(),
        Some(12),
    )
    .unwrap_or_else(|_| unreachable!())
}

#[test]
fn first_revision_is_r1_and_uses_revision_scoped_source_key() {
    let document = document();
    let revision = document
        .initial_revision()
        .unwrap_or_else(|_| unreachable!());

    assert_eq!(revision.revision_no(), 1);
    assert_eq!(revision.document_id(), document.id());
    assert_eq!(document.current_revision_id(), revision.id());
    assert_eq!(
        revision.source_object_ref(),
        format!(
            "tenants/{}/documents/{}/revisions/{}/source",
            document.tenant_id(),
            document.id(),
            revision.id()
        )
    );
}

#[test]
fn new_revision_does_not_mutate_r1_and_stale_current_is_rejected() {
    let mut document = document();
    let r1 = document
        .initial_revision()
        .unwrap_or_else(|_| unreachable!());
    let r2 = document
        .replace_content_revision(
            "replacement.pdf".to_string(),
            Some("user correction".to_string()),
        )
        .unwrap_or_else(|_| unreachable!());

    assert_eq!(r1.revision_no(), 1);
    assert_eq!(r2.revision_no(), 2);
    assert_eq!(r2.parent_revision_id(), Some(r1.id()));
    assert_ne!(r1.source_object_ref(), r2.source_object_ref());
    assert_eq!(document.current_revision_id(), r2.id());
    assert!(matches!(
        document.assert_expected_revision(r1.id()),
        Err(DocumentRevisionError::StaleCurrentRevision { .. })
    ));
}

#[test]
fn archive_trash_restore_are_independent_from_content_revision() {
    let mut document = document();
    let revision_id = document.current_revision_id();
    assert_eq!(document.lifecycle_state(), DocumentLifecycleState::Active);
    assert_eq!(document.deletion_state(), DocumentDeletionState::Present);

    document.archive().unwrap_or_else(|_| unreachable!());
    document.trash().unwrap_or_else(|_| unreachable!());
    assert_eq!(document.lifecycle_state(), DocumentLifecycleState::Archived);
    assert_eq!(document.deletion_state(), DocumentDeletionState::Trashed);
    document
        .restore_from_trash()
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(document.lifecycle_state(), DocumentLifecycleState::Archived);
    assert_eq!(document.deletion_state(), DocumentDeletionState::Present);
    assert_eq!(document.current_revision_id(), revision_id);
}

#[test]
fn revision_identity_is_not_a_storage_provider_version() {
    let revision = document()
        .initial_revision()
        .unwrap_or_else(|_| unreachable!());
    assert!(revision.provider_version_id().is_none());
    assert!(DocumentRevision::validate_source_object_ref(revision.source_object_ref()).is_ok());
}

#[test]
fn purge_requires_retention_reference_and_hold_preconditions() {
    let mut document = document();
    document.trash().unwrap_or_else(|_| unreachable!());
    assert!(matches!(
        document.request_purge(false, false, false),
        Err(DocumentDomainError::PurgeRetentionNotReleased)
    ));
    assert!(matches!(
        document.request_purge(true, true, false),
        Err(DocumentDomainError::PurgeReferenced)
    ));
    assert!(matches!(
        document.request_purge(true, false, true),
        Err(DocumentDomainError::PurgeHeld)
    ));
    assert!(document.request_purge(true, false, false).is_ok());
    assert!(document.complete_purge().is_ok());
}
