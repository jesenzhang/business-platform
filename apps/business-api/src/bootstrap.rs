//! Startup bootstrap composition (PLAN-0013 §7).
//!
//! Two server-controlled modes, never a request surface:
//! 1. production `[auth.bootstrap]` → the Identity `BootstrapAdministrator`
//!    use case (provision → membership → bind `system.bootstrap-admin`
//!    through the Policy binding port → durable ledger row);
//! 2. dev-auth (dev/test only, already production-forbidden) → the same
//!    ledger, audit, ports, and version/digest no-op semantics for the
//!    server-configured dev principal.
//!
//! Mode 2 cannot call `BootstrapAdministrator::execute` verbatim: that
//! use case validates its configuration and requires an `https` issuer,
//! while the locked dev-auth resolver issuer is the fixed
//! `urn:business-api:dev-auth` constant (preflight §5). The runner below
//! therefore mirrors the use case step-for-step against the same ports,
//! reusing the use case's own `config_digest()` semantics, ledger, and
//! audit vocabulary; the dev principal is provisioned with its trusted
//! server-config `user_id` as the claimed id, which is exactly what the
//! request-path resolver passes for dev-auth requests, so bootstrap and
//! resolution converge on one platform user. HTTP never triggers
//! bootstrap.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use identity::application::{
    BootstrapAdministrator, BootstrapAdministratorConfig, BootstrapBindingError,
    BootstrapBindingPort, BootstrapError, BOOTSTRAP_ROLE_STABLE_KEY,
};
use identity::ports::{
    BootstrapLedgerEntry, BootstrapLedgerPort, BootstrapOutcome, CreateMembershipCommit,
    IdentityCommandPort, IdentityResolvePort, IdentityStoreError, MembershipTarget,
    MutationActorKind as IdentityMutationActorKind, MutationContext as IdentityMutationContext,
    ResolvePrincipalCommit,
};
use policy::{
    application::{BindRole, BindRoleCommand, PolicyApplicationError},
    domain::ResourceScope,
    ports::{OrganizationScopePort, PolicyCommandPort, PolicyQueryPort, SubjectStatusPort},
};

use crate::metrics::{record_bootstrap_outcome, BootstrapMetricOutcome};
use crate::platform_authorization::DEV_AUTH_ISSUER;

/// Canonical `UUIDv5` derivation of the migration-seeded system role ids.
///
/// The id for `stable_key` is `UUIDv5(NAMESPACE_URL,
/// "policy-system-role:<stable_key>")` — the exact derivation of
/// `identity_authorization_contracts::system_role_id`, pinned by the
/// PostgreSQL/SQLite adapter contract suites (any drift fails the
/// contract tests). This copy exists because `business-api` must not
/// depend on the test-only contract suite in its runtime graph; the
/// contract suite is the enforcement point that keeps the two derivations
/// equal.
#[must_use]
pub fn system_role_id(stable_key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("policy-system-role:{stable_key}").as_bytes(),
    )
}

/// Canonical audit actor for the bootstrap service, pinned by Identity to
/// `identity::application::bootstrap_service_actor()`
/// (`UUIDv5(EXTERNAL_IDENTITY_NAMESPACE, b"bootstrap-service")`). The
/// Identity stores parse `MutationContext.actor_id` as a UUID and fail
/// closed on non-UUID actors; the contract suite pins this behavior.
///
/// `BindRole`'s self-escalation guard is a human-actor rule (an actor may
/// not newly grant IAM-management authority to itself). The bootstrap
/// service is the server-side root of trust at cold start, not a user, so
/// it binds under this stable synthetic actor id; audit shows one
/// constant actor for every bootstrap binding.
#[must_use]
pub fn bootstrap_actor_id() -> Uuid {
    identity::application::bootstrap_service_actor()
}

/// `identity::BootstrapBindingPort` implemented over the Policy `BindRole`
/// use case (Identity never writes `role_bindings` itself).
pub struct PolicyBootstrapBinding {
    bind_role: Arc<BindRole>,
    role_id: Uuid,
}

impl PolicyBootstrapBinding {
    #[must_use]
    pub fn new(bind_role: Arc<BindRole>) -> Self {
        Self {
            bind_role,
            role_id: system_role_id(BOOTSTRAP_ROLE_STABLE_KEY),
        }
    }
}

#[async_trait::async_trait]
impl BootstrapBindingPort for PolicyBootstrapBinding {
    async fn bind_bootstrap_role(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        idempotency_key: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<(), BootstrapBindingError> {
        // Full `BindRole` validation still runs (target membership, role
        // visibility + active status, scope validity, store caps, audit).
        self.bind_role
            .execute(BindRoleCommand {
                tenant_id,
                binding_id: None,
                user_id,
                role_id: self.role_id,
                scope: ResourceScope::Tenant,
                effective_at: now,
                expires_at: None,
                actor_user_id: bootstrap_actor_id(),
                idempotency_key: Some(idempotency_key.to_owned()),
                reason: Some(reason.to_owned()),
            })
            .await
            .map(|_| ())
            .map_err(|error| match error {
                PolicyApplicationError::Unavailable => BootstrapBindingError::Unavailable,
                other => {
                    tracing::error!(%other, "bootstrap role binding failed");
                    BootstrapBindingError::Failed
                }
            })
    }
}

/// Everything the bootstrap composition needs. Both backends construct
/// this identically; only the port implementations differ.
pub struct BootstrapComposition {
    resolve: Arc<dyn IdentityResolvePort>,
    command: Arc<dyn IdentityCommandPort>,
    ledger: Arc<dyn BootstrapLedgerPort>,
    binding: Arc<dyn BootstrapBindingPort>,
}

impl BootstrapComposition {
    #[must_use]
    pub fn new(
        resolve: Arc<dyn IdentityResolvePort>,
        command: Arc<dyn IdentityCommandPort>,
        ledger: Arc<dyn BootstrapLedgerPort>,
        query: Arc<dyn PolicyQueryPort>,
        policy_command: Arc<dyn PolicyCommandPort>,
        subject: Arc<dyn SubjectStatusPort>,
        org: Arc<dyn OrganizationScopePort>,
    ) -> Self {
        let bind_role = Arc::new(BindRole::new(policy_command, query, subject, org));
        Self {
            resolve,
            command,
            ledger,
            binding: Arc::new(PolicyBootstrapBinding::new(bind_role)),
        }
    }

    /// Production mode: run the Identity `BootstrapAdministrator` use case
    /// verbatim. Returns the recorded outcome (or `None` when disabled).
    pub async fn run_production(
        &self,
        config: &BootstrapAdministratorConfig,
    ) -> Result<Option<BootstrapOutcome>, BootstrapError> {
        let use_case = BootstrapAdministrator::new(
            Arc::clone(&self.resolve),
            Arc::clone(&self.command),
            Arc::clone(&self.ledger),
            Arc::clone(&self.binding),
        );
        let outcome = use_case.execute(config).await;
        match &outcome {
            Ok(Some(BootstrapOutcome::Executed)) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::Executed);
            }
            Ok(Some(BootstrapOutcome::NoOp)) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::NoOp);
            }
            Ok(None | Some(BootstrapOutcome::Failed)) => {}
            Err(BootstrapError::ConfigStale) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::ConfigStale);
            }
            Err(BootstrapError::PrincipalMismatch) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::PrincipalMismatch);
            }
            Err(BootstrapError::Unavailable) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::Unavailable);
            }
            Err(_) => record_bootstrap_outcome(BootstrapMetricOutcome::Failed),
        }
        outcome
    }

    /// Dev-auth mode (preflight §7.2): bind the server-configured dev
    /// principal with `system.bootstrap-admin` using the same ledger,
    /// audit, ports, and version/digest no-op semantics as the production
    /// use case. Mirrors `BootstrapAdministrator::execute` step-for-step;
    /// see the module docs for why the use case entry point itself cannot
    /// be reused verbatim here.
    pub async fn run_dev_auth(
        &self,
        config: &BootstrapAdministratorConfig,
        dev_user_id: Uuid,
    ) -> Result<Option<BootstrapOutcome>, BootstrapError> {
        let outcome = self.run_dev_steps(config, dev_user_id).await;
        // Metric parity with `run_production`: every outcome class —
        // including early Config/ConfigStale exits — is recorded exactly
        // once here, never inline in the step runner.
        match &outcome {
            Ok(Some(BootstrapOutcome::Executed)) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::Executed);
            }
            Ok(Some(BootstrapOutcome::NoOp)) => {
                record_bootstrap_outcome(BootstrapMetricOutcome::NoOp);
            }
            Ok(None | Some(BootstrapOutcome::Failed)) => {}
            Err(error) => record_bootstrap_outcome(outcome_metric_for_error(error)),
        }
        outcome
    }

    async fn run_dev_steps(
        &self,
        config: &BootstrapAdministratorConfig,
        dev_user_id: Uuid,
    ) -> Result<Option<BootstrapOutcome>, BootstrapError> {
        if !config.enabled {
            return Ok(None);
        }
        // Dev-mode validation mirrors `BootstrapAdministratorConfig::validate`
        // except for the https-issuer rule: that guard protects operator
        // configuration of the production issuer, while the dev issuer is
        // the fixed compile-time `urn:business-api:dev-auth` constant
        // (preflight §5) that the dev resolver also derives. Calling the
        // shared `validate()` here would reject the locked constant and
        // crash every dev/demo startup.
        let issuer_trimmed = config.issuer.trim();
        if issuer_trimmed != DEV_AUTH_ISSUER
            || issuer_trimmed.len() > identity::domain::MAX_EXTERNAL_ISSUER_LEN
        {
            return Err(BootstrapError::Config(
                "dev-auth bootstrap issuer must be the fixed dev issuer constant".to_string(),
            ));
        }
        if config.tenant_id.is_nil() {
            return Err(BootstrapError::Config(
                "dev-auth tenant_id must not be nil".to_string(),
            ));
        }
        let subject_trimmed = config.subject.trim();
        if subject_trimmed.is_empty()
            || subject_trimmed.len() > identity::domain::MAX_EXTERNAL_SUBJECT_LEN
            || subject_trimmed.contains('\0')
        {
            return Err(BootstrapError::Config(
                "dev-auth subject must be non-empty".to_string(),
            ));
        }
        if config.version < 1 {
            return Err(BootstrapError::Config(
                "dev-auth version must be >= 1".to_string(),
            ));
        }
        if dev_user_id.is_nil() {
            return Err(BootstrapError::Config(
                "dev-auth user id must not be nil".to_string(),
            ));
        }
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
                                return Err(BootstrapError::ConfigStale);
                            }
                            return Ok(Some(BootstrapOutcome::NoOp));
                        }
                        std::cmp::Ordering::Greater => {
                            return Err(BootstrapError::ConfigStale);
                        }
                        std::cmp::Ordering::Less => {}
                    }
                }
                BootstrapOutcome::Failed => {}
            }
        }

        let now = Utc::now();
        let audit = IdentityMutationContext {
            actor_id: bootstrap_actor_id().to_string(),
            actor_kind: IdentityMutationActorKind::Bootstrap,
            operation_id: Uuid::now_v7(),
            trace_id: None,
            reason: Some("dev-auth bootstrap administrator".to_string()),
        };

        let result = self
            .execute_dev_steps(config, dev_user_id, &issuer, &subject, &digest, &audit, now)
            .await;
        let outcome = match result {
            Ok(()) => BootstrapOutcome::Executed,
            Err(error) => {
                // Best-effort durable failure record, same as production.
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
    async fn execute_dev_steps(
        &self,
        config: &BootstrapAdministratorConfig,
        dev_user_id: Uuid,
        issuer: &str,
        subject: &str,
        digest: &str,
        audit: &IdentityMutationContext,
        now: DateTime<Utc>,
    ) -> Result<(), BootstrapError> {
        // The trusted server-config id is supplied as the claimed id (the
        // deterministic fallback is never consulted for a claimed id), so
        // the request-path resolver — which claims the same trusted id —
        // and this bootstrap always address the same platform user.
        let resolved = self
            .resolve
            .resolve_or_provision(ResolvePrincipalCommit {
                issuer: issuer.to_owned(),
                subject: subject.to_owned(),
                claimed_user_id: Some(dev_user_id),
                deterministic_user_id: dev_user_id,
                audit: audit.clone(),
                now,
            })
            .await
            .map_err(map_store_error)?;
        let user_id = resolved.user.user_id();
        if user_id != dev_user_id {
            // Defensive: the resolve port must honor the trusted claim.
            return Err(BootstrapError::PrincipalMismatch);
        }

        match self
            .command
            .create_membership(CreateMembershipCommit {
                tenant_id: config.tenant_id,
                target: MembershipTarget::UserId(user_id),
                source: identity::domain::MembershipSource::Bootstrap,
                audit: audit.clone(),
                idempotency_key: Some(format!("bootstrap-membership|{digest}")),
                now,
            })
            .await
        {
            Ok(_) | Err(IdentityStoreError::AlreadyExists) => {}
            Err(error) => return Err(map_store_error(error)),
        }

        self.binding
            .bind_bootstrap_role(
                config.tenant_id,
                user_id,
                &format!("bootstrap-bind|{digest}"),
                "dev-auth bootstrap administrator",
                now,
            )
            .await
            .map_err(|error| match error {
                BootstrapBindingError::Unavailable => BootstrapError::Unavailable,
                BootstrapBindingError::Failed => BootstrapError::Failed,
            })
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
                issuer: issuer.to_owned(),
                subject: subject.to_owned(),
                role_stable_key: BOOTSTRAP_ROLE_STABLE_KEY.to_owned(),
                config_version: config.version,
                config_digest: digest.to_owned(),
                outcome,
                recorded_at: now,
            })
            .await
            .map_err(|error| match error {
                // Mirror the production use case: non-`Unavailable` ledger
                // write failures surface as the generic `Failed` class.
                IdentityStoreError::Unavailable => BootstrapError::Unavailable,
                _ => BootstrapError::Failed,
            })?;
        Ok(())
    }
}

fn outcome_metric_for_error(error: &BootstrapError) -> BootstrapMetricOutcome {
    match error {
        BootstrapError::ConfigStale => BootstrapMetricOutcome::ConfigStale,
        BootstrapError::PrincipalMismatch => BootstrapMetricOutcome::PrincipalMismatch,
        BootstrapError::Unavailable => BootstrapMetricOutcome::Unavailable,
        _ => BootstrapMetricOutcome::Failed,
    }
}

fn map_store_error(error: IdentityStoreError) -> BootstrapError {
    match error {
        IdentityStoreError::PrincipalMismatch => BootstrapError::PrincipalMismatch,
        IdentityStoreError::Unavailable => BootstrapError::Unavailable,
        _ => BootstrapError::Failed,
    }
}

/// Assemble the dev-auth bootstrap configuration (mode 2). `enabled` is
/// always true: the composition root only calls this for a validated
/// dev-auth deployment (dev-auth itself is rejected by production config
/// validation).
#[must_use]
pub fn dev_auth_bootstrap_config(
    tenant_id: Uuid,
    subject: String,
    version: i64,
) -> BootstrapAdministratorConfig {
    BootstrapAdministratorConfig {
        enabled: true,
        tenant_id,
        issuer: DEV_AUTH_ISSUER.to_owned(),
        subject,
        version: version.max(1),
    }
}
