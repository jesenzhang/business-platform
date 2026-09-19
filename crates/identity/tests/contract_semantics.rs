//! Stage-1 acceptance semantics for the identity ports, executed against the
//! in-memory fakes. The same expectations bind the `PostgreSQL` and `SQLite`
//! adapters (WP-05 runs the shared adapter contract suite for these).

#![cfg(feature = "testing")]
#![allow(clippy::expect_used)]

use std::sync::Arc;

use identity::application::{
    ChangeTenantMembershipStatus, ChangeTenantMembershipStatusCommand, ChangeUserStatus,
    ChangeUserStatusCommand, ChangeUserStatusError, CreateMembershipTarget, CreateTenantMembership,
    CreateTenantMembershipCommand, CreateTenantMembershipError, IdentityQueryError,
    ListMemberships, ListTenantUsers, ResolveAuthenticatedUser, ResolveAuthenticatedUserCommand,
    ResolveCallerError, IDENTITY_MAX_PAGE_SIZE,
};
use identity::domain::{
    MembershipSource, MembershipStatus, PlatformUser, TenantMembership, UserLifecycleStatus,
};
use identity::testing::FakeIdentityStores;
use uuid::Uuid;

fn tenant_a() -> Uuid {
    Uuid::from_bytes([0x0a; 16])
}

fn tenant_b() -> Uuid {
    Uuid::from_bytes([0x0b; 16])
}

fn user_one() -> Uuid {
    Uuid::from_bytes([0x11; 16])
}

fn ts(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(seconds, 0).single().expect("valid ts")
}

fn seed_member(stores: &FakeIdentityStores, tenant: Uuid, user: Uuid, joined: i64) {
    stores.seed_user(PlatformUser::create(user, ts(joined)).expect("valid user"));
    stores.seed_membership(
        TenantMembership::join(
            Uuid::now_v7(),
            tenant,
            user,
            MembershipSource::Admin,
            ts(joined),
        )
        .expect("valid membership"),
    );
}

#[tokio::test]
async fn tenant_reads_are_invisible_across_tenant_boundaries() {
    let stores = FakeIdentityStores::default();
    seed_member(&stores, tenant_a(), user_one(), 100);

    // Direct reads.
    let other = stores
        .query
        .get_membership(tenant_b(), user_one())
        .await
        .expect("store ok");
    assert!(other.is_none(), "membership must not leak across tenants");
    let other_user = stores
        .query
        .get_tenant_user(tenant_b(), user_one())
        .await
        .expect("store ok");
    assert!(other_user.is_none());

    // Listings.
    let list = ListTenantUsers::new(Arc::clone(&stores.query));
    let page = list.execute(tenant_b(), 10, None).await.expect("query ok");
    assert!(page.items.is_empty());
    let memberships = ListMemberships::new(Arc::clone(&stores.query));
    let page = memberships
        .execute(tenant_b(), 10, None)
        .await
        .expect("query ok");
    assert!(page.items.is_empty());

    // Same calls in the owning tenant see the record.
    let page = list.execute(tenant_a(), 10, None).await.expect("query ok");
    assert_eq!(page.items.len(), 1);
}

#[tokio::test]
async fn store_unavailable_maps_to_retryable_unavailable() {
    let stores = FakeIdentityStores::default();
    stores.poison();
    let resolve = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));
    let error = resolve
        .execute(ResolveAuthenticatedUserCommand {
            issuer: "https://idp.example".to_string(),
            subject: "sub".to_string(),
            claimed_user_id: None,
        })
        .await
        .expect_err("poisoned store must fail");
    assert_eq!(error, ResolveCallerError::Unavailable);
    assert!(error.retryable());

    let list = ListTenantUsers::new(Arc::clone(&stores.query));
    assert_eq!(
        list.execute(tenant_a(), 10, None)
            .await
            .expect_err("poisoned store must fail"),
        IdentityQueryError::Unavailable
    );
}

#[tokio::test]
async fn page_size_cap_is_enforced() {
    let stores = FakeIdentityStores::default();
    let list = ListTenantUsers::new(Arc::clone(&stores.query));
    assert!(matches!(
        list.execute(tenant_a(), 0, None).await,
        Err(IdentityQueryError::Validation(_))
    ));
    assert!(matches!(
        list.execute(tenant_a(), IDENTITY_MAX_PAGE_SIZE + 1, None)
            .await,
        Err(IdentityQueryError::Validation(_))
    ));
}

#[tokio::test]
async fn find_user_by_external_identity_matches_exact_key_only() {
    let stores = FakeIdentityStores::default();
    let resolve = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));
    let resolved = resolve
        .execute(ResolveAuthenticatedUserCommand {
            issuer: "https://idp.example".to_string(),
            subject: "unique-subject".to_string(),
            claimed_user_id: None,
        })
        .await
        .expect("resolve ok");

    let found = stores
        .query
        .find_user_by_external_identity("https://idp.example", "unique-subject")
        .await
        .expect("query ok");
    assert_eq!(found.map(|u| u.user_id()), Some(resolved.user.user_id()));

    let wrong_issuer = stores
        .query
        .find_user_by_external_identity("https://evil.example", "unique-subject")
        .await
        .expect("query ok");
    assert!(wrong_issuer.is_none());
}

#[tokio::test]
async fn membership_by_subject_converges_with_later_login_resolution() {
    let stores = FakeIdentityStores::default();
    let create = CreateTenantMembership::new(Arc::clone(&stores.command));
    let issuer = "https://idp.example".to_string();
    let subject = "future-user".to_string();

    // Admin attaches a membership before the person ever logs in.
    let outcome = create
        .execute(CreateTenantMembershipCommand {
            tenant_id: tenant_a(),
            target: CreateMembershipTarget::ExternalSubject {
                issuer: issuer.clone(),
                subject: subject.clone(),
            },
            actor_user_id: user_one(),
            idempotency_key: Some("onboard-1".to_string()),
            reason: None,
            source: MembershipSource::Admin,
        })
        .await
        .expect("create ok");

    // The same person later logs in: resolution must converge on the same
    // platform user id, not fork a second identity.
    let resolve = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));
    let resolved = resolve
        .execute(ResolveAuthenticatedUserCommand {
            issuer,
            subject,
            claimed_user_id: None,
        })
        .await
        .expect("resolve ok");
    assert_eq!(resolved.user.user_id(), outcome.membership.user_id());
    assert!(!resolved.provisioned);
}

#[tokio::test]
async fn membership_create_rejects_control_characters_in_reason() {
    let stores = FakeIdentityStores::default();
    let create = CreateTenantMembership::new(Arc::clone(&stores.command));
    // Reason boundary validation must match the sibling status-change use
    // case: injection is rejected before anything reaches the store.
    let sneaky = CreateTenantMembershipCommand {
        tenant_id: tenant_a(),
        target: CreateMembershipTarget::UserId(Uuid::from_bytes([0x22; 16])),
        actor_user_id: user_one(),
        idempotency_key: Some("onboard-1".to_string()),
        reason: Some("evil\u{0}detail".to_string()),
        source: MembershipSource::Admin,
    };
    assert!(matches!(
        create.execute(sneaky).await,
        Err(CreateTenantMembershipError::Validation(_))
    ));
    assert!(stores.audit_records().is_empty());
}

#[tokio::test]
async fn adoption_links_an_existing_unlinked_user_by_trusted_claim() {
    let stores = FakeIdentityStores::default();
    seed_member(&stores, tenant_a(), user_one(), 100); // user without external link

    let resolve = ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve));
    let resolved = resolve
        .execute(ResolveAuthenticatedUserCommand {
            issuer: "https://first-party.example".to_string(),
            subject: "sub-1".to_string(),
            claimed_user_id: Some(user_one()),
        })
        .await
        .expect("adoption ok");
    assert_eq!(resolved.user.user_id(), user_one());
    assert!(!resolved.provisioned);
    assert!(resolved
        .external_identity
        .matches("https://first-party.example", "sub-1"));

    // The user now has a link; a different external key claiming the same
    // user must fail closed (one link per user in v1).
    let conflict = resolve
        .execute(ResolveAuthenticatedUserCommand {
            issuer: "https://other.example".to_string(),
            subject: "sub-2".to_string(),
            claimed_user_id: Some(user_one()),
        })
        .await;
    assert_eq!(conflict, Err(ResolveCallerError::PrincipalMismatch));
}

#[tokio::test]
async fn change_user_status_enforces_versions_replays_and_noops() {
    let stores = FakeIdentityStores::default();
    seed_member(&stores, tenant_a(), user_one(), 100);
    let change = ChangeUserStatus::new(Arc::clone(&stores.command));
    let actor = user_one();

    // Wrong expected version ⇒ conflict.
    let conflict = ChangeUserStatusCommand {
        user_id: user_one(),
        target_status: UserLifecycleStatus::Disabled,
        expected_version: 99,
        actor_user_id: actor,
        idempotency_key: None,
        reason: None,
    };
    assert_eq!(
        change.execute(conflict).await,
        Err(ChangeUserStatusError::VersionConflict)
    );

    // Disable with the right version, idempotent replay converges.
    let command = ChangeUserStatusCommand {
        user_id: user_one(),
        target_status: UserLifecycleStatus::Disabled,
        expected_version: 1,
        actor_user_id: actor,
        idempotency_key: Some("disable-1".to_string()),
        reason: Some("offboarded".to_string()),
    };
    let outcome = change.execute(command.clone()).await.expect("disable ok");
    assert_eq!(outcome.user.status(), UserLifecycleStatus::Disabled);
    assert!(!outcome.replayed);
    let replay = change.execute(command.clone()).await.expect("replay ok");
    assert!(replay.replayed);
    assert_eq!(replay.user.version(), outcome.user.version());

    // Already-disabled with the current version converges as a no-op.
    let noop = change
        .execute(ChangeUserStatusCommand {
            expected_version: 2,
            idempotency_key: None,
            ..command.clone()
        })
        .await
        .expect("no-op ok");
    assert!(noop.replayed);

    // Reason control-character injection is rejected at the boundary.
    let sneaky = ChangeUserStatusCommand {
        reason: Some("evil\u{0}detail".to_string()),
        idempotency_key: Some("disable-9".to_string()),
        ..command
    };
    assert!(matches!(
        change.execute(sneaky).await,
        Err(ChangeUserStatusError::Validation(_))
    ));

    // Every real mutation was audited.
    let audits = stores.audit_records();
    assert!(audits
        .iter()
        .any(|record| record.action == "identity.user.disabled"));
    assert_eq!(
        audits
            .iter()
            .filter(|record| record.action == "identity.user.disabled")
            .count(),
        1,
        "replays and no-ops must not duplicate audit records"
    );
}

#[tokio::test]
async fn suspend_then_next_access_denies_and_audits() {
    let stores = FakeIdentityStores::default();
    seed_member(&stores, tenant_a(), user_one(), 100);
    let actor = Uuid::from_bytes([0xaa; 16]);
    let change = ChangeTenantMembershipStatus::new(Arc::clone(&stores.command));
    let outcome = change
        .execute(ChangeTenantMembershipStatusCommand {
            tenant_id: tenant_a(),
            user_id: user_one(),
            target_status: MembershipStatus::Suspended,
            expected_version: 1,
            actor_user_id: actor,
            idempotency_key: Some("suspend-1".to_string()),
            reason: Some("incident response".to_string()),
        })
        .await
        .expect("suspend ok");
    assert!(!outcome.membership.is_active());
    assert!(stores
        .audit_records()
        .iter()
        .any(|record| record.action == "identity.membership.suspended"));
    let record = stores
        .query
        .get_tenant_user(tenant_a(), user_one())
        .await
        .expect("query ok")
        .expect("member exists");
    assert_eq!(record.membership.status(), MembershipStatus::Suspended);
}
