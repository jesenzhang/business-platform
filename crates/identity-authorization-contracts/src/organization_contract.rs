//! Behavior contract for the organization adapters
//! (`organization-postgres` and `organization-sqlite` run the same suite).

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use organization::domain::{
    OrganizationMembership, OrganizationMembershipStatus, OrganizationMembershipType,
    OrganizationUnitStatus, OrganizationUnitType, MAX_UNIT_DEPTH,
};
use organization::ports::{
    AddMemberCommit, CreateUnitCommit, MoveUnitCommit, MutationActorKind, MutationContext,
    OrganizationCommandPort, OrganizationQueryPort, OrganizationStoreError, RemoveMemberCommit,
    UpdateUnitCommit,
};

use crate::{check, second_now};

/// All organization ports under test.
pub struct OrganizationContractPorts {
    /// Command (unit/membership mutation) port.
    pub command: Arc<dyn OrganizationCommandPort>,
    /// Read-only query port.
    pub query: Arc<dyn OrganizationQueryPort>,
}

/// Run the full organization contract against one adapter set.
pub async fn verify_organization_contract(ports: &OrganizationContractPorts) -> Result<(), String> {
    let cx = Contract::new();
    verify_unit_creation(&cx, ports).await?;
    verify_unit_update_and_move(&cx, ports).await?;
    verify_tree_safety(&cx, ports).await?;
    verify_members(&cx, ports).await?;
    verify_listings(&cx, ports).await?;
    Ok(())
}

struct Contract {
    actor: Uuid,
    tenant_a: Uuid,
    tenant_b: Uuid,
    base: DateTime<Utc>,
}

impl Contract {
    fn new() -> Self {
        Self {
            actor: Uuid::now_v7(),
            tenant_a: Uuid::now_v7(),
            tenant_b: Uuid::now_v7(),
            // Whole-second precision keeps full-record replay equality
            // lossless on `PostgreSQL` (microsecond truncation).
            base: second_now(),
        }
    }

    fn at(&self, offset: i64) -> DateTime<Utc> {
        self.base + TimeDelta::seconds(offset)
    }

    fn mutation(&self) -> MutationContext {
        MutationContext {
            actor_id: self.actor.to_string(),
            actor_kind: MutationActorKind::User,
            operation_id: Uuid::now_v7(),
            trace_id: Some("contract-trace".to_string()),
            reason: Some("PLAN-0013 contract suite".to_string()),
        }
    }

    fn create_unit(
        &self,
        tenant: Uuid,
        unit_id: Option<Uuid>,
        parent: Option<Uuid>,
        name: &str,
        key: Option<&str>,
        offset: i64,
    ) -> CreateUnitCommit {
        CreateUnitCommit {
            tenant_id: tenant,
            unit_id,
            parent_id: parent,
            unit_type: OrganizationUnitType::Department,
            name: name.to_string(),
            audit: self.mutation(),
            idempotency_key: key.map(str::to_string),
            now: self.at(offset),
        }
    }
}

async fn verify_unit_creation(
    cx: &Contract,
    ports: &OrganizationContractPorts,
) -> Result<(), String> {
    let root = ports
        .command
        .create_unit(cx.create_unit(cx.tenant_a, None, None, "HQ", None, 0))
        .await
        .map_err(|error| format!("create root: {error}"))?;
    check(
        !root.replayed
            && root.unit.status() == OrganizationUnitStatus::Active
            && root.unit.version().value() == 1
            && root.unit.parent_id().is_none(),
        "fresh root unit is Active at v1 without a parent",
    )?;

    // Caller-chosen ids are honoured; reusing the id fails with AlreadyExists.
    let chosen = Uuid::now_v7();
    let named = ports
        .command
        .create_unit(cx.create_unit(
            cx.tenant_a,
            Some(chosen),
            Some(root.unit.unit_id()),
            "Platform",
            None,
            1,
        ))
        .await
        .map_err(|error| format!("create named unit: {error}"))?;
    check(
        named.unit.unit_id() == chosen,
        "caller-chosen unit id must be used verbatim",
    )?;
    check(
        ports
            .command
            .create_unit(cx.create_unit(cx.tenant_a, Some(chosen), None, "Other", None, 2))
            .await
            == Err(OrganizationStoreError::AlreadyExists),
        "reusing an existing unit id must be AlreadyExists",
    )?;

    // Same key + same payload converges; a payload change conflicts.
    let keyed = cx.create_unit(cx.tenant_a, None, None, "Keyed", Some("unit-key"), 3);
    let created = ports
        .command
        .create_unit(keyed.clone())
        .await
        .map_err(|error| format!("keyed create: {error}"))?;
    let replay = ports
        .command
        .create_unit(keyed.clone())
        .await
        .map_err(|error| format!("keyed replay: {error}"))?;
    check(
        replay.replayed && replay.unit == created.unit,
        "same key + payload must replay the stored unit",
    )?;
    let conflict = CreateUnitCommit {
        name: "Renamed".to_string(),
        ..keyed
    };
    check(
        ports.command.create_unit(conflict).await
            == Err(OrganizationStoreError::IdempotencyConflict),
        "same key + different payload must be IdempotencyConflict",
    )?;

    // Parent placement rules.
    let foreign = ports
        .command
        .create_unit(cx.create_unit(cx.tenant_b, None, None, "Foreign", None, 4))
        .await
        .map_err(|error| format!("foreign root: {error}"))?
        .unit;
    check(
        ports
            .command
            .create_unit(cx.create_unit(
                cx.tenant_a,
                None,
                Some(foreign.unit_id()),
                "CrossTenant",
                None,
                5,
            ))
            .await
            == Err(OrganizationStoreError::InvalidParent),
        "a parent from another tenant must be InvalidParent",
    )?;
    check(
        ports
            .command
            .create_unit(cx.create_unit(
                cx.tenant_a,
                None,
                Some(Uuid::now_v7()),
                "UnknownParent",
                None,
                6,
            ))
            .await
            == Err(OrganizationStoreError::InvalidParent),
        "a nonexistent parent must be InvalidParent",
    )?;
    let self_id = Uuid::now_v7();
    check(
        ports
            .command
            .create_unit(cx.create_unit(
                cx.tenant_a,
                Some(self_id),
                Some(self_id),
                "SelfParent",
                None,
                7,
            ))
            .await
            == Err(OrganizationStoreError::InvalidParent),
        "self-parenthood must be InvalidParent",
    )?;
    let retired = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: cx.tenant_a,
            unit_id: chosen,
            name: None,
            unit_type: None,
            status: Some(OrganizationUnitStatus::Disabled),
            expected_version: 1,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(8),
        })
        .await
        .map_err(|error| format!("disable unit: {error}"))?;
    check(
        !retired.replayed && retired.unit.status() == OrganizationUnitStatus::Disabled,
        "disabling a unit must bump its version",
    )?;
    check(
        ports
            .command
            .create_unit(cx.create_unit(cx.tenant_a, None, Some(chosen), "UnderDisabled", None, 9))
            .await
            == Err(OrganizationStoreError::InvalidParent),
        "a disabled parent must be InvalidParent",
    )?;
    Ok(())
}

async fn verify_unit_update_and_move(
    cx: &Contract,
    ports: &OrganizationContractPorts,
) -> Result<(), String> {
    let unit = ports
        .command
        .create_unit(cx.create_unit(cx.tenant_a, None, None, "Update Me", None, 10))
        .await
        .map_err(|error| format!("update fixture: {error}"))?
        .unit;

    let update = UpdateUnitCommit {
        tenant_id: cx.tenant_a,
        unit_id: unit.unit_id(),
        name: Some("Renamed".to_string()),
        unit_type: Some(OrganizationUnitType::Team),
        status: None,
        expected_version: 1,
        audit: cx.mutation(),
        idempotency_key: Some("update-key".to_string()),
        now: cx.at(11),
    };
    let updated = ports
        .command
        .update_unit(update.clone())
        .await
        .map_err(|error| format!("rename: {error}"))?;
    check(
        !updated.replayed
            && updated.unit.version().value() == 2
            && updated.unit.name() == "Renamed",
        "rename bumps to v2",
    )?;

    // Same key + payload replays; a same-valued commit converges without a
    // version bump or audit record.
    let replay = ports
        .command
        .update_unit(update)
        .await
        .map_err(|error| format!("rename replay: {error}"))?;
    check(
        replay.replayed && replay.unit == updated.unit,
        "same key + payload must replay",
    )?;
    let converged = ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: cx.tenant_a,
            unit_id: unit.unit_id(),
            name: Some("Renamed".to_string()),
            unit_type: Some(OrganizationUnitType::Team),
            status: None,
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(12),
        })
        .await
        .map_err(|error| format!("same-value update: {error}"))?;
    check(
        converged.replayed && converged.unit.version().value() == 2,
        "a same-valued update must converge with no version bump",
    )?;
    check(
        ports
            .command
            .update_unit(UpdateUnitCommit {
                tenant_id: cx.tenant_b,
                unit_id: unit.unit_id(),
                name: Some("Cross".to_string()),
                unit_type: None,
                status: None,
                expected_version: 2,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(13),
            })
            .await
            == Err(OrganizationStoreError::NotFound),
        "cross-tenant update must be NotFound",
    )?;
    check(
        ports
            .command
            .update_unit(UpdateUnitCommit {
                tenant_id: cx.tenant_a,
                unit_id: unit.unit_id(),
                name: Some("Stale".to_string()),
                unit_type: None,
                status: None,
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(14),
            })
            .await
            == Err(OrganizationStoreError::VersionConflict),
        "stale expected_version must be VersionConflict",
    )?;

    // Move: reparent under a new root.
    let new_root = ports
        .command
        .create_unit(cx.create_unit(cx.tenant_a, None, None, "New Root", None, 15))
        .await
        .map_err(|error| format!("move target: {error}"))?
        .unit;
    let moved = ports
        .command
        .move_unit(MoveUnitCommit {
            tenant_id: cx.tenant_a,
            unit_id: unit.unit_id(),
            new_parent_id: Some(new_root.unit_id()),
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(16),
        })
        .await
        .map_err(|error| format!("move: {error}"))?;
    check(
        moved.unit.parent_id() == Some(new_root.unit_id()) && moved.unit.version().value() == 3,
        "move reparents and bumps the version",
    )?;

    // Moving under a unit from another tenant is InvalidParent; a move that
    // would create a cycle is Cycle; stale versions fail closed.
    let foreign_unit = ports
        .query
        .list_units(cx.tenant_b)
        .await
        .map_err(|error| format!("list foreign: {error}"))?
        .into_iter()
        .next()
        .ok_or("tenant B fixture unit missing")?;
    check(
        ports
            .command
            .move_unit(MoveUnitCommit {
                tenant_id: cx.tenant_a,
                unit_id: unit.unit_id(),
                new_parent_id: Some(foreign_unit.unit_id()),
                expected_version: 3,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(17),
            })
            .await
            == Err(OrganizationStoreError::InvalidParent),
        "moving under another tenant's unit must be InvalidParent",
    )?;
    check(
        ports
            .command
            .move_unit(MoveUnitCommit {
                tenant_id: cx.tenant_a,
                unit_id: new_root.unit_id(),
                new_parent_id: Some(unit.unit_id()),
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(18),
            })
            .await
            == Err(OrganizationStoreError::Cycle),
        "creating a parent cycle must be Cycle",
    )?;
    check(
        ports
            .command
            .move_unit(MoveUnitCommit {
                tenant_id: cx.tenant_a,
                unit_id: unit.unit_id(),
                new_parent_id: None,
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(19),
            })
            .await
            == Err(OrganizationStoreError::VersionConflict),
        "stale move must be VersionConflict",
    )?;
    Ok(())
}

async fn verify_tree_safety(
    cx: &Contract,
    ports: &OrganizationContractPorts,
) -> Result<(), String> {
    // A chain of `MAX_UNIT_DEPTH + 1` units is legal; one deeper fails
    // closed with Cycle (bounded traversal, same bound the domain walks).
    let tenant = Uuid::now_v7();
    let mut parent: Option<Uuid> = None;
    for depth in 0..=i64::try_from(MAX_UNIT_DEPTH).map_err(|_| "depth bound overflow")? {
        let created = ports
            .command
            .create_unit(cx.create_unit(
                tenant,
                None,
                parent,
                &format!("L{depth}"),
                None,
                100 + depth,
            ))
            .await
            .map_err(|error| format!("chain depth {depth}: {error}"))?;
        parent = Some(created.unit.unit_id());
    }
    check(
        ports
            .command
            .create_unit(cx.create_unit(tenant, None, parent, "TooDeep", None, 200))
            .await
            == Err(OrganizationStoreError::Cycle),
        "exceeding the tree depth bound must be Cycle",
    )?;
    Ok(())
}

async fn verify_members(cx: &Contract, ports: &OrganizationContractPorts) -> Result<(), String> {
    let unit = ports
        .command
        .create_unit(cx.create_unit(cx.tenant_a, None, None, "Staffed", None, 300))
        .await
        .map_err(|error| format!("member fixture: {error}"))?
        .unit;
    let user = Uuid::now_v7();
    let leader = Uuid::now_v7();

    let add = AddMemberCommit {
        tenant_id: cx.tenant_a,
        unit_id: unit.unit_id(),
        user_id: user,
        membership_type: OrganizationMembershipType::Member,
        audit: cx.mutation(),
        idempotency_key: Some("add-member-key".to_string()),
        now: cx.at(301),
    };
    let added = ports
        .command
        .add_member(add.clone())
        .await
        .map_err(|error| format!("add member: {error}"))?;
    check(
        !added.replayed && added.membership.is_active() && added.membership.version().value() == 1,
        "a new membership is Active at v1",
    )?;
    check(
        added.membership.joined_at() == add.now,
        "joined_at must be the commit timestamp",
    )?;
    let replay = ports
        .command
        .add_member(add)
        .await
        .map_err(|error| format!("add member replay: {error}"))?;
    check(
        replay.replayed && replay.membership == added.membership,
        "same key + payload must replay",
    )?;
    check(
        ports
            .command
            .add_member(AddMemberCommit {
                idempotency_key: None,
                ..duplicate_payload(cx, unit.unit_id(), user)
            })
            .await
            == Err(OrganizationStoreError::AlreadyExists),
        "adding an already-active member must be AlreadyExists",
    )?;

    // The (unit, user, type) triple is the uniqueness key: a second leader
    // row for the same user coexists with the member row.
    let leader_add = ports
        .command
        .add_member(AddMemberCommit {
            tenant_id: cx.tenant_a,
            unit_id: unit.unit_id(),
            user_id: user,
            membership_type: OrganizationMembershipType::Leader,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(302),
        })
        .await
        .map_err(|error| format!("add leader: {error}"))?;
    check(
        !leader_add.replayed
            && leader_add.membership.membership_type() == OrganizationMembershipType::Leader,
        "membership_type is part of the uniqueness key",
    )?;

    // Disabled units accept no new members.
    let disabled = ports
        .command
        .create_unit(cx.create_unit(cx.tenant_a, None, None, "Closed", None, 303))
        .await
        .map_err(|error| format!("disabled unit fixture: {error}"))?
        .unit;
    ports
        .command
        .update_unit(UpdateUnitCommit {
            tenant_id: cx.tenant_a,
            unit_id: disabled.unit_id(),
            name: None,
            unit_type: None,
            status: Some(OrganizationUnitStatus::Disabled),
            expected_version: 1,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(304),
        })
        .await
        .map_err(|error| format!("disable unit: {error}"))?;
    check(
        ports
            .command
            .add_member(AddMemberCommit {
                tenant_id: cx.tenant_a,
                unit_id: disabled.unit_id(),
                user_id: leader,
                membership_type: OrganizationMembershipType::Member,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(305),
            })
            .await
            == Err(OrganizationStoreError::UnitDisabled),
        "a disabled unit must refuse new members with UnitDisabled",
    )?;

    // Removal: deactivate bumps; already-inactive converges; stale fails
    // closed; re-add reactivates (new version, replayed = false).
    let remove = RemoveMemberCommit {
        tenant_id: cx.tenant_a,
        unit_id: unit.unit_id(),
        user_id: user,
        membership_type: OrganizationMembershipType::Member,
        expected_version: 1,
        audit: cx.mutation(),
        idempotency_key: Some("remove-key".to_string()),
        now: cx.at(306),
    };
    let removed = ports
        .command
        .remove_member(remove.clone())
        .await
        .map_err(|error| format!("remove: {error}"))?;
    check(
        removed.membership.status() == OrganizationMembershipStatus::Inactive
            && removed.membership.version().value() == 2
            && removed.membership.deactivated_at().is_some(),
        "removal deactivates and stamps deactivated_at",
    )?;
    let remove_replay = ports
        .command
        .remove_member(remove)
        .await
        .map_err(|error| format!("remove replay: {error}"))?;
    check(
        remove_replay.replayed && remove_replay.membership == removed.membership,
        "same key + payload must replay",
    )?;
    let converged = ports
        .command
        .remove_member(RemoveMemberCommit {
            tenant_id: cx.tenant_a,
            unit_id: unit.unit_id(),
            user_id: user,
            membership_type: OrganizationMembershipType::Member,
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(307),
        })
        .await
        .map_err(|error| format!("remove convergence: {error}"))?;
    check(
        converged.replayed && converged.membership.version().value() == 2,
        "removing an already-inactive member converges with no bump",
    )?;
    check(
        ports
            .command
            .remove_member(RemoveMemberCommit {
                tenant_id: cx.tenant_a,
                unit_id: unit.unit_id(),
                user_id: user,
                membership_type: OrganizationMembershipType::Member,
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(308),
            })
            .await
            == Err(OrganizationStoreError::VersionConflict),
        "stale removal must fail closed as VersionConflict",
    )?;
    let readded = ports
        .command
        .add_member(AddMemberCommit {
            tenant_id: cx.tenant_a,
            unit_id: unit.unit_id(),
            user_id: user,
            membership_type: OrganizationMembershipType::Member,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(309),
        })
        .await
        .map_err(|error| format!("re-add: {error}"))?;
    check(
        !readded.replayed
            && readded.membership.is_active()
            && readded.membership.version().value() == 3,
        "re-adding an inactive membership reactivates the same row",
    )?;
    check(
        ports
            .command
            .add_member(AddMemberCommit {
                tenant_id: cx.tenant_a,
                unit_id: unit.unit_id(),
                user_id: Uuid::now_v7(),
                membership_type: OrganizationMembershipType::Member,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(310),
            })
            .await
            .is_ok(),
        "attaching a platform user id is not existence-validated by org",
    )?;
    check(
        ports
            .command
            .remove_member(RemoveMemberCommit {
                tenant_id: cx.tenant_a,
                unit_id: unit.unit_id(),
                user_id: Uuid::now_v7(),
                membership_type: OrganizationMembershipType::Member,
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(311),
            })
            .await
            == Err(OrganizationStoreError::NotFound),
        "removing an unknown membership must be NotFound",
    )?;
    Ok(())
}

fn duplicate_payload(cx: &Contract, unit: Uuid, user: Uuid) -> AddMemberCommit {
    AddMemberCommit {
        tenant_id: cx.tenant_a,
        unit_id: unit,
        user_id: user,
        membership_type: OrganizationMembershipType::Member,
        audit: cx.mutation(),
        idempotency_key: None,
        now: cx.at(302),
    }
}

async fn verify_listings(cx: &Contract, ports: &OrganizationContractPorts) -> Result<(), String> {
    let tenant = Uuid::now_v7();
    let root = ports
        .command
        .create_unit(cx.create_unit(tenant, None, None, "Tree Root", None, 400))
        .await
        .map_err(|error| format!("listing fixture: {error}"))?
        .unit;
    let child = ports
        .command
        .create_unit(cx.create_unit(tenant, None, Some(root.unit_id()), "Child", None, 401))
        .await
        .map_err(|error| format!("listing child: {error}"))?
        .unit;

    let units = ports
        .query
        .list_units(tenant)
        .await
        .map_err(|error| format!("list units: {error}"))?;
    check(
        units.len() == 2,
        "list_units returns exactly the tenant units",
    )?;
    check(
        ports
            .query
            .list_units(cx.tenant_b)
            .await
            .map_err(|error| format!("list units isolation: {error}"))?
            .iter()
            .all(|unit| unit.tenant_id() == cx.tenant_b),
        "tenant isolation: list_units never leaks other tenants",
    )?;

    // Membership listings: only active rows, join-ordered.
    let user_a = Uuid::now_v7();
    let user_b = Uuid::now_v7();
    for (user, offset) in [(user_a, 402_i64), (user_b, 403)] {
        ports
            .command
            .add_member(AddMemberCommit {
                tenant_id: tenant,
                unit_id: child.unit_id(),
                user_id: user,
                membership_type: OrganizationMembershipType::Member,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(offset),
            })
            .await
            .map_err(|error| format!("listing member: {error}"))?;
    }
    ports
        .command
        .remove_member(RemoveMemberCommit {
            tenant_id: tenant,
            unit_id: child.unit_id(),
            user_id: user_a,
            membership_type: OrganizationMembershipType::Member,
            expected_version: 1,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(404),
        })
        .await
        .map_err(|error| format!("listing remove: {error}"))?;

    let members = ports
        .query
        .list_unit_members(tenant, child.unit_id())
        .await
        .map_err(|error| format!("list members: {error}"))?;
    check(
        members.len() == 1 && members[0].membership.user_id() == user_b,
        "unit member listings exclude inactive rows",
    )?;
    let user_memberships: Vec<OrganizationMembership> = ports
        .query
        .list_user_memberships(tenant, user_b)
        .await
        .map_err(|error| format!("list user memberships: {error}"))?;
    check(
        user_memberships.len() == 1
            && user_memberships[0].unit_id() == child.unit_id()
            && user_memberships[0].is_active(),
        "user membership listing surfaces active rows for policy scoping",
    )?;
    check(
        ports
            .query
            .list_user_memberships(tenant, user_a)
            .await
            .map_err(|error| format!("list user memberships (inactive): {error}"))?
            .is_empty(),
        "removed memberships disappear from the user listing",
    )?;

    let fetched = ports
        .query
        .get_unit(tenant, root.unit_id())
        .await
        .map_err(|error| format!("get unit: {error}"))?;
    check(
        fetched.is_some_and(|unit| unit.tenant_id() == tenant),
        "get_unit returns tenant-visible units",
    )?;
    check(
        ports
            .query
            .get_unit(cx.tenant_b, root.unit_id())
            .await
            .map_err(|error| format!("get unit cross-tenant: {error}"))?
            .is_none(),
        "tenant isolation: get_unit never crosses tenants",
    )?;
    Ok(())
}
