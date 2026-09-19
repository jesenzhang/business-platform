//! Read-side use cases for identity (`GetUser` / `ListUsers` / list
//! memberships) with validated keyset pagination.

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::ports::{
    IdentityQueryPort, IdentityStoreError, KeysetPosition, MembershipRecord, TenantUserRecord,
};

/// Maximum page size for identity listings (repo-wide convention).
pub const IDENTITY_MAX_PAGE_SIZE: u32 = 200;

/// Opaque-friendly keyset cursor for identity listings. The delivery layer
/// serializes this into a versioned opaque token; the domain never exposes
/// raw DB fields.
pub type IdentityPageCursor = KeysetPosition;

/// A page of identity listing results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPage<T> {
    /// Ordered page items.
    pub items: Vec<T>,
    /// Cursor to pass as `after` for the next page, if any.
    pub next_cursor: Option<IdentityPageCursor>,
}

/// Errors of identity read use cases.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityQueryError {
    /// The request was malformed (limit out of range etc.).
    #[error("validation failed: {0}")]
    Validation(String),
    /// Identity persistence is unavailable.
    #[error("identity persistence is unavailable")]
    Unavailable,
    /// Identity persistence failed.
    #[error("identity query failed")]
    Failed,
}

impl From<IdentityStoreError> for IdentityQueryError {
    fn from(error: IdentityStoreError) -> Self {
        match error {
            IdentityStoreError::Unavailable => Self::Unavailable,
            _ => Self::Failed,
        }
    }
}

fn validate_limit(limit: u32) -> Result<u32, String> {
    if limit == 0 || limit > IDENTITY_MAX_PAGE_SIZE {
        return Err(format!(
            "limit must be between 1 and {IDENTITY_MAX_PAGE_SIZE}"
        ));
    }
    Ok(limit)
}

/// Fetch one tenant user (user + membership) by id.
pub struct GetTenantUser {
    query_port: Arc<dyn IdentityQueryPort>,
}

impl GetTenantUser {
    #[must_use]
    pub fn new(query_port: Arc<dyn IdentityQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantUserRecord>, IdentityQueryError> {
        if tenant_id.is_nil() || user_id.is_nil() {
            return Err(IdentityQueryError::Validation(
                "tenant and user ids must not be nil".to_string(),
            ));
        }
        Ok(self.query_port.get_tenant_user(tenant_id, user_id).await?)
    }
}

/// Keyset list of users holding a membership in a tenant.
pub struct ListTenantUsers {
    query_port: Arc<dyn IdentityQueryPort>,
}

impl ListTenantUsers {
    #[must_use]
    pub fn new(query_port: Arc<dyn IdentityQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<IdentityPageCursor>,
    ) -> Result<IdentityPage<TenantUserRecord>, IdentityQueryError> {
        if tenant_id.is_nil() {
            return Err(IdentityQueryError::Validation(
                "tenant id must not be nil".to_string(),
            ));
        }
        let limit = validate_limit(limit).map_err(IdentityQueryError::Validation)?;
        let (items, next_cursor) = self
            .query_port
            .list_tenant_users(tenant_id, limit, after)
            .await?;
        Ok(IdentityPage { items, next_cursor })
    }
}

/// Keyset list of tenant memberships.
pub struct ListMemberships {
    query_port: Arc<dyn IdentityQueryPort>,
}

impl ListMemberships {
    #[must_use]
    pub fn new(query_port: Arc<dyn IdentityQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<IdentityPageCursor>,
    ) -> Result<IdentityPage<MembershipRecord>, IdentityQueryError> {
        if tenant_id.is_nil() {
            return Err(IdentityQueryError::Validation(
                "tenant id must not be nil".to_string(),
            ));
        }
        let limit = validate_limit(limit).map_err(IdentityQueryError::Validation)?;
        let (items, next_cursor) = self
            .query_port
            .list_memberships(tenant_id, limit, after)
            .await?;
        Ok(IdentityPage { items, next_cursor })
    }
}
