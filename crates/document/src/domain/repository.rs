//! Port: document metadata persistence.

use async_trait::async_trait;
use uuid::Uuid;

use super::entity::DocumentMetadata;

/// Port trait for document metadata persistence.
///
/// Implementations must enforce tenant isolation: all queries filter by
/// `tenant_id` so that cross-tenant reads are impossible.
#[async_trait]
pub trait DocumentRepository: Send + Sync {
    /// Persist a document within an existing transaction.
    ///
    /// The caller owns the transaction to allow atomic writes of business
    /// data, outbox events, and audit records.
    async fn save(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        doc: &DocumentMetadata,
    ) -> Result<(), sqlx::Error>;

    /// Find a document by ID, scoped to the given tenant.
    ///
    /// Returns `None` if the document does not exist or belongs to a
    /// different tenant.
    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DocumentMetadata>, sqlx::Error>;

    /// List documents for a tenant with pagination.
    ///
    /// Returns the page of documents and the total count.
    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DocumentMetadata>, i64), sqlx::Error>;
}
