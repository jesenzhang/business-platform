//! `PlatformUser` — the durable platform identity of a person.
//!
//! Identity is the stable `user_id`; display names and emails are IdP-side
//! profile data and never decide identity. A disabled user can never obtain
//! business authorization, and historical business references survive
//! disabling forever (status change only, no deletion).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::IdentityDomainError;
use super::version::AggregateVersion;

/// Platform-visible lifecycle of a user. Kept minimal on purpose: the full
/// IAM lifecycle (invite, lockout, offboarding) is out of PLAN-0013 scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserLifecycleStatus {
    /// The user exists and may act in tenants where they hold an active
    /// membership.
    Active,
    /// The user is switched off: no business authorization, in every tenant.
    Disabled,
}

impl UserLifecycleStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

/// The platform user aggregate. Fields are private; adapters restore state
/// only through [`PlatformUser::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformUser {
    user_id: Uuid,
    status: UserLifecycleStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: AggregateVersion,
}

/// Raw persisted state accepted by [`PlatformUser::rehydrate`].
#[derive(Debug, Clone, Copy)]
pub struct RehydratePlatformUser {
    /// Stable user id.
    pub user_id: Uuid,
    /// Persisted lifecycle status.
    pub status: UserLifecycleStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Persisted aggregate version.
    pub version: i64,
}

impl PlatformUser {
    /// Create a new active user at version 1.
    pub fn create(user_id: Uuid, now: DateTime<Utc>) -> Result<Self, IdentityDomainError> {
        if user_id.is_nil() {
            return Err(IdentityDomainError::InvalidIdentity);
        }
        Ok(Self {
            user_id,
            status: UserLifecycleStatus::Active,
            created_at: now,
            updated_at: now,
            version: AggregateVersion::initial(),
        })
    }

    /// Restore a persisted user after full validation.
    pub fn rehydrate(state: RehydratePlatformUser) -> Result<Self, IdentityDomainError> {
        if state.user_id.is_nil() {
            return Err(IdentityDomainError::InvalidIdentity);
        }
        let version = AggregateVersion::new(state.version)
            .map_err(|_| IdentityDomainError::InvalidVersion)?;
        if state.updated_at < state.created_at {
            return Err(IdentityDomainError::InvalidTimestamps);
        }
        Ok(Self {
            user_id: state.user_id,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            version,
        })
    }

    #[must_use]
    pub const fn user_id(&self) -> Uuid {
        self.user_id
    }

    #[must_use]
    pub const fn status(&self) -> UserLifecycleStatus {
        self.status
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, UserLifecycleStatus::Active)
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

    /// Disable the user. Idempotent attempts against an already disabled user
    /// are rejected so management flows observe an explicit transition rule.
    pub fn disable(&mut self, now: DateTime<Utc>) -> Result<(), IdentityDomainError> {
        self.transition_to(UserLifecycleStatus::Disabled, "disable", now)
    }

    /// Re-enable a disabled user.
    pub fn enable(&mut self, now: DateTime<Utc>) -> Result<(), IdentityDomainError> {
        self.transition_to(UserLifecycleStatus::Active, "enable", now)
    }

    fn transition_to(
        &mut self,
        target: UserLifecycleStatus,
        operation: &'static str,
        now: DateTime<Utc>,
    ) -> Result<(), IdentityDomainError> {
        let allowed = matches!(
            (self.status, target),
            (UserLifecycleStatus::Active, UserLifecycleStatus::Disabled)
                | (UserLifecycleStatus::Disabled, UserLifecycleStatus::Active)
        );
        if !allowed {
            return Err(IdentityDomainError::InvalidTransition {
                operation,
                status: self.status.as_str(),
            });
        }
        self.status = target;
        if now > self.updated_at {
            self.updated_at = now;
        }
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

    fn user_id() -> Uuid {
        Uuid::parse_str("66666666-7777-8888-9999-000000000001").unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn rejects_nil_identity_and_non_positive_versions() {
        assert_eq!(
            PlatformUser::create(Uuid::nil(), ts(100)),
            Err(IdentityDomainError::InvalidIdentity)
        );
        let state = RehydratePlatformUser {
            user_id: user_id(),
            status: UserLifecycleStatus::Active,
            created_at: ts(1),
            updated_at: ts(1),
            version: 0,
        };
        assert_eq!(
            PlatformUser::rehydrate(state),
            Err(IdentityDomainError::InvalidVersion)
        );
    }

    #[test]
    fn rehydrate_rejects_inverted_timestamps() {
        let state = RehydratePlatformUser {
            user_id: user_id(),
            status: UserLifecycleStatus::Active,
            created_at: ts(10),
            updated_at: ts(9),
            version: 1,
        };
        assert_eq!(
            PlatformUser::rehydrate(state),
            Err(IdentityDomainError::InvalidTimestamps)
        );
    }

    #[test]
    fn disable_and_enable_toggle_status_and_increment_version() {
        let mut user = PlatformUser::create(user_id(), ts(100)).unwrap_or_else(|_| unreachable!());
        assert_eq!(user.version().value(), 1);
        assert!(user.is_active());

        user.disable(ts(200)).unwrap_or_else(|_| unreachable!());
        assert_eq!(user.status(), UserLifecycleStatus::Disabled);
        assert_eq!(user.version().value(), 2);

        // Repeated disable is a rejected transition and does not mutate state.
        assert!(matches!(
            user.disable(ts(300)),
            Err(IdentityDomainError::InvalidTransition { .. })
        ));
        assert_eq!(user.version().value(), 2);

        user.enable(ts(300)).unwrap_or_else(|_| unreachable!());
        assert!(user.is_active());
        assert_eq!(user.version().value(), 3);
        assert_eq!(user.updated_at(), ts(300));
    }
}
