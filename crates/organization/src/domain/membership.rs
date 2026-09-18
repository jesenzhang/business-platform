//! `OrganizationMembership` — links a platform user to an organization unit.
//!
//! Soft lifecycle (`Active | Inactive`): removal deactivates so membership
//! history stays queryable; re-add converges by reactivating.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::OrganizationDomainError;
use super::version::AggregateVersion;

/// Kind of membership. `Leader` marks the unit lead for escalation and
/// future org-aware policy; it grants nothing by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrganizationMembershipType {
    /// Regular member.
    Member,
    /// Unit lead.
    Leader,
}

impl OrganizationMembershipType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Leader => "leader",
        }
    }
}

/// Membership lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrganizationMembershipStatus {
    /// The user belongs to the unit.
    Active,
    /// The link was removed but the history row is retained.
    Inactive,
}

impl OrganizationMembershipStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }
}

/// The organization membership aggregate. Fields are private; adapters
/// restore state only through [`OrganizationMembership::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationMembership {
    membership_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    unit_id: Uuid,
    membership_type: OrganizationMembershipType,
    status: OrganizationMembershipStatus,
    joined_at: DateTime<Utc>,
    deactivated_at: Option<DateTime<Utc>>,
    version: AggregateVersion,
}

/// Raw persisted state accepted by [`OrganizationMembership::rehydrate`].
#[derive(Debug, Clone, Copy)]
pub struct RehydrateOrganizationMembership {
    /// Membership row id.
    pub membership_id: Uuid,
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Platform user.
    pub user_id: Uuid,
    /// Organization unit.
    pub unit_id: Uuid,
    /// Kind of membership.
    pub membership_type: OrganizationMembershipType,
    /// Persisted status.
    pub status: OrganizationMembershipStatus,
    /// Join timestamp.
    pub joined_at: DateTime<Utc>,
    /// Deactivation timestamp, if inactive.
    pub deactivated_at: Option<DateTime<Utc>>,
    /// Persisted aggregate version.
    pub version: i64,
}

impl OrganizationMembership {
    /// Join a unit: fresh memberships are `Active` at version 1.
    pub fn join(
        membership_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
        unit_id: Uuid,
        membership_type: OrganizationMembershipType,
        now: DateTime<Utc>,
    ) -> Result<Self, OrganizationDomainError> {
        if membership_id.is_nil() || tenant_id.is_nil() || user_id.is_nil() || unit_id.is_nil() {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        Ok(Self {
            membership_id,
            tenant_id,
            user_id,
            unit_id,
            membership_type,
            status: OrganizationMembershipStatus::Active,
            joined_at: now,
            deactivated_at: None,
            version: AggregateVersion::initial(),
        })
    }

    /// Restore a persisted membership after full validation.
    pub fn rehydrate(
        state: RehydrateOrganizationMembership,
    ) -> Result<Self, OrganizationDomainError> {
        if state.membership_id.is_nil()
            || state.tenant_id.is_nil()
            || state.user_id.is_nil()
            || state.unit_id.is_nil()
        {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        let version = AggregateVersion::new(state.version)
            .map_err(|_| OrganizationDomainError::InvalidVersion)?;
        let deactivated_at = match state.status {
            OrganizationMembershipStatus::Active => None,
            OrganizationMembershipStatus::Inactive => {
                let Some(deactivated_at) = state.deactivated_at else {
                    return Err(OrganizationDomainError::InvalidTransition {
                        operation: "rehydrate-consistency",
                        status: state.status.as_str(),
                    });
                };
                if deactivated_at < state.joined_at {
                    return Err(OrganizationDomainError::InvalidTransition {
                        operation: "rehydrate-consistency",
                        status: state.status.as_str(),
                    });
                }
                Some(deactivated_at)
            }
        };
        Ok(Self {
            membership_id: state.membership_id,
            tenant_id: state.tenant_id,
            user_id: state.user_id,
            unit_id: state.unit_id,
            membership_type: state.membership_type,
            status: state.status,
            joined_at: state.joined_at,
            deactivated_at,
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
    pub const fn unit_id(&self) -> Uuid {
        self.unit_id
    }

    #[must_use]
    pub const fn membership_type(&self) -> OrganizationMembershipType {
        self.membership_type
    }

    #[must_use]
    pub const fn status(&self) -> OrganizationMembershipStatus {
        self.status
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, OrganizationMembershipStatus::Active)
    }

    #[must_use]
    pub const fn joined_at(&self) -> DateTime<Utc> {
        self.joined_at
    }

    #[must_use]
    pub const fn deactivated_at(&self) -> Option<DateTime<Utc>> {
        self.deactivated_at
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    /// Deactivate (remove) the membership.
    pub fn deactivate(&mut self, now: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        if !self.is_active() {
            return Err(OrganizationDomainError::InvalidTransition {
                operation: "deactivate",
                status: self.status.as_str(),
            });
        }
        self.status = OrganizationMembershipStatus::Inactive;
        self.deactivated_at = Some(now);
        self.bump_version()
    }

    /// Reactivate an inactive membership (re-add convergence). `joined_at`
    /// keeps the original join time; `now` currently only future-proofs the
    /// call site for a potential `reactivated_at` field.
    pub fn reactivate(&mut self, now: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        if self.is_active() {
            return Err(OrganizationDomainError::InvalidTransition {
                operation: "reactivate",
                status: self.status.as_str(),
            });
        }
        let _ = now;
        self.status = OrganizationMembershipStatus::Active;
        self.deactivated_at = None;
        self.bump_version()
    }

    /// Switch the membership kind (member ↔ leader).
    pub fn set_type(
        &mut self,
        membership_type: OrganizationMembershipType,
        now: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        if self.membership_type == membership_type {
            return Ok(());
        }
        self.membership_type = membership_type;
        let _ = now;
        self.bump_version()
    }

    fn bump_version(&mut self) -> Result<(), OrganizationDomainError> {
        self.version = self
            .version
            .increment()
            .map_err(|_| OrganizationDomainError::InvalidVersion)?;
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
    fn join_deactivate_reactivate_cycle() {
        let mut membership = OrganizationMembership::join(
            id(1),
            id(2),
            id(3),
            id(4),
            OrganizationMembershipType::Member,
            ts(100),
        )
        .unwrap_or_else(|_| unreachable!());
        assert!(membership.is_active());

        membership
            .deactivate(ts(200))
            .unwrap_or_else(|_| unreachable!());
        assert!(!membership.is_active());
        assert_eq!(membership.deactivated_at(), Some(ts(200)));
        assert_eq!(membership.version().value(), 2);

        membership
            .reactivate(ts(300))
            .unwrap_or_else(|_| unreachable!());
        assert!(membership.is_active());
        assert_eq!(membership.deactivated_at(), None);
        assert_eq!(membership.version().value(), 3);

        membership
            .set_type(OrganizationMembershipType::Leader, ts(400))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            membership.membership_type(),
            OrganizationMembershipType::Leader
        );
        assert_eq!(membership.version().value(), 4);
    }

    #[test]
    fn rehydrate_enforces_status_timestamp_consistency() {
        assert!(
            OrganizationMembership::rehydrate(RehydrateOrganizationMembership {
                membership_id: id(1),
                tenant_id: id(2),
                user_id: id(3),
                unit_id: id(4),
                membership_type: OrganizationMembershipType::Member,
                status: OrganizationMembershipStatus::Inactive,
                joined_at: ts(1),
                deactivated_at: None,
                version: 2,
            })
            .is_err()
        );
    }
}
