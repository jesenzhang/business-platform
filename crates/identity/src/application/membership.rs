//! Membership management use cases (PLAN-0013 WP-01): create tenant
//! membership, suspend/reactivate a membership.
//!
//! Authorization to *call* these use cases is enforced at the delivery/policy
//! boundary; the self-reaction guard here is a domain-safety invariant
//! (baseline §10: high-risk admin grants must not silently self-serve) and
//! must hold even if the policy layer misconfigures.

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    MembershipSource, MembershipStatus, MAX_EXTERNAL_ISSUER_LEN, MAX_EXTERNAL_SUBJECT_LEN,
};
use crate::ports::{
    ChangeMembershipStatusCommit, CreateMembershipCommit, IdentityCommandPort, IdentityStoreError,
    MembershipCommitOutcome, MembershipTarget, MutationActorKind, MutationContext,
};

use super::resolve::EXTERNAL_IDENTITY_NAMESPACE;
use super::{validate_idempotency_key, validate_text_field, MAX_REASON_LEN};

/// Errors shared by membership management use cases.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChangeTenantMembershipStatusError {
    /// The request was malformed.
    #[error("validation failed: {0}")]
    Validation(String),
    /// The caller attempted to reactivate their own suspended membership.
    #[error("a user cannot reactivate their own suspended membership")]
    ForbiddenSelfReaction,
    /// The membership was not found in this tenant.
    #[error("membership not found")]
    NotFound,
    /// `expected_version` did not match the stored aggregate.
    #[error("version conflict")]
    VersionConflict,
    /// Idempotency key reuse with a different payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// Identity persistence is unavailable.
    #[error("identity persistence is unavailable")]
    Unavailable,
    /// Identity persistence failed.
    #[error("membership change failed")]
    Failed,
}

/// Errors of the create-membership use case.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CreateTenantMembershipError {
    /// The request was malformed.
    #[error("validation failed: {0}")]
    Validation(String),
    /// The user is already a member of this tenant.
    #[error("user already belongs to this tenant")]
    AlreadyExists,
    /// The referenced user does not exist (when targeting by user id).
    #[error("user not found")]
    NotFound,
    /// Idempotency key reuse with a different payload.
    #[error("idempotency key was reused with different request content")]
    IdempotencyConflict,
    /// Identity persistence is unavailable.
    #[error("identity persistence is unavailable")]
    Unavailable,
    /// Identity persistence failed.
    #[error("membership creation failed")]
    Failed,
}

impl From<IdentityStoreError> for ChangeTenantMembershipStatusError {
    fn from(error: IdentityStoreError) -> Self {
        match error {
            IdentityStoreError::NotFound => Self::NotFound,
            IdentityStoreError::VersionConflict => Self::VersionConflict,
            IdentityStoreError::IdempotencyConflict => Self::IdempotencyConflict,
            IdentityStoreError::Unavailable => Self::Unavailable,
            IdentityStoreError::AlreadyExists
            | IdentityStoreError::PrincipalMismatch
            | IdentityStoreError::Failed => Self::Failed,
        }
    }
}

impl From<IdentityStoreError> for CreateTenantMembershipError {
    fn from(error: IdentityStoreError) -> Self {
        match error {
            IdentityStoreError::NotFound => Self::NotFound,
            IdentityStoreError::AlreadyExists => Self::AlreadyExists,
            IdentityStoreError::IdempotencyConflict => Self::IdempotencyConflict,
            IdentityStoreError::Unavailable => Self::Unavailable,
            IdentityStoreError::VersionConflict
            | IdentityStoreError::PrincipalMismatch
            | IdentityStoreError::Failed => Self::Failed,
        }
    }
}

/// Membership target as supplied by the management plane (deterministic user
/// ids are derived by this layer, never by callers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateMembershipTarget {
    /// Attach an existing platform user.
    UserId(Uuid),
    /// Provision-or-match by verified external issuer/subject, then attach.
    ExternalSubject {
        /// Verified issuer.
        issuer: String,
        /// Verified subject.
        subject: String,
    },
}

fn map_target(target: CreateMembershipTarget) -> Result<MembershipTarget, String> {
    match target {
        CreateMembershipTarget::UserId(user_id) => {
            if user_id.is_nil() {
                return Err("user id must not be nil".to_string());
            }
            Ok(MembershipTarget::UserId(user_id))
        }
        CreateMembershipTarget::ExternalSubject { issuer, subject } => {
            let issuer = issuer.trim().to_string();
            let subject = subject.trim().to_string();
            if issuer.is_empty() || issuer.len() > MAX_EXTERNAL_ISSUER_LEN || issuer.contains('\0')
            {
                return Err("issuer is invalid".to_string());
            }
            if subject.is_empty()
                || subject.len() > MAX_EXTERNAL_SUBJECT_LEN
                || subject.contains('\0')
            {
                return Err("subject is invalid".to_string());
            }
            // Same deterministic derivation as first-contact resolution, so
            // onboarding-before-login and login converge on one user id.
            let deterministic_user_id = Uuid::new_v5(
                &EXTERNAL_IDENTITY_NAMESPACE,
                format!("{issuer}\u{1f}{subject}").as_bytes(),
            );
            Ok(MembershipTarget::ExternalSubject {
                issuer,
                subject,
                deterministic_user_id,
            })
        }
    }
}

/// Command for [`CreateTenantMembership`].
#[derive(Debug, Clone)]
pub struct CreateTenantMembershipCommand {
    /// Tenant being joined.
    pub tenant_id: Uuid,
    /// Existing user or external subject to provision.
    pub target: CreateMembershipTarget,
    /// Who requested it (audit + guards).
    pub actor_user_id: Uuid,
    /// Caller idempotency key.
    pub idempotency_key: Option<String>,
    /// Optional reason for the audit trail.
    pub reason: Option<String>,
    /// Record origin.
    pub source: MembershipSource,
}

/// Create a tenant membership (attaches a user to a tenant).
pub struct CreateTenantMembership {
    command_port: Arc<dyn IdentityCommandPort>,
}

impl CreateTenantMembership {
    #[must_use]
    pub fn new(command_port: Arc<dyn IdentityCommandPort>) -> Self {
        Self { command_port }
    }

    pub async fn execute(
        &self,
        command: CreateTenantMembershipCommand,
    ) -> Result<MembershipCommitOutcome, CreateTenantMembershipError> {
        if command.tenant_id.is_nil() {
            return Err(CreateTenantMembershipError::Validation(
                "tenant id must not be nil".to_string(),
            ));
        }
        if command.actor_user_id.is_nil() {
            return Err(CreateTenantMembershipError::Validation(
                "actor user id must not be nil".to_string(),
            ));
        }
        let target = map_target(command.target).map_err(CreateTenantMembershipError::Validation)?;
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(CreateTenantMembershipError::Validation)?;
        if let Some(reason) = &command.reason {
            validate_text_field(reason, MAX_REASON_LEN, "reason")
                .map_err(CreateTenantMembershipError::Validation)?;
        }

        self.command_port
            .create_membership(CreateMembershipCommit {
                tenant_id: command.tenant_id,
                target,
                source: command.source,
                audit: MutationContext {
                    actor_id: command.actor_user_id.to_string(),
                    actor_kind: MutationActorKind::User,
                    operation_id: Uuid::now_v7(),
                    trace_id: None,
                    reason: command.reason,
                },
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await
            .map_err(CreateTenantMembershipError::from)
    }
}

/// Command for [`ChangeTenantMembershipStatus`].
#[derive(Debug, Clone)]
pub struct ChangeTenantMembershipStatusCommand {
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Target user.
    pub user_id: Uuid,
    /// Requested new status.
    pub target_status: MembershipStatus,
    /// Optimistic version observed by the caller.
    pub expected_version: i64,
    /// Who requested it (audit + self-reaction guard).
    pub actor_user_id: Uuid,
    /// Caller idempotency key.
    pub idempotency_key: Option<String>,
    /// Optional reason for the audit trail.
    pub reason: Option<String>,
}

/// Suspend/reactivate a tenant membership.
pub struct ChangeTenantMembershipStatus {
    command_port: Arc<dyn IdentityCommandPort>,
}

impl ChangeTenantMembershipStatus {
    #[must_use]
    pub fn new(command_port: Arc<dyn IdentityCommandPort>) -> Self {
        Self { command_port }
    }

    pub async fn execute(
        &self,
        command: ChangeTenantMembershipStatusCommand,
    ) -> Result<MembershipCommitOutcome, ChangeTenantMembershipStatusError> {
        if command.tenant_id.is_nil() || command.user_id.is_nil() {
            return Err(ChangeTenantMembershipStatusError::Validation(
                "tenant and user ids must not be nil".to_string(),
            ));
        }
        if command.actor_user_id.is_nil() {
            return Err(ChangeTenantMembershipStatusError::Validation(
                "actor user id must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(ChangeTenantMembershipStatusError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        // Domain-safety invariant (must hold regardless of policy): a user
        // cannot lift their own suspension.
        if command.actor_user_id == command.user_id
            && matches!(command.target_status, MembershipStatus::Active)
        {
            return Err(ChangeTenantMembershipStatusError::ForbiddenSelfReaction);
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(ChangeTenantMembershipStatusError::Validation)?;
        if let Some(reason) = &command.reason {
            validate_text_field(reason, MAX_REASON_LEN, "reason")
                .map_err(ChangeTenantMembershipStatusError::Validation)?;
        }

        self.command_port
            .change_membership_status(ChangeMembershipStatusCommit {
                tenant_id: command.tenant_id,
                user_id: command.user_id,
                target_status: command.target_status,
                expected_version: command.expected_version,
                audit: MutationContext {
                    actor_id: command.actor_user_id.to_string(),
                    actor_kind: MutationActorKind::User,
                    operation_id: Uuid::now_v7(),
                    trace_id: None,
                    reason: command.reason,
                },
                idempotency_key: command.idempotency_key,
                now: chrono::Utc::now(),
            })
            .await
            .map_err(ChangeTenantMembershipStatusError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeIdentityStores;
    use chrono::{TimeZone, Utc};

    fn ts(seconds: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .unwrap_or_else(|| unreachable!())
    }

    fn tenant() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn actor() -> Uuid {
        Uuid::from_bytes([3; 16])
    }

    fn target_user() -> Uuid {
        Uuid::from_bytes([4; 16])
    }

    #[tokio::test]
    async fn create_membership_validates_and_persists() {
        let stores = FakeIdentityStores::default();
        stores.seed_user(
            crate::domain::PlatformUser::create(target_user(), ts(1))
                .unwrap_or_else(|_| unreachable!()),
        );
        let use_case = CreateTenantMembership::new(Arc::clone(&stores.command));

        // Nil tenant rejected.
        let bad = CreateTenantMembershipCommand {
            tenant_id: Uuid::nil(),
            target: CreateMembershipTarget::UserId(target_user()),
            actor_user_id: actor(),
            idempotency_key: None,
            reason: None,
            source: MembershipSource::Admin,
        };
        assert!(matches!(
            use_case.execute(bad).await,
            Err(CreateTenantMembershipError::Validation(_))
        ));

        let ok = CreateTenantMembershipCommand {
            tenant_id: tenant(),
            target: CreateMembershipTarget::UserId(target_user()),
            actor_user_id: actor(),
            idempotency_key: Some("key-1".to_string()),
            reason: Some("onboarding".to_string()),
            source: MembershipSource::Admin,
        };
        let outcome = use_case
            .execute(ok.clone())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!outcome.replayed);
        assert!(outcome.membership.is_active());

        // Idempotent replay converges onto the same record.
        let replay = use_case
            .execute(ok.clone())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(replay.replayed);
        assert_eq!(
            replay.membership.membership_id(),
            outcome.membership.membership_id()
        );

        // Same operation scope (op+tenant) + same key, semantic payload
        // changed ⇒ IdempotencyConflict.
        let conflicting = CreateTenantMembershipCommand {
            idempotency_key: Some("key-1".to_string()),
            source: MembershipSource::Bootstrap,
            ..ok
        };
        assert_eq!(
            use_case.execute(conflicting).await,
            Err(CreateTenantMembershipError::IdempotencyConflict)
        );
    }

    #[tokio::test]
    async fn status_changes_enforce_version_and_self_reaction_guard() {
        let stores = FakeIdentityStores::default();
        let use_case = ChangeTenantMembershipStatus::new(Arc::clone(&stores.command));
        stores.seed_membership(
            crate::domain::TenantMembership::join(
                Uuid::now_v7(),
                tenant(),
                target_user(),
                MembershipSource::Admin,
                ts(1),
            )
            .unwrap_or_else(|_| unreachable!()),
        );

        let command = ChangeTenantMembershipStatusCommand {
            tenant_id: tenant(),
            user_id: target_user(),
            target_status: MembershipStatus::Suspended,
            expected_version: 99,
            actor_user_id: actor(),
            idempotency_key: None,
            reason: None,
        };
        assert_eq!(
            use_case.execute(command).await,
            Err(ChangeTenantMembershipStatusError::VersionConflict)
        );

        // Self-reaction guard: suspend of self allowed, reactivate of self
        // denied even though the caller would hold the permission.
        let self_suspend = ChangeTenantMembershipStatusCommand {
            tenant_id: tenant(),
            user_id: target_user(),
            target_status: MembershipStatus::Suspended,
            expected_version: 1,
            actor_user_id: target_user(),
            idempotency_key: None,
            reason: None,
        };
        let suspended = use_case
            .execute(self_suspend.clone())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!suspended.membership.is_active());

        let self_reactivate = ChangeTenantMembershipStatusCommand {
            target_status: MembershipStatus::Active,
            ..self_suspend
        };
        assert_eq!(
            use_case.execute(self_reactivate).await,
            Err(ChangeTenantMembershipStatusError::ForbiddenSelfReaction)
        );

        let admin_reactivate = ChangeTenantMembershipStatusCommand {
            tenant_id: tenant(),
            user_id: target_user(),
            target_status: MembershipStatus::Active,
            expected_version: 2,
            actor_user_id: actor(),
            idempotency_key: None,
            reason: None,
        };
        let reactivated = use_case
            .execute(admin_reactivate)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(reactivated.membership.is_active());
    }
}
