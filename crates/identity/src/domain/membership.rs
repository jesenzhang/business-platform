//! `TenantMembership` — the tenant boundary of a platform user.
//!
//! Invariant: no active membership ⇒ no business authorization inside that
//! tenant, regardless of any token claims. Statuses stay `Active | Suspended`
//! on purpose; complex IAM lifecycle is out of PLAN-0013 scope.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::IdentityDomainError;
use super::version::AggregateVersion;

/// Membership lifecycle status (minimal by design).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MembershipStatus {
    /// The user may act inside the tenant (subject to policy decisions).
    Active,
    /// The user is switched off inside this tenant; every authorization for
    /// this tenant must deny.
    Suspended,
}

impl MembershipStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

/// Where a membership record came from. Kept as a small closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MembershipSource {
    /// Created by the server-controlled bootstrap (initial administrator).
    Bootstrap,
    /// Created by an authenticated administrator via the management API.
    Admin,
    /// Backfilled by a migration/rehearsal import.
    Migration,
}

impl MembershipSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Admin => "admin",
            Self::Migration => "migration",
        }
    }
}

/// The tenant membership aggregate. Fields are private; adapters restore
/// state only through [`TenantMembership::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantMembership {
    membership_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    status: MembershipStatus,
    joined_at: DateTime<Utc>,
    suspended_at: Option<DateTime<Utc>>,
    source: MembershipSource,
    version: AggregateVersion,
}

/// Raw persisted state accepted by [`TenantMembership::rehydrate`].
#[derive(Debug, Clone, Copy)]
pub struct RehydrateTenantMembership {
    /// Membership row id.
    pub membership_id: Uuid,
    /// Tenant boundary of the membership.
    pub tenant_id: Uuid,
    /// Platform user.
    pub user_id: Uuid,
    /// Persisted status.
    pub status: MembershipStatus,
    /// When the user joined the tenant.
    pub joined_at: DateTime<Utc>,
    /// When the membership was suspended, if suspended.
    pub suspended_at: Option<DateTime<Utc>>,
    /// Origin of the record.
    pub source: MembershipSource,
    /// Persisted aggregate version.
    pub version: i64,
}

impl TenantMembership {
    /// Join a tenant: fresh memberships are always `Active` at version 1.
    pub fn join(
        membership_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
        source: MembershipSource,
        now: DateTime<Utc>,
    ) -> Result<Self, IdentityDomainError> {
        if tenant_id.is_nil() || user_id.is_nil() || membership_id.is_nil() {
            return Err(IdentityDomainError::InvalidMembershipIdentity);
        }
        Ok(Self {
            membership_id,
            tenant_id,
            user_id,
            status: MembershipStatus::Active,
            joined_at: now,
            suspended_at: None,
            source,
            version: AggregateVersion::initial(),
        })
    }

    /// Restore a persisted membership after full validation.
    pub fn rehydrate(state: RehydrateTenantMembership) -> Result<Self, IdentityDomainError> {
        if state.tenant_id.is_nil() || state.user_id.is_nil() || state.membership_id.is_nil() {
            return Err(IdentityDomainError::InvalidMembershipIdentity);
        }
        let version = AggregateVersion::new(state.version)
            .map_err(|_| IdentityDomainError::InvalidVersion)?;
        let suspended_at = match (state.status, state.suspended_at) {
            (MembershipStatus::Active, _) => None,
            (MembershipStatus::Suspended, Some(suspended_at)) => {
                if suspended_at < state.joined_at {
                    return Err(IdentityDomainError::InvalidTimestamps);
                }
                Some(suspended_at)
            }
            (MembershipStatus::Suspended, None) => {
                return Err(IdentityDomainError::InvalidTimestamps)
            }
        };
        Ok(Self {
            membership_id: state.membership_id,
            tenant_id: state.tenant_id,
            user_id: state.user_id,
            status: state.status,
            joined_at: state.joined_at,
            suspended_at,
            source: state.source,
            version,
        })
    }

    #[must_use]
    pub const fn membership_id(&self) -> Uuid {
        self.membership_id
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
    pub const fn status(&self) -> MembershipStatus {
        self.status
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, MembershipStatus::Active)
    }

    #[must_use]
    pub const fn joined_at(&self) -> DateTime<Utc> {
        self.joined_at
    }

    #[must_use]
    pub const fn suspended_at(&self) -> Option<DateTime<Utc>> {
        self.suspended_at
    }

    #[must_use]
    pub const fn source(&self) -> MembershipSource {
        self.source
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    /// Suspend the membership inside its tenant.
    pub fn suspend(&mut self, now: DateTime<Utc>) -> Result<(), IdentityDomainError> {
        if !matches!(self.status, MembershipStatus::Active) {
            return Err(IdentityDomainError::InvalidTransition {
                operation: "suspend",
                status: self.status.as_str(),
            });
        }
        self.status = MembershipStatus::Suspended;
        self.suspended_at = Some(now);
        self.bump_version()
    }

    /// Reactivate a suspended membership. The `now` argument is accepted for
    /// timestamp symmetry with [`Self::suspend`]; reactivation clears the
    /// suspension timestamp rather than recording a new one.
    pub fn reactivate(&mut self, now: DateTime<Utc>) -> Result<(), IdentityDomainError> {
        let _ = now;
        if !matches!(self.status, MembershipStatus::Suspended) {
            return Err(IdentityDomainError::InvalidTransition {
                operation: "reactivate",
                status: self.status.as_str(),
            });
        }
        self.status = MembershipStatus::Active;
        self.suspended_at = None;
        self.bump_version()
    }

    fn bump_version(&mut self) -> Result<(), IdentityDomainError> {
        self.version = self
            .version
            .increment()
            .map_err(|_| IdentityDomainError::InvalidVersion)?;
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
    fn join_rejects_nil_identifiers() {
        assert_eq!(
            TenantMembership::join(id(1), Uuid::nil(), id(2), MembershipSource::Admin, ts(1)),
            Err(IdentityDomainError::InvalidMembershipIdentity)
        );
        assert_eq!(
            TenantMembership::join(id(1), id(3), Uuid::nil(), MembershipSource::Admin, ts(1)),
            Err(IdentityDomainError::InvalidMembershipIdentity)
        );
    }

    #[test]
    fn suspend_reactivate_cycle_with_version_and_timestamps() {
        let mut membership =
            TenantMembership::join(id(1), id(2), id(3), MembershipSource::Admin, ts(100))
                .unwrap_or_else(|_| unreachable!());
        assert_eq!(membership.version().value(), 1);
        assert!(membership.is_active());

        membership
            .suspend(ts(200))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(membership.status(), MembershipStatus::Suspended);
        assert_eq!(membership.suspended_at(), Some(ts(200)));
        assert_eq!(membership.version().value(), 2);

        // Repeated suspend is rejected and does not mutate state.
        assert!(matches!(
            membership.suspend(ts(250)),
            Err(IdentityDomainError::InvalidTransition { .. })
        ));
        assert_eq!(membership.version().value(), 2);
        assert_eq!(membership.suspended_at(), Some(ts(200)));

        membership
            .reactivate(ts(300))
            .unwrap_or_else(|_| unreachable!());
        assert!(membership.is_active());
        assert_eq!(membership.suspended_at(), None);
        assert_eq!(membership.version().value(), 3);

        // Repeated reactivate is rejected.
        assert!(matches!(
            membership.reactivate(ts(400)),
            Err(IdentityDomainError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn rehydrate_normalizes_status_timestamp_consistency() {
        // Suspended without timestamp ⇒ fail closed.
        assert_eq!(
            TenantMembership::rehydrate(RehydrateTenantMembership {
                membership_id: id(1),
                tenant_id: id(2),
                user_id: id(3),
                status: MembershipStatus::Suspended,
                joined_at: ts(1),
                suspended_at: None,
                source: MembershipSource::Admin,
                version: 2,
            }),
            Err(IdentityDomainError::InvalidTimestamps)
        );
        // Suspended before joined_at ⇒ fail closed.
        assert_eq!(
            TenantMembership::rehydrate(RehydrateTenantMembership {
                membership_id: id(1),
                tenant_id: id(2),
                user_id: id(3),
                status: MembershipStatus::Suspended,
                joined_at: ts(5),
                suspended_at: Some(ts(4)),
                source: MembershipSource::Admin,
                version: 2,
            }),
            Err(IdentityDomainError::InvalidTimestamps)
        );
        // Active with a stale suspended_at is normalized away.
        let restored = TenantMembership::rehydrate(RehydrateTenantMembership {
            membership_id: id(1),
            tenant_id: id(2),
            user_id: id(3),
            status: MembershipStatus::Active,
            joined_at: ts(1),
            suspended_at: Some(ts(4)),
            source: MembershipSource::Admin,
            version: 3,
        })
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(restored.suspended_at(), None);
    }
}
