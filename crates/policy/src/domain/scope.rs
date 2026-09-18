//! `ResourceScope` — the complete, bounded scope model for role bindings.
//!
//! This enum is deliberately **not** extensible at runtime: PLAN-0013
//! forbids a policy DSL or arbitrary ABAC expressions. A scope only ever
//! *narrows* authority; `OrganizationUnit.include_subtree` is the single
//! expansion knob and subtree resolution runs against trusted organization
//! data, never against request parameters.

use uuid::Uuid;

use super::error::PolicyDomainError;

/// Maximum length of a resource kind string.
pub const MAX_RESOURCE_KIND_LEN: usize = 128;

/// Validate the resource-kind grammar: 1..=3 dot-separated segments of
/// `[a-z][a-z0-9_-]*` (e.g. `contract`, `contract.summary`).
pub fn validate_resource_kind(raw: &str) -> Result<(), PolicyDomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_RESOURCE_KIND_LEN {
        return Err(PolicyDomainError::InvalidResourceKind(raw.to_string()));
    }
    let segments: Vec<&str> = trimmed.split('.').collect();
    if segments.len() > 3 {
        return Err(PolicyDomainError::InvalidResourceKind(raw.to_string()));
    }
    for segment in segments {
        if segment.is_empty()
            || !segment.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            || !segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
        {
            return Err(PolicyDomainError::InvalidResourceKind(raw.to_string()));
        }
    }
    Ok(())
}

/// Where a role binding is effective. Bounded enum — the only scope shapes
/// PLAN-0013 allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceScope {
    /// The whole tenant (default for management-plane roles).
    Tenant,
    /// One organization unit; with `include_subtree` also its descendants.
    /// The unit id must exist in the binding's tenant (validated at bind
    /// time against trusted organization data; re-checked at decision time).
    OrganizationUnit {
        /// The unit addressed by the scope.
        org_unit_id: Uuid,
        /// Whether descendants of `org_unit_id` are included.
        include_subtree: bool,
    },
    /// All resources of one kind inside the tenant.
    ResourceType {
        /// Resource kind (validated grammar).
        kind: String,
    },
    /// One concrete resource. Intended for narrow explicit shares.
    Resource {
        /// Resource kind (validated grammar).
        kind: String,
        /// The single resource id.
        resource_id: Uuid,
    },
}

impl ResourceScope {
    /// Construct an organization-unit scope (nil unit rejected).
    pub fn organization_unit(
        org_unit_id: Uuid,
        include_subtree: bool,
    ) -> Result<Self, PolicyDomainError> {
        if org_unit_id.is_nil() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        Ok(Self::OrganizationUnit {
            org_unit_id,
            include_subtree,
        })
    }

    /// Construct a resource-kind scope (grammar validated).
    pub fn resource_type(kind: impl Into<String>) -> Result<Self, PolicyDomainError> {
        let kind = kind.into();
        validate_resource_kind(&kind)?;
        Ok(Self::ResourceType { kind })
    }

    /// Construct a single-resource scope (grammar + nil validated).
    pub fn resource(kind: impl Into<String>, resource_id: Uuid) -> Result<Self, PolicyDomainError> {
        let kind = kind.into();
        validate_resource_kind(&kind)?;
        if resource_id.is_nil() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        Ok(Self::Resource { kind, resource_id })
    }

    /// Stable label for audit/decision output (bounded, never user data).
    #[must_use]
    pub const fn kind_label(&self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::OrganizationUnit { .. } => "org_unit",
            Self::ResourceType { .. } => "resource_type",
            Self::Resource { .. } => "resource",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_kind_grammar() {
        assert!(validate_resource_kind("contract").is_ok());
        assert!(validate_resource_kind("contract.summary").is_ok());
        assert!(validate_resource_kind("a.b.c").is_ok());
        assert!(validate_resource_kind("").is_err());
        assert!(validate_resource_kind("Contract").is_err());
        assert!(validate_resource_kind("contract.").is_err());
        assert!(validate_resource_kind(".contract").is_err());
        assert!(validate_resource_kind("a.b.c.d").is_err());
        assert!(validate_resource_kind("contract;drop").is_err());
        assert!(validate_resource_kind(&"x".repeat(MAX_RESOURCE_KIND_LEN + 1)).is_err());
    }

    #[test]
    fn constructors_reject_nil() {
        assert!(ResourceScope::organization_unit(Uuid::nil(), false).is_err());
        assert!(ResourceScope::resource("contract", Uuid::nil()).is_err());
        assert!(ResourceScope::resource("Contract", Uuid::now_v7()).is_err());
        assert!(ResourceScope::resource("contract", Uuid::now_v7()).is_ok());
    }
}
