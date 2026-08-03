use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatusView {
    Active,
    Archived,
    Deleted,
}

impl DocumentStatusView {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(value: &str) -> Result<Self, QueryError> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "deleted" => Ok(Self::Deleted),
            _ => Err(QueryError::InvalidStoredData),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentDetailView {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub status: DocumentStatusView,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentListItem {
    pub id: Uuid,
    pub original_filename: String,
    pub content_type: String,
    pub status: DocumentStatusView,
    pub version: i64,
    pub size_bytes: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStatusFilter {
    Active,
    Archived,
    Deleted,
}

impl DocumentStatusFilter {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentListFilter {
    pub status: Option<DocumentStatusFilter>,
    pub filename_contains: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct DocumentListCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentListPage {
    pub items: Vec<DocumentListItem>,
    pub next_cursor: Option<DocumentListCursor>,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum QueryError {
    #[error("document query unavailable")]
    Unavailable,
    #[error("document query contains invalid stored data")]
    InvalidStoredData,
    #[error("document query failed")]
    Failed,
}
