//! `RoleBinding` — attaches a role to a platform user inside a tenant with
//! an explicit [`ResourceScope`] and an optional validity window.
//!
//! Revocation is soft (`Revoked` rows are retained): decision evaluation
//! distinguishes "binding revoked" from "no binding at all", and audit
//! history stays resolvable. Binding a role across tenants is unrepresentable:
//! the store only ever sees `(tenant_id, role_id)` pairs where the role is
//! the tenant's own or a system role.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::PolicyDomainError;
use super::scope::ResourceScope;
use super::version::AggregateVersion;

/// Binding lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingStatus {
    /// Potentially grants (subject to validity window and role status).
    Active,
    /// Revoked: never grants; the row is kept for auditability.
    Revoked,
}

impl BindingStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }
}

/// The role binding aggregate. Fields are private; adapters restore state
/// only through [`RoleBinding::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleBinding {
    binding_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    role_id: Uuid,
    scope: ResourceScope,
    status: BindingStatus,
    effective_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: AggregateVersion,
}

/// Raw persisted state accepted by [`RoleBinding::rehydrate`].
#[derive(Debug, Clone)]
pub struct RehydrateRoleBinding {
    /// Binding id.
    pub binding_id: Uuid,
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Bound platform user.
    pub user_id: Uuid,
    /// Bound role (same tenant or system).
    pub role_id: Uuid,
    /// Granted scope.
    pub scope: ResourceScope,
    /// Persisted status.
    pub status: BindingStatus,
    /// Start of the validity window.
    pub effective_at: DateTime<Utc>,
    /// End of the validity window (exclusive when set).
    pub expires_at: Option<DateTime<Utc>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Persisted aggregate version.
    pub version: i64,
}

/// Temporal validity window of a binding: inclusive start, exclusive end
/// (when set). A single concept with a single invariant, so it travels as
/// one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityWindow {
    /// Inclusive start of validity.
    pub effective_at: DateTime<Utc>,
    /// Exclusive end of validity (`None` = open-ended).
    pub expires_at: Option<DateTime<Utc>>,
}

impl ValidityWindow {
    fn validate(self) -> Result<(), PolicyDomainError> {
        if let Some(expires_at) = self.expires_at {
            if expires_at <= self.effective_at {
                return Err(PolicyDomainError::InvalidValidityWindow);
            }
        }
        Ok(())
    }
}

impl RoleBinding {
    /// Create an active binding. The window's `expires_at`, when set, must
    /// be strictly after `effective_at`.
    pub fn create(
        binding_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
        role_id: Uuid,
        scope: ResourceScope,
        window: ValidityWindow,
        now: DateTime<Utc>,
    ) -> Result<Self, PolicyDomainError> {
        if binding_id.is_nil() || tenant_id.is_nil() || user_id.is_nil() || role_id.is_nil() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        window.validate()?;
        Ok(Self {
            binding_id,
            tenant_id,
            user_id,
            role_id,
            scope,
            status: BindingStatus::Active,
            effective_at: window.effective_at,
            expires_at: window.expires_at,
            created_at: now,
            updated_at: now,
            version: AggregateVersion::initial(),
        })
    }

    /// Restore a persisted binding after full validation.
    pub fn rehydrate(state: RehydrateRoleBinding) -> Result<Self, PolicyDomainError> {
        let RehydrateRoleBinding {
            binding_id,
            tenant_id,
            user_id,
            role_id,
            scope,
            status,
            effective_at,
            expires_at,
            created_at,
            updated_at,
            version,
        } = state;
        if binding_id.is_nil() || tenant_id.is_nil() || user_id.is_nil() || role_id.is_nil() {
            return Err(PolicyDomainError::InvalidIdentity);
        }
        if let Some(expires_at) = expires_at {
            if expires_at <= effective_at {
                return Err(PolicyDomainError::InvalidValidityWindow);
            }
        }
        let version =
            AggregateVersion::new(version).map_err(|_| PolicyDomainError::InvalidVersion)?;
        if updated_at < created_at {
            return Err(PolicyDomainError::InvalidTransition {
                operation: "rehydrate-timestamps",
                status: status.as_str(),
            });
        }
        Ok(Self {
            binding_id,
            tenant_id,
            user_id,
            role_id,
            scope,
            status,
            effective_at,
            expires_at,
            created_at,
            updated_at,
            version,
        })
    }

    #[must_use]
    pub const fn binding_id(&self) -> Uuid {
        self.binding_id
    }

    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    #[must_use]
    pub const fn user_id(&self) -> Uuid {
        self.user_id
    }

    #[must_use]
    pub const fn role_id(&self) -> Uuid {
        self.role_id
    }

    #[must_use]
    pub const fn scope(&self) -> &ResourceScope {
        &self.scope
    }

    #[must_use]
    pub const fn status(&self) -> BindingStatus {
        self.status
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, BindingStatus::Active)
    }

    #[must_use]
    pub const fn effective_at(&self) -> DateTime<Utc> {
        self.effective_at
    }

    #[must_use]
    pub const fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
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

    /// Whether the validity window covers `now` (expiry is exclusive).
    #[must_use]
    pub fn is_within_validity(&self, now: DateTime<Utc>) -> bool {
        now >= self.effective_at && self.expires_at.is_none_or(|expires_at| now < expires_at)
    }

    /// Revoke the binding. Already-revoked is a domain-level no-op signal
    /// handled by callers as idempotent convergence.
    pub fn revoke(&mut self, now: DateTime<Utc>) -> Result<(), PolicyDomainError> {
        if !self.is_active() {
            return Err(PolicyDomainError::InvalidTransition {
                operation: "revoke",
                status: self.status.as_str(),
            });
        }
        self.status = BindingStatus::Revoked;
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
    fn validity_window_and_revocation_semantics() {
        let binding = RoleBinding::create(
            id(1),
            id(2),
            id(3),
            id(4),
            ResourceScope::Tenant,
            ValidityWindow {
                effective_at: ts(100),
                expires_at: Some(ts(200)),
            },
            ts(100),
        )
        .unwrap_or_else(|_| unreachable!());
        assert!(!binding.is_within_validity(ts(99)));
        assert!(binding.is_within_validity(ts(100)));
        assert!(binding.is_within_validity(ts(199)));
        assert!(!binding.is_within_validity(ts(200)), "expiry is exclusive");

        // Inverted window rejected.
        assert!(RoleBinding::create(
            id(1),
            id(2),
            id(3),
            id(4),
            ResourceScope::Tenant,
            ValidityWindow {
                effective_at: ts(200),
                expires_at: Some(ts(100)),
            },
            ts(100),
        )
        .is_err());

        let mut binding = binding;
        binding.revoke(ts(150)).unwrap_or_else(|_| unreachable!());
        assert!(!binding.is_active());
        assert_eq!(binding.version().value(), 2);
        assert_eq!(
            binding
                .revoke(ts(160))
                .err()
                .unwrap_or_else(|| unreachable!()),
            PolicyDomainError::InvalidTransition {
                operation: "revoke",
                status: "revoked"
            }
        );
    }
}
