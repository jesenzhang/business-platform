//! Port contract semantics exercised against the in-memory fakes.
//!
//! These are the *same* behavioural expectations the PostgreSQL/SQLite
//! adapters must pass in the shared contract suite (PLAN-0013 WP-02/05):
//! tenant isolation, poison mapping, optimistic versioning, idempotency
//! conflict scoping, and audit-once mutation semantics.

#![cfg(feature = "testing")]

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use organization::application::{
    AddOrganizationMember, AddOrganizationMemberCommand, RemoveOrganizationMember,
    RemoveOrganizationMemberCommand,
};
use organization::domain::{
    OrganizationMembershipType, OrganizationUnit, OrganizationUnitStatus, OrganizationUnitType,
};
use organization::ports::{OrganizationStoreError, UpdateUnitCommit};
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
    OrganizationUnit::create(
        unit_id(),
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

    // Same key, same fingerprint ⇒ replay onto the stored row.
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
        organization::application::OrganizationApplicationError::UnitDisabled
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
        organization::application::OrganizationApplicationError::NotFound
    );
}

#[tokio::test]
async fn cross_tenant_move_and_add_are_not_expressible() {
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
            organization::application::OrganizationApplicationError::NotTenantMember
                | organization::application::OrganizationApplicationError::NotFound
        )
    ));
}
