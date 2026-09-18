//! In-memory fake identity ports for application and API tests.
//!
//! The fakes deliberately emulate the **database-level** guarantees the real
//! adapters must provide — `(issuer, subject)` and `(tenant, user)` unique
//! indexes, optimistic version checks, idempotent replay convergence, and
//! audit-on-commit — so tests written against the fakes remain honest
//! expectations for the adapters (both adapters additionally run the shared
//! contract suite).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    ExternalIdentity, MembershipStatus, PlatformUser, TenantMembership, UserLifecycleStatus,
};
use crate::ports::{
    BootstrapLedgerEntry, BootstrapLedgerPort, ChangeMembershipStatusCommit,
    ChangeUserStatusCommit, CreateMembershipCommit, IdentityCommandPort, IdentityQueryPort,
    IdentityResolvePort, IdentityStoreError, KeysetPosition, MembershipCommitOutcome,
    MembershipRecord, MembershipTarget, MutationContext, ResolvePrincipalCommit, ResolvedPrincipal,
    TenantUserRecord, UserCommitOutcome,
};

/// A captured fake audit record (mirrors the shape the unified audit trail
/// receives in production).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeAuditRecord {
    /// Stable audit action, e.g. `identity.membership.suspended`.
    pub action: String,
    /// Actor identity as recorded.
    pub actor_id: String,
    /// Tenant scope when the action is tenant-bound.
    pub tenant_id: Option<Uuid>,
    /// Affected resource id (stringified).
    pub resource_id: String,
}

#[derive(Debug, Clone)]
enum StoredOutcome {
    Membership(TenantMembership),
    User(PlatformUser),
}

#[derive(Default)]
struct State {
    users: HashMap<Uuid, PlatformUser>,
    external: Vec<ExternalIdentity>,
    memberships: HashMap<(Uuid, Uuid), TenantMembership>,
    idempotency: HashMap<(String, String), (String, StoredOutcome)>,
    ledger: Vec<BootstrapLedgerEntry>,
    audits: Vec<FakeAuditRecord>,
    poisoned: bool,
}

/// Shared fake stores for all identity ports.
pub struct FakeIdentityStores {
    state: Arc<Mutex<State>>,
    /// Port handle for resolve-or-provision.
    pub resolve: Arc<dyn IdentityResolvePort>,
    /// Port handle for command-side mutations.
    pub command: Arc<dyn IdentityCommandPort>,
    /// Port handle for read queries.
    pub query: Arc<dyn IdentityQueryPort>,
    /// Port handle for the bootstrap ledger.
    pub ledger: Arc<dyn BootstrapLedgerPort>,
}

impl Default for FakeIdentityStores {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeIdentityStores {
    #[must_use]
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        Self {
            resolve: Arc::new(FakeResolve {
                state: Arc::clone(&state),
            }),
            command: Arc::new(FakeCommand {
                state: Arc::clone(&state),
            }),
            query: Arc::new(FakeQuery {
                state: Arc::clone(&state),
            }),
            ledger: Arc::new(FakeLedger {
                state: Arc::clone(&state),
            }),
            state,
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, IdentityStoreError> {
        self.state.lock().map_err(|_| IdentityStoreError::Failed)
    }

    /// Insert a user (test seeding).
    pub fn seed_user(&self, user: PlatformUser) {
        if let Ok(mut state) = self.lock() {
            state.users.insert(user.user_id(), user);
        }
    }

    /// Insert a membership (test seeding).
    pub fn seed_membership(&self, membership: TenantMembership) {
        if let Ok(mut state) = self.lock() {
            state
                .memberships
                .insert((membership.tenant_id(), membership.user_id()), membership);
        }
    }

    /// Directly suspend a membership (test seeding).
    pub fn suspend_membership(&self, tenant_id: Uuid, user_id: Uuid) {
        if let Ok(mut state) = self.lock() {
            if let Some(membership) = state.memberships.get_mut(&(tenant_id, user_id)) {
                if membership.suspend(Utc::now()).is_err() {
                    state.poisoned = true;
                }
            }
        }
    }

    /// Directly reactivate a membership (test seeding).
    pub fn reactivate_membership(&self, tenant_id: Uuid, user_id: Uuid) {
        if let Ok(mut state) = self.lock() {
            if let Some(membership) = state.memberships.get_mut(&(tenant_id, user_id)) {
                if membership.reactivate(Utc::now()).is_err() {
                    state.poisoned = true;
                }
            }
        }
    }

    /// All audit records captured so far, in commit order.
    #[must_use]
    pub fn audit_records(&self) -> Vec<FakeAuditRecord> {
        self.lock()
            .map(|state| state.audits.clone())
            .unwrap_or_default()
    }

    /// Simulate a store failure for the next operations (poison switch).
    pub fn poison(&self) {
        if let Ok(mut state) = self.lock() {
            state.poisoned = true;
        }
    }
}

fn check_poisoned(state: &State) -> Result<(), IdentityStoreError> {
    if state.poisoned {
        return Err(IdentityStoreError::Unavailable);
    }
    Ok(())
}

fn record_audit(
    state: &mut State,
    action: &str,
    audit: &MutationContext,
    tenant_id: Option<Uuid>,
    resource_id: String,
) {
    state.audits.push(FakeAuditRecord {
        action: action.to_string(),
        actor_id: audit.actor_id.clone(),
        tenant_id,
        resource_id,
    });
}

fn provision_user_locked(
    state: &mut State,
    issuer: &str,
    subject: &str,
    claimed_user_id: Option<Uuid>,
    deterministic_user_id: Uuid,
    audit: &MutationContext,
    now: DateTime<Utc>,
) -> Result<ResolvedPrincipal, IdentityStoreError> {
    if let Some(existing) = state
        .external
        .iter()
        .find(|link| link.matches(issuer, subject))
        .cloned()
    {
        if claimed_user_id.is_some_and(|claimed| claimed != existing.user_id()) {
            return Err(IdentityStoreError::PrincipalMismatch);
        }
        let user = state
            .users
            .get(&existing.user_id())
            .cloned()
            .ok_or(IdentityStoreError::Failed)?;
        return Ok(ResolvedPrincipal {
            user,
            external_identity: existing,
            provisioned: false,
        });
    }

    let candidate = claimed_user_id.unwrap_or(deterministic_user_id);
    let mut provisioned = false;
    let user = if let Some(user) = state.users.get(&candidate) {
        // The user exists but is not linked to this external key yet.
        // Adopt only if it has no other link; a different existing link
        // fails closed.
        if state
            .external
            .iter()
            .any(|link| link.user_id() == candidate && !link.matches(issuer, subject))
        {
            return Err(IdentityStoreError::PrincipalMismatch);
        }
        user.clone()
    } else {
        let user = PlatformUser::create(candidate, now).map_err(|_| IdentityStoreError::Failed)?;
        state.users.insert(user.user_id(), user.clone());
        provisioned = true;
        user
    };
    let link = ExternalIdentity::link(Uuid::now_v7(), issuer, subject, user.user_id(), now)
        .map_err(|_| IdentityStoreError::Failed)?;
    state.external.push(link.clone());
    if provisioned {
        record_audit(
            state,
            "identity.user.provisioned",
            audit,
            None,
            user.user_id().to_string(),
        );
    } else {
        record_audit(
            state,
            "identity.external_identity.linked",
            audit,
            None,
            user.user_id().to_string(),
        );
    }
    Ok(ResolvedPrincipal {
        user,
        external_identity: link,
        provisioned,
    })
}

struct FakeResolve {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl IdentityResolvePort for FakeResolve {
    async fn resolve_or_provision(
        &self,
        command: ResolvePrincipalCommit,
    ) -> Result<ResolvedPrincipal, IdentityStoreError> {
        let mut state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        provision_user_locked(
            &mut state,
            &command.issuer,
            &command.subject,
            command.claimed_user_id,
            command.deterministic_user_id,
            &command.audit,
            command.now,
        )
    }
}

fn idempotent_replay(
    state: &State,
    op: &str,
    key: Option<&String>,
    fingerprint: &str,
) -> Result<Option<StoredOutcome>, IdentityStoreError> {
    let Some(key) = key else {
        return Ok(None);
    };
    match state.idempotency.get(&(op.to_string(), key.clone())) {
        Some((stored_fingerprint, outcome)) => {
            if stored_fingerprint == fingerprint {
                Ok(Some(outcome.clone()))
            } else {
                Err(IdentityStoreError::IdempotencyConflict)
            }
        }
        None => Ok(None),
    }
}

fn store_idempotent(
    state: &mut State,
    op: &str,
    key: Option<&String>,
    fingerprint: String,
    outcome: StoredOutcome,
) {
    if let Some(key) = key {
        state
            .idempotency
            .insert((op.to_string(), key.clone()), (fingerprint, outcome));
    }
}

fn create_membership_op(tenant_id: Uuid) -> String {
    format!("create_membership|{tenant_id}")
}

fn change_membership_status_op(tenant_id: Uuid) -> String {
    format!("change_membership_status|{tenant_id}")
}

fn membership_fingerprint(
    tenant_id: Uuid,
    user_id: Uuid,
    target_status: MembershipStatus,
    expected_version: i64,
) -> String {
    format!("{tenant_id}|{user_id}|{target_status:?}|{expected_version}")
}

struct FakeCommand {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl IdentityCommandPort for FakeCommand {
    async fn create_membership(
        &self,
        command: CreateMembershipCommit,
    ) -> Result<MembershipCommitOutcome, IdentityStoreError> {
        let mut state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;

        let target_user_id = match &command.target {
            MembershipTarget::UserId(user_id) => {
                if state.users.contains_key(user_id) {
                    *user_id
                } else {
                    return Err(IdentityStoreError::NotFound);
                }
            }
            MembershipTarget::ExternalSubject {
                issuer,
                subject,
                deterministic_user_id,
            } => provision_user_locked(
                &mut state,
                issuer,
                subject,
                None,
                *deterministic_user_id,
                &command.audit,
                command.now,
            )?
            .user
            .user_id(),
        };

        let fingerprint = format!(
            "{}|{target_user_id}|{:?}",
            command.tenant_id, command.source
        );
        if let Some(StoredOutcome::Membership(existing)) = idempotent_replay(
            &state,
            &create_membership_op(command.tenant_id),
            command.idempotency_key.as_ref(),
            &fingerprint,
        )? {
            return Ok(MembershipCommitOutcome {
                membership: existing,
                replayed: true,
            });
        }

        if state
            .memberships
            .contains_key(&(command.tenant_id, target_user_id))
        {
            return Err(IdentityStoreError::AlreadyExists);
        }

        let membership = TenantMembership::join(
            Uuid::now_v7(),
            command.tenant_id,
            target_user_id,
            command.source,
            command.now,
        )
        .map_err(|_| IdentityStoreError::Failed)?;
        state
            .memberships
            .insert((command.tenant_id, target_user_id), membership.clone());
        record_audit(
            &mut state,
            "identity.membership.created",
            &command.audit,
            Some(command.tenant_id),
            target_user_id.to_string(),
        );
        store_idempotent(
            &mut state,
            &create_membership_op(command.tenant_id),
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Membership(membership.clone()),
        );
        Ok(MembershipCommitOutcome {
            membership,
            replayed: false,
        })
    }

    async fn change_membership_status(
        &self,
        command: ChangeMembershipStatusCommit,
    ) -> Result<MembershipCommitOutcome, IdentityStoreError> {
        let mut state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;

        let fingerprint = membership_fingerprint(
            command.tenant_id,
            command.user_id,
            command.target_status,
            command.expected_version,
        );
        if let Some(StoredOutcome::Membership(existing)) = idempotent_replay(
            &state,
            &change_membership_status_op(command.tenant_id),
            command.idempotency_key.as_ref(),
            &fingerprint,
        )? {
            return Ok(MembershipCommitOutcome {
                membership: existing,
                replayed: true,
            });
        }

        let membership = state
            .memberships
            .get(&(command.tenant_id, command.user_id))
            .cloned()
            .ok_or(IdentityStoreError::NotFound)?;
        if membership.version().value() != command.expected_version {
            return Err(IdentityStoreError::VersionConflict);
        }

        let mut membership = membership;
        // Convergent no-op when already at the target status.
        let changed = membership.status() != command.target_status;
        if changed {
            match command.target_status {
                MembershipStatus::Active => membership
                    .reactivate(command.now)
                    .map_err(|_| IdentityStoreError::Failed)?,
                MembershipStatus::Suspended => membership
                    .suspend(command.now)
                    .map_err(|_| IdentityStoreError::Failed)?,
            }
            state
                .memberships
                .insert((command.tenant_id, command.user_id), membership.clone());
            let action = match command.target_status {
                MembershipStatus::Active => "identity.membership.reactivated",
                MembershipStatus::Suspended => "identity.membership.suspended",
            };
            record_audit(
                &mut state,
                action,
                &command.audit,
                Some(command.tenant_id),
                command.user_id.to_string(),
            );
        }
        store_idempotent(
            &mut state,
            &change_membership_status_op(command.tenant_id),
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::Membership(membership.clone()),
        );
        Ok(MembershipCommitOutcome {
            membership,
            replayed: !changed,
        })
    }

    async fn change_user_status(
        &self,
        command: ChangeUserStatusCommit,
    ) -> Result<UserCommitOutcome, IdentityStoreError> {
        let mut state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;

        let fingerprint = format!(
            "{}|{:?}|{}",
            command.user_id, command.target_status, command.expected_version
        );
        if let Some(StoredOutcome::User(existing)) = idempotent_replay(
            &state,
            "change_user_status",
            command.idempotency_key.as_ref(),
            &fingerprint,
        )? {
            return Ok(UserCommitOutcome {
                user: existing,
                replayed: true,
            });
        }

        let user = state
            .users
            .get(&command.user_id)
            .cloned()
            .ok_or(IdentityStoreError::NotFound)?;
        if user.version().value() != command.expected_version {
            return Err(IdentityStoreError::VersionConflict);
        }

        let mut user = user;
        let changed = user.status() != command.target_status;
        if changed {
            match command.target_status {
                UserLifecycleStatus::Active => user
                    .enable(command.now)
                    .map_err(|_| IdentityStoreError::Failed)?,
                UserLifecycleStatus::Disabled => user
                    .disable(command.now)
                    .map_err(|_| IdentityStoreError::Failed)?,
            }
            state.users.insert(user.user_id(), user.clone());
            let action = match command.target_status {
                UserLifecycleStatus::Active => "identity.user.enabled",
                UserLifecycleStatus::Disabled => "identity.user.disabled",
            };
            record_audit(
                &mut state,
                action,
                &command.audit,
                None,
                command.user_id.to_string(),
            );
        }
        store_idempotent(
            &mut state,
            "change_user_status",
            command.idempotency_key.as_ref(),
            fingerprint,
            StoredOutcome::User(user.clone()),
        );
        Ok(UserCommitOutcome {
            user,
            replayed: !changed,
        })
    }
}

fn keyset_position(timestamp: DateTime<Utc>, row_id: Uuid) -> KeysetPosition {
    KeysetPosition { timestamp, row_id }
}

fn after_position(position: &KeysetPosition, timestamp: DateTime<Utc>, row_id: Uuid) -> bool {
    timestamp < position.timestamp
        || (timestamp == position.timestamp && row_id.as_u128() < position.row_id.as_u128())
}

struct FakeQuery {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl IdentityQueryPort for FakeQuery {
    async fn get_tenant_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantUserRecord>, IdentityStoreError> {
        let state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        let Some(membership) = state.memberships.get(&(tenant_id, user_id)).cloned() else {
            return Ok(None);
        };
        let Some(user) = state.users.get(&user_id).cloned() else {
            return Ok(None);
        };
        Ok(Some(TenantUserRecord {
            external_subjects: state
                .external
                .iter()
                .filter(|link| link.user_id() == user_id)
                .map(|link| (link.issuer().to_string(), link.subject().to_string()))
                .collect(),
            user,
            membership,
        }))
    }

    async fn list_tenant_users(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<KeysetPosition>,
    ) -> Result<(Vec<TenantUserRecord>, Option<KeysetPosition>), IdentityStoreError> {
        let state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        let mut ordered: Vec<(DateTime<Utc>, Uuid)> = state
            .memberships
            .values()
            .filter(|m| m.tenant_id() == tenant_id)
            .map(|m| (m.joined_at(), m.membership_id()))
            .collect();
        ordered.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.as_u128().cmp(&a.1.as_u128()))
        });
        let filtered: Vec<(DateTime<Utc>, Uuid)> = match after {
            Some(position) => ordered
                .into_iter()
                .filter(|(timestamp, row_id)| after_position(&position, *timestamp, *row_id))
                .collect(),
            None => ordered,
        };
        // Fetch limit+1 to detect a following page; the cursor is the last
        // returned row (adapters do the same with SQL LIMIT n+1).
        let page: Vec<(DateTime<Utc>, Uuid)> = filtered
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX) + 1)
            .collect();
        let has_more = page.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        let page = if has_more {
            page.into_iter()
                .take(usize::try_from(limit).unwrap_or(usize::MAX))
                .collect::<Vec<_>>()
        } else {
            page
        };
        let next_cursor = if has_more {
            page.last()
                .map(|(timestamp, row_id)| keyset_position(*timestamp, *row_id))
        } else {
            None
        };
        let mut items = Vec::with_capacity(page.len());
        for (_, membership_id) in page {
            let membership = state
                .memberships
                .values()
                .find(|m| m.membership_id() == membership_id)
                .cloned()
                .ok_or(IdentityStoreError::Failed)?;
            let user = state
                .users
                .get(&membership.user_id())
                .cloned()
                .ok_or(IdentityStoreError::Failed)?;
            items.push(TenantUserRecord {
                external_subjects: state
                    .external
                    .iter()
                    .filter(|link| link.user_id() == membership.user_id())
                    .map(|link| (link.issuer().to_string(), link.subject().to_string()))
                    .collect(),
                user,
                membership,
            });
        }
        Ok((items, next_cursor))
    }

    async fn list_memberships(
        &self,
        tenant_id: Uuid,
        limit: u32,
        after: Option<KeysetPosition>,
    ) -> Result<(Vec<MembershipRecord>, Option<KeysetPosition>), IdentityStoreError> {
        let (users, next_cursor) = self.list_tenant_users(tenant_id, limit, after).await?;
        Ok((
            users
                .into_iter()
                .map(|record| MembershipRecord {
                    user_status: record.user.status(),
                    membership: record.membership,
                })
                .collect(),
            next_cursor,
        ))
    }

    async fn get_membership(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<TenantMembership>, IdentityStoreError> {
        let state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state.memberships.get(&(tenant_id, user_id)).cloned())
    }

    async fn find_user_by_external_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<PlatformUser>, IdentityStoreError> {
        let state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state
            .external
            .iter()
            .find(|link| link.matches(issuer, subject))
            .and_then(|link| state.users.get(&link.user_id()).cloned()))
    }
}

struct FakeLedger {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl BootstrapLedgerPort for FakeLedger {
    async fn latest_for(
        &self,
        tenant_id: Uuid,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<BootstrapLedgerEntry>, IdentityStoreError> {
        let state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        Ok(state
            .ledger
            .iter()
            .filter(|entry| {
                entry.tenant_id == tenant_id && entry.issuer == issuer && entry.subject == subject
            })
            .max_by_key(|entry| (entry.recorded_at, entry.config_version))
            .cloned())
    }

    async fn record(&self, entry: &BootstrapLedgerEntry) -> Result<bool, IdentityStoreError> {
        let mut state = self.state.lock().map_err(|_| IdentityStoreError::Failed)?;
        check_poisoned(&state)?;
        let duplicate = state.ledger.iter().any(|existing| {
            existing.tenant_id == entry.tenant_id
                && existing.issuer == entry.issuer
                && existing.subject == entry.subject
                && existing.config_digest == entry.config_digest
        });
        if duplicate {
            return Ok(false);
        }
        state.ledger.push(entry.clone());
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MembershipSource;
    use crate::ports::MutationActorKind;

    fn mutation_context() -> MutationContext {
        MutationContext {
            actor_id: "actor".to_string(),
            actor_kind: MutationActorKind::User,
            operation_id: Uuid::now_v7(),
            trace_id: None,
            reason: None,
        }
    }

    #[tokio::test]
    async fn resolve_conflict_same_user_different_claim_denies() {
        let stores = FakeIdentityStores::new();
        let first = stores
            .resolve
            .resolve_or_provision(ResolvePrincipalCommit {
                issuer: "iss".to_string(),
                subject: "sub".to_string(),
                claimed_user_id: Some(Uuid::from_bytes([1; 16])),
                deterministic_user_id: Uuid::from_bytes([2; 16]),
                audit: mutation_context(),
                now: Utc::now(),
            })
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(first.provisioned);

        let conflict = stores
            .resolve
            .resolve_or_provision(ResolvePrincipalCommit {
                issuer: "iss".to_string(),
                subject: "sub".to_string(),
                claimed_user_id: Some(Uuid::from_bytes([3; 16])),
                deterministic_user_id: Uuid::nil(),
                audit: mutation_context(),
                now: Utc::now(),
            })
            .await;
        assert_eq!(conflict, Err(IdentityStoreError::PrincipalMismatch));
    }

    #[tokio::test]
    async fn membership_keyset_pages_without_overlap() {
        let stores = FakeIdentityStores::new();
        for index in 0..5u8 {
            let user = PlatformUser::create(Uuid::from_bytes([index + 1; 16]), Utc::now())
                .unwrap_or_else(|_| unreachable!());
            stores.seed_user(user);
            stores.seed_membership(
                TenantMembership::join(
                    Uuid::from_bytes([index + 20; 16]),
                    Uuid::from_bytes([9; 16]),
                    Uuid::from_bytes([index + 1; 16]),
                    MembershipSource::Admin,
                    Utc::now(),
                )
                .unwrap_or_else(|_| unreachable!()),
            );
        }
        let (page1, cursor) = stores
            .query
            .list_tenant_users(Uuid::from_bytes([9; 16]), 2, None)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(page1.len(), 2);
        let cursor = cursor.unwrap_or_else(|| unreachable!());
        let (page2, _) = stores
            .query
            .list_tenant_users(Uuid::from_bytes([9; 16]), 10, Some(cursor))
            .await
            .unwrap_or_else(|_| unreachable!());
        let first_ids: Vec<Uuid> = page1.iter().map(|r| r.user.user_id()).collect();
        assert!(page2
            .iter()
            .all(|record| !first_ids.contains(&record.user.user_id())));
    }
}
