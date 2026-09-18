//! Global user lifecycle management (disable/enable) — PLAN-0013 WP-01.

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::domain::UserLifecycleStatus;
use crate::ports::{
    ChangeUserStatusCommit, IdentityCommandPort, IdentityStoreError, MutationActorKind,
    MutationContext, UserCommitOutcome,
};

use super::validate_idempotency_key;

/// Errors of the change-user-status use case.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChangeUserStatusError {
    /// The request was malformed.
    #[error("validation failed: {0}")]
    Validation(String),
    /// The user was not found.
    #[error("user not found")]
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
    #[error("user status change failed")]
    Failed,
}

impl From<IdentityStoreError> for ChangeUserStatusError {
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

/// Command for [`ChangeUserStatus`].
#[derive(Debug, Clone)]
pub struct ChangeUserStatusCommand {
    /// Target user.
    pub user_id: Uuid,
    /// Requested lifecycle status.
    pub target_status: UserLifecycleStatus,
    /// Optimistic version observed by the caller.
    pub expected_version: i64,
    /// Requesting actor (audit).
    pub actor_user_id: Uuid,
    /// Caller idempotency key.
    pub idempotency_key: Option<String>,
    /// Optional reason for the audit trail.
    pub reason: Option<String>,
}

/// Disable or enable a platform user globally. A disabled user has zero
/// business authority in every tenant, immediately on the next check.
pub struct ChangeUserStatus {
    command_port: Arc<dyn IdentityCommandPort>,
}

impl ChangeUserStatus {
    #[must_use]
    pub fn new(command_port: Arc<dyn IdentityCommandPort>) -> Self {
        Self { command_port }
    }

    pub async fn execute(
        &self,
        command: ChangeUserStatusCommand,
    ) -> Result<UserCommitOutcome, ChangeUserStatusError> {
        if command.user_id.is_nil() || command.actor_user_id.is_nil() {
            return Err(ChangeUserStatusError::Validation(
                "user and actor ids must not be nil".to_string(),
            ));
        }
        if command.expected_version < 1 {
            return Err(ChangeUserStatusError::Validation(
                "expected_version must be positive".to_string(),
            ));
        }
        validate_idempotency_key(command.idempotency_key.as_ref())
            .map_err(ChangeUserStatusError::Validation)?;

        self.command_port
            .change_user_status(ChangeUserStatusCommit {
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
            .map_err(ChangeUserStatusError::from)
    }
}
