//! Real calls into the released `identity` contract, from outside the workspace.

use chrono::{TimeZone, Utc};
use identity::application::TenantAccessReason;
use identity::domain::{MembershipSource, PlatformUser, TenantMembership};
use identity::testing::FakeIdentityStores;
use uuid::Uuid;
use wave5a_consumer_identity::{check_tenant_access, tenant_access_checker};

fn tenant() -> Uuid {
    Uuid::from_u128(0x1001_0000_0000_0000_0000_0000_0000_0001)
}

fn user() -> Uuid {
    Uuid::from_u128(0x2002_0000_0000_0000_0000_0000_0000_0002)
}

fn fixture_time() -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000, 0)
        .single()
        .expect("fixed fixture timestamp")
}

#[tokio::test]
async fn tenant_access_fails_closed_and_then_grants_a_seeded_membership() {
    let stores = FakeIdentityStores::new();
    let checker = tenant_access_checker(&stores);

    let denied = check_tenant_access(&checker, tenant(), user())
        .await
        .expect("the module's in-memory query port is healthy");
    assert!(
        !denied.allowed,
        "an unknown (tenant, user) pair must not be granted access"
    );
    assert_eq!(denied.reason, TenantAccessReason::NoMembership);

    stores.seed_user(
        PlatformUser::create(user(), fixture_time()).expect("valid platform user fixture"),
    );
    stores.seed_membership(
        TenantMembership::join(
            Uuid::now_v7(),
            tenant(),
            user(),
            MembershipSource::Admin,
            fixture_time(),
        )
        .expect("valid membership fixture"),
    );

    let granted = check_tenant_access(&checker, tenant(), user())
        .await
        .expect("the module's in-memory query port is healthy");
    assert!(granted.allowed);
    assert_eq!(granted.reason, TenantAccessReason::Active);
}
