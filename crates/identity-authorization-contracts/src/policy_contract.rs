//! Behavior contract for the policy adapters (`policy-postgres` and
//! `policy-sqlite` run the same suite). The bulk caps (per-user binding,
//! tenant role budget) are part of the store contract and run here; the
//! 2 000-unit / 5 000-binding listing scales are exercised per-adapter in
//! the `SQLite` test targets and documented as such for `PostgreSQL`.

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use policy::application::{MAX_BINDINGS_PER_USER, MAX_ROLES_PER_TENANT};
use policy::domain::{PermissionKey, ResourceScope, RoleBinding, RoleStatus};
use policy::ports::{
    BindRoleCommit, CreateRoleCommit, MutationActorKind, MutationContext, PolicyCommandPort,
    PolicyQueryPort, PolicyStoreError, RevokeBindingCommit, SetRolePermissionsCommit,
    UpdateRoleCommit,
};

use crate::{check, second_now};

/// Stable `UUIDv5` ids of the two migration-seeded system roles
/// (`policy-system-role:<stable-key>` in the URL namespace).
#[must_use]
pub fn system_role_id(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("policy-system-role:{key}").as_bytes(),
    )
}

/// All policy ports under test.
pub struct PolicyContractPorts {
    /// Command (role/binding mutation) port.
    pub command: Arc<dyn PolicyCommandPort>,
    /// Read-only query port.
    pub query: Arc<dyn PolicyQueryPort>,
}

/// Run the full policy contract against one adapter set.
pub async fn verify_policy_contract(ports: &PolicyContractPorts) -> Result<(), String> {
    let cx = Contract::new();
    verify_catalog_and_system_roles(&cx, ports).await?;
    verify_roles(&cx, ports).await?;
    verify_permissions(&cx, ports).await?;
    verify_bindings(&cx, ports).await?;
    verify_role_cap(ports).await?;
    verify_user_binding_cap(ports).await?;
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

    fn create_role(
        &self,
        tenant: Uuid,
        role_id: Option<Uuid>,
        key: &str,
        display: &str,
        idem: Option<&str>,
        offset: i64,
    ) -> CreateRoleCommit {
        CreateRoleCommit {
            tenant_id: tenant,
            role_id,
            stable_key: key.to_string(),
            display_name: display.to_string(),
            audit: self.mutation(),
            idempotency_key: idem.map(str::to_string),
            now: self.at(offset),
        }
    }

    fn bind(
        &self,
        tenant: Uuid,
        user: Uuid,
        role: Uuid,
        scope: ResourceScope,
        effective_offset: i64,
        expires_offset: Option<i64>,
        idem: Option<&str>,
    ) -> BindRoleCommit {
        BindRoleCommit {
            tenant_id: tenant,
            binding_id: None,
            user_id: user,
            role_id: role,
            scope,
            effective_at: self.at(effective_offset),
            expires_at: expires_offset.map(|offset| self.at(offset)),
            audit: self.mutation(),
            idempotency_key: idem.map(str::to_string),
            now: self.at(effective_offset),
        }
    }
}

async fn verify_catalog_and_system_roles(
    cx: &Contract,
    ports: &PolicyContractPorts,
) -> Result<(), String> {
    let active = ports
        .query
        .get_permission(&permission_key("audit.read"))
        .await
        .map_err(|error| format!("get audit.read: {error}"))?;
    check(
        active.is_some_and(|entry| entry.is_active() && !entry.is_reserved()),
        "seeded governance key must be an active catalog entry",
    )?;
    let unknown = ports
        .query
        .get_permission(&permission_key("does.not.exist"))
        .await
        .map_err(|error| format!("get unknown: {error}"))?;
    check(
        unknown.is_none(),
        "unknown keys must be absent from the catalog",
    )?;
    let catalog = ports
        .query
        .list_permissions()
        .await
        .map_err(|error| format!("list permissions: {error}"))?;
    check(
        catalog.len() >= 24
            && catalog
                .iter()
                .all(policy::domain::PermissionDefinition::is_active),
        "seeded catalog must contain the full built-in catalog, all active",
    )?;

    // System roles are migration-seeded, tenant-visible, and immutable at
    // every store mutation surface.
    let bootstrap_role = system_role_id("system.bootstrap-admin");
    let role = ports
        .query
        .get_role(cx.tenant_a, bootstrap_role)
        .await
        .map_err(|error| format!("get system role: {error}"))?
        .ok_or("system role must be seeded and tenant-visible")?;
    check(
        role.is_system() && role.is_active(),
        "system role seeded active",
    )?;
    let grants = ports
        .query
        .get_role_permissions(cx.tenant_a, bootstrap_role)
        .await
        .map_err(|error| format!("system grants: {error}"))?;
    check(
        grants.contains(&"audit.read".to_string())
            && grants.contains(&"policy.role.manage".to_string())
            && !grants.contains(&"contract.read".to_string()),
        "system roles grant the non-reserved catalog keys only",
    )?;
    check(
        ports
            .command
            .update_role(UpdateRoleCommit {
                tenant_id: cx.tenant_a,
                role_id: bootstrap_role,
                display_name: Some("Hijacked".to_string()),
                status: None,
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(0),
            })
            .await
            == Err(PolicyStoreError::RoleImmutable),
        "system roles must reject metadata updates",
    )?;
    check(
        ports
            .command
            .set_role_permissions(SetRolePermissionsCommit {
                tenant_id: cx.tenant_a,
                role_id: bootstrap_role,
                permission_keys: vec![],
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(0),
            })
            .await
            == Err(PolicyStoreError::RoleImmutable),
        "system roles must reject permission replacement",
    )?;
    Ok(())
}

fn permission_key(raw: &str) -> PermissionKey {
    // Every literal in this suite is grammar-valid; a parse failure here
    // would be a bug in the suite itself.
    PermissionKey::parse(raw).unwrap_or_else(|_| unreachable!())
}

async fn verify_roles(cx: &Contract, ports: &PolicyContractPorts) -> Result<(), String> {
    let created = ports
        .command
        .create_role(cx.create_role(cx.tenant_a, None, "editor", "Editor", None, 10))
        .await
        .map_err(|error| format!("create role: {error}"))?;
    check(
        !created.replayed
            && created.role.version().value() == 1
            && created.role.tenant_id() == Some(cx.tenant_a),
        "fresh tenant role is Active at v1",
    )?;
    let role_id = created.role.role_id();

    // Key uniqueness is per tenant; role ids are globally unique.
    check(
        ports
            .command
            .create_role(cx.create_role(cx.tenant_a, None, "editor", "Editor 2", None, 11))
            .await
            == Err(PolicyStoreError::AlreadyExists),
        "duplicate tenant stable key must be AlreadyExists",
    )?;
    check(
        ports
            .command
            .create_role(cx.create_role(cx.tenant_b, Some(role_id), "other-key", "Other", None, 12))
            .await
            == Err(PolicyStoreError::AlreadyExists),
        "reusing an existing role id must be AlreadyExists",
    )?;
    // The same key in another tenant is a different role.
    let other = ports
        .command
        .create_role(cx.create_role(cx.tenant_b, None, "editor", "Editor B", None, 13))
        .await
        .map_err(|error| format!("create role tenant B: {error}"))?;
    check(
        !other.replayed && other.role.role_id() != role_id,
        "stable keys are unique per tenant, not globally",
    )?;

    // Idempotent create convergence.
    let keyed = cx.create_role(
        cx.tenant_a,
        None,
        "keyed",
        "Keyed",
        Some("create-role-key"),
        14,
    );
    let first = ports
        .command
        .create_role(keyed.clone())
        .await
        .map_err(|error| format!("keyed create: {error}"))?;
    let replay = ports
        .command
        .create_role(keyed)
        .await
        .map_err(|error| format!("keyed replay: {error}"))?;
    check(
        replay.replayed && replay.role == first.role,
        "same key + payload must replay the stored role",
    )?;

    // Visibility: own + system only.
    check(
        ports
            .query
            .get_role(cx.tenant_b, role_id)
            .await
            .map_err(|error| format!("role visibility: {error}"))?
            .is_none(),
        "tenant roles are invisible to other tenants",
    )?;
    let roles = ports
        .query
        .list_roles(cx.tenant_a)
        .await
        .map_err(|error| format!("list roles: {error}"))?;
    check(
        roles.iter().any(policy::domain::RoleDefinition::is_system)
            && roles.iter().any(|role| role.role_id() == role_id),
        "list_roles surfaces own + system roles",
    )?;
    let keys: Vec<&str> = roles
        .iter()
        .map(policy::domain::RoleDefinition::stable_key)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    check(keys == sorted, "list_roles must be ordered by stable key")?;

    // Update: bump, same-value convergence, stale version.
    let updated = ports
        .command
        .update_role(UpdateRoleCommit {
            tenant_id: cx.tenant_a,
            role_id,
            display_name: Some("Renamed".to_string()),
            status: None,
            expected_version: 1,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(15),
        })
        .await
        .map_err(|error| format!("rename role: {error}"))?;
    check(
        !updated.replayed && updated.role.version().value() == 2,
        "rename bumps the role version",
    )?;
    let converged = ports
        .command
        .update_role(UpdateRoleCommit {
            tenant_id: cx.tenant_a,
            role_id,
            display_name: Some("Renamed".to_string()),
            status: None,
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(16),
        })
        .await
        .map_err(|error| format!("same-value update: {error}"))?;
    check(
        converged.replayed && converged.role.version().value() == 2,
        "same-valued update converges with no bump",
    )?;
    check(
        ports
            .command
            .update_role(UpdateRoleCommit {
                tenant_id: cx.tenant_a,
                role_id,
                display_name: Some("Stale".to_string()),
                status: None,
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(17),
            })
            .await
            == Err(PolicyStoreError::VersionConflict),
        "stale role version must be VersionConflict",
    )?;
    let disabled = ports
        .command
        .update_role(UpdateRoleCommit {
            tenant_id: cx.tenant_a,
            role_id,
            display_name: None,
            status: Some(RoleStatus::Disabled),
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(18),
        })
        .await
        .map_err(|error| format!("disable role: {error}"))?;
    check(
        !disabled.replayed && !disabled.role.is_active() && disabled.role.version().value() == 3,
        "roles can be disabled through metadata updates",
    )?;
    Ok(())
}

async fn verify_permissions(cx: &Contract, ports: &PolicyContractPorts) -> Result<(), String> {
    let role = ports
        .command
        .create_role(cx.create_role(cx.tenant_a, None, "perms", "Perms", None, 20))
        .await
        .map_err(|error| format!("perms role: {error}"))?
        .role;

    let set = SetRolePermissionsCommit {
        tenant_id: cx.tenant_a,
        role_id: role.role_id(),
        permission_keys: vec![
            "document.read".to_string(),
            "audit.read".to_string(),
            "document.read".to_string(),
        ],
        expected_version: 1,
        audit: cx.mutation(),
        idempotency_key: Some("set-perms-key".to_string()),
        now: cx.at(21),
    };
    let applied = ports
        .command
        .set_role_permissions(set.clone())
        .await
        .map_err(|error| format!("set permissions: {error}"))?;
    check(
        !applied.replayed
            && applied.permission_keys
                == vec!["audit.read".to_string(), "document.read".to_string()]
            && applied.role.version().value() == 2,
        "set replace dedupes, sorts, and bumps the role version",
    )?;
    let stored = ports
        .query
        .get_role_permissions(cx.tenant_a, role.role_id())
        .await
        .map_err(|error| format!("stored grants: {error}"))?;
    check(
        stored == applied.permission_keys,
        "get_role_permissions reflects the replaced set",
    )?;

    // Same key + payload replays the stored outcome.
    let replay = ports
        .command
        .set_role_permissions(set)
        .await
        .map_err(|error| format!("set replay: {error}"))?;
    check(
        replay.replayed && replay.permission_keys == applied.permission_keys,
        "same key + payload must replay",
    )?;
    // Same (unsorted) set converges with no version bump.
    let same_set = ports
        .command
        .set_role_permissions(SetRolePermissionsCommit {
            tenant_id: cx.tenant_a,
            role_id: role.role_id(),
            permission_keys: vec!["document.read".to_string(), "audit.read".to_string()],
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(22),
        })
        .await
        .map_err(|error| format!("same set: {error}"))?;
    check(
        same_set.replayed && same_set.role.version().value() == 2,
        "an unchanged permission set converges with no version bump",
    )?;
    check(
        ports
            .command
            .set_role_permissions(SetRolePermissionsCommit {
                tenant_id: cx.tenant_a,
                role_id: role.role_id(),
                permission_keys: vec!["not.a.catalog.key".to_string()],
                expected_version: 2,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(23),
            })
            .await
            == Err(PolicyStoreError::UnknownPermission),
        "keys outside the catalog must be UnknownPermission",
    )?;
    check(
        ports
            .command
            .set_role_permissions(SetRolePermissionsCommit {
                tenant_id: cx.tenant_a,
                role_id: role.role_id(),
                permission_keys: vec!["audit.read".to_string()],
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(24),
            })
            .await
            == Err(PolicyStoreError::VersionConflict),
        "version check precedes the set replace",
    )?;
    check(
        ports
            .command
            .set_role_permissions(SetRolePermissionsCommit {
                tenant_id: cx.tenant_b,
                role_id: role.role_id(),
                permission_keys: vec![],
                expected_version: 2,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(25),
            })
            .await
            == Err(PolicyStoreError::NotFound),
        "set permissions is tenant-scoped",
    )?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn verify_bindings(cx: &Contract, ports: &PolicyContractPorts) -> Result<(), String> {
    let role = ports
        .command
        .create_role(cx.create_role(cx.tenant_a, None, "binder", "Binder", None, 30))
        .await
        .map_err(|error| format!("binding fixture role: {error}"))?
        .role;
    let user = Uuid::now_v7();

    let bind = cx.bind(
        cx.tenant_a,
        user,
        role.role_id(),
        ResourceScope::Tenant,
        31,
        None,
        Some("bind-key"),
    );
    let bound = ports
        .command
        .bind_role(bind.clone())
        .await
        .map_err(|error| format!("bind: {error}"))?;
    check(
        !bound.replayed && bound.binding.is_active() && bound.binding.version().value() == 1,
        "fresh binding is Active at v1",
    )?;

    // Identical active binding converges (replay via key, and without a key
    // the identical row must not be duplicated).
    let replay = ports
        .command
        .bind_role(bind.clone())
        .await
        .map_err(|error| format!("bind replay: {error}"))?;
    check(
        replay.replayed && replay.binding == bound.binding,
        "same key + payload must replay",
    )?;
    let duplicate_active = ports
        .command
        .bind_role(BindRoleCommit {
            idempotency_key: None,
            ..bind.clone()
        })
        .await
        .map_err(|error| format!("identical active bind: {error}"))?;
    check(
        duplicate_active.replayed
            && duplicate_active.binding.binding_id() == bound.binding.binding_id(),
        "an identical active binding converges without a new row",
    )?;

    // A different validity window is a distinct binding (legal by design).
    let wider = cx.bind(
        cx.tenant_a,
        user,
        role.role_id(),
        ResourceScope::Tenant,
        40,
        Some(100),
        None,
    );
    let second = ports
        .command
        .bind_role(wider)
        .await
        .map_err(|error| format!("second window bind: {error}"))?;
    check(
        !second.replayed && second.binding.binding_id() != bound.binding.binding_id(),
        "a distinct validity window creates a distinct row",
    )?;

    // Scope round-trips exactly for every variant.
    let org_unit = Uuid::now_v7();
    let scoped = cx.bind(
        cx.tenant_a,
        user,
        role.role_id(),
        ResourceScope::organization_unit(org_unit, true).unwrap_or_else(|_| unreachable!()),
        41,
        None,
        None,
    );
    let scoped_bound = ports
        .command
        .bind_role(scoped.clone())
        .await
        .map_err(|error| format!("org bind: {error}"))?;
    check(
        *scoped_bound.binding.scope() == scoped.scope,
        "organization-unit scope round-trips",
    )?;
    let kind_bind = cx.bind(
        cx.tenant_a,
        user,
        role.role_id(),
        ResourceScope::resource_type("contract".to_string()).unwrap_or_else(|_| unreachable!()),
        42,
        None,
        None,
    );
    let kind_bound = ports
        .command
        .bind_role(kind_bind.clone())
        .await
        .map_err(|error| format!("kind bind: {error}"))?;
    check(
        *kind_bound.binding.scope() == kind_bind.scope,
        "resource-type scope round-trips",
    )?;

    // Role visibility: foreign/unknown roles cannot be bound; system roles
    // can be.
    check(
        ports
            .command
            .bind_role(cx.bind(
                cx.tenant_a,
                user,
                Uuid::now_v7(),
                ResourceScope::Tenant,
                43,
                None,
                None,
            ))
            .await
            == Err(PolicyStoreError::NotFound),
        "binding an unknown role must be NotFound",
    )?;
    let foreign_role = ports
        .command
        .create_role(cx.create_role(cx.tenant_b, None, "foreign", "Foreign", None, 44))
        .await
        .map_err(|error| format!("foreign role: {error}"))?
        .role;
    check(
        ports
            .command
            .bind_role(cx.bind(
                cx.tenant_a,
                user,
                foreign_role.role_id(),
                ResourceScope::Tenant,
                45,
                None,
                None,
            ))
            .await
            == Err(PolicyStoreError::NotFound),
        "cross-tenant roles must not be bindable",
    )?;
    let system_bound = ports
        .command
        .bind_role(cx.bind(
            cx.tenant_a,
            user,
            system_role_id("system.platform-admin"),
            ResourceScope::Tenant,
            46,
            None,
            None,
        ))
        .await
        .map_err(|error| format!("system bind: {error}"))?;
    check(
        !system_bound.replayed,
        "system roles must be bindable inside any tenant",
    )?;

    // Revoke: bump, converge, stale fails closed; after revoke an identical
    // bind creates a NEW active row.
    let revoke = RevokeBindingCommit {
        tenant_id: cx.tenant_a,
        binding_id: bound.binding.binding_id(),
        expected_version: 1,
        audit: cx.mutation(),
        idempotency_key: Some("revoke-key".to_string()),
        now: cx.at(47),
    };
    let revoked = ports
        .command
        .revoke_binding(revoke.clone())
        .await
        .map_err(|error| format!("revoke: {error}"))?;
    check(
        !revoked.binding.is_active() && revoked.binding.version().value() == 2,
        "revoked binding keeps its row at v2",
    )?;
    let revoke_replay = ports
        .command
        .revoke_binding(revoke)
        .await
        .map_err(|error| format!("revoke replay: {error}"))?;
    check(
        revoke_replay.replayed && revoke_replay.binding == revoked.binding,
        "same key + payload must replay the revocation",
    )?;
    let converge = ports
        .command
        .revoke_binding(RevokeBindingCommit {
            tenant_id: cx.tenant_a,
            binding_id: bound.binding.binding_id(),
            expected_version: 2,
            audit: cx.mutation(),
            idempotency_key: None,
            now: cx.at(48),
        })
        .await
        .map_err(|error| format!("revoke convergence: {error}"))?;
    check(
        converge.replayed && converge.binding.version().value() == 2,
        "already-revoked converges with no bump",
    )?;
    check(
        ports
            .command
            .revoke_binding(RevokeBindingCommit {
                tenant_id: cx.tenant_a,
                binding_id: bound.binding.binding_id(),
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(49),
            })
            .await
            == Err(PolicyStoreError::VersionConflict),
        "stale revoke must fail closed as VersionConflict",
    )?;
    check(
        ports
            .command
            .revoke_binding(RevokeBindingCommit {
                tenant_id: cx.tenant_b,
                binding_id: second.binding.binding_id(),
                expected_version: 1,
                audit: cx.mutation(),
                idempotency_key: None,
                now: cx.at(50),
            })
            .await
            == Err(PolicyStoreError::NotFound),
        "revoke is tenant-scoped",
    )?;
    let rebound = ports
        .command
        .bind_role(BindRoleCommit {
            idempotency_key: None,
            ..bind
        })
        .await
        .map_err(|error| format!("rebind after revoke: {error}"))?;
    check(
        !rebound.replayed
            && rebound.binding.binding_id() != bound.binding.binding_id()
            && rebound.binding.is_active(),
        "an identical bind after revoke creates a fresh active row",
    )?;

    // Query surfaces.
    let all = ports
        .query
        .list_bindings_for_user(cx.tenant_a, user)
        .await
        .map_err(|error| format!("list bindings: {error}"))?;
    check(
        all.len() >= 5 && all.iter().any(|binding| !binding.is_active()),
        "user binding listings include revoked rows",
    )?;
    let ids: Vec<Uuid> = all.iter().map(RoleBinding::binding_id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    check(ids == sorted, "user bindings come ordered by binding id")?;
    let active_for_role = ports
        .query
        .list_active_bindings_for_role(cx.tenant_a, role.role_id())
        .await
        .map_err(|error| format!("active role bindings: {error}"))?;
    check(
        !active_for_role.iter().any(|binding| !binding.is_active()),
        "active binding listing excludes revoked rows",
    )?;
    let filtered = ports
        .query
        .list_bindings(cx.tenant_a, Some(user))
        .await
        .map_err(|error| format!("filtered bindings: {error}"))?;
    check(
        !filtered.is_empty() && filtered.iter().all(|binding| binding.user_id() == user),
        "binding filter narrows to the requested user",
    )?;
    let one = ports
        .query
        .get_binding(cx.tenant_a, rebound.binding.binding_id())
        .await
        .map_err(|error| format!("get binding: {error}"))?;
    check(one.is_some(), "get_binding returns the tenant's binding")?;
    check(
        ports
            .query
            .get_binding(cx.tenant_b, rebound.binding.binding_id())
            .await
            .map_err(|error| format!("get binding cross-tenant: {error}"))?
            .is_none(),
        "tenant isolation: get_binding never crosses tenants",
    )?;
    Ok(())
}

async fn verify_role_cap(ports: &PolicyContractPorts) -> Result<(), String> {
    // Tenant role budget: system roles never count; the store refuses past
    // `MAX_ROLES_PER_TENANT` tenant-owned rows.
    let tenant = Uuid::now_v7();
    let actor = MutationContext {
        actor_id: Uuid::now_v7().to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason: None,
    };
    for index in 0..u64::try_from(MAX_ROLES_PER_TENANT).map_err(|_| "cap overflow")? {
        ports
            .command
            .create_role(CreateRoleCommit {
                tenant_id: tenant,
                role_id: None,
                stable_key: format!("cap-role-{index}"),
                display_name: format!("Cap role {index}"),
                audit: MutationContext {
                    operation_id: Uuid::now_v7(),
                    ..actor.clone()
                },
                idempotency_key: None,
                now: second_now(),
            })
            .await
            .map_err(|error| format!("role cap fill {index}: {error}"))?;
    }
    check(
        ports
            .command
            .create_role(CreateRoleCommit {
                tenant_id: tenant,
                role_id: None,
                stable_key: "one-too-many".to_string(),
                display_name: "One too many".to_string(),
                audit: actor,
                idempotency_key: None,
                now: second_now(),
            })
            .await
            == Err(PolicyStoreError::TooManyResources),
        "the store must refuse past MAX_ROLES_PER_TENANT tenant rows",
    )?;
    Ok(())
}

async fn verify_user_binding_cap(ports: &PolicyContractPorts) -> Result<(), String> {
    // Per-user active binding budget: `MAX_BINDINGS_PER_USER` distinct
    // active rows fit; the next distinct row is refused.
    let tenant = Uuid::now_v7();
    let role = ports
        .command
        .create_role(CreateRoleCommit {
            tenant_id: tenant,
            role_id: None,
            stable_key: "cap-bind-role".to_string(),
            display_name: "Cap bind role".to_string(),
            audit: MutationContext {
                actor_id: Uuid::now_v7().to_string(),
                actor_kind: MutationActorKind::User,
                operation_id: Uuid::now_v7(),
                trace_id: None,
                reason: None,
            },
            idempotency_key: None,
            now: second_now(),
        })
        .await
        .map_err(|error| format!("bind cap role: {error}"))?
        .role;
    let actor = MutationContext {
        actor_id: Uuid::now_v7().to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason: None,
    };
    let user = Uuid::now_v7();
    for index in 0..u64::try_from(MAX_BINDINGS_PER_USER).map_err(|_| "cap overflow")? {
        ports
            .command
            .bind_role(BindRoleCommit {
                tenant_id: tenant,
                binding_id: None,
                user_id: user,
                role_id: role.role_id(),
                scope: ResourceScope::Tenant,
                effective_at: second_now()
                    + TimeDelta::seconds(i64::try_from(index).map_err(|_| "cap index overflow")?),
                expires_at: None,
                audit: MutationContext {
                    operation_id: Uuid::now_v7(),
                    ..actor.clone()
                },
                idempotency_key: None,
                now: second_now(),
            })
            .await
            .map_err(|error| format!("bind cap fill {index}: {error}"))?;
    }
    check(
        ports
            .command
            .bind_role(BindRoleCommit {
                tenant_id: tenant,
                binding_id: None,
                user_id: user,
                role_id: role.role_id(),
                scope: ResourceScope::Tenant,
                effective_at: second_now() + TimeDelta::seconds(10_000),
                expires_at: None,
                audit: actor,
                idempotency_key: None,
                now: second_now(),
            })
            .await
            == Err(PolicyStoreError::TooManyResources),
        "the store must refuse past MAX_BINDINGS_PER_USER active rows",
    )?;
    Ok(())
}
