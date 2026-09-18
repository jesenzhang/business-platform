//! `ResolveAuthenticatedUser` — map an authenticated external subject to
//! platform identity (PLAN-0013 WP-01).
//!
//! Runs strictly *after* authentication (ADR-0024): the issuer and subject
//! must already be verified by the OIDC boundary (or produced by the
//! server-side dev-auth config). This use case never trusts headers.

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    ExternalIdentity, PlatformUser, MAX_EXTERNAL_ISSUER_LEN, MAX_EXTERNAL_SUBJECT_LEN,
};
use crate::ports::{
    IdentityResolvePort, IdentityStoreError, MutationActorKind, MutationContext,
    ResolvePrincipalCommit,
};

/// Fixed namespace for deterministic user ids derived from external keys.
/// Versioned: changing it would fork identity, so it is a stable contract.
pub const EXTERNAL_IDENTITY_NAMESPACE: Uuid =
    Uuid::from_u128(0x2f4a0e0d_7c1b_4a53_9c8e_5b7d0a9f6e21);

/// Input for [`ResolveAuthenticatedUser`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveAuthenticatedUserCommand {
    /// Verified token issuer (server-side value).
    pub issuer: String,
    /// Verified token subject.
    pub subject: String,
    /// The trusted `user_id` claim when the token carries one.
    pub claimed_user_id: Option<Uuid>,
}

/// The caller as the platform sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCaller {
    /// The durable platform user.
    pub user: PlatformUser,
    /// The durable external identity link.
    pub external_identity: ExternalIdentity,
    /// True when this request provisioned the user for the first time.
    pub provisioned: bool,
}

/// Errors of the resolve use case.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ResolveCallerError {
    /// The external key was malformed (defense in depth; auth already did).
    #[error("validation failed: {0}")]
    Validation(String),
    /// The external key conflicts with the platform identity state
    /// (same key bound to a different user, or a claimed user id already
    /// linked to a different subject). Fail closed.
    #[error("external identity mapping conflict")]
    PrincipalMismatch,
    /// Identity persistence is unavailable.
    #[error("identity persistence is unavailable")]
    Unavailable,
    /// Identity persistence failed.
    #[error("identity resolution failed")]
    Failed,
}

impl ResolveCallerError {
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }
}

impl From<IdentityStoreError> for ResolveCallerError {
    fn from(error: IdentityStoreError) -> Self {
        match error {
            IdentityStoreError::PrincipalMismatch => Self::PrincipalMismatch,
            IdentityStoreError::Unavailable => Self::Unavailable,
            IdentityStoreError::NotFound
            | IdentityStoreError::AlreadyExists
            | IdentityStoreError::VersionConflict
            | IdentityStoreError::IdempotencyConflict
            | IdentityStoreError::Failed => Self::Failed,
        }
    }
}

/// Resolve-or-provision use case (see module docs).
pub struct ResolveAuthenticatedUser {
    resolve_port: Arc<dyn IdentityResolvePort>,
}

impl ResolveAuthenticatedUser {
    #[must_use]
    pub fn new(resolve_port: Arc<dyn IdentityResolvePort>) -> Self {
        Self { resolve_port }
    }

    /// Resolve the caller, provisioning a `PlatformUser` on first contact.
    /// A provisioned user has **no** memberships and therefore zero
    /// business authority until an admin attaches one.
    pub async fn execute(
        &self,
        command: ResolveAuthenticatedUserCommand,
    ) -> Result<ResolvedCaller, ResolveCallerError> {
        let issuer = command.issuer.trim().to_string();
        let subject = command.subject.trim().to_string();
        if issuer.is_empty() || issuer.len() > MAX_EXTERNAL_ISSUER_LEN || issuer.contains('\0') {
            return Err(ResolveCallerError::Validation(
                "issuer is invalid".to_string(),
            ));
        }
        if subject.is_empty() || subject.len() > MAX_EXTERNAL_SUBJECT_LEN || subject.contains('\0')
        {
            return Err(ResolveCallerError::Validation(
                "subject is invalid".to_string(),
            ));
        }
        if command.claimed_user_id.is_some_and(|id| id.is_nil()) {
            return Err(ResolveCallerError::Validation(
                "claimed user id must not be nil".to_string(),
            ));
        }

        // Deterministic identity: no claim ⇒ stable UUIDv5 over the external
        // key, so concurrent first requests converge on the same user id.
        let deterministic_user_id = Uuid::new_v5(
            &EXTERNAL_IDENTITY_NAMESPACE,
            format!("{issuer}\u{1f}{subject}").as_bytes(),
        );

        let user_id_for_audit = command.claimed_user_id.unwrap_or(deterministic_user_id);
        let commit = ResolvePrincipalCommit {
            issuer,
            subject,
            claimed_user_id: command.claimed_user_id,
            deterministic_user_id,
            audit: MutationContext {
                actor_id: user_id_for_audit.to_string(),
                actor_kind: MutationActorKind::User,
                operation_id: Uuid::now_v7(),
                trace_id: None,
                reason: None,
            },
            now: chrono::Utc::now(),
        };

        let resolved = self.resolve_port.resolve_or_provision(commit).await?;
        Ok(ResolvedCaller {
            user: resolved.user,
            external_identity: resolved.external_identity,
            provisioned: resolved.provisioned,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeIdentityStores;

    fn command() -> ResolveAuthenticatedUserCommand {
        ResolveAuthenticatedUserCommand {
            issuer: "https://idp.example/realms/t".to_string(),
            subject: "user-1".to_string(),
            claimed_user_id: None,
        }
    }

    #[tokio::test]
    async fn rejects_blank_or_oversized_keys_and_nil_claim() {
        let stores = FakeIdentityStores::default();
        let use_case = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));

        let blank = ResolveAuthenticatedUserCommand {
            issuer: "   ".to_string(),
            ..command()
        };
        assert!(matches!(
            use_case.execute(blank).await,
            Err(ResolveCallerError::Validation(_))
        ));

        let oversized = ResolveAuthenticatedUserCommand {
            subject: "s".repeat(MAX_EXTERNAL_SUBJECT_LEN + 1),
            ..command()
        };
        assert!(matches!(
            use_case.execute(oversized).await,
            Err(ResolveCallerError::Validation(_))
        ));

        let nil_claim = ResolveAuthenticatedUserCommand {
            claimed_user_id: Some(Uuid::nil()),
            ..command()
        };
        assert!(matches!(
            use_case.execute(nil_claim).await,
            Err(ResolveCallerError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn first_contact_provisions_and_repeat_requests_converge() {
        let stores = FakeIdentityStores::default();
        let use_case = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));

        let first = use_case
            .execute(command())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(first.provisioned);

        let second = use_case
            .execute(command())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!second.provisioned);
        assert_eq!(second.user.user_id(), first.user.user_id());
        assert_eq!(
            second.external_identity, first.external_identity,
            "issuer+subject must stably point to one user"
        );
    }

    #[tokio::test]
    async fn claimed_user_id_is_honored_and_conflicting_claim_denies() {
        let stores = FakeIdentityStores::default();
        let use_case = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));

        let claimed = Uuid::from_bytes([7; 16]);
        let resolved = use_case
            .execute(ResolveAuthenticatedUserCommand {
                claimed_user_id: Some(claimed),
                ..command()
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(resolved.user.user_id(), claimed);

        // Same external key, different claimed user id ⇒ fail closed.
        let conflict = use_case
            .execute(ResolveAuthenticatedUserCommand {
                claimed_user_id: Some(Uuid::from_bytes([9; 16])),
                ..command()
            })
            .await;
        assert_eq!(conflict, Err(ResolveCallerError::PrincipalMismatch));
    }
}
