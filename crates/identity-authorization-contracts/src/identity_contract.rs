//! Behavior contract for the identity adapters (`identity-postgres` and
//! `identity-sqlite` run the same suite).

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use identity::domain::{MembershipSource, MembershipStatus, UserLifecycleStatus};
use identity::ports::{
    BootstrapLedgerEntry, BootstrapLedgerPort, BootstrapOutcome, ChangeMembershipStatusCommit,
    ChangeUserStatusCommit, CreateMembershipCommit, IdentityCommandPort, IdentityQueryPort,
    IdentityResolvePort, IdentityStoreError, MembershipTarget, MutationActorKind, MutationContext,
    ResolvePrincipalCommit,
};

use crate::{check, second_now};

/// All identity ports under test.
pub struct IdentityContractPorts {
    /// Resolve-or-provision port.
    pub resolve: Arc<dyn IdentityResolvePort>,
    /// Command (membership/user mutation) port.
    pub command: Arc<dyn IdentityCommandPort>,
    /// Read-only query port.
    pub query: Arc<dyn IdentityQueryPort>,
    /// Bootstrap ledger port.
    pub ledger: Arc<dyn BootstrapLedgerPort>,
}

/// Run the full identity contract against one adapter set.
pub async fn verify_identity_contract(ports: &IdentityContractPorts) -> Result<(), String> {
    let cx = Contract::new();
    verify_resolve(&cx, ports).await?;
    verify_memberships(&cx, ports).await?;
    verify_membership_status(&cx, ports).await?;
    verify_user_status(&cx, ports).await?;
    verify_listings(&cx, ports).await?;
    verify_ledger(&cx, ports).await?;
    Ok(())
}

struct Contract {
    actor: Uuid,
    tenant_a: Uuid,
    tenant_b: Uuid,
    tenant_c: Uuid,
    /// Spare tenant for the disabled-user attach in `verify_user_status`.
    /// Attaching a disabled user must succeed (ports rule 5) and the
    /// authoritative fake (`identity::testing`) keeps every membership row
    /// in tenant listings, so the attach may not share the listing tenant:
    /// `verify_listings` asserts its "fresh" tenant holds exactly three
    /// memberships.
    tenant_d: Uuid,
    /// Issuer salted per suite run so repeated runs never collide.
    issuer: String,
    base: DateTime<Utc>,
}

impl Contract {
    fn new() -> Self {
        Self {
            actor: Uuid::now_v7(),
            tenant_a: Uuid::now_v7(),
            tenant_b: Uuid::now_v7(),
            tenant_c: Uuid::now_v7(),
            tenant_d: Uuid::now_v7(),
            issuer: format!("https://identity-contract-{}.invalid", Uuid::now_v7()),
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

    fn deterministic(&self, subject: &str) -> Uuid {
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{}|{}", self.issuer, subject).as_bytes(),
        )
    }

    fn resolve_commit(&self, subject: &str, claimed: Option<Uuid>) -> ResolvePrincipalCommit {
        ResolvePrincipalCommit {
            issuer: self.issuer.clone(),
            subject: subject.to_string(),
            claimed_user_id: claimed,
            deterministic_user_id: self.deterministic(subject),
            audit: self.mutation(),
            now: self.at(0),
        }
    }

    fn create_membership(
        &self,
        tenant: Uuid,
        user: Uuid,
        key: Option<&str>,
        offset: i64,
    ) -> CreateMembershipCommit {
        CreateMembershipCommit {
            tenant_id: tenant,
            target: MembershipTarget::UserId(user),
            source: MembershipSource::Admin,
            audit: self.mutation(),
            idempotency_key: key.map(str::to_string),
            now: self.at(offset),
        }
    }

    fn change_membership(
        &self,
        tenant: Uuid,
        user: Uuid,
        target: MembershipStatus,
        expected_version: i64,
        key: Option<&str>,
        offset: i64,
    ) -> ChangeMembershipStatusCommit {
        ChangeMembershipStatusCommit {
            tenant_id: tenant,
            user_id: user,
            target_status: target,
            expected_version,
            audit: self.mutation(),
            idempotency_key: key.map(str::to_string),
            now: self.at(offset),
        }
    }

    fn change_user(
        &self,
        user: Uuid,
        target: UserLifecycleStatus,
        expected_version: i64,
        key: Option<&str>,
        offset: i64,
    ) -> ChangeUserStatusCommit {
        ChangeUserStatusCommit {
            user_id: user,
            target_status: target,
            expected_version,
            audit: self.mutation(),
            idempotency_key: key.map(str::to_string),
            now: self.at(offset),
        }
    }
}

async fn verify_resolve(cx: &Contract, ports: &IdentityContractPorts) -> Result<(), String> {
    // First authenticated subject provisions exactly one user.
    let first = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-first", None))
        .await
        .map_err(|error| format!("resolve first: {error}"))?;
    check(first.provisioned, "first resolve must provision")?;
    check(first.user.is_active(), "provisioned user must be active")?;
    check(first.user.version().value() == 1, "provisioned user at v1")?;
    check(
        first.external_identity.matches(&cx.issuer, "subject-first"),
        "link must carry the resolved external key",
    )?;

    // Re-resolve converges onto the same user and link, provisioning nothing.
    let again = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-first", None))
        .await
        .map_err(|error| format!("resolve again: {error}"))?;
    check(!again.provisioned, "second resolve must not provision")?;
    check(
        again.user.user_id() == first.user.user_id(),
        "same key must resolve to the same user",
    )?;
    check(
        again.external_identity.external_identity_id()
            == first.external_identity.external_identity_id(),
        "same key must resolve to the same link row",
    )?;

    // Same issuer, different subject ⇒ a different user.
    let second = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-second", None))
        .await
        .map_err(|error| format!("resolve second subject: {error}"))?;
    check(
        second.user.user_id() != first.user.user_id(),
        "distinct subjects must map to distinct users",
    )?;

    // A claimed `user_id` pointing at an already-linked different principal
    // fails closed (never rebinds).
    let mismatch = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-claim", Some(first.user.user_id())))
        .await;
    check(
        mismatch == Err(IdentityStoreError::PrincipalMismatch),
        "claim pointing at another linked principal must be PrincipalMismatch",
    )?;
    Ok(())
}

async fn verify_memberships(cx: &Contract, ports: &IdentityContractPorts) -> Result<(), String> {
    let user = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-membership", None))
        .await
        .map_err(|error| format!("resolve membership subject: {error}"))?
        .user;

    let create = cx.create_membership(
        cx.tenant_a,
        user.user_id(),
        Some("create-membership-key"),
        10,
    );
    let created = ports
        .command
        .create_membership(create.clone())
        .await
        .map_err(|error| format!("create membership: {error}"))?;
    check(!created.replayed, "first create must not be a replay")?;
    check(
        created.membership.status() == MembershipStatus::Active
            && created.membership.version().value() == 1,
        "fresh membership is Active at v1",
    )?;
    check(
        created.membership.joined_at() == create.now,
        "joined_at must be the commit timestamp, not wall time",
    )?;

    // Same key + same payload converges onto the stored membership.
    let replay = ports
        .command
        .create_membership(create.clone())
        .await
        .map_err(|error| format!("create membership replay: {error}"))?;
    check(
        replay.replayed && replay.membership == created.membership,
        "same key + payload must replay the stored membership",
    )?;

    // Same key + different payload fails without mutating.
    let other = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-membership-other", None))
        .await
        .map_err(|error| format!("resolve other subject: {error}"))?
        .user;
    let conflict = CreateMembershipCommit {
        target: MembershipTarget::UserId(other.user_id()),
        ..create.clone()
    };
    check(
        ports.command.create_membership(conflict).await
            == Err(IdentityStoreError::IdempotencyConflict),
        "same key + different payload must be IdempotencyConflict",
    )?;
    check(
        ports
            .query
            .get_membership(cx.tenant_a, other.user_id())
            .await
            .map_err(|error| format!("conflict readback: {error}"))?
            .is_none(),
        "an idempotency conflict must not mutate",
    )?;

    // Duplicate membership (no key) fails with AlreadyExists; a different
    // tenant is a different membership.
    let duplicate = CreateMembershipCommit {
        idempotency_key: None,
        ..create.clone()
    };
    check(
        ports.command.create_membership(duplicate).await == Err(IdentityStoreError::AlreadyExists),
        "duplicate membership must be AlreadyExists",
    )?;
    let in_b = CreateMembershipCommit {
        tenant_id: cx.tenant_b,
        ..create
    };
    let created_b = ports
        .command
        .create_membership(in_b)
        .await
        .map_err(|error| format!("cross-tenant membership: {error}"))?;
    check(
        !created_b.replayed,
        "idempotency keys are per (operation, tenant) — tenant B is independent",
    )?;

    // Unknown target user ⇒ NotFound.
    let unknown = cx.create_membership(cx.tenant_a, Uuid::now_v7(), None, 11);
    check(
        ports.command.create_membership(unknown).await == Err(IdentityStoreError::NotFound),
        "membership for an unknown user must be NotFound",
    )?;

    // External-subject target provisions on demand in the same commit.
    let deterministic = cx.deterministic("subject-provisioned-on-demand");
    let provisioned_target = CreateMembershipCommit {
        tenant_id: cx.tenant_a,
        target: MembershipTarget::ExternalSubject {
            issuer: cx.issuer.clone(),
            subject: "subject-provisioned-on-demand".to_string(),
            deterministic_user_id: deterministic,
        },
        source: MembershipSource::Admin,
        audit: cx.mutation(),
        idempotency_key: Some("provision-target-key".to_string()),
        now: cx.at(12),
    };
    let provisioned = ports
        .command
        .create_membership(provisioned_target.clone())
        .await
        .map_err(|error| format!("provision-on-demand membership: {error}"))?;
    check(
        !provisioned.replayed && provisioned.membership.user_id() == deterministic,
        "external-subject target must provision the deterministic user",
    )?;
    let replay = ports
        .command
        .create_membership(provisioned_target)
        .await
        .map_err(|error| format!("provision-on-demand replay: {error}"))?;
    check(
        replay.replayed && replay.membership == provisioned.membership,
        "provision-on-demand must replay without duplicate rows",
    )?;
    let found = ports
        .query
        .find_user_by_external_identity(&cx.issuer, "subject-provisioned-on-demand")
        .await
        .map_err(|error| format!("find provisioned user: {error}"))?;
    check(
        found.is_some_and(|found| found.user_id() == deterministic),
        "provisioned user must be findable by external identity",
    )?;
    Ok(())
}

async fn verify_membership_status(
    cx: &Contract,
    ports: &IdentityContractPorts,
) -> Result<(), String> {
    let user = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-status", None))
        .await
        .map_err(|error| format!("resolve status subject: {error}"))?
        .user;
    ports
        .command
        .create_membership(cx.create_membership(cx.tenant_a, user.user_id(), None, 20))
        .await
        .map_err(|error| format!("status fixture: {error}"))?;

    let suspend = cx.change_membership(
        cx.tenant_a,
        user.user_id(),
        MembershipStatus::Suspended,
        1,
        Some("suspend-key"),
        21,
    );
    let suspended = ports
        .command
        .change_membership_status(suspend.clone())
        .await
        .map_err(|error| format!("suspend: {error}"))?;
    check(
        suspended.membership.status() == MembershipStatus::Suspended
            && suspended.membership.version().value() == 2
            && suspended.membership.suspended_at().is_some(),
        "suspend bumps to v2 and stamps suspended_at",
    )?;

    // Same key + payload replays the stored result.
    let replay = ports
        .command
        .change_membership_status(suspend)
        .await
        .map_err(|error| format!("suspend replay: {error}"))?;
    check(
        replay.replayed && replay.membership == suspended.membership,
        "same key + payload must replay the stored membership",
    )?;

    // Already-suspended converges without a version bump and without audit.
    let converged = ports
        .command
        .change_membership_status(cx.change_membership(
            cx.tenant_a,
            user.user_id(),
            MembershipStatus::Suspended,
            2,
            None,
            21,
        ))
        .await
        .map_err(|error| format!("suspend convergence: {error}"))?;
    check(
        converged.replayed && converged.membership.version().value() == 2,
        "same-status change must converge (replayed, no version bump)",
    )?;

    // Reactivation bumps again; a stale expected_version fails closed.
    let reactivate = ports
        .command
        .change_membership_status(cx.change_membership(
            cx.tenant_a,
            user.user_id(),
            MembershipStatus::Active,
            2,
            None,
            22,
        ))
        .await
        .map_err(|error| format!("reactivate: {error}"))?;
    check(
        reactivate.membership.status() == MembershipStatus::Active
            && reactivate.membership.version().value() == 3,
        "reactivation bumps to v3",
    )?;
    check(
        ports
            .command
            .change_membership_status(cx.change_membership(
                cx.tenant_a,
                user.user_id(),
                MembershipStatus::Suspended,
                1,
                None,
                23,
            ))
            .await
            == Err(IdentityStoreError::VersionConflict),
        "stale expected_version must be VersionConflict",
    )?;
    check(
        ports
            .command
            .change_membership_status(cx.change_membership(
                cx.tenant_a,
                Uuid::now_v7(),
                MembershipStatus::Suspended,
                1,
                None,
                23,
            ))
            .await
            == Err(IdentityStoreError::NotFound),
        "status change for a non-member must be NotFound",
    )?;
    Ok(())
}

async fn verify_user_status(cx: &Contract, ports: &IdentityContractPorts) -> Result<(), String> {
    let user = ports
        .resolve
        .resolve_or_provision(cx.resolve_commit("subject-user-status", None))
        .await
        .map_err(|error| format!("resolve user-status subject: {error}"))?
        .user;

    let disable = cx.change_user(
        user.user_id(),
        UserLifecycleStatus::Disabled,
        1,
        Some("disable-key"),
        30,
    );
    let disabled = ports
        .command
        .change_user_status(disable.clone())
        .await
        .map_err(|error| format!("disable: {error}"))?;
    check(
        disabled.user.status() == UserLifecycleStatus::Disabled
            && disabled.user.version().value() == 2
            && !disabled.replayed,
        "disable bumps to v2",
    )?;
    let replay = ports
        .command
        .change_user_status(disable)
        .await
        .map_err(|error| format!("disable replay: {error}"))?;
    check(
        replay.replayed && replay.user == disabled.user,
        "same key + payload must replay the stored user",
    )?;

    // Already-disabled converges without a bump; stale versions fail closed.
    let converged = ports
        .command
        .change_user_status(cx.change_user(
            user.user_id(),
            UserLifecycleStatus::Disabled,
            2,
            None,
            30,
        ))
        .await
        .map_err(|error| format!("disable convergence: {error}"))?;
    check(
        converged.replayed && converged.user.version().value() == 2,
        "same-status user change must converge (replayed, no bump)",
    )?;
    check(
        ports
            .command
            .change_user_status(cx.change_user(
                user.user_id(),
                UserLifecycleStatus::Disabled,
                1,
                None,
                31,
            ))
            .await
            == Err(IdentityStoreError::VersionConflict),
        "stale user version must be VersionConflict",
    )?;

    // Disabled users may be attached to new tenants (the access checker,
    // not membership writes, denies at decision time). The attach targets
    // the spare tenant so `verify_listings` still sees a three-row fresh
    // tenant (see `Contract::tenant_d`).
    let attached = ports
        .command
        .create_membership(cx.create_membership(cx.tenant_d, user.user_id(), None, 32))
        .await
        .map_err(|error| format!("attach disabled user: {error}"))?;
    check(
        !attached.replayed,
        "a disabled user may still gain a membership (decision-time deny)",
    )?;

    let enabled = ports
        .command
        .change_user_status(cx.change_user(
            user.user_id(),
            UserLifecycleStatus::Active,
            2,
            None,
            33,
        ))
        .await
        .map_err(|error| format!("enable: {error}"))?;
    check(
        enabled.user.is_active() && enabled.user.version().value() == 3,
        "enable bumps to v3",
    )?;
    Ok(())
}

async fn verify_listings(cx: &Contract, ports: &IdentityContractPorts) -> Result<(), String> {
    // Fresh tenant with three members joined at distinct offsets; the middle
    // one is globally disabled.
    let tenant = cx.tenant_c;
    let mut joined = Vec::new();
    for (index, offset) in [(0usize, 41_i64), (1, 42), (2, 43)] {
        let subject = format!("subject-list-{index}");
        let user = ports
            .resolve
            .resolve_or_provision(cx.resolve_commit(&subject, None))
            .await
            .map_err(|error| format!("resolve list fixture: {error}"))?
            .user;
        ports
            .command
            .create_membership(cx.create_membership(tenant, user.user_id(), None, offset))
            .await
            .map_err(|error| format!("list fixture membership: {error}"))?;
        joined.push((user.user_id(), offset));
    }
    ports
        .command
        .change_user_status(cx.change_user(joined[1].0, UserLifecycleStatus::Disabled, 1, None, 44))
        .await
        .map_err(|error| format!("disable middle: {error}"))?;

    let page1 = ports
        .query
        .list_tenant_users(tenant, 2, None)
        .await
        .map_err(|error| format!("list page 1: {error}"))?;
    check(page1.0.len() == 2, "page 1 must hold exactly `limit` rows")?;
    let cursor = page1.1.ok_or("full page must yield a cursor")?;
    check(
        page1.0[0].membership.joined_at() > page1.0[1].membership.joined_at()
            || (page1.0[0].membership.joined_at() == page1.0[1].membership.joined_at()
                && page1.0[0].membership.membership_id() > page1.0[1].membership.membership_id()),
        "listing must be ordered (joined_at, membership_id) DESC",
    )?;
    let page2 = ports
        .query
        .list_tenant_users(tenant, 2, Some(cursor))
        .await
        .map_err(|error| format!("list page 2: {error}"))?;
    check(page2.0.len() == 1, "final page must hold the remainder")?;
    check(page2.1.is_none(), "short page must not advertise a cursor")?;
    let mut seen: Vec<Uuid> = page1
        .0
        .iter()
        .chain(&page2.0)
        .map(|record| record.membership.user_id())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    check(
        seen.len() == 3,
        "keyset pages must cover each member exactly once",
    )?;
    check(
        page1.0[0].external_subjects.len() == 1,
        "tenant user records must carry linked external subjects",
    )?;

    let memberships = ports
        .query
        .list_memberships(tenant, 10, None)
        .await
        .map_err(|error| format!("list memberships: {error}"))?;
    check(
        memberships.0.len() == 3,
        "membership listing returns all rows",
    )?;
    check(
        memberships.0.iter().any(|record| {
            record.membership.user_id() == joined[1].0
                && record.user_status == UserLifecycleStatus::Disabled
        }),
        "membership listing must surface the global user status",
    )?;

    let detail = ports
        .query
        .get_tenant_user(tenant, joined[2].0)
        .await
        .map_err(|error| format!("get tenant user: {error}"))?;
    check(
        detail.is_some_and(|detail| {
            detail.user.user_id() == joined[2].0
                && detail.membership.tenant_id() == tenant
                && detail.external_subjects.len() == 1
        }),
        "get_tenant_user returns user + membership + links",
    )?;
    check(
        ports
            .query
            .get_tenant_user(cx.tenant_a, joined[2].0)
            .await
            .map_err(|error| format!("cross-tenant detail: {error}"))?
            .is_none(),
        "tenant isolation: no membership ⇒ no detail row",
    )?;
    let found = ports
        .query
        .find_user_by_external_identity(&cx.issuer, "subject-list-2")
        .await
        .map_err(|error| format!("find by external: {error}"))?;
    check(
        found.is_some_and(|found| found.user_id() == joined[2].0),
        "external lookup must find the provisioned user",
    )?;
    check(
        ports
            .query
            .find_user_by_external_identity(&cx.issuer, "subject-absent")
            .await
            .map_err(|error| format!("find by external (absent): {error}"))?
            .is_none(),
        "external lookup for an unknown subject must be None",
    )?;
    Ok(())
}

async fn verify_ledger(cx: &Contract, ports: &IdentityContractPorts) -> Result<(), String> {
    // Salted like every other fixture so a suite re-run never collides.
    let issuer = format!("https://bootstrap-contract-{}.invalid", Uuid::now_v7());
    let subject = "bootstrap-subject";
    check(
        ports
            .ledger
            .latest_for(cx.tenant_a, &issuer, subject)
            .await
            .map_err(|error| format!("ledger empty read: {error}"))?
            .is_none(),
        "ledger starts empty",
    )?;
    let entry = BootstrapLedgerEntry {
        tenant_id: cx.tenant_a,
        issuer: issuer.clone(),
        subject: subject.to_string(),
        role_stable_key: "system.bootstrap-admin".to_string(),
        config_version: 1,
        config_digest: "a".repeat(64),
        outcome: BootstrapOutcome::Executed,
        recorded_at: cx.at(50),
    };
    check(
        ports
            .ledger
            .record(&entry)
            .await
            .map_err(|error| format!("ledger record v1: {error}"))?,
        "first record must insert",
    )?;
    check(
        !ports
            .ledger
            .record(&entry)
            .await
            .map_err(|error| format!("ledger record v1 again: {error}"))?,
        "identical digest must converge to a no-op",
    )?;
    let latest = ports
        .ledger
        .latest_for(cx.tenant_a, &issuer, subject)
        .await
        .map_err(|error| format!("ledger latest: {error}"))?
        .ok_or("ledger row must be readable")?;
    check(
        latest.config_version == 1 && latest.outcome == BootstrapOutcome::Executed,
        "latest must return the recorded execution",
    )?;
    let bumped = BootstrapLedgerEntry {
        config_version: 2,
        config_digest: "b".repeat(64),
        recorded_at: cx.at(51),
        ..entry.clone()
    };
    check(
        ports
            .ledger
            .record(&bumped)
            .await
            .map_err(|error| format!("ledger record v2: {error}"))?,
        "a deliberate config bump must record",
    )?;
    let latest = ports
        .ledger
        .latest_for(cx.tenant_a, &issuer, subject)
        .await
        .map_err(|error| format!("ledger latest v2: {error}"))?
        .ok_or("ledger row must be readable")?;
    check(
        latest.config_version == 2,
        "latest must surface the newest recorded execution",
    )?;
    Ok(())
}
