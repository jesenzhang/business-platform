//! Real calls into the released `policy` contract, from outside the workspace.

use policy::application::AuthorizationContext;
use policy::testing::FakePolicyPorts;
use uuid::Uuid;
use wave5a_consumer_policy::{authorize, check};

fn tenant() -> Uuid {
    Uuid::from_u128(0x1001_0000_0000_0000_0000_0000_0000_0001)
}

fn user() -> Uuid {
    Uuid::from_u128(0x2002_0000_0000_0000_0000_0000_0000_0002)
}

#[tokio::test]
async fn an_unbound_permission_is_denied_by_the_released_contract() {
    let ports = FakePolicyPorts::new();
    let authorize = authorize(&ports);
    let context = AuthorizationContext::new(user(), tenant());

    let decision = check(&authorize, &context, "contract.read")
        .await
        .expect("the module's in-memory ports are healthy");
    assert!(
        !decision.allowed,
        "the released contract must deny a permission nothing binds"
    );
    assert!(
        decision.matched_binding.is_none(),
        "a denial must not report a matching binding"
    );
    assert!(
        decision.policy_reference.starts_with("policy:v1:"),
        "the decision must carry a stable policy reference for audit: {:?}",
        decision.policy_reference
    );
}

#[tokio::test]
async fn a_permission_catalog_alone_does_not_grant_anything() {
    let ports = FakePolicyPorts::new();
    ports.seed_catalog();
    let authorize = authorize(&ports);
    let context = AuthorizationContext::new(user(), tenant());

    let decision = check(&authorize, &context, "contract.read")
        .await
        .expect("the module's in-memory ports are healthy");
    assert!(
        !decision.allowed,
        "defining a permission must not grant it to a subject with no binding"
    );
}
