//! Typed relations from Document Management to business resources.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentResourceKind {
    Contract,
    Project,
    Customer,
    Party,
    LegalMatter,
    FinanceRecord,
    AssuranceCase,
    Employee,
    PerformanceReview,
}

impl DocumentResourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::Project => "project",
            Self::Customer => "customer",
            Self::Party => "party",
            Self::LegalMatter => "legal_matter",
            Self::FinanceRecord => "finance_record",
            Self::AssuranceCase => "assurance_case",
            Self::Employee => "employee",
            Self::PerformanceReview => "performance_review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentLinkRole {
    MainContract,
    SignedCopy,
    Appendix,
    Amendment,
    Quotation,
    Invoice,
    Evidence,
    Other,
}

impl DocumentLinkRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MainContract => "main_contract",
            Self::SignedCopy => "signed_copy",
            Self::Appendix => "appendix",
            Self::Amendment => "amendment",
            Self::Quotation => "quotation",
            Self::Invoice => "invoice",
            Self::Evidence => "evidence",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DocumentLinkError {
    #[error("document link identity is invalid")]
    InvalidIdentity,
    #[error("document link is not tenant-scoped")]
    TenantMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLink {
    id: Uuid,
    tenant_id: Uuid,
    document_id: Uuid,
    resource_kind: DocumentResourceKind,
    resource_id: Uuid,
    role: DocumentLinkRole,
    created_at: DateTime<Utc>,
    created_by: Uuid,
}

impl DocumentLink {
    pub fn new(
        tenant_id: Uuid,
        document_id: Uuid,
        resource_kind: DocumentResourceKind,
        resource_id: Uuid,
        role: DocumentLinkRole,
        created_by: Uuid,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DocumentLinkError> {
        if tenant_id.is_nil() || document_id.is_nil() || resource_id.is_nil() || created_by.is_nil()
        {
            return Err(DocumentLinkError::InvalidIdentity);
        }
        Ok(Self {
            id: Uuid::now_v7(),
            tenant_id,
            document_id,
            resource_kind,
            resource_id,
            role,
            created_at,
            created_by,
        })
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
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
    pub const fn resource_kind(&self) -> DocumentResourceKind {
        self.resource_kind
    }
    #[must_use]
    pub const fn resource_id(&self) -> Uuid {
        self.resource_id
    }
    #[must_use]
    pub const fn role(&self) -> DocumentLinkRole {
        self.role
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub const fn created_by(&self) -> Uuid {
        self.created_by
    }
}
