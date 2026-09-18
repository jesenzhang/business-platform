//! Policy read use cases.

use std::sync::Arc;

use uuid::Uuid;

use crate::application::error::PolicyApplicationError;
use crate::application::MAX_BINDINGS_PER_TENANT;
use crate::domain::{PermissionDefinition, RoleBinding, RoleDefinition};
use crate::ports::PolicyQueryPort;

/// The permission catalog (safe to expose to authenticated admins).
pub struct ListPermissions {
    query: Arc<dyn PolicyQueryPort>,
}

impl ListPermissions {
    #[must_use]
    pub fn new(query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { query }
    }

    pub async fn execute(&self) -> Result<Vec<PermissionDefinition>, PolicyApplicationError> {
        Ok(self.query.list_permissions().await?)
    }
}

/// One role visible to the tenant.
pub struct GetRole {
    query: Arc<dyn PolicyQueryPort>,
}

impl GetRole {
    #[must_use]
    pub fn new(query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { query }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<RoleDefinition>, PolicyApplicationError> {
        if tenant_id.is_nil() || role_id.is_nil() {
            return Err(PolicyApplicationError::Validation(
                "tenant and role ids must not be nil".to_string(),
            ));
        }
        Ok(self.query.get_role(tenant_id, role_id).await?)
    }
}

/// Roles visible to the tenant (system + own).
pub struct ListRoles {
    query: Arc<dyn PolicyQueryPort>,
}

impl ListRoles {
    #[must_use]
    pub fn new(query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { query }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RoleDefinition>, PolicyApplicationError> {
        if tenant_id.is_nil() {
            return Err(PolicyApplicationError::Validation(
                "tenant id must not be nil".to_string(),
            ));
        }
        Ok(self.query.list_roles(tenant_id).await?)
    }
}

/// Role bindings of a tenant, optionally narrowed to one user.
pub struct ListRoleBindings {
    query: Arc<dyn PolicyQueryPort>,
}

impl ListRoleBindings {
    #[must_use]
    pub fn new(query: Arc<dyn PolicyQueryPort>) -> Self {
        Self { query }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        user_filter: Option<Uuid>,
    ) -> Result<Vec<RoleBinding>, PolicyApplicationError> {
        if tenant_id.is_nil() {
            return Err(PolicyApplicationError::Validation(
                "tenant id must not be nil".to_string(),
            ));
        }
        if let Some(user_id) = user_filter {
            if user_id.is_nil() {
                return Err(PolicyApplicationError::Validation(
                    "user filter must not be nil".to_string(),
                ));
            }
        }
        let bindings = self.query.list_bindings(tenant_id, user_filter).await?;
        if bindings.len() > MAX_BINDINGS_PER_TENANT {
            return Err(PolicyApplicationError::TooManyResources);
        }
        Ok(bindings)
    }
}
