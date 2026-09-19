//! `BootstrapAdministrator` — server-controlled cold-start of the first
//! administrator (PLAN-0013 WP-06).
//!
//! There is deliberately **no** "first user becomes admin" behavior
//! anywhere in this code base. Bootstrap runs only from the composition
//! root at startup from explicit server configuration
//! (`[auth.bootstrap]`); HTTP never triggers it and no request surface can
//! enable it. The flow is fixed:
//!
//! 1. resolve-or-provision the `PlatformUser` for `(issuer, subject)`,
//! 2. ensure an **active** `TenantMembership` (source = bootstrap),
//! 3. bind `system.bootstrap-admin` (tenant scope) **through the Policy
//!    binding port** — Identity never writes `role_bindings`,
//! 4. persist a durable ledger row in `platform_bootstrap_executions`
//!    (identity-owned).
//!
//! Re-running with an unchanged configuration digest is a no-op *even
//! after the binding was revoked*: repeating a bootstrap requires a
//! deliberate `version` bump in the configuration, which produces a new
//! digest and is fully audited through the regular mutation audit trail.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    MembershipSource, MembershipStatus, MAX_EXTERNAL_ISSUER_LEN, MAX_EXTERNAL_SUBJECT_LEN,
};
use crate::ports::{
    BootstrapLedgerEntry, BootstrapLedgerPort, BootstrapOutcome, ChangeMembershipStatusCommit,
    CreateMembershipCommit, IdentityCommandPort, IdentityQueryPort, IdentityResolvePort,
    IdentityStoreError, MembershipTarget, MutationActorKind, MutationContext,
    ResolvePrincipalCommit,
};

use super::resolve::EXTERNAL_IDENTITY_NAMESPACE;

/// Role the bootstrap service binds (owned by Policy; stable key).
pub const BOOTSTRAP_ROLE_STABLE_KEY: &str = "system.bootstrap-admin";

/// Audit actor identity for bootstrap mutations. Adapters persist the
/// audit actor as a UUID and fail closed on non-UUID actors, so this must
/// be a canonical, deterministic UUID shared by every bootstrap run.
#[must_use]
pub fn bootstrap_service_actor() -> Uuid {
    Uuid::new_v5(&EXTERNAL_IDENTITY_NAMESPACE, b"bootstrap-service")
}

/// Server-side bootstrap configuration (mirrors `[auth.bootstrap]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapAdministratorConfig {
    /// Must be `true` for the composition root to run bootstrap.
    pub enabled: bool,
    /// Tenant the administrator is bound into.
    pub tenant_id: Uuid,
    /// Verified external issuer (https, exact match).
    pub issuer: String,
    /// Verified external subject.
    pub subject: String,
    /// Operator-chosen configuration version. Repeating a bootstrap
    /// requires bumping this value.
    pub version: i64,
}

impl BootstrapAdministratorConfig {
    /// Reject malformed configuration before anything is executed.
    /// Production loaders additionally require the issuer scheme to be
    /// `https` — enforced here so every caller inherits the rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.tenant_id.is_nil() {
            return Err("bootstrap tenant_id must not be nil".to_string());
        }
        let issuer = self.issuer.trim();
        if issuer.is_empty()
            || issuer.len() > MAX_EXTERNAL_ISSUER_LEN
            || issuer.contains('\0')
            || !issuer.starts_with("https://")
        {
            return Err("bootstrap issuer must be a non-empty https URL".to_string());
        }
        let subject = self.subject.trim();
        if subject.is_empty() || subject.len() > MAX_EXTERNAL_SUBJECT_LEN || subject.contains('\0')
        {
            return Err("bootstrap subject must be non-empty".to_string());
        }
        if self.version < 1 {
            return Err("bootstrap version must be >= 1".to_string());
        }
        Ok(())
    }

    /// Canonical SHA-256 hex digest over the semantic configuration. Any
    /// change to the targeted identity, tenant, role, or version yields a
    /// new digest; formatting changes do not.
    #[must_use]
    pub fn config_digest(&self) -> String {
        let canonical = format!(
            "{}|{}|{}|{}|{BOOTSTRAP_ROLE_STABLE_KEY}",
            self.tenant_id,
            self.issuer.trim(),
            self.subject.trim(),
            self.version,
        );
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hasher.finalize();
        let mut hex = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }
}

/// Failure to bind the bootstrap role through the Policy binding port.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapBindingError {
    /// Policy persistence is unavailable (retryable).
    #[error("policy persistence is unavailable")]
    Unavailable,
    /// The binding failed (validation, conflict, or storage error).
    #[error("bootstrap role binding failed")]
    Failed,
}

/// Port that binds [`BOOTSTRAP_ROLE_STABLE_KEY`] for a user. The
/// composition root implements it with the Policy crate's `BindRole`
/// application use case; Identity never touches `role_bindings` itself.
#[async_trait::async_trait]
pub trait BootstrapBindingPort: Send + Sync {
    /// Bind the bootstrap role (tenant scope, open-ended validity) to
    /// `user_id` inside `tenant_id`. The idempotency key makes repeated
    /// executions for the same configuration digest converge.
    async fn bind_bootstrap_role(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        idempotency_key: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<(), BootstrapBindingError>;
}

/// Errors of the bootstrap use case.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BootstrapError {
    /// The configuration is malformed.
    #[error("bootstrap configuration is invalid: {0}")]
    Config(String),
    /// A newer bootstrap version has already been applied; this (older or
    /// drifted) configuration must not re-execute. Fix the configuration
    /// and bump `version` deliberately.
    #[error("a newer bootstrap version has already been recorded")]
    ConfigStale,
    /// The external key conflicts with existing identity state.
    #[error("external identity mapping conflict")]
    PrincipalMismatch,
    /// A port is unavailable (retry on next startup).
    #[error("bootstrap dependencies are unavailable")]
    Unavailable,
    /// A step failed; the failure was recorded in the ledger.
    #[error("bootstrap execution failed")]
    Failed,
}

/// Cold-start use case (see module docs).
pub struct BootstrapAdministrator {
    resolve: Arc<dyn IdentityResolvePort>,
    command: Arc<dyn IdentityCommandPort>,
    query: Arc<dyn IdentityQueryPort>,
    ledger: Arc<dyn BootstrapLedgerPort>,
    binding: Arc<dyn BootstrapBindingPort>,
}

impl BootstrapAdministrator {
    #[must_use]
    pub fn new(
        resolve: Arc<dyn IdentityResolvePort>,
        command: Arc<dyn IdentityCommandPort>,
        query: Arc<dyn IdentityQueryPort>,
        ledger: Arc<dyn BootstrapLedgerPort>,
        binding: Arc<dyn BootstrapBindingPort>,
    ) -> Self {
        Self {
            resolve,
            command,
            query,
            ledger,
            binding,
        }
    }

    /// Execute bootstrap when `config.enabled`. Returns `None` when the
    /// configuration disables bootstrap (nothing was read or written).
    pub async fn execute(
        &self,
        config: &BootstrapAdministratorConfig,
    ) -> Result<Option<BootstrapOutcome>, BootstrapError> {
        if !config.enabled {
            return Ok(None);
        }
        config.validate().map_err(BootstrapError::Config)?;

        let digest = config.config_digest();
        let issuer = config.issuer.trim().to_string();
        let subject = config.subject.trim().to_string();

        let latest = self
            .ledger
            .latest_for(config.tenant_id, &issuer, &subject)
            .await
            .map_err(map_store_error)?;
        if let Some(entry) = latest {
            match entry.outcome {
                BootstrapOutcome::Executed | BootstrapOutcome::NoOp => {
                    match entry.config_version.cmp(&config.version) {
                        std::cmp::Ordering::Equal => {
                            if entry.config_digest != digest {
                                // Same version, different content: the config
                                // drifted without a deliberate bump. Fail closed.
                                return Err(BootstrapError::ConfigStale);
                            }
                            // Same digest: terminal no-op, *even after the
                            // binding was later revoked*. Repetition requires a
                            // deliberate version bump.
                            return Ok(Some(BootstrapOutcome::NoOp));
                        }
                        std::cmp::Ordering::Greater => {
                            // A newer version was already applied; replaying
                            // older configuration must fail closed.
                            return Err(BootstrapError::ConfigStale);
                        }
                        // A strictly higher recorded version means the operator
                        // deliberately bumped the configuration: re-execute.
                        std::cmp::Ordering::Less => {}
                    }
                }
                // A recorded failure stays retryable with the same digest.
                BootstrapOutcome::Failed => {}
            }
        }

        let now = Utc::now();
        let audit = MutationContext {
            actor_id: bootstrap_service_actor().to_string(),
            actor_kind: MutationActorKind::Bootstrap,
            operation_id: Uuid::now_v7(),
            trace_id: None,
            reason: Some("server-configured bootstrap administrator".to_string()),
        };

        let execution = self
            .execute_steps(config, &issuer, &subject, &digest, &audit, now)
            .await;

        let outcome = match execution {
            Ok(()) => BootstrapOutcome::Executed,
            Err(error) => {
                // Best-effort durable failure record; the deployment must
                // react to a Failed ledger row. If the ledger write itself
                // fails, the original execution cause still surfaces — the
                // missing ledger row is itself an alertable anomaly, and
                // masking the real cause would break diagnosis.
                let _ = self
                    .record(
                        config,
                        &issuer,
                        &subject,
                        &digest,
                        BootstrapOutcome::Failed,
                        Utc::now(),
                    )
                    .await;
                return Err(error);
            }
        };
        self.record(config, &issuer, &subject, &digest, outcome, Utc::now())
            .await?;
        Ok(Some(outcome))
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_steps(
        &self,
        config: &BootstrapAdministratorConfig,
        issuer: &str,
        subject: &str,
        digest: &str,
        audit: &MutationContext,
        now: DateTime<Utc>,
    ) -> Result<(), BootstrapError> {
        // Deterministic identity: the same external key always maps to the
        // same user id, so repeated or concurrent bootstraps converge.
        let deterministic_user_id = Uuid::new_v5(
            &EXTERNAL_IDENTITY_NAMESPACE,
            format!("{issuer}\u{1f}{subject}").as_bytes(),
        );
        let resolved = self
            .resolve
            .resolve_or_provision(ResolvePrincipalCommit {
                issuer: issuer.to_string(),
                subject: subject.to_string(),
                claimed_user_id: None,
                deterministic_user_id,
                audit: audit.clone(),
                now,
            })
            .await
            .map_err(map_store_error)?;
        let user_id = resolved.user.user_id();

        match self
            .command
            .create_membership(CreateMembershipCommit {
                tenant_id: config.tenant_id,
                target: MembershipTarget::UserId(user_id),
                source: MembershipSource::Bootstrap,
                audit: audit.clone(),
                idempotency_key: Some(format!("bootstrap-membership|{digest}")),
                now,
            })
            .await
        {
            Ok(_) | Err(IdentityStoreError::AlreadyExists) => {}
            Err(error) => return Err(map_store_error(error)),
        }
        // The module contract is an **active** membership, so the create
        // response is never trusted for status (an idempotent replay
        // returns the row as it was when first written, and an existing
        // row may have been suspended between runs): the authoritative
        // status is read back and a suspended membership is reactivated
        // through the versioned status-change commit, audited under the
        // bootstrap actor. A deliberate configuration bump is exactly the
        // server-side recovery path for a self-suspended sole bootstrap
        // admin.
        self.ensure_active_membership(config.tenant_id, user_id, digest, audit, now)
            .await?;

        self.binding
            .bind_bootstrap_role(
                config.tenant_id,
                user_id,
                &format!("bootstrap-bind|{digest}"),
                "server-configured bootstrap administrator",
                now,
            )
            .await
            .map_err(|error| match error {
                BootstrapBindingError::Unavailable => BootstrapError::Unavailable,
                BootstrapBindingError::Failed => BootstrapError::Failed,
            })
    }

    /// Verify the bootstrap membership is Active; reactivate a suspended
    /// one with a versioned, bootstrapped, idempotent status change. A
    /// missing row after a converged create is a store anomaly and fails
    /// closed.
    async fn ensure_active_membership(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        digest: &str,
        audit: &MutationContext,
        now: DateTime<Utc>,
    ) -> Result<(), BootstrapError> {
        let membership = self
            .query
            .get_membership(tenant_id, user_id)
            .await
            .map_err(map_store_error)?
            .ok_or(BootstrapError::Failed)?;
        if membership.is_active() {
            return Ok(());
        }
        self.command
            .change_membership_status(ChangeMembershipStatusCommit {
                tenant_id,
                user_id,
                target_status: MembershipStatus::Active,
                expected_version: membership.version().value(),
                audit: audit.clone(),
                idempotency_key: Some(format!("bootstrap-membership-activate|{digest}")),
                now,
            })
            .await
            .map(|_| ())
            .map_err(map_store_error)
    }

    #[allow(clippy::too_many_arguments)]
    async fn record(
        &self,
        config: &BootstrapAdministratorConfig,
        issuer: &str,
        subject: &str,
        digest: &str,
        outcome: BootstrapOutcome,
        now: DateTime<Utc>,
    ) -> Result<(), BootstrapError> {
        self.ledger
            .record(&BootstrapLedgerEntry {
                tenant_id: config.tenant_id,
                issuer: issuer.to_string(),
                subject: subject.to_string(),
                role_stable_key: BOOTSTRAP_ROLE_STABLE_KEY.to_string(),
                config_version: config.version,
                config_digest: digest.to_string(),
                outcome,
                recorded_at: now,
            })
            .await
            .map(|_| ())
            .map_err(|error| {
                let mapped = map_store_error(error);
                // Ledger write failure is itself an availability problem.
                if matches!(mapped, BootstrapError::Unavailable) {
                    BootstrapError::Unavailable
                } else {
                    BootstrapError::Failed
                }
            })
    }
}

fn map_store_error(error: IdentityStoreError) -> BootstrapError {
    match error {
        IdentityStoreError::PrincipalMismatch => BootstrapError::PrincipalMismatch,
        IdentityStoreError::Unavailable => BootstrapError::Unavailable,
        _ => BootstrapError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::testing::FakeIdentityStores;

    #[derive(Default)]
    struct RecordingBinding {
        calls: Mutex<Vec<(Uuid, Uuid, String)>>,
        fail_next: Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl BootstrapBindingPort for RecordingBinding {
        async fn bind_bootstrap_role(
            &self,
            tenant_id: Uuid,
            user_id: Uuid,
            idempotency_key: &str,
            _reason: &str,
            _now: DateTime<Utc>,
        ) -> Result<(), BootstrapBindingError> {
            let should_fail = self
                .fail_next
                .lock()
                .map(|mut flag| std::mem::replace(&mut *flag, false))
                .unwrap_or(false);
            if should_fail {
                return Err(BootstrapBindingError::Failed);
            }
            if let Ok(mut calls) = self.calls.lock() {
                calls.push((tenant_id, user_id, idempotency_key.to_string()));
            }
            Ok(())
        }
    }

    fn config() -> BootstrapAdministratorConfig {
        BootstrapAdministratorConfig {
            enabled: true,
            tenant_id: Uuid::now_v7(),
            issuer: "https://idp.example/realms/platform".to_string(),
            subject: "bootstrap-operator".to_string(),
            version: 1,
        }
    }

    struct Fixture {
        use_case: BootstrapAdministrator,
        stores: FakeIdentityStores,
        binding: Arc<RecordingBinding>,
    }

    fn fixture() -> Fixture {
        let stores = FakeIdentityStores::default();
        let binding = Arc::new(RecordingBinding::default());
        let use_case = BootstrapAdministrator::new(
            Arc::clone(&stores.resolve),
            Arc::clone(&stores.command),
            Arc::clone(&stores.query),
            Arc::clone(&stores.ledger),
            Arc::clone(&binding) as Arc<dyn BootstrapBindingPort>,
        );
        Fixture {
            use_case,
            stores,
            binding,
        }
    }

    #[tokio::test]
    async fn disabled_configuration_is_inert() {
        let fixture = fixture();
        let outcome = fixture
            .use_case
            .execute(&BootstrapAdministratorConfig {
                enabled: false,
                ..config()
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(outcome.is_none());
        assert!(fixture
            .binding
            .calls
            .lock()
            .is_ok_and(|calls| calls.is_empty()));
    }

    #[tokio::test]
    async fn rejects_malformed_configuration() {
        let fixture = fixture();
        for bad in [
            BootstrapAdministratorConfig {
                tenant_id: Uuid::nil(),
                ..config()
            },
            BootstrapAdministratorConfig {
                issuer: "http://insecure.example".to_string(),
                ..config()
            },
            BootstrapAdministratorConfig {
                subject: "  ".to_string(),
                ..config()
            },
            BootstrapAdministratorConfig {
                version: 0,
                ..config()
            },
        ] {
            assert!(matches!(
                fixture.use_case.execute(&bad).await,
                Err(BootstrapError::Config(_))
            ));
        }
    }

    #[tokio::test]
    async fn cold_start_provisions_member_and_binds_then_noops() {
        let fixture = fixture();
        let cfg = config();
        let outcome = fixture
            .use_case
            .execute(&cfg)
            .await
            .unwrap_or_else(|err| unreachable!("bootstrap failed: {err}"));
        assert_eq!(outcome, Some(BootstrapOutcome::Executed));

        // Exactly one binding call, and the user is an active member.
        let calls = fixture
            .binding
            .calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default();
        assert_eq!(calls.len(), 1);
        let user = fixture
            .stores
            .resolve
            .resolve_or_provision(ResolvePrincipalCommit {
                issuer: cfg.issuer.clone(),
                subject: cfg.subject.clone(),
                claimed_user_id: None,
                deterministic_user_id: Uuid::new_v5(
                    &EXTERNAL_IDENTITY_NAMESPACE,
                    format!("{}\u{1f}{}", cfg.issuer, cfg.subject).as_bytes(),
                ),
                audit: MutationContext {
                    actor_id: bootstrap_service_actor().to_string(),
                    actor_kind: MutationActorKind::User,
                    operation_id: Uuid::now_v7(),
                    trace_id: None,
                    reason: None,
                },
                now: Utc::now(),
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        let member = fixture
            .stores
            .query
            .get_tenant_user(cfg.tenant_id, user.user.user_id())
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(member.is_some(), "bootstrap user must be a tenant member");

        // Re-run with the same digest is a terminal no-op (revocation-proof:
        // it short-circuits before touching any port).
        let second = fixture
            .use_case
            .execute(&cfg)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(second, Some(BootstrapOutcome::NoOp));
        let calls = fixture
            .binding
            .calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default();
        assert_eq!(calls.len(), 1, "no-op must not re-bind the role");
    }

    #[tokio::test]
    async fn failure_is_ledgered_and_retryable_with_same_digest() {
        let fixture = fixture();
        let cfg = config();
        {
            let mut flag = fixture
                .binding
                .fail_next
                .lock()
                .unwrap_or_else(|_| unreachable!());
            *flag = true;
        }
        assert!(matches!(
            fixture.use_case.execute(&cfg).await,
            Err(BootstrapError::Failed)
        ));

        // The retry re-executes (Failed is not terminal) and succeeds.
        let outcome = fixture
            .use_case
            .execute(&cfg)
            .await
            .unwrap_or_else(|err| unreachable!("retry failed: {err}"));
        assert_eq!(outcome, Some(BootstrapOutcome::Executed));

        // The successful retry supersedes the durable failure row: the
        // ledger's latest entry for this digest is now Executed, so the
        // deployment stops repeating the failed-retry path every restart.
        let latest = fixture
            .stores
            .ledger
            .latest_for(cfg.tenant_id, &cfg.issuer, &cfg.subject)
            .await
            .unwrap_or_else(|err| unreachable!("ledger read failed: {err}"));
        assert_eq!(
            latest.map(|entry| entry.outcome),
            Some(BootstrapOutcome::Executed),
            "a successful retry must converge the failed ledger row"
        );

        // A third run with the unchanged digest is a terminal no-op: it
        // neither re-executes nor re-binds the role.
        let third = fixture
            .use_case
            .execute(&cfg)
            .await
            .unwrap_or_else(|err| unreachable!("no-op run failed: {err}"));
        assert_eq!(third, Some(BootstrapOutcome::NoOp));
        assert_eq!(
            fixture
                .binding
                .calls
                .lock()
                .map(|calls| calls.len())
                .unwrap_or_default(),
            1,
            "the converged no-op must not re-bind the role"
        );
    }

    #[tokio::test]
    async fn suspended_membership_is_reactivated_by_a_deliberate_bump() {
        let fixture = fixture();
        let cfg = config();
        fixture
            .use_case
            .execute(&cfg)
            .await
            .unwrap_or_else(|err| unreachable!("bootstrap failed: {err}"));

        // An operator suspends the bootstrap admin's membership.
        let user = fixture
            .stores
            .query
            .find_user_by_external_identity(&cfg.issuer, &cfg.subject)
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!("bootstrap user must exist"));
        let membership = fixture
            .stores
            .query
            .get_membership(cfg.tenant_id, user.user_id())
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!("membership must exist"));
        fixture
            .stores
            .command
            .change_membership_status(ChangeMembershipStatusCommit {
                tenant_id: cfg.tenant_id,
                user_id: user.user_id(),
                target_status: MembershipStatus::Suspended,
                expected_version: membership.version().value(),
                audit: MutationContext {
                    actor_id: Uuid::now_v7().to_string(),
                    actor_kind: MutationActorKind::User,
                    operation_id: Uuid::now_v7(),
                    trace_id: None,
                    reason: None,
                },
                idempotency_key: None,
                now: Utc::now(),
            })
            .await
            .unwrap_or_else(|_| unreachable!());

        // A deliberate configuration bump re-executes and restores an
        // *active* membership (the module contract), not merely an
        // existing row: the bump is the server-side recovery path for a
        // self-suspended sole bootstrap admin.
        let outcome = fixture
            .use_case
            .execute(&BootstrapAdministratorConfig {
                version: 2,
                ..cfg.clone()
            })
            .await
            .unwrap_or_else(|err| unreachable!("re-bootstrap failed: {err}"));
        assert_eq!(outcome, Some(BootstrapOutcome::Executed));
        let membership = fixture
            .stores
            .query
            .get_membership(cfg.tenant_id, user.user_id())
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!("membership must exist"));
        assert_eq!(membership.status(), MembershipStatus::Active);
    }

    #[tokio::test]
    async fn older_or_drifted_configuration_fails_closed() {
        let fixture = fixture();
        let cfg = config();
        fixture
            .use_case
            .execute(&cfg)
            .await
            .unwrap_or_else(|_| unreachable!());

        // Same identity tuple but a *lower* version: replaying older config
        // after a bump must fail closed instead of re-granting.
        fixture
            .use_case
            .execute(&BootstrapAdministratorConfig {
                version: 5,
                ..cfg.clone()
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        let stale = fixture.use_case.execute(&cfg).await;
        assert_eq!(stale, Err(BootstrapError::ConfigStale));
    }
}
