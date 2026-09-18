//! The locked PLAN-0013 decision matrix, exercised against the in-memory
//! fakes (and later, via the shared contract suite, against the real
//! PostgreSQL/SQLite adapters). Default DENY everywhere; every row asserts
//! the exact `DecisionReason`, not just allow/deny.

#![cfg(feature = "testing")]

use std::sync::Arc;

use chrono::{DateTime, Days, TimeZone, Utc};
use uuid::Uuid;

use policy::application::{
    AuthorizationContext, Authorize, BindRole, BindRoleCommand, ExplainDecision,
    ManagementCompatGrant, ResourceTarget,
};
use policy::domain::{DecisionReason, ResourceScope, RoleBinding, RoleDefinition, ValidityWindow};
use policy::ports::SubjectStatus;
use policy::testing::FakePolicyPorts;

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

fn subject_user() -> Uuid {
    id(3)
}

fn actor_admin() -> Uuid {
    id(4)
}

fn role_id() -> Uuid {
    id(5)
}

fn foreign_role_id() -> Uuid {
    id(6)
}

fn unit_a() -> Uuid {
    id(7)
}

fn unit_b() -> Uuid {
    id(8)
}

/// Seeded fakes with both principals Active and both tenants unit-aware.
fn setup() -> (FakePolicyPorts, AuthorizationContext) {
    let ports = FakePolicyPorts::new();
    ports.set_subject(tenant_a(), subject_user(), SubjectStatus::Active);
    ports.set_subject(tenant_a(), actor_admin(), SubjectStatus::Active);
    ports.set_unit(tenant_a(), unit_a(), None, true);
    ports.set_unit(tenant_a(), unit_b(), Some(unit_a()), true);
    let ctx = AuthorizationContext::new(subject_user(), tenant_a());
    (ports, ctx)
}

fn tenant_role(ports: &FakePolicyPorts, key: &str, permission_keys: &[&str]) {
    let role = RoleDefinition::create_tenant_role(role_id(), tenant_a(), key, "Test Role", ts(500))
        .unwrap_or_else(|_| unreachable!());
    ports.seed_role(role, permission_keys);
}

async fn bind_tenant(ports: &FakePolicyPorts, scope: ResourceScope) {
    let binder = BindRole::new(
        Arc::clone(&ports.command),
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    );
    binder
        .execute(BindRoleCommand {
            tenant_id: tenant_a(),
            binding_id: None,
            user_id: subject_user(),
            role_id: role_id(),
            scope,
            effective_at: ts(1000),
            expires_at: None,
            actor_user_id: actor_admin(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
}

fn authorize(ports: &FakePolicyPorts) -> Authorize {
    Authorize::new(
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    )
}

#[tokio::test]
async fn default_deny_without_any_binding() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    let decision = authorize(&ports)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, DecisionReason::DenyNoBinding);
    assert_eq!(decision.matched_binding, None);
    assert!(decision.policy_reference.starts_with("policy:v1:"));
}

#[tokio::test]
async fn unknown_and_malformed_permissions_deny() {
    let (ports, ctx) = setup();
    let engine = authorize(&ports);
    for raw in [
        "nonexistent.key",
        "Bad.Key",
        "a.b.c.d",
        "document.read; drop",
        "",
    ] {
        let decision = engine
            .check(&ctx, raw, None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(
            !decision.allowed && decision.reason == DecisionReason::DenyUnknownPermission,
            "{raw:?} must deny as unknown"
        );
    }
}

#[tokio::test]
async fn tenant_role_binding_grants_within_tenant() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    let decision = authorize(&ports)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(decision.allowed);
    assert_eq!(decision.reason, DecisionReason::AllowRoleBinding);
    assert_eq!(
        decision.matched_permission.as_deref(),
        Some("document.read")
    );
    assert!(decision.matched_binding.is_some());
    let binding_id = decision.matched_binding.unwrap_or_else(|| unreachable!());
    assert_eq!(
        decision.policy_reference,
        format!("policy:v1:binding/{binding_id}")
    );
}

#[tokio::test]
async fn subject_status_overrides_every_grant_path() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    let compat = ctx
        .clone()
        .with_compat_grants([ManagementCompatGrant::AuditRead]);
    let cases = [
        (SubjectStatus::MissingUser, DecisionReason::DenyNoUser),
        (
            SubjectStatus::UserDisabled,
            DecisionReason::DenyUserDisabled,
        ),
        (
            SubjectStatus::NoMembership,
            DecisionReason::DenyNoMembership,
        ),
        (
            SubjectStatus::MembershipSuspended,
            DecisionReason::DenyMembershipSuspended,
        ),
    ];
    for (status, reason) in cases {
        ports.set_subject(tenant_a(), subject_user(), status);
        let via_binding = authorize(&ports)
            .check(&ctx, "document.read", None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(
            !via_binding.allowed && via_binding.reason == reason,
            "{status:?}"
        );
        // Suspension also overrides the compat bridge (revoked after JWT).
        let via_compat = authorize(&ports)
            .check(&compat, "audit.read", None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!via_compat.allowed, "{status:?} must not use compat grants");
    }
}

#[tokio::test]
async fn compat_bridge_is_governance_only() {
    let (ports, ctx) = setup();
    let ctx = ctx.with_compat_grants([
        ManagementCompatGrant::AuditRead,
        ManagementCompatGrant::IntegrityRead,
        ManagementCompatGrant::IntegrityScan,
        ManagementCompatGrant::RepairDryRun,
        ManagementCompatGrant::RepairExecute,
        ManagementCompatGrant::RepairApprove,
        ManagementCompatGrant::RepairCancel,
    ]);
    let engine = authorize(&ports);
    for grant in [
        ManagementCompatGrant::AuditRead,
        ManagementCompatGrant::RepairDryRun,
        ManagementCompatGrant::RepairCancel,
    ] {
        let decision = engine
            .check(&ctx, grant.key(), None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(decision.allowed);
        assert_eq!(
            decision.reason,
            DecisionReason::AllowCompatManagementClaim,
            "compat allows must be explainable as compat"
        );
    }
    // Compat can never name an IAM or reserved permission.
    for other in ["identity.read", "policy.binding.manage", "contract.read"] {
        let decision = engine
            .check(&ctx, other, None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!decision.allowed, "{other} must not be compat-grantable");
    }
}

#[tokio::test]
async fn foreign_roles_are_invisible_and_never_grant() {
    let (ports, ctx) = setup();
    // A role owned by tenant B, bound (store-side) into tenant A.
    let foreign = RoleDefinition::create_tenant_role(
        foreign_role_id(),
        tenant_b(),
        "outsider",
        "Foreign Role",
        ts(500),
    )
    .unwrap_or_else(|_| unreachable!());
    ports.seed_role(foreign, &["document.read"]);
    ports.seed_binding(seedable_binding(ResourceScope::Tenant));
    let decision = authorize(&ports)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, DecisionReason::DenyCrossTenant);
    // Query side: the foreign role is not even visible to tenant A.
    let invisible = ports
        .query
        .get_role(tenant_a(), foreign_role_id())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(invisible.is_none());
    // …and visible to its own tenant.
    let visible = ports
        .query
        .get_role(tenant_b(), foreign_role_id())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(visible.is_some());
}

fn seedable_binding(scope: ResourceScope) -> RoleBinding {
    RoleBinding::create(
        id(30),
        tenant_a(),
        subject_user(),
        foreign_role_id(),
        scope,
        ValidityWindow {
            effective_at: ts(1000),
            expires_at: None,
        },
        ts(1000),
    )
    .unwrap_or_else(|_| unreachable!())
}

#[tokio::test]
async fn role_without_the_permission_does_not_grant() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    let decision = authorize(&ports)
        .check(&ctx, "document.review", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, DecisionReason::DenyNoBinding);
}

#[tokio::test]
async fn validity_windows_are_enforced_per_decision() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    let now = Utc::now();
    let expired = RoleBinding::create(
        id(31),
        tenant_a(),
        subject_user(),
        role_id(),
        ResourceScope::Tenant,
        ValidityWindow {
            effective_at: now - chrono::Duration::hours(2),
            expires_at: Some(now - chrono::Duration::hours(1)),
        },
        ts(1000),
    )
    .unwrap_or_else(|_| unreachable!());
    let future = RoleBinding::create(
        id(32),
        tenant_a(),
        subject_user(),
        role_id(),
        ResourceScope::Tenant,
        ValidityWindow {
            effective_at: now + chrono::Duration::hours(1),
            expires_at: None,
        },
        ts(1000),
    )
    .unwrap_or_else(|_| unreachable!());
    ports.seed_binding(expired);
    ports.seed_binding(future.clone());
    let decision = authorize(&ports)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, DecisionReason::DenyBindingExpired);

    // A binding not yet effective alone reports its own reason.
    let ports2 = FakePolicyPorts::new();
    ports2.set_subject(tenant_a(), subject_user(), SubjectStatus::Active);
    tenant_role(&ports2, "viewer", &["document.read"]);
    ports2.seed_binding(future.with_id(id(33)));
    let decision = authorize(&ports2)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(decision.reason, DecisionReason::DenyBindingNotYetEffective);
    // A far-future expiry still grants today (bounded clock skew aside).
    ports2.seed_binding(
        RoleBinding::create(
            id(34),
            tenant_a(),
            subject_user(),
            role_id(),
            ResourceScope::Tenant,
            ValidityWindow {
                effective_at: now - chrono::Duration::hours(1),
                expires_at: Some(
                    now.checked_add_days(Days::new(30))
                        .unwrap_or_else(|| unreachable!()),
                ),
            },
            ts(1000),
        )
        .unwrap_or_else(|_| unreachable!()),
    );
    let decision = authorize(&ports2)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(decision.allowed, "a live window inside validity must grant");
}

trait WithId {
    fn with_id(&self, binding_id: Uuid) -> Self;
}

impl WithId for RoleBinding {
    fn with_id(&self, binding_id: Uuid) -> Self {
        RoleBinding::create(
            binding_id,
            self.tenant_id(),
            self.user_id(),
            self.role_id(),
            self.scope().clone(),
            ValidityWindow {
                effective_at: self.effective_at(),
                expires_at: self.expires_at(),
            },
            self.effective_at(),
        )
        .unwrap_or_else(|_| unreachable!())
    }
}

#[tokio::test]
async fn disabled_role_denies_existing_bindings() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    // Disable the role via the store (what UpdateRole commits).
    ports
        .command
        .update_role(policy::ports::UpdateRoleCommit {
            tenant_id: tenant_a(),
            role_id: role_id(),
            display_name: None,
            status: Some(policy::domain::RoleStatus::Disabled),
            expected_version: 1,
            audit: policy::ports::MutationContext {
                actor_id: actor_admin().to_string(),
                actor_kind: policy::ports::MutationActorKind::User,
                operation_id: Uuid::now_v7(),
                trace_id: None,
                reason: None,
            },
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    let decision = authorize(&ports)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, DecisionReason::DenyRoleDisabled);
}

#[tokio::test]
async fn org_scope_exact_and_subtree_semantics() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(
        &ports,
        ResourceScope::organization_unit(unit_a(), false).unwrap_or_else(|_| unreachable!()),
    )
    .await;
    let engine = authorize(&ports);
    let in_unit = ResourceTarget {
        org_unit_id: Some(unit_a()),
        ..Default::default()
    };
    let in_child = ResourceTarget {
        org_unit_id: Some(unit_b()),
        ..Default::default()
    };
    // Exact scope: the unit itself grants, the child does not leak in, and
    // a request without trusted org metadata cannot match either.
    assert!(
        engine
            .check(&ctx, "document.read", Some(&in_unit))
            .await
            .unwrap_or_else(|_| unreachable!())
            .allowed
    );
    let leak = engine
        .check(&ctx, "document.read", Some(&in_child))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(leak.reason, DecisionReason::DenyScopeMismatch);
    let headless = engine
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(headless.reason, DecisionReason::DenyScopeMismatch);

    // Subtree scope: descendants match, the unit itself still matches.
    let ports2 = FakePolicyPorts::new();
    ports2.set_subject(tenant_a(), subject_user(), SubjectStatus::Active);
    ports2.set_subject(tenant_a(), actor_admin(), SubjectStatus::Active);
    ports2.set_unit(tenant_a(), unit_a(), None, true);
    ports2.set_unit(tenant_a(), unit_b(), Some(unit_a()), true);
    tenant_role(&ports2, "viewer", &["document.read"]);
    bind_tenant(
        &ports2,
        ResourceScope::organization_unit(unit_a(), true).unwrap_or_else(|_| unreachable!()),
    )
    .await;
    let ctx2 = AuthorizationContext::new(subject_user(), tenant_a());
    let engine2 = authorize(&ports2);
    assert!(
        engine2
            .check(&ctx2, "document.read", Some(&in_child))
            .await
            .unwrap_or_else(|_| unreachable!())
            .allowed,
        "subtree scope must cover descendants"
    );
    // Inactive units stop granting even for the exact unit.
    ports2.set_unit(tenant_a(), unit_a(), None, false);
    let inactive = engine2
        .check(&ctx2, "document.read", Some(&in_unit))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(inactive.reason, DecisionReason::DenyScopeMismatch);
}

#[tokio::test]
async fn resource_and_kind_scopes_require_trusted_metadata() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    let resource_uuid = id(40);
    bind_tenant(
        &ports,
        ResourceScope::resource("document", resource_uuid).unwrap_or_else(|_| unreachable!()),
    )
    .await;
    let engine = authorize(&ports);
    let exact = ResourceTarget {
        kind: Some("document".to_string()),
        resource_id: Some(resource_uuid),
        org_unit_id: None,
    };
    assert!(
        engine
            .check(&ctx, "document.read", Some(&exact))
            .await
            .unwrap_or_else(|_| unreachable!())
            .allowed
    );
    let wrong_kind = ResourceTarget {
        kind: Some("invoice".to_string()),
        resource_id: Some(resource_uuid),
        org_unit_id: None,
    };
    let mismatch = engine
        .check(&ctx, "document.read", Some(&wrong_kind))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(mismatch.reason, DecisionReason::DenyScopeMismatch);
    let headless = ResourceTarget {
        kind: None,
        resource_id: Some(resource_uuid),
        org_unit_id: None,
    };
    let mismatch = engine
        .check(&ctx, "document.read", Some(&headless))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(mismatch.reason, DecisionReason::DenyScopeMismatch);
}

#[tokio::test]
async fn store_failures_deny_internally_never_allow() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    ports.poison();
    let decision = authorize(&ports)
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, DecisionReason::DenyInternal);
}

/// Seeds three candidates: id 20 revoked (rank 1), id 21 scope mismatch
/// (rank 2), and optionally id 22 which grants.
fn seed_ranked_bindings(ports: &FakePolicyPorts, include_grant: bool) {
    let now = Utc::now();
    let window = ValidityWindow {
        effective_at: now - chrono::Duration::hours(1),
        expires_at: None,
    };
    let mut revoked = RoleBinding::create(
        id(20),
        tenant_a(),
        subject_user(),
        role_id(),
        ResourceScope::Tenant,
        window,
        ts(1000),
    )
    .unwrap_or_else(|_| unreachable!());
    revoked.revoke(ts(1050)).unwrap_or_else(|_| unreachable!());
    ports.seed_binding(revoked);
    ports.seed_binding(
        RoleBinding::create(
            id(21),
            tenant_a(),
            subject_user(),
            role_id(),
            ResourceScope::resource("document", id(41)).unwrap_or_else(|_| unreachable!()),
            window,
            ts(1000),
        )
        .unwrap_or_else(|_| unreachable!()),
    );
    if include_grant {
        ports.seed_binding(
            RoleBinding::create(
                id(22),
                tenant_a(),
                subject_user(),
                role_id(),
                ResourceScope::Tenant,
                window,
                ts(1000),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
    }
}

#[tokio::test]
async fn explain_reports_every_candidate_in_binding_id_order() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    seed_ranked_bindings(&ports, true);
    let explainer = ExplainDecision::new(
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    );
    let allowed = explainer
        .explain(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(allowed.allowed);
    let outcomes: Vec<DecisionReason> = allowed
        .evaluations
        .iter()
        .map(|evaluation| evaluation.outcome)
        .collect();
    assert_eq!(
        outcomes,
        vec![
            DecisionReason::DenyBindingRevoked,
            DecisionReason::DenyScopeMismatch,
            DecisionReason::AllowRoleBinding,
        ],
        "evaluations cover every candidate in binding_id order, allow last"
    );
}

#[tokio::test]
async fn worst_deny_reason_wins_when_nothing_grants() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    seed_ranked_bindings(&ports, false);
    let explainer = ExplainDecision::new(
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    );
    let denied = explainer
        .explain(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(denied.reason, DecisionReason::DenyScopeMismatch);
    assert_eq!(denied.evaluations.len(), 2);
}

#[tokio::test]
async fn subtree_scope_fails_closed_when_bound_or_host_unit_disabled() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(
        &ports,
        ResourceScope::organization_unit(unit_a(), true).unwrap_or_else(|_| unreachable!()),
    )
    .await;
    let engine = authorize(&ports);
    let in_child = ResourceTarget {
        org_unit_id: Some(unit_b()),
        ..Default::default()
    };
    assert!(
        engine
            .check(&ctx, "document.read", Some(&in_child))
            .await
            .unwrap_or_else(|_| unreachable!())
            .allowed
    );
    // Disabling the bound (ancestor) unit stops the subtree grant.
    ports.set_unit(tenant_a(), unit_a(), None, false);
    let denied = engine
        .check(&ctx, "document.read", Some(&in_child))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(denied.reason, DecisionReason::DenyScopeMismatch);
    // Disabling the unit that hosts the resource stops it too, even with
    // an active ancestor.
    ports.set_unit(tenant_a(), unit_a(), None, true);
    ports.set_unit(tenant_a(), unit_b(), Some(unit_a()), false);
    let denied = engine
        .check(&ctx, "document.read", Some(&in_child))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(denied.reason, DecisionReason::DenyScopeMismatch);
}

#[tokio::test]
async fn mid_evaluation_store_failures_deny_internally() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    let engine = authorize(&ports);
    // One shot per failure point: list bindings, role visibility, role
    // grants — each must produce DenyInternal, never a deny-skip to a
    // weaker reason and never an allow.
    for poison in [
        FakePolicyPorts::fail_next_list_bindings,
        FakePolicyPorts::fail_next_get_role,
        FakePolicyPorts::fail_next_get_role_permissions,
    ] {
        poison(&ports);
        let decision = engine
            .check(&ctx, "document.read", None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(!decision.allowed);
        assert_eq!(decision.reason, DecisionReason::DenyInternal);
    }
    // The organization bridge failing mid-scope-match also denies
    // internally (the scope branch must not silently mismatch-and-continue
    // towards other bindings).
    let (ports2, ctx2) = setup();
    tenant_role(&ports2, "viewer", &["document.read"]);
    bind_tenant(
        &ports2,
        ResourceScope::organization_unit(unit_a(), true).unwrap_or_else(|_| unreachable!()),
    )
    .await;
    let engine2 = authorize(&ports2);
    let in_child = ResourceTarget {
        org_unit_id: Some(unit_b()),
        ..Default::default()
    };
    ports2.fail_next_subtree_contains();
    let decision = engine2
        .check(&ctx2, "document.read", Some(&in_child))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(decision.reason, DecisionReason::DenyInternal);
}

#[tokio::test]
async fn retired_permission_denies_as_unknown() {
    let (ports, ctx) = setup();
    tenant_role(&ports, "viewer", &["document.read"]);
    bind_tenant(&ports, ResourceScope::Tenant).await;
    let engine = authorize(&ports);
    assert!(
        engine
            .check(&ctx, "document.read", None)
            .await
            .unwrap_or_else(|_| unreachable!())
            .allowed
    );
    ports.retire_permission("document.read");
    let decision = engine
        .check(&ctx, "document.read", None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        DecisionReason::DenyUnknownPermission,
        "a retired catalog key evaluates as unknown"
    );
}
