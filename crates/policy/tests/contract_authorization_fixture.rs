//! PLAN-0013 WP-11 — the next vertical slice's authorization contract,
//! expressed as a fixture against the Policy platform **without** any
//! Contract domain code.
//!
//! It proves the shape a future Contract (or any business module) slice
//! consumes today: reserved catalog keys + `RoleBindings` + `ResourceScope`
//! decide access; business handlers pass a `ResourceTarget` and never
//! parse roles. Requirements pinned here (plan §15):
//!
//! - a `contract.read` `RoleBinding` authorizes contract resources only
//!   within its tenant and scope;
//! - a cross-tenant binding fixture fails closed (plan §15 rule 5);
//! - reserved keys are bindable through `RoleBindings` but unreachable via
//!   the governance compat claim bridge;
//! - revocation takes effect on the very next decision.

#![cfg(feature = "testing")]

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use policy::application::{
    AuthorizationContext, Authorize, BindRole, BindRoleCommand, ManagementCompatGrant,
    ResourceTarget, RevokeRoleBinding, RevokeRoleBindingCommand,
};
use policy::domain::{DecisionReason, ResourceScope, RoleDefinition};
use policy::ports::SubjectStatus;
use policy::testing::FakePolicyPorts;

const CONTRACT_READ: &str = "contract.read";

fn ts(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0)
        .single()
        .unwrap_or_else(|| unreachable!())
}

fn id(byte: u8) -> Uuid {
    Uuid::from_bytes([byte; 16])
}

fn tenant_a() -> Uuid {
    id(1)
}

fn tenant_b() -> Uuid {
    id(2)
}

fn viewer_user() -> Uuid {
    id(3)
}

fn actor_admin() -> Uuid {
    id(4)
}

fn contract_role() -> Uuid {
    id(5)
}

fn contract_one() -> Uuid {
    id(10)
}

fn contract_two() -> Uuid {
    id(11)
}

fn setup() -> (FakePolicyPorts, AuthorizationContext) {
    let ports = FakePolicyPorts::new();
    ports.set_subject(tenant_a(), viewer_user(), SubjectStatus::Active);
    ports.set_subject(tenant_a(), actor_admin(), SubjectStatus::Active);
    ports.set_subject(tenant_b(), viewer_user(), SubjectStatus::Active);
    let ctx = AuthorizationContext::new(viewer_user(), tenant_a());
    (ports, ctx)
}

fn contract_viewer_role(ports: &FakePolicyPorts) {
    seed_viewer_role(ports, contract_role(), tenant_a());
}

fn seed_viewer_role(ports: &FakePolicyPorts, role_id: Uuid, tenant: Uuid) {
    let role = RoleDefinition::create_tenant_role(
        role_id,
        tenant,
        "contract.viewer",
        "Contract Viewer",
        ts(500),
    )
    .unwrap_or_else(|_| unreachable!());
    ports.seed_role(role, &[CONTRACT_READ]);
}

async fn bind(
    ports: &FakePolicyPorts,
    binding_id: Uuid,
    tenant: Uuid,
    user: Uuid,
    scope: ResourceScope,
) -> policy::domain::RoleBinding {
    bind_role(ports, binding_id, tenant, user, contract_role(), scope).await
}

async fn bind_role(
    ports: &FakePolicyPorts,
    binding_id: Uuid,
    tenant: Uuid,
    user: Uuid,
    role_id: Uuid,
    scope: ResourceScope,
) -> policy::domain::RoleBinding {
    let binder = BindRole::new(
        Arc::clone(&ports.command),
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    );
    binder
        .execute(BindRoleCommand {
            tenant_id: tenant,
            binding_id: Some(binding_id),
            user_id: user,
            role_id,
            scope,
            effective_at: ts(1000),
            expires_at: None,
            actor_user_id: actor_admin(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .unwrap_or_else(|_| unreachable!())
        .binding
}

fn authorize(ports: &FakePolicyPorts) -> Authorize {
    Authorize::new(
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    )
}

fn contract_target(resource_id: Uuid) -> ResourceTarget {
    ResourceTarget {
        kind: Some("contract".to_string()),
        resource_id: Some(resource_id),
        org_unit_id: None,
    }
}

#[tokio::test]
async fn resource_type_binding_authorizes_contract_reads_in_tenant() {
    let (ports, ctx) = setup();
    contract_viewer_role(&ports);
    bind(
        &ports,
        id(20),
        tenant_a(),
        viewer_user(),
        ResourceScope::resource_type("contract").unwrap_or_else(|_| unreachable!()),
    )
    .await;

    let engine = authorize(&ports);
    for contract in [contract_one(), contract_two()] {
        let decision = engine
            .check(&ctx, CONTRACT_READ, Some(&contract_target(contract)))
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(
            decision.allowed && decision.reason == DecisionReason::AllowRoleBinding,
            "any contract in the tenant must be readable through the type scope"
        );
    }

    // Wrong resource kind is a different resource: scope mismatch denies.
    let wrong_kind = ResourceTarget {
        kind: Some("invoice".to_string()),
        resource_id: Some(contract_one()),
        org_unit_id: None,
    };
    let decision = engine
        .check(&ctx, CONTRACT_READ, Some(&wrong_kind))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !decision.allowed && decision.reason == DecisionReason::DenyScopeMismatch,
        "a contract grant must not read other resource kinds"
    );
}

#[tokio::test]
async fn resource_scoped_binding_authorizes_only_the_exact_contract() {
    let (ports, ctx) = setup();
    contract_viewer_role(&ports);
    bind(
        &ports,
        id(21),
        tenant_a(),
        viewer_user(),
        ResourceScope::resource("contract", contract_one()).unwrap_or_else(|_| unreachable!()),
    )
    .await;

    let engine = authorize(&ports);
    let allowed = engine
        .check(&ctx, CONTRACT_READ, Some(&contract_target(contract_one())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(allowed.allowed);

    let other = engine
        .check(&ctx, CONTRACT_READ, Some(&contract_target(contract_two())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !other.allowed && other.reason == DecisionReason::DenyScopeMismatch,
        "a per-contract grant must not leak to sibling contracts"
    );
}

#[tokio::test]
async fn cross_tenant_binding_fixture_fails_closed() {
    // Plan §15 rule 5: a binding created in tenant B must never authorize
    // a tenant-A decision for the same user.
    let (ports, _ctx_a) = setup();
    // Tenant B owns its own viewer role: a tenant-A role is not even
    // bindable there. The fixture binds in B, then decides in A.
    seed_viewer_role(&ports, id(30), tenant_b());
    bind_role(
        &ports,
        id(22),
        tenant_b(),
        viewer_user(),
        id(30),
        ResourceScope::resource_type("contract").unwrap_or_else(|_| unreachable!()),
    )
    .await;

    let engine = authorize(&ports);

    // Tenant A (the original ctx): no tenant-A binding exists.
    let in_a = AuthorizationContext::new(viewer_user(), tenant_a());
    let decision = engine
        .check(&in_a, CONTRACT_READ, Some(&contract_target(contract_one())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !decision.allowed && decision.reason == DecisionReason::DenyNoBinding,
        "a tenant-B grant must not authorize in tenant A"
    );

    // Tenant B: allowed — the grant works exactly where it belongs.
    let in_b = AuthorizationContext::new(viewer_user(), tenant_b());
    let decision = engine
        .check(&in_b, CONTRACT_READ, Some(&contract_target(contract_one())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(decision.allowed);
}

#[tokio::test]
async fn reserved_contract_key_is_unreachable_through_compat_claims() {
    // The compat bridge only speaks the seven governance keys; reserved
    // business keys (contract.*) must deny even when a token claim
    // injects them — RoleBindings are their only authority path.
    let (ports, _ctx) = setup();
    contract_viewer_role(&ports);
    let grant = ManagementCompatGrant::parse(CONTRACT_READ);
    assert!(
        grant.is_none(),
        "reserved keys must not parse as compat grants"
    );

    // Every compat grant a governance token could ever claim — the
    // reserved business keys must remain unreachable even with the full
    // bridge set attached.
    let full_bridge = std::collections::BTreeSet::from([
        ManagementCompatGrant::AuditRead,
        ManagementCompatGrant::IntegrityRead,
        ManagementCompatGrant::IntegrityScan,
        ManagementCompatGrant::RepairDryRun,
        ManagementCompatGrant::RepairExecute,
        ManagementCompatGrant::RepairApprove,
        ManagementCompatGrant::RepairCancel,
    ]);
    let ctx = AuthorizationContext::new(viewer_user(), tenant_a()).with_compat_grants(full_bridge);
    let decision = authorize(&ports)
        .check(&ctx, CONTRACT_READ, Some(&contract_target(contract_one())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !decision.allowed,
        "full compat grants must never reach reserved keys"
    );
}

#[tokio::test]
async fn revocation_stops_authorization_on_the_next_decision() {
    let (ports, ctx) = setup();
    contract_viewer_role(&ports);
    let binding = bind(
        &ports,
        id(23),
        tenant_a(),
        viewer_user(),
        ResourceScope::resource_type("contract").unwrap_or_else(|_| unreachable!()),
    )
    .await;

    let engine = authorize(&ports);
    let before = engine
        .check(&ctx, CONTRACT_READ, Some(&contract_target(contract_one())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(before.allowed);

    let revoker = RevokeRoleBinding::new(Arc::clone(&ports.command));
    revoker
        .execute(RevokeRoleBindingCommand {
            tenant_id: tenant_a(),
            binding_id: id(23),
            expected_version: binding.version().value(),
            actor_user_id: actor_admin(),
            idempotency_key: None,
            reason: Some("fixture: revocation must take effect instantly".to_string()),
        })
        .await
        .unwrap_or_else(|_| unreachable!());

    let after = engine
        .check(&ctx, CONTRACT_READ, Some(&contract_target(contract_one())))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !after.allowed && after.reason == DecisionReason::DenyBindingRevoked,
        "revoked bindings deny on the very next decision"
    );
}
