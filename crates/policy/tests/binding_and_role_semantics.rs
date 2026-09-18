//! Role/binding write semantics: the self-escalation invariants, system
//! role immutability, and the adapter contract guarantees (convergence,
//! optimistic versions, idempotency scoping, audit-once) that the
//! PostgreSQL/SQLite adapters must reproduce in the shared suite.

#![cfg(feature = "testing")]

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use policy::application::{
    BindRole, BindRoleCommand, CreateRole, CreateRoleCommand, PolicyApplicationError,
    RevokeRoleBinding, RevokeRoleBindingCommand, SetRolePermissions, SetRolePermissionsCommand,
    UpdateRole, UpdateRoleCommand,
};
use policy::domain::{ResourceScope, RoleBinding, RoleDefinition, RoleStatus, ValidityWindow};
use policy::ports::{
    BindRoleCommit, CreateRoleCommit, MutationActorKind, MutationContext, PolicyStoreError,
    RevokeBindingCommit, SetRolePermissionsCommit, SubjectStatus, UpdateRoleCommit,
};
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

fn subject_user() -> Uuid {
    id(3)
}

fn iam_role_id() -> Uuid {
    id(5)
}

fn plain_role_id() -> Uuid {
    id(6)
}

fn second_iam_role_id() -> Uuid {
    id(7)
}

fn system_bootstrap_role_id() -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        b"policy-system-role:system.bootstrap-admin",
    )
}

fn setup() -> FakePolicyPorts {
    let ports = FakePolicyPorts::new();
    ports.set_subject(tenant_a(), subject_user(), SubjectStatus::Active);
    ports
}

fn seed_role(ports: &FakePolicyPorts, role_id: Uuid, key: &str, permissions: &[&str]) {
    let role = RoleDefinition::create_tenant_role(role_id, tenant_a(), key, "Role", ts(500))
        .unwrap_or_else(|_| unreachable!());
    ports.seed_role(role, permissions);
}

fn seed_active_binding(ports: &FakePolicyPorts, binding_id: Uuid, user: Uuid, role: Uuid) {
    ports.seed_binding(
        RoleBinding::create(
            binding_id,
            tenant_a(),
            user,
            role,
            ResourceScope::Tenant,
            ValidityWindow {
                effective_at: ts(1000),
                expires_at: None,
            },
            ts(1000),
        )
        .unwrap_or_else(|_| unreachable!()),
    );
}

fn bind_use_case(ports: &FakePolicyPorts) -> BindRole {
    BindRole::new(
        Arc::clone(&ports.command),
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    )
}

fn bind_command(user: Uuid, role: Uuid) -> BindRoleCommand {
    BindRoleCommand {
        tenant_id: tenant_a(),
        binding_id: None,
        user_id: user,
        role_id: role,
        scope: ResourceScope::Tenant,
        effective_at: ts(1000),
        expires_at: None,
        actor_user_id: user,
        idempotency_key: None,
        reason: None,
    }
}

fn mutation_context() -> MutationContext {
    MutationContext {
        actor_id: subject_user().to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason: None,
    }
}

#[tokio::test]
async fn self_bind_to_iam_role_is_denied() {
    let ports = setup();
    seed_role(&ports, iam_role_id(), "iam", &["policy.binding.manage"]);
    let err = bind_use_case(&ports)
        .execute(bind_command(subject_user(), iam_role_id()))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::SelfEscalationDenied);
    // A non-IAM role remains self-bindable (day-to-day convenience).
    seed_role(&ports, plain_role_id(), "viewer", &["document.read"]);
    let outcome = bind_use_case(&ports)
        .execute(bind_command(subject_user(), plain_role_id()))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!outcome.replayed);
}

#[tokio::test]
async fn self_bind_idempotent_rebind_converges() {
    let ports = setup();
    seed_role(&ports, iam_role_id(), "iam", &["policy.role.manage"]);
    seed_active_binding(&ports, id(30), subject_user(), iam_role_id());
    // Identical re-bind passes the guard and converges at the store.
    let outcome = bind_use_case(&ports)
        .execute(bind_command(subject_user(), iam_role_id()))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(outcome.replayed, "identical active binding must converge");
    assert_eq!(outcome.binding.binding_id(), id(30));
}

#[tokio::test]
async fn self_bind_with_wider_scope_or_window_is_denied() {
    let ports = setup();
    seed_role(
        &ports,
        iam_role_id(),
        "iam",
        &["policy.role.manage", "document.read"],
    );
    // Existing self-binding is narrow: ResourceType scope, expiring window.
    ports.seed_binding(
        RoleBinding::create(
            id(30),
            tenant_a(),
            subject_user(),
            iam_role_id(),
            ResourceScope::resource_type("document").unwrap_or_else(|_| unreachable!()),
            ValidityWindow {
                effective_at: ts(1000),
                expires_at: Some(ts(100_000_000_000)),
            },
            ts(1000),
        )
        .unwrap_or_else(|_| unreachable!()),
    );
    // Same role but TENANT scope is new authority, not a replay → denied.
    let err = bind_use_case(&ports)
        .execute(bind_command(subject_user(), iam_role_id()))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::SelfEscalationDenied);
    // Same scope but a wider (open-ended) window is also denied.
    let err = bind_use_case(&ports)
        .execute(BindRoleCommand {
            scope: ResourceScope::resource_type("document").unwrap_or_else(|_| unreachable!()),
            effective_at: ts(1000),
            expires_at: None,
            ..bind_command(subject_user(), iam_role_id())
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::SelfEscalationDenied);
    assert_eq!(
        ports.audit_records().len(),
        0,
        "denied self-binds must not write or audit anything"
    );
}

#[tokio::test]
async fn bootstrap_source_carveout_lives_and_dies_with_the_system_binding() {
    let ports = setup();
    seed_role(&ports, iam_role_id(), "iam", &["policy.binding.manage"]);
    seed_role(
        &ports,
        second_iam_role_id(),
        "iam2",
        &["policy.role.manage"],
    );
    // No system binding yet → self-bind denied.
    let err = bind_use_case(&ports)
        .execute(bind_command(subject_user(), iam_role_id()))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::SelfEscalationDenied);
    // Active binding to the immutable system role → carve-out applies.
    seed_active_binding(&ports, id(31), subject_user(), system_bootstrap_role_id());
    let outcome = bind_use_case(&ports)
        .execute(bind_command(subject_user(), iam_role_id()))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!outcome.replayed);
    // Revoke the system binding: the carve-out ends on the *next* request
    // (decision-time evaluation, never a historical flag).
    let revoker = RevokeRoleBinding::new(Arc::clone(&ports.command));
    revoker
        .execute(RevokeRoleBindingCommand {
            tenant_id: tenant_a(),
            binding_id: id(31),
            expected_version: 1,
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: Some("rotate admin".to_string()),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    let err = bind_use_case(&ports)
        .execute(bind_command(subject_user(), second_iam_role_id()))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::SelfEscalationDenied);
}

#[tokio::test]
async fn self_dos_revocation_is_allowed_and_audited() {
    let ports = setup();
    seed_role(&ports, plain_role_id(), "viewer", &["document.read"]);
    let outcome = bind_use_case(&ports)
        .execute(bind_command(subject_user(), plain_role_id()))
        .await
        .unwrap_or_else(|_| unreachable!());
    let binding_id = outcome.binding.binding_id();
    let revoked = RevokeRoleBinding::new(Arc::clone(&ports.command))
        .execute(RevokeRoleBindingCommand {
            tenant_id: tenant_a(),
            binding_id,
            expected_version: outcome.binding.version().value(),
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!revoked.binding.is_active());
    let actions: Vec<String> = ports
        .audit_records()
        .iter()
        .map(|record| record.action.clone())
        .collect();
    assert!(actions.contains(&"policy.binding.revoked".to_string()));
}

#[tokio::test]
async fn set_permissions_iam_addition_requires_unbound_actor() {
    let ports = setup();
    seed_role(&ports, iam_role_id(), "iam", &["document.read"]);
    seed_role(&ports, plain_role_id(), "plain", &["document.read"]);
    // Bound actor may not add an IAM-management key to their own role.
    seed_active_binding(&ports, id(30), subject_user(), iam_role_id());
    let setter = SetRolePermissions::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
    let err = setter
        .execute(SetRolePermissionsCommand {
            tenant_id: tenant_a(),
            role_id: iam_role_id(),
            permission_keys: vec![
                "document.read".to_string(),
                "policy.role.manage".to_string(),
            ],
            expected_version: 1,
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::SelfEscalationDenied);
    // …but the same actor may change non-management grants of that role.
    let outcome = setter
        .execute(SetRolePermissionsCommand {
            tenant_id: tenant_a(),
            role_id: iam_role_id(),
            permission_keys: vec!["document.review".to_string()],
            expected_version: 1,
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(outcome.role.version().value(), 2);
    // Unbound actor may add the management key (normal administration).
    let outcome = setter
        .execute(SetRolePermissionsCommand {
            tenant_id: tenant_a(),
            role_id: plain_role_id(),
            permission_keys: vec!["policy.role.manage".to_string()],
            expected_version: 1,
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        outcome.permission_keys,
        vec!["policy.role.manage".to_string()]
    );
}

#[tokio::test]
async fn system_roles_reject_every_mutation_surface() {
    let ports = setup();
    let system_id = system_bootstrap_role_id();
    // Visible (system roles are tenant-visible for binding)…
    let role = ports
        .query
        .get_role(tenant_a(), system_id)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(role.is_some_and(|role| role.is_system()));
    // …but immutable through both application and store surfaces.
    let updater = UpdateRole::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
    let err = updater
        .execute(UpdateRoleCommand {
            tenant_id: tenant_a(),
            role_id: system_id,
            display_name: Some("hijacked".to_string()),
            status: None,
            expected_version: 1,
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::RoleImmutable);
    let store_err = ports
        .command
        .set_role_permissions(SetRolePermissionsCommit {
            tenant_id: tenant_a(),
            role_id: system_id,
            permission_keys: vec!["document.read".to_string()],
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(store_err, PolicyStoreError::RoleImmutable);
    let store_update = ports
        .command
        .update_role(UpdateRoleCommit {
            tenant_id: tenant_a(),
            role_id: system_id,
            display_name: None,
            status: Some(RoleStatus::Disabled),
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(store_update, PolicyStoreError::RoleImmutable);
}

#[tokio::test]
async fn binding_convergence_and_revoke_versions_at_store() {
    let ports = setup();
    seed_role(&ports, plain_role_id(), "viewer", &["document.read"]);
    let commit = BindRoleCommit {
        tenant_id: tenant_a(),
        binding_id: None,
        user_id: subject_user(),
        role_id: plain_role_id(),
        scope: ResourceScope::Tenant,
        effective_at: ts(1000),
        expires_at: None,
        audit: mutation_context(),
        idempotency_key: Some("bind-1".to_string()),
        now: ts(1000),
    };
    let first = ports
        .command
        .bind_role(commit.clone())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!first.replayed);
    let replay = ports
        .command
        .bind_role(commit)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed, "idempotency key replays");

    // Revoke: version bump once; replay at the new version converges;
    // a stale expected_version stays fail-closed.
    let revoke = |expected_version: i64| RevokeBindingCommit {
        tenant_id: tenant_a(),
        binding_id: first.binding.binding_id(),
        expected_version,
        audit: mutation_context(),
        idempotency_key: None,
        now: ts(1100),
    };
    let revoked = ports
        .command
        .revoke_binding(revoke(1))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(revoked.binding.version().value(), 2);
    let reconverged = ports
        .command
        .revoke_binding(revoke(2))
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(reconverged.replayed, "already revoked converges at v2");
    let stale = ports
        .command
        .revoke_binding(revoke(1))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(stale, PolicyStoreError::VersionConflict);
    let audits = ports.audit_records();
    let created = audits
        .iter()
        .filter(|record| record.action == "policy.binding.created")
        .count();
    let revoked_count = audits
        .iter()
        .filter(|record| record.action == "policy.binding.revoked")
        .count();
    assert_eq!(created, 1, "replays must not duplicate audit");
    assert_eq!(revoked_count, 1);
}

#[tokio::test]
async fn idempotency_conflicts_are_scoped_per_operation() {
    let ports = setup();
    seed_role(&ports, plain_role_id(), "viewer", &["document.read"]);
    let commit = BindRoleCommit {
        tenant_id: tenant_a(),
        binding_id: None,
        user_id: subject_user(),
        role_id: plain_role_id(),
        scope: ResourceScope::Tenant,
        effective_at: ts(1000),
        expires_at: None,
        audit: mutation_context(),
        idempotency_key: Some("bind-x".to_string()),
        now: ts(1000),
    };
    ports
        .command
        .bind_role(commit.clone())
        .await
        .unwrap_or_else(|_| unreachable!());
    // Same key, different payload → conflict, never a silent rewrite.
    let conflict = ports
        .command
        .bind_role(BindRoleCommit {
            user_id: id(99),
            ..commit
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(conflict, PolicyStoreError::IdempotencyConflict);
}

#[tokio::test]
async fn permission_set_replace_validates_catalog_and_caps() {
    let ports = setup();
    seed_role(&ports, plain_role_id(), "viewer", &["document.read"]);
    let unknown = ports
        .command
        .set_role_permissions(SetRolePermissionsCommit {
            tenant_id: tenant_a(),
            role_id: plain_role_id(),
            permission_keys: vec!["not.in.catalog".to_string()],
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(unknown, PolicyStoreError::UnknownPermission);
    let oversize = vec!["document.read".to_string(); 201];
    let capped = ports
        .command
        .set_role_permissions(SetRolePermissionsCommit {
            tenant_id: tenant_a(),
            role_id: plain_role_id(),
            permission_keys: oversize,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(capped, PolicyStoreError::TooManyPermissions);
    // Identical set converges: no version bump, no second audit.
    ports
        .command
        .set_role_permissions(SetRolePermissionsCommit {
            tenant_id: tenant_a(),
            role_id: plain_role_id(),
            permission_keys: vec!["document.read".to_string()],
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    let role = ports
        .query
        .get_role(tenant_a(), plain_role_id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        role.version().value(),
        1,
        "identical set must not bump version"
    );
    assert!(ports
        .audit_records()
        .iter()
        .all(|record| record.action != "policy.role.permissions_updated"));
}

#[tokio::test]
async fn role_creation_enforces_unique_keys_and_chosen_ids() {
    let ports = setup();
    let creator = CreateRole::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
    let command = CreateRoleCommand {
        tenant_id: tenant_a(),
        role_id: Some(id(60)),
        stable_key: "auditors".to_string(),
        display_name: "Auditors".to_string(),
        actor_user_id: subject_user(),
        idempotency_key: Some("create-1".to_string()),
        reason: None,
    };
    let first = creator
        .execute(command.clone())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!first.replayed);
    let replay = creator
        .execute(command)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed, "caller-chosen id participates in replay");
    // Same key, different tenant-visible namespace: duplicate rejected.
    let dup = creator
        .execute(CreateRoleCommand {
            role_id: None,
            idempotency_key: Some("create-2".to_string()),
            ..command_shape()
        })
        .await;
    assert!(matches!(dup, Err(PolicyApplicationError::AlreadyExists)));
    // Fresh key reusing the chosen id collides at the store (direct commit,
    // bypassing the command surface): primary-key AlreadyExists.
    let store_reused = ports
        .command
        .create_role(CreateRoleCommit {
            tenant_id: tenant_a(),
            role_id: Some(id(60)),
            stable_key: "different".to_string(),
            display_name: "Other".to_string(),
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1200),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(store_reused, PolicyStoreError::AlreadyExists);
}

fn command_shape() -> CreateRoleCommand {
    CreateRoleCommand {
        tenant_id: tenant_a(),
        role_id: None,
        stable_key: "auditors".to_string(),
        display_name: "Auditors Again".to_string(),
        actor_user_id: subject_user(),
        idempotency_key: None,
        reason: None,
    }
}

#[tokio::test]
async fn bind_requires_active_membership_visible_role_and_active_target() {
    let ports = setup();
    seed_role(&ports, plain_role_id(), "viewer", &["document.read"]);
    // Target without active membership.
    ports.set_subject(tenant_a(), id(70), SubjectStatus::NoMembership);
    let err = bind_use_case(&ports)
        .execute(BindRoleCommand {
            user_id: id(70),
            ..bind_command(subject_user(), plain_role_id())
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::NotTenantMember);
    // Foreign role is not visible → NotFound.
    let err = bind_use_case(&ports)
        .execute(bind_command(subject_user(), id(71)))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyApplicationError::NotFound);
    // Disabled role cannot be bound.
    ports
        .command
        .update_role(UpdateRoleCommit {
            tenant_id: tenant_a(),
            role_id: plain_role_id(),
            display_name: None,
            status: Some(RoleStatus::Disabled),
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1100),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    let err = bind_use_case(&ports)
        .execute(bind_command(subject_user(), plain_role_id()))
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(matches!(err, PolicyApplicationError::Validation(_)));
}

#[tokio::test]
async fn store_side_caps_are_authoritative() {
    let ports = setup();
    // Per-user active binding cap: 100 seeded rows for one user block the
    // 101st bind at the store even without the application pre-check.
    for n in 0..100_u128 {
        ports.seed_binding(
            RoleBinding::create(
                Uuid::from_u128(n + 1),
                tenant_a(),
                id(80),
                plain_role_id(),
                ResourceScope::resource_type(format!("kind-{n}"))
                    .unwrap_or_else(|_| unreachable!()),
                ValidityWindow {
                    effective_at: ts(1000),
                    expires_at: None,
                },
                ts(1000),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
    }
    let seeded_role = RoleDefinition::create_tenant_role(
        plain_role_id(),
        tenant_a(),
        "viewer",
        "Viewer",
        ts(500),
    )
    .unwrap_or_else(|_| unreachable!());
    ports.seed_role(seeded_role, &["document.read"]);
    let err = ports
        .command
        .bind_role(BindRoleCommit {
            tenant_id: tenant_a(),
            binding_id: None,
            user_id: id(80),
            role_id: plain_role_id(),
            scope: ResourceScope::resource_type("kind-200").unwrap_or_else(|_| unreachable!()),
            effective_at: ts(1000),
            expires_at: None,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1000),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyStoreError::TooManyResources);
}

#[tokio::test]
async fn role_budget_counts_tenant_rows_not_system_roles() {
    let ports = setup();
    // Two system roles exist from the seeded catalog. They never consume a
    // tenant budget: with 499 tenant roles present (499 + 2 = 501 visible
    // rows, the buggy pre-fix count), the application check must still
    // admit the 500th tenant role.
    for n in 0..499_u128 {
        let role = RoleDefinition::create_tenant_role(
            Uuid::from_u128(1000 + n),
            tenant_a(),
            &format!("role-{n}"),
            "Role",
            ts(500),
        )
        .unwrap_or_else(|_| unreachable!());
        ports.seed_role(role, &[]);
    }
    let creator = CreateRole::new(Arc::clone(&ports.command), Arc::clone(&ports.query));
    let outcome = creator
        .execute(CreateRoleCommand {
            tenant_id: tenant_a(),
            role_id: None,
            stable_key: "role-499".to_string(),
            display_name: "Role 499".to_string(),
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!outcome.replayed);
    // The 501st visible row is now refused by the advisory app check…
    let err = creator
        .execute(CreateRoleCommand {
            tenant_id: tenant_a(),
            role_id: None,
            stable_key: "one-too-many".to_string(),
            display_name: "One too many".to_string(),
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(matches!(err, PolicyApplicationError::Validation(_)));
    // …and by the authoritative store check, which counts tenant rows.
    let store_err = ports
        .command
        .create_role(CreateRoleCommit {
            tenant_id: tenant_a(),
            role_id: None,
            stable_key: "store-side".to_string(),
            display_name: "Store side".to_string(),
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1200),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(store_err, PolicyStoreError::TooManyResources);
    // The `system.` namespace stays closed to tenant roles.
    let err = creator
        .execute(CreateRoleCommand {
            tenant_id: tenant_a(),
            role_id: None,
            stable_key: "system.bootstrap-admin".to_string(),
            display_name: "Look-alike".to_string(),
            actor_user_id: subject_user(),
            idempotency_key: None,
            reason: None,
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(matches!(err, PolicyApplicationError::Validation(_)));
}

#[tokio::test]
async fn tenant_binding_listing_and_write_caps_bind_at_the_store() {
    let ports = setup();
    let seeded_role = RoleDefinition::create_tenant_role(
        plain_role_id(),
        tenant_a(),
        "viewer",
        "Viewer",
        ts(500),
    )
    .unwrap_or_else(|_| unreachable!());
    ports.seed_role(seeded_role, &["document.read"]);
    // Fill the tenant to the cap with one binding per distinct user so the
    // per-user cap does not shadow the tenant-wide one.
    for n in 0..5_000_u128 {
        ports.seed_binding(
            RoleBinding::create(
                Uuid::from_u128(n + 1),
                tenant_a(),
                Uuid::from_u128(100_000 + n),
                plain_role_id(),
                ResourceScope::Tenant,
                ValidityWindow {
                    effective_at: ts(1000),
                    expires_at: None,
                },
                ts(1000),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
    }
    // Exactly at the cap, listing still works; writing refuses.
    let listed = ports
        .query
        .list_bindings(tenant_a(), None)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(listed.len(), 5_000);
    let err = ports
        .command
        .bind_role(BindRoleCommit {
            tenant_id: tenant_a(),
            binding_id: None,
            user_id: id(81),
            role_id: plain_role_id(),
            scope: ResourceScope::Tenant,
            effective_at: ts(1000),
            expires_at: None,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(1000),
        })
        .await
        .err()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(err, PolicyStoreError::TooManyResources);
}
