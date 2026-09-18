//! Port contract semantics exercised against the in-memory fakes.
//!
//! These are the *same* behavioural expectations the PostgreSQL/SQLite
//! adapters must pass in the shared contract suite (PLAN-0013 WP-02/05):
//! tenant isolation, poison mapping, optimistic versioning, idempotency
//! fingerprint coverage, in-transaction placement re-validation, membership
//! convergence, and audit-once mutation semantics. The store-side tests
//! drive the command ports directly (bypassing application pre-checks) so
//! the in-transaction re-validation itself is pinned.

#![cfg(feature = "testing")]

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use organization::application::{
    AddOrganizationMember, AddOrganizationMemberCommand, OrganizationApplicationError,
    RemoveOrganizationMember, RemoveOrganizationMemberCommand, MAX_UNITS_PER_TENANT,
};
use organization::domain::{
    OrganizationMembershipType, OrganizationUnit, OrganizationUnitStatus, OrganizationUnitType,
};
use organization::ports::{
    AddMemberCommit, CreateUnitCommit, MoveUnitCommit, MutationActorKind, OrganizationStoreError,
    RemoveMemberCommit, UpdateUnitCommit,
};
use organization::testing::FakeOrganizationPorts;
use uuid::Uuid;

fn ts(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0)
        .single()
        .unwrap_or_else(|| unreachable!())
}

fn tenant_a() -> Uuid {
    Uuid::from_bytes([1; 16])
}

fn tenant_b() -> Uuid {
    Uuid::from_bytes([2; 16])
}

fn actor() -> Uuid {
    Uuid::from_bytes([3; 16])
}

fn user() -> Uuid {
    Uuid::from_bytes([4; 16])
}

fn unit_id() -> Uuid {
    Uuid::from_bytes([5; 16])
}

fn unit(tenant: Uuid) -> OrganizationUnit {
    unit_of(tenant, unit_id())
}

fn unit_of(tenant: Uuid, id: Uuid) -> OrganizationUnit {
    OrganizationUnit::create(
        id,
        tenant,
        None,
        OrganizationUnitType::Company,
        "Acme",
        ts(100),
    )
    .unwrap_or_else(|_| unreachable!())
}

fn mutation_context() -> organization::ports::MutationContext {
    organization::ports::MutationContext {
        actor_id: actor().to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason: None,
    }
}

#[tokio::test]
async fn queries_are_tenant_isolated() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));

    assert!(ports
        .query
        .get_unit(tenant_a(), unit_id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .is_some());
    assert!(ports
        .query
        .get_unit(tenant_b(), unit_id())
        .await
        .unwrap_or_else(|_| unreachable!())
        .is_none());
    assert!(ports
        .query
        .list_units(tenant_b())
        .await
        .unwrap_or_else(|_| unreachable!())
        .is_empty());
    assert_eq!(
        ports
            .query
            .list_unit_members(tenant_b(), unit_id())
            .await
            .unwrap_or_else(|_| unreachable!())
            .len(),
        0
    );
}

#[tokio::test]
async fn poisoned_store_maps_to_unavailable_on_every_port() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));
    ports.poison();

    assert_eq!(
        ports
            .query
            .list_units(tenant_a())
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::Unavailable
    );
    assert_eq!(
        ports
            .reader
            .is_active_member(tenant_a(), user())
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::Unavailable
    );
    assert_eq!(
        ports
            .command
            .update_unit(UpdateUnitCommit {
                tenant_id: tenant_a(),
                unit_id: unit_id(),
                name: Some("Renamed".to_string()),
                unit_type: None,
                status: None,
                expected_version: 1,
                audit: mutation_context(),
                idempotency_key: None,
                now: ts(200),
            })
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::Unavailable
    );
}

#[tokio::test]
async fn version_conflict_and_idempotent_update_semantics() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));

    // Stale version fails closed.
    let stale = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            name: Some("Renamed".to_string()),
            unit_type: None,
            status: None,
            expected_version: 99,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(200),
        })
        .await;
    assert_eq!(
        stale.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::VersionConflict
    );

    // Correct version applies, bumps version, audits once.
    let applied = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            name: Some("Renamed".to_string()),
            unit_type: None,
            status: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: Some("upd-1".to_string()),
            now: ts(200),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!applied.replayed);
    assert_eq!(applied.unit.version().value(), 2);
    assert_eq!(applied.unit.name(), "Renamed");

    // Same key, same fingerprint (body included the same expected_version)
    // ⇒ replay onto the stored row, before any version check.
    let replay = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            name: Some("Renamed".to_string()),
            unit_type: None,
            status: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: Some("upd-1".to_string()),
            now: ts(200),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed);
    assert_eq!(replay.unit.version().value(), 2);
    assert_eq!(
        ports
            .audit_records()
            .iter()
            .filter(|record| record.action == "organization.unit.updated")
            .count(),
        1,
        "audit must be written exactly once for a key-matched mutation"
    );

    // Same key, different payload ⇒ conflict.
    let conflicting = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            name: Some("Other".to_string()),
            unit_type: None,
            status: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: Some("upd-1".to_string()),
            now: ts(200),
        })
        .await;
    assert_eq!(
        conflicting.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::IdempotencyConflict
    );
}

#[tokio::test]
async fn update_with_identical_values_converges_without_bump_or_audit() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));

    // Same-value rename is a convergence, not a mutation (identity rule).
    let same_value = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            name: Some("Acme".to_string()),
            unit_type: None,
            status: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(200),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(same_value.replayed);
    assert_eq!(same_value.unit.version().value(), 1);
    assert!(
        ports
            .audit_records()
            .iter()
            .all(|record| record.action != "organization.unit.updated"),
        "a no-change convergence must not write audit"
    );

    // A real change still bumps.
    let renamed = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            name: Some("Acme Corp".to_string()),
            unit_type: None,
            status: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(250),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!renamed.replayed);
    assert_eq!(renamed.unit.version().value(), 2);
}

#[tokio::test]
async fn create_unit_idempotency_covers_caller_chosen_id() {
    let ports = FakeOrganizationPorts::default();
    let chosen = Uuid::from_bytes([7; 16]);
    let base = CreateUnitCommit {
        tenant_id: tenant_a(),
        unit_id: Some(chosen),
        parent_id: None,
        unit_type: OrganizationUnitType::Company,
        name: "Acme".to_string(),
        audit: mutation_context(),
        idempotency_key: Some("create-1".to_string()),
        now: ts(100),
    };

    let created = ports
        .command
        .create_unit(base.clone())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!created.replayed);
    assert_eq!(created.unit.unit_id(), chosen);

    // Same key, same payload ⇒ replay.
    let replay = ports
        .command
        .create_unit(base.clone())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed);
    assert_eq!(replay.unit.unit_id(), chosen);

    // Same key, only the caller-chosen unit id differs ⇒ IdempotencyConflict
    // (the chosen id is part of the request payload; MAJOR review rule).
    let swapped_id = CreateUnitCommit {
        unit_id: Some(Uuid::from_bytes([8; 16])),
        idempotency_key: Some("create-1".to_string()),
        ..base.clone()
    };
    assert_eq!(
        ports
            .command
            .create_unit(swapped_id)
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::IdempotencyConflict
    );

    // A *different* key with the same caller-chosen id ⇒ AlreadyExists
    // (the id is taken), not a silent second row.
    let taken_id = CreateUnitCommit {
        idempotency_key: Some("create-2".to_string()),
        ..base
    };
    assert_eq!(
        ports
            .command
            .create_unit(taken_id)
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::AlreadyExists
    );
}

#[tokio::test]
async fn store_side_placement_revalidation() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));

    // Create under a foreign-tenant parent bypasses the application
    // pre-check; the store must reject it inside the write path.
    let cross_parent = CreateUnitCommit {
        tenant_id: tenant_b(),
        unit_id: Some(Uuid::from_bytes([9; 16])),
        parent_id: Some(unit_id()),
        unit_type: OrganizationUnitType::Team,
        name: "Alien".to_string(),
        audit: mutation_context(),
        idempotency_key: None,
        now: ts(100),
    };
    assert_eq!(
        ports
            .command
            .create_unit(cross_parent)
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::InvalidParent
    );

    // Self-parent on create (fresh id that references itself as parent).
    let self_parent = CreateUnitCommit {
        tenant_id: tenant_a(),
        unit_id: Some(Uuid::from_bytes([10; 16])),
        parent_id: Some(Uuid::from_bytes([10; 16])),
        unit_type: OrganizationUnitType::Team,
        name: "Loop".to_string(),
        audit: mutation_context(),
        idempotency_key: None,
        now: ts(100),
    };
    assert_eq!(
        ports
            .command
            .create_unit(self_parent)
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::InvalidParent
    );
}

#[tokio::test]
async fn store_side_move_validation_and_replay() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));

    // Legal move: second root becomes parent of the first.
    let second = Uuid::from_bytes([6; 16]);
    ports.seed_unit(unit_of(tenant_a(), second));
    let moved = ports
        .command
        .move_unit(MoveUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            new_parent_id: Some(second),
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: Some("move-1".to_string()),
            now: ts(120),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!moved.replayed);
    assert_eq!(moved.unit.parent_id(), Some(second));

    // Replay converges before any version check.
    let replay = ports
        .command
        .move_unit(MoveUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            new_parent_id: Some(second),
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: Some("move-1".to_string()),
            now: ts(120),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed);

    // Now a cycle move (second under unit_id) is rejected store-side.
    let cycle = ports
        .command
        .move_unit(MoveUnitCommit {
            tenant_id: tenant_a(),
            unit_id: second,
            new_parent_id: Some(unit_id()),
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(130),
        })
        .await;
    assert_eq!(
        cycle.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::Cycle
    );

    // Self-parent on move.
    let self_move = ports
        .command
        .move_unit(MoveUnitCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            new_parent_id: Some(unit_id()),
            expected_version: 2,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(140),
        })
        .await;
    assert_eq!(
        self_move.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::InvalidParent
    );
}

#[tokio::test]
async fn membership_add_remove_convergence_at_store() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));
    let add = AddMemberCommit {
        tenant_id: tenant_a(),
        unit_id: unit_id(),
        user_id: user(),
        membership_type: OrganizationMembershipType::Member,
        audit: mutation_context(),
        idempotency_key: Some("add-1".to_string()),
        now: ts(110),
    };

    let added = ports
        .command
        .add_member(add.clone())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!added.replayed);
    assert!(added.membership.is_active());

    let replay = ports
        .command
        .add_member(add)
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed);

    // Duplicate add under a fresh key fails closed.
    let duplicate = AddMemberCommit {
        idempotency_key: Some("add-2".to_string()),
        ..base_add()
    };
    assert_eq!(
        ports
            .command
            .add_member(duplicate)
            .await
            .err()
            .unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::AlreadyExists
    );
}

#[tokio::test]
async fn membership_remove_readd_convergence_at_store() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));
    ports
        .command
        .add_member(AddMemberCommit {
            idempotency_key: None,
            ..base_add()
        })
        .await
        .unwrap_or_else(|_| unreachable!());

    let removed = ports
        .command
        .remove_member(RemoveMemberCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            user_id: user(),
            membership_type: OrganizationMembershipType::Member,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(120),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!removed.membership.is_active());

    // Already-inactive converges while expected_version still matches the
    // (now version-2) inactive row; a stale version stays fail-closed.
    let reconverge = ports
        .command
        .remove_member(RemoveMemberCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            user_id: user(),
            membership_type: OrganizationMembershipType::Member,
            expected_version: 2,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(130),
        })
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(reconverge.replayed);
    let stale_remove = ports
        .command
        .remove_member(RemoveMemberCommit {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            user_id: user(),
            membership_type: OrganizationMembershipType::Member,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(140),
        })
        .await;
    assert_eq!(
        stale_remove.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::VersionConflict
    );

    // Re-add converges onto the history row as a real mutation.
    let readded = ports
        .command
        .add_member(base_add())
        .await
        .unwrap_or_else(|_| unreachable!());
    assert!(!readded.replayed);
    assert!(readded.membership.is_active());
    assert_eq!(
        readded.membership.membership_id(),
        removed.membership.membership_id()
    );
    assert_eq!(readded.membership.version().value(), 3);
}

fn base_add() -> AddMemberCommit {
    AddMemberCommit {
        tenant_id: tenant_a(),
        unit_id: unit_id(),
        user_id: user(),
        membership_type: OrganizationMembershipType::Member,
        audit: mutation_context(),
        idempotency_key: Some("add-auto".to_string()),
        now: ts(150),
    }
}

#[tokio::test]
async fn tenant_resource_cap_is_enforced_store_side() {
    let ports = FakeOrganizationPorts::default();
    for index in 0..MAX_UNITS_PER_TENANT {
        let created = ports
            .command
            .create_unit(CreateUnitCommit {
                tenant_id: tenant_a(),
                unit_id: None,
                parent_id: None,
                unit_type: OrganizationUnitType::Team,
                name: format!("Unit {index}"),
                audit: mutation_context(),
                idempotency_key: None,
                now: ts(100 + i64::try_from(index).unwrap_or(i64::MAX)),
            })
            .await;
        assert!(created.is_ok(), "unit {index} must be creatable");
    }
    let overflow = ports
        .command
        .create_unit(CreateUnitCommit {
            tenant_id: tenant_a(),
            unit_id: None,
            parent_id: None,
            unit_type: OrganizationUnitType::Team,
            name: "Overflow".to_string(),
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(9999),
        })
        .await;
    assert_eq!(
        overflow.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::TooManyResources
    );
}

#[tokio::test]
async fn disabled_unit_rejects_membership_add_and_removal_converges() {
    let ports = FakeOrganizationPorts::default();
    let mut disabled = unit(tenant_a());
    disabled
        .update(None, None, Some(OrganizationUnitStatus::Disabled), ts(150))
        .unwrap_or_else(|_| unreachable!());
    ports.seed_unit(disabled);
    ports.set_active_member(tenant_a(), user());

    let add = AddOrganizationMember::new(
        Arc::clone(&ports.command),
        Arc::clone(&ports.query),
        Arc::clone(&ports.reader),
    );
    let denied = add
        .execute(AddOrganizationMemberCommand {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            user_id: user(),
            membership_type: OrganizationMembershipType::Member,
            actor_user_id: actor(),
            idempotency_key: None,
            reason: None,
        })
        .await;
    assert_eq!(
        denied.err().unwrap_or_else(|| unreachable!()),
        OrganizationApplicationError::UnitDisabled
    );

    // Removal of an unknown membership fails closed.
    let remove = RemoveOrganizationMember::new(Arc::clone(&ports.command));
    let missing = remove
        .execute(RemoveOrganizationMemberCommand {
            tenant_id: tenant_a(),
            unit_id: unit_id(),
            user_id: user(),
            membership_type: OrganizationMembershipType::Member,
            expected_version: 1,
            actor_user_id: actor(),
            idempotency_key: None,
            reason: None,
        })
        .await;
    assert_eq!(
        missing.err().unwrap_or_else(|| unreachable!()),
        OrganizationApplicationError::NotFound
    );
}

#[tokio::test]
async fn cross_tenant_operations_are_not_expressible() {
    let ports = FakeOrganizationPorts::default();
    ports.seed_unit(unit(tenant_a()));
    ports.set_active_member(tenant_a(), user());

    // Tenant B cannot attach its user to tenant A's unit: the unit is
    // invisible in tenant B.
    let add = AddOrganizationMember::new(
        Arc::clone(&ports.command),
        Arc::clone(&ports.query),
        Arc::clone(&ports.reader),
    );
    let denied = add
        .execute(AddOrganizationMemberCommand {
            tenant_id: tenant_b(),
            unit_id: unit_id(),
            user_id: user(),
            membership_type: OrganizationMembershipType::Member,
            actor_user_id: actor(),
            idempotency_key: None,
            reason: None,
        })
        .await;
    assert!(matches!(
        denied.err(),
        Some(
            OrganizationApplicationError::NotTenantMember | OrganizationApplicationError::NotFound
        )
    ));

    // Tenant B cannot move tenant A's unit either (invisible at the store).
    let moved = ports
        .command
        .move_unit(MoveUnitCommit {
            tenant_id: tenant_b(),
            unit_id: unit_id(),
            new_parent_id: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(120),
        })
        .await;
    assert_eq!(
        moved.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::NotFound
    );

    // Tenant B cannot mutate tenant A's unit metadata.
    let renamed = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: tenant_b(),
            unit_id: unit_id(),
            name: Some("Hijacked".to_string()),
            unit_type: None,
            status: None,
            expected_version: 1,
            audit: mutation_context(),
            idempotency_key: None,
            now: ts(130),
        })
        .await;
    assert_eq!(
        renamed.err().unwrap_or_else(|| unreachable!()),
        OrganizationStoreError::NotFound
    );
}
