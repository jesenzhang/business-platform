//! Request DTOs for the document API.

use serde::Deserialize;

/// Request body for creating a document metadata record.
#[derive(Debug, Deserialize)]
pub struct CreateDocumentRequest {
    /// Original filename as uploaded by the user.
    pub original_filename: String,
    /// MIME content type (e.g. "application/pdf").
    pub content_type: String,
    /// Object storage key where the file is stored.
    pub object_key: String,
    /// Optional file size in bytes.
    #[serde(default)]
    pub size_bytes: Option<i64>,
}

/// Query parameters for listing documents.
#[derive(Debug, Deserialize)]
pub struct ListDocumentsParams {
    /// Page number, starting from 1.
    #[serde(default = "default_page")]
    pub page: u32,
    /// Number of items per page.
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}
