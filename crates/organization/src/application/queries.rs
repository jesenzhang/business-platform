//! Organization read use cases.

use std::sync::Arc;

use uuid::Uuid;

use crate::application::error::OrganizationApplicationError;
use crate::domain::{OrganizationMembership, OrganizationUnit};
use crate::ports::{OrganizationQueryPort, UnitMemberRecord};

/// Fetch one unit.
pub struct GetUnit {
    query_port: Arc<dyn OrganizationQueryPort>,
}

impl GetUnit {
    #[must_use]
    pub fn new(query_port: Arc<dyn OrganizationQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Option<OrganizationUnit>, OrganizationApplicationError> {
        if tenant_id.is_nil() || unit_id.is_nil() {
            return Err(OrganizationApplicationError::Validation(
                "tenant and unit ids must not be nil".to_string(),
            ));
        }
        Ok(self.query_port.get_unit(tenant_id, unit_id).await?)
    }
}

/// Flat listing of the tenant unit tree (delivery renders hierarchy).
pub struct ListOrganizationTree {
    query_port: Arc<dyn OrganizationQueryPort>,
}

impl ListOrganizationTree {
    #[must_use]
    pub fn new(query_port: Arc<dyn OrganizationQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<OrganizationUnit>, OrganizationApplicationError> {
        if tenant_id.is_nil() {
            return Err(OrganizationApplicationError::Validation(
                "tenant id must not be nil".to_string(),
            ));
        }
        Ok(self.query_port.list_units(tenant_id).await?)
    }
}

/// Members of one unit.
pub struct ListUnitMembers {
    query_port: Arc<dyn OrganizationQueryPort>,
}

impl ListUnitMembers {
    #[must_use]
    pub fn new(query_port: Arc<dyn OrganizationQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Vec<UnitMemberRecord>, OrganizationApplicationError> {
        if tenant_id.is_nil() || unit_id.is_nil() {
            return Err(OrganizationApplicationError::Validation(
                "tenant and unit ids must not be nil".to_string(),
            ));
        }
        Ok(self
            .query_port
            .list_unit_members(tenant_id, unit_id)
            .await?)
    }
}

/// Active unit memberships of a user in a tenant (policy input).
pub struct ListUserMemberships {
    query_port: Arc<dyn OrganizationQueryPort>,
}

impl ListUserMemberships {
    #[must_use]
    pub fn new(query_port: Arc<dyn OrganizationQueryPort>) -> Self {
        Self { query_port }
    }

    pub async fn execute(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationApplicationError> {
        if tenant_id.is_nil() || user_id.is_nil() {
            return Err(OrganizationApplicationError::Validation(
                "tenant and user ids must not be nil".to_string(),
            ));
        }
        Ok(self
            .query_port
            .list_user_memberships(tenant_id, user_id)
            .await?)
    }
}
