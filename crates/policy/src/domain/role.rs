//! `RoleDefinition` — a named, versioned set of permissions with an
//! explicit authority scope (`system` or `tenant`).
//!
//! System roles are **immutable**: they are created only by the migration
//! seed and the domain rejects every mutation path on them. Two system
//! roles exist by contract (see [`crate::catalog`]): `system.bootstrap-admin`
//! and `system.platform-admin`. "bootstrap-sourced" actor status is
//! evaluated as *an active binding to an immutable system role at decision
//! time* — never a historical flag.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::PolicyDomainError;
use super::version::AggregateVersion;

/// Maximum length of a role display name.
pub const MAX_ROLE_DISPLAY_NAME_LEN: usize = 200;

/// Validate a role stable key: 1..=3 dot-separated segments of
/// `[a-z][a-z0-9_-]*` (e.g. `auditor`, `system.bootstrap-admin`).
pub fn validate_role_stable_key(raw: &str) -> Result<(), PolicyDomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err(PolicyDomainError::InvalidStableKey(raw.to_string()));
    }
    let segments: Vec<&str> = trimmed.split('.').collect();
    if segments.len() > 3 {
        return Err(PolicyDomainError::InvalidStableKey(raw.to_string()));
    }
    for segment in segments {
        if segment.is_empty()
            || !segment.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            || !segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
        {
            return Err(PolicyDomainError::InvalidStableKey(raw.to_string()));
        }
    }
    Ok(())
}

/// Role lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoleStatus {
    /// Grants authority through active bindings.
    Active,
    /// Switched off: bindings referencing it deny (`DenyRoleDisabled`).
    Disabled,
}

impl RoleStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

/// The role aggregate. Fields are private; adapters restore state only
/// through [`RoleDefinition::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleDefinition {
    role_id: Uuid,
    /// `None` marks a system-scoped (global, immutable) role.
    tenant_id: Option<Uuid>,
    stable_key: String,
    display_name: String,
    status: RoleStatus,
    /// System roles are immutable; derived from `tenant_id.is_none()` but
    /// kept explicit so rehydration proves the invariant.
    system: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: AggregateVersion,
}

/// Raw persisted state accepted by [`RoleDefinition::rehydrate`].
#[derive(Debug, Clone)]
pub struct RehydrateRoleDefinition {
    /// Role id.
    pub role_id: Uuid,
    /// Tenant boundary (`None` = system role).
    pub tenant_id: Option<Uuid>,
    /// Stable key, unique globally for system roles, per tenant otherwise.
    pub stable_key: String,
    /// Display name.
    pub display_name: String,
    /// Persisted status.
    pub status: RoleStatus,
    /// Persisted system flag.
    pub system: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Persisted aggregate version.
    pub version: i64,
}

impl RoleDefinition {
    /// Create a **tenant** role. System roles are migration-seeded and only
    /// ever restored via [`Self::rehydrate`]; there is no create path for
    /// them at runtime.
    pub fn create_tenant_role(
        role_id: Uuid,
        tenant_id: Uuid,
        stable_key: &str,
        display_name: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, PolicyDomainError> {
        if role_id.is_nil() || tenant_id.is_nil() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        validate_role_stable_key(stable_key)?;
        let display_name = validate_display_name(display_name)?;
        Ok(Self {
            role_id,
            tenant_id: Some(tenant_id),
            stable_key: stable_key.trim().to_string(),
            display_name,
            status: RoleStatus::Active,
            system: false,
            created_at: now,
            updated_at: now,
            version: AggregateVersion::initial(),
        })
    }

    /// Restore a persisted role after full validation, proving the
    /// system/tenant consistency (`system ⇔ tenant_id is None`).
    pub fn rehydrate(state: RehydrateRoleDefinition) -> Result<Self, PolicyDomainError> {
        let RehydrateRoleDefinition {
            role_id,
            tenant_id,
            stable_key,
            display_name,
            status,
            system,
            created_at,
            updated_at,
            version,
        } = state;
        if role_id.is_nil() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        if system != tenant_id.is_none() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        if let Some(tenant_id) = tenant_id {
            if tenant_id.is_nil() {
                return Err(PolicyDomainError::InvalidIdentity);
            }
        }
        validate_role_stable_key(&stable_key)?;
        let display_name = validate_display_name(&display_name)?;
        let version =
            AggregateVersion::new(version).map_err(|_| PolicyDomainError::InvalidVersion)?;
        if updated_at < created_at {
            return Err(PolicyDomainError::InvalidTransition {
                operation: "rehydrate-timestamps",
                status: status.as_str(),
            });
        }
        Ok(Self {
            role_id,
            tenant_id,
            stable_key: stable_key.trim().to_string(),
            display_name,
            status,
            system,
            created_at,
            updated_at,
            version,
        })
    }

    #[must_use]
    pub const fn role_id(&self) -> Uuid {
        self.role_id
    }

    #[must_use]
    pub const fn tenant_id(&self) -> Option<Uuid> {
        self.tenant_id
    }

    #[must_use]
    pub const fn is_system(&self) -> bool {
        self.system
    }

    #[must_use]
    pub fn stable_key(&self) -> &str {
        &self.stable_key
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn status(&self) -> RoleStatus {
        self.status
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, RoleStatus::Active)
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    /// Management update (rename / status). Rejected wholesale on system
    /// roles — they are immutable by contract.
    pub fn update(
        &mut self,
        display_name: Option<&str>,
        status: Option<RoleStatus>,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyDomainError> {
        if self.system {
            return Err(PolicyDomainError::SystemRoleImmutable);
        }
        if display_name.is_none() && status.is_none() {
            return Ok(());
        }
        if let Some(display_name) = display_name {
            self.display_name = validate_display_name(display_name)?;
        }
        if let Some(status) = status {
            self.status = status;
        }
        self.touch(now)
    }

    /// Record a permission-set replacement after a real (non-replayed)
    /// transactional replace: only the version stamp moves, the identity
    /// fields do not. Rejected wholesale on system roles.
    pub fn record_permissions_replaced(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyDomainError> {
        if self.system {
            return Err(PolicyDomainError::SystemRoleImmutable);
        }
        self.touch(now)
    }

    fn touch(&mut self, now: DateTime<Utc>) -> Result<(), PolicyDomainError> {
        if now > self.updated_at {
            self.updated_at = now;
        }
        self.version = self
            .version
            .increment()
            .map_err(|_| PolicyDomainError::InvalidVersion)?;
        Ok(())
    }
}

fn validate_display_name(raw: &str) -> Result<String, PolicyDomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_ROLE_DISPLAY_NAME_LEN
        || raw.chars().any(char::is_control)
    {
        return Err(PolicyDomainError::InvalidText);
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .unwrap_or_else(|| unreachable!())
    }

    fn id(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    #[test]
    fn tenant_role_lifecycle_and_key_grammar() {
        assert!(
            RoleDefinition::create_tenant_role(id(1), id(2), "auditor", "Auditors", ts(1)).is_ok()
        );
        assert!(
            RoleDefinition::create_tenant_role(id(1), id(2), "Auditors", "Auditors", ts(1))
                .is_err()
        );
        assert!(
            RoleDefinition::create_tenant_role(id(1), id(2), "a.b.c.d", "Auditors", ts(1)).is_err()
        );
        assert!(
            RoleDefinition::create_tenant_role(id(1), Uuid::nil(), "auditor", "A", ts(1)).is_err()
        );
        assert!(RoleDefinition::create_tenant_role(id(1), id(2), "auditor", " ", ts(1)).is_err());
    }

    #[test]
    fn system_roles_reject_mutation_and_rehydrate_proves_consistency() {
        let mut system_role = RoleDefinition::rehydrate(RehydrateRoleDefinition {
            role_id: id(1),
            tenant_id: None,
            stable_key: "system.bootstrap-admin".to_string(),
            display_name: "Bootstrap Admin".to_string(),
            status: RoleStatus::Active,
            system: true,
            created_at: ts(1),
            updated_at: ts(1),
            version: 1,
        })
        .unwrap_or_else(|_| unreachable!());
        assert!(system_role.is_system());
        assert_eq!(
            system_role
                .update(Some("Hijacked"), None, ts(5))
                .err()
                .unwrap_or_else(|| unreachable!()),
            PolicyDomainError::SystemRoleImmutable
        );

        // system flag inconsistent with tenant scoping is rejected.
        assert!(RoleDefinition::rehydrate(RehydrateRoleDefinition {
            role_id: id(1),
            tenant_id: Some(id(2)),
            stable_key: "x".to_string(),
            display_name: "X".to_string(),
            status: RoleStatus::Active,
            system: true,
            created_at: ts(1),
            updated_at: ts(1),
            version: 1,
        })
        .is_err());
    }
}
