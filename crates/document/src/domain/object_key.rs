//! Tenant-owned document content references and canonical storage keys.

use std::fmt;

use thiserror::Error;
use uuid::Uuid;

use super::version::ContentRevision;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContentReferenceError {
    #[error("tenant and document identifiers must not be nil")]
    InvalidIdentity,
    #[error("logical path is empty")]
    EmptyPath,
    #[error("logical path contains a forbidden segment")]
    InvalidPath,
    #[error("logical path contains NUL")]
    Nul,
    #[error("storage key has an invalid format")]
    InvalidFormat,
    #[error("storage key tenant does not match the expected tenant")]
    TenantMismatch,
    #[error("storage key document does not match the expected document")]
    DocumentMismatch,
    #[error("storage key revision is invalid")]
    InvalidRevision,
}

/// Canonical tenant/document/content-revision reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentContentReference {
    tenant_id: Uuid,
    document_id: Uuid,
    content_revision: ContentRevision,
    logical_path: String,
}

impl DocumentContentReference {
    pub fn new(
        tenant_id: Uuid,
        document_id: Uuid,
        content_revision: ContentRevision,
        logical_path: String,
    ) -> Result<Self, ContentReferenceError> {
        if tenant_id.is_nil() || document_id.is_nil() {
            return Err(ContentReferenceError::InvalidIdentity);
        }
        validate_logical_path(&logical_path)?;
        Ok(Self {
            tenant_id,
            document_id,
            content_revision,
            logical_path,
        })
    }

    pub fn parse_storage_key(
        expected_tenant_id: Uuid,
        expected_document_id: Uuid,
        value: &str,
    ) -> Result<Self, ContentReferenceError> {
        if expected_tenant_id.is_nil() || expected_document_id.is_nil() {
            return Err(ContentReferenceError::InvalidIdentity);
        }
        let parts: Vec<_> = value.split('/').collect();
        if parts.len() < 6 || parts[0] != "tenants" || parts[2] != "documents" {
            return Err(ContentReferenceError::InvalidFormat);
        }
        let tenant_id =
            Uuid::parse_str(parts[1]).map_err(|_| ContentReferenceError::InvalidFormat)?;
        if tenant_id != expected_tenant_id {
            return Err(ContentReferenceError::TenantMismatch);
        }
        let document_id =
            Uuid::parse_str(parts[3]).map_err(|_| ContentReferenceError::InvalidFormat)?;
        if document_id != expected_document_id {
            return Err(ContentReferenceError::DocumentMismatch);
        }
        let revision_value = parts[4]
            .strip_prefix('v')
            .filter(|value| {
                !(value.is_empty() || (value.len() > 1 && value.starts_with('0')))
                    && value.chars().all(|character| character.is_ascii_digit())
            })
            .ok_or(ContentReferenceError::InvalidRevision)?
            .parse::<i64>()
            .map_err(|_| ContentReferenceError::InvalidRevision)?;
        let content_revision = ContentRevision::new(revision_value)
            .map_err(|_| ContentReferenceError::InvalidRevision)?;
        let logical_path = parts[5..].join("/");
        Self::new(tenant_id, document_id, content_revision, logical_path)
    }

    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    #[must_use]
    pub const fn document_id(&self) -> Uuid {
        self.document_id
    }

    #[must_use]
    pub const fn content_revision(&self) -> ContentRevision {
        self.content_revision
    }

    #[must_use]
    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    #[must_use]
    pub fn as_storage_key(&self) -> String {
        format!(
            "tenants/{}/documents/{}/v{}/{}",
            self.tenant_id, self.document_id, self.content_revision, self.logical_path
        )
    }
}

impl fmt::Display for DocumentContentReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_storage_key())
    }
}

fn validate_logical_path(logical_path: &str) -> Result<(), ContentReferenceError> {
    if logical_path.trim().is_empty() {
        return Err(ContentReferenceError::EmptyPath);
    }
    if logical_path.contains('\0') {
        return Err(ContentReferenceError::Nul);
    }
    if logical_path.starts_with('/')
        || logical_path.ends_with('/')
        || logical_path.starts_with('\\')
        || logical_path.ends_with('\\')
        || logical_path.contains('\\')
        || logical_path.starts_with("//")
        || logical_path.starts_with("\\\\")
        || (logical_path.len() >= 2
            && logical_path.as_bytes()[0].is_ascii_alphabetic()
            && logical_path.as_bytes()[1] == b':')
        || logical_path.starts_with("tenants/")
        || logical_path.starts_with("documents/")
    {
        return Err(ContentReferenceError::InvalidPath);
    }
    if logical_path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(ContentReferenceError::InvalidPath);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_round_trips_with_expected_ownership() {
        let tenant = Uuid::now_v7();
        let document = Uuid::now_v7();
        let reference = DocumentContentReference::new(
            tenant,
            document,
            ContentRevision::new(1).unwrap_or_else(|_| unreachable!()),
            "incoming/report.txt".to_string(),
        )
        .unwrap_or_else(|_| unreachable!());
        let parsed =
            DocumentContentReference::parse_storage_key(tenant, document, &reference.to_string())
                .unwrap_or_else(|_| unreachable!());
        assert_eq!(parsed, reference);
    }

    #[test]
    fn storage_key_rejects_wrong_owners_and_unsafe_paths() {
        let tenant = Uuid::now_v7();
        let document = Uuid::now_v7();
        let other_tenant = Uuid::now_v7();
        let other_document = Uuid::now_v7();
        let key = format!("tenants/{tenant}/documents/{document}/v1/report.txt");
        assert!(matches!(
            DocumentContentReference::parse_storage_key(other_tenant, document, &key),
            Err(ContentReferenceError::TenantMismatch)
        ));
        assert!(matches!(
            DocumentContentReference::parse_storage_key(tenant, other_document, &key),
            Err(ContentReferenceError::DocumentMismatch)
        ));
        for invalid in [
            format!("tenants/{tenant}/documents/{document}/v0/report.txt"),
            format!("tenants/{tenant}/documents/{document}/v1/"),
            format!("tenants/{tenant}/documents/{document}/v1/../secret"),
            format!("tenants/{tenant}/documents/{document}/v1/tenants/{tenant}"),
        ] {
            assert!(
                DocumentContentReference::parse_storage_key(tenant, document, &invalid).is_err()
            );
        }
    }
}
