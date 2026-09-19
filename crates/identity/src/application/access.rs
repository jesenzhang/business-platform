//! `TenantAccess` check — the identity half of the authorization gate.
//!
//! Answers "is this platform user an active member of this tenant?". No
//! role/permission logic lives here; a positive result only means the caller
//! *may be evaluated* by the Policy context for this tenant.

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::ports::{IdentityQueryPort, IdentityStoreError};

/// Why a tenant access check resolved the way it did (bounded vocabulary;
/// safe to use as a metric label and in explanations).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantAccessReason {
    /// Active user with an active tenant membership.
    Active,
    /// No membership row for this (tenant, user) — or the user record does
    /// not exist at all, which fails closed the same way.
    NoMembership,
    /// Membership exists but is suspended.
    MembershipSuspended,
    /// The platform user is globally disabled.
    UserDisabled,
}

/// Result of a tenant access check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantAccess {
    /// Whether the caller may enter policy evaluation for this tenant.
    pub allowed: bool,
    /// Bounded reason category.
    pub reason: TenantAccessReason,
}

/// Errors of the access check.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TenantAccessError {
    /// Identity persistence is unavailable.
    #[error("identity persistence is unavailable")]
    Unavailable,
    /// Identity persistence failed.
    #[error("identity persistence failed")]
    Failed,
}

impl From<IdentityStoreError> for TenantAccessError {
    fn from(error: IdentityStoreError) -> Self {
        match error {
            IdentityStoreError::Unavailable => Self::Unavailable,
            IdentityStoreError::NotFound
            | IdentityStoreError::AlreadyExists
            | IdentityStoreError::VersionConflict
            | IdentityStoreError::PrincipalMismatch
            | IdentityStoreError::IdempotencyConflict
            | IdentityStoreError::Failed => Self::Failed,
        }
    }
}

/// Check whether a (already resolved) platform user may act in a tenant.
pub struct TenantAccessChecker {
    query_port: Arc<dyn IdentityQueryPort>,
}

impl TenantAccessChecker {
    #[must_use]
    pub fn new(query_port: Arc<dyn IdentityQueryPort>) -> Self {
        Self { query_port }
    }

    /// Both user status and membership status are re-read from the store on
    /// every call (no caller-supplied freshness claims), so suspension,
    /// removal, or disable takes effect on the very next request even while
    /// the caller's JWT is still valid.
    pub async fn check(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<TenantAccess, TenantAccessError> {
        let record = self.query_port.get_tenant_user(tenant_id, user_id).await?;
        Ok(match record {
            None => TenantAccess {
                allowed: false,
                reason: TenantAccessReason::NoMembership,
            },
            Some(record) if !record.user.is_active() => TenantAccess {
                allowed: false,
                reason: TenantAccessReason::UserDisabled,
            },
            Some(record) if !record.membership.is_active() => TenantAccess {
                allowed: false,
                reason: TenantAccessReason::MembershipSuspended,
            },
            Some(_) => TenantAccess {
                allowed: true,
                reason: TenantAccessReason::Active,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MembershipSource, PlatformUser, TenantMembership};
    use crate::testing::FakeIdentityStores;
    use chrono::{DateTime, TimeZone, Utc};

    fn ts(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .unwrap_or_else(|| unreachable!())
    }

    fn tenant() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn user() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    #[tokio::test]
    async fn access_matrix_no_membership_suspended_disabled_active() {
        let stores = FakeIdentityStores::default();
        let checker = TenantAccessChecker::new(Arc::clone(&stores.query));

        // No membership yet.
        let access = checker.check(tenant(), user()).await;
        assert_eq!(
            access.unwrap_or_else(|_| unreachable!()).reason,
            TenantAccessReason::NoMembership
        );

        stores.seed_user(PlatformUser::create(user(), ts(1)).unwrap_or_else(|_| unreachable!()));
        stores.seed_membership(
            TenantMembership::join(
                Uuid::now_v7(),
                tenant(),
                user(),
                MembershipSource::Admin,
                ts(1),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
        assert!(
            checker
                .check(tenant(), user())
                .await
                .unwrap_or_else(|_| unreachable!())
                .allowed
        );

        // Suspended membership denies from the very next request.
        stores.suspend_membership(tenant(), user());
        let access = checker
            .check(tenant(), user())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!access.allowed);
        assert_eq!(access.reason, TenantAccessReason::MembershipSuspended);

        // Disabled user denies even with an active membership.
        stores.reactivate_membership(tenant(), user());
        let disabled_user = {
            let mut u = PlatformUser::create(user(), ts(1)).unwrap_or_else(|_| unreachable!());
            u.disable(ts(2)).unwrap_or_else(|_| unreachable!());
            u
        };
        stores.seed_user(disabled_user);
        let access = checker
            .check(tenant(), user())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!access.allowed);
        assert_eq!(access.reason, TenantAccessReason::UserDisabled);
    }
}
