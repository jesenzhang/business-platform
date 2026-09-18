//! Dev-auth bootstrap composition regression tests (PLAN-0013 §7 mode 2).
//!
//! Pins that `run_dev_auth` runs against the fixed `urn:` dev issuer —
//! calling the production `validate()` (which requires `https`) crashed
//! every dev/demo startup — and that the mirror keeps the use case's
//! terminal no-op / `ConfigStale` / version-bump ledger semantics.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use uuid::Uuid;

use business_api::bootstrap::{dev_auth_bootstrap_config, BootstrapComposition};
use business_api::platform_authorization::IdentitySubjectStatusBridge;
use identity::application::{BootstrapError, TenantAccessChecker};
use identity::ports::BootstrapOutcome;
use identity::testing::FakeIdentityStores;
use policy::testing::FakePolicyPorts;

fn tenant_id() -> Uuid {
    Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00a1)
}

fn dev_user_id() -> Uuid {
    Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00b2)
}

fn composition(stores: &FakeIdentityStores, policy: &FakePolicyPorts) -> BootstrapComposition {
    let subject = Arc::new(IdentitySubjectStatusBridge::new(Arc::new(
        TenantAccessChecker::new(Arc::clone(&stores.query)),
    )));
    BootstrapComposition::new(
        Arc::clone(&stores.resolve),
        Arc::clone(&stores.command),
        Arc::clone(&stores.ledger),
        Arc::clone(&policy.query),
        Arc::clone(&policy.command),
        subject,
        Arc::clone(&policy.org),
    )
}

#[tokio::test]
async fn dev_auth_bootstrap_runs_with_the_locked_dev_issuer_and_converges() {
    let stores = FakeIdentityStores::new();
    let policy = FakePolicyPorts::new();
    policy.seed_catalog();
    let boot = composition(&stores, &policy);
    let config = dev_auth_bootstrap_config(tenant_id(), "dev-principal".to_string(), 1);

    let first = boot
        .run_dev_auth(&config, dev_user_id())
        .await
        .expect("dev-auth bootstrap must succeed with the fixed urn issuer");
    assert!(matches!(first, Some(BootstrapOutcome::Executed)));

    // Same digest: terminal no-op on every later startup.
    let again = boot
        .run_dev_auth(&config, dev_user_id())
        .await
        .expect("repeat startup must converge to a no-op");
    assert!(matches!(again, Some(BootstrapOutcome::NoOp)));

    // Same version, stored digest disagrees (e.g. a digest-rule upgrade or
    // a hand-edited ledger): fail closed, never re-execute.
    stores
        .ledger
        .record(&identity::ports::BootstrapLedgerEntry {
            tenant_id: tenant_id(),
            issuer: "urn:business-api:dev-auth".to_string(),
            subject: "tampered-principal".to_string(),
            role_stable_key: identity::application::BOOTSTRAP_ROLE_STABLE_KEY.to_string(),
            config_version: 1,
            config_digest: "0".repeat(64),
            outcome: BootstrapOutcome::Executed,
            recorded_at: chrono::Utc::now(),
        })
        .await
        .expect("ledger fixture");
    let tampered = dev_auth_bootstrap_config(tenant_id(), "tampered-principal".to_string(), 1);
    let error = boot
        .run_dev_auth(&tampered, dev_user_id())
        .await
        .expect_err("digest mismatch at the same version must fail closed");
    assert!(matches!(error, BootstrapError::ConfigStale));

    // Deliberate version bump re-executes (documented recovery knob).
    let bumped = dev_auth_bootstrap_config(tenant_id(), "dev-principal".to_string(), 2);
    let reexecuted = boot
        .run_dev_auth(&bumped, dev_user_id())
        .await
        .expect("a version bump is the documented recovery knob");
    assert!(matches!(reexecuted, Some(BootstrapOutcome::Executed)));

    // Same subject, different trusted user id: the external link already
    // belongs to another user, so the resolve port rejects the claim and
    // bootstrap fails closed instead of binding a second identity.
    let other_user = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00c3);
    let mismatch = dev_auth_bootstrap_config(tenant_id(), "dev-principal".to_string(), 3);
    let error = boot
        .run_dev_auth(&mismatch, other_user)
        .await
        .expect_err("a claimed id conflicting with the external link must fail closed");
    assert!(matches!(error, BootstrapError::PrincipalMismatch));
}

#[tokio::test]
async fn dev_auth_bootstrap_validation_is_dev_scoped() {
    let stores = FakeIdentityStores::new();
    let policy = FakePolicyPorts::new();
    policy.seed_catalog();
    let boot = composition(&stores, &policy);

    // Disabled config is inert.
    let mut disabled = dev_auth_bootstrap_config(tenant_id(), "dev-principal".to_string(), 1);
    disabled.enabled = false;
    assert!(matches!(
        boot.run_dev_auth(&disabled, dev_user_id()).await,
        Ok(None)
    ));

    // An https issuer is production vocabulary; the dev runner only
    // accepts the fixed constant (the dev resolver derives the same one).
    let mut wrong_issuer = dev_auth_bootstrap_config(tenant_id(), "dev-principal".to_string(), 1);
    wrong_issuer.issuer = "https://identity.invalid".to_string();
    assert!(matches!(
        boot.run_dev_auth(&wrong_issuer, dev_user_id()).await,
        Err(BootstrapError::Config(_))
    ));

    // Nil user id is rejected before any port is touched.
    assert!(matches!(
        boot.run_dev_auth(
            &dev_auth_bootstrap_config(tenant_id(), "dev-principal".to_string(), 1),
            Uuid::nil(),
        )
        .await,
        Err(BootstrapError::Config(_))
    ));
}
