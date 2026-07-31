use std::fmt;

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DocumentObjectKeyError {
    #[error("object key is empty")]
    Empty,
    #[error("object key contains a forbidden path segment")]
    InvalidPath,
    #[error("object key contains NUL")]
    Nul,
}

/// Business-stable object reference. Adapters map it to a physical key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentObjectKey {
    tenant_id: Uuid,
    document_id: Uuid,
    version: i64,
    logical_path: String,
}

impl DocumentObjectKey {
    pub fn new(
        tenant_id: Uuid,
        document_id: Uuid,
        version: i64,
        logical_path: impl Into<String>,
    ) -> Result<Self, DocumentObjectKeyError> {
        let logical_path = logical_path.into();
        if logical_path.trim().is_empty() || logical_path.contains('\0') {
            return Err(if logical_path.contains('\0') {
                DocumentObjectKeyError::Nul
            } else {
                DocumentObjectKeyError::Empty
            });
        }
        if logical_path.starts_with("//")
            || logical_path.starts_with("\\\\")
            || (logical_path.len() >= 2
                && logical_path.as_bytes()[0].is_ascii_alphabetic()
                && logical_path.as_bytes()[1] == b':')
        {
            return Err(DocumentObjectKeyError::InvalidPath);
        }
        if logical_path
            .split(['/', '\\'])
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
            || logical_path.starts_with('/')
            || logical_path.starts_with('\\')
        {
            return Err(DocumentObjectKeyError::InvalidPath);
        }
        Ok(Self {
            tenant_id,
            document_id,
            version,
            logical_path,
        })
    }

    #[must_use]
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    #[must_use]
    pub fn as_storage_key(&self) -> String {
        format!(
            "tenants/{}/documents/{}/v{}/{}",
            self.tenant_id, self.document_id, self.version, self.logical_path
        )
    }
}

impl fmt::Display for DocumentObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_storage_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_is_tenant_scoped() {
        let tenant = Uuid::now_v7();
        let document = Uuid::now_v7();
        let Ok(key) = DocumentObjectKey::new(tenant, document, 1, "report.pdf") else {
            return;
        };
        assert!(key
            .as_storage_key()
            .starts_with(&format!("tenants/{tenant}/")));
    }

    #[test]
    fn rejects_platform_escape_forms() {
        let tenant = Uuid::now_v7();
        let document = Uuid::now_v7();
        assert!(DocumentObjectKey::new(tenant, document, 1, "C:\\temp\\x").is_err());
        assert!(DocumentObjectKey::new(tenant, document, 1, "//server/share").is_err());
    }
}
