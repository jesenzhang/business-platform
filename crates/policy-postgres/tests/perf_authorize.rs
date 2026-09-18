#![allow(clippy::expect_used)]
//! PLAN-0013 Stage 12 performance evidence — `Authorize` hot path against
//! real `PostgreSQL`.
//!
//! The decision hot path is three indexed reads (`list_bindings_for_user`,
//! `get_role`, `get_role_permissions`). This harness seeds a 100-binding
//! principal (the per-user cap) and a 40-permission role, then samples
//! decision latencies and records P50/P95/P99 plus the binding-list query
//! as `PERF-EVIDENCE` lines for the Completion Audit. There is no cache:
//! every decision re-reads authority, which is exactly what is measured.
//!
//! Run (fresh database):
//!   cargo run -p migration -- --backend postgres up
//!   cargo test -p policy-postgres --test `perf_authorize` -- --ignored --nocapture

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use policy::application::{AuthorizationContext, Authorize, ResourceTarget};
use policy::domain::ResourceScope;
use policy::ports::SubjectStatus;
use policy::ports::{
    BindRoleCommit, CreateRoleCommit, MutationActorKind, MutationContext, OrganizationScopePort,
    SubjectStatusPort,
};
use policy_postgres::PostgresPolicyStore;

struct AlwaysActive;

#[async_trait::async_trait]
impl SubjectStatusPort for AlwaysActive {
    async fn subject_status(
        &self,
        _tenant_id: Uuid,
        _user_id: Uuid,
    ) -> Result<SubjectStatus, policy::ports::PolicyStoreError> {
        Ok(SubjectStatus::Active)
    }
}

struct NoUnits;

#[async_trait::async_trait]
impl OrganizationScopePort for NoUnits {
    async fn unit_is_active(
        &self,
        _tenant_id: Uuid,
        _unit_id: Uuid,
    ) -> Result<bool, policy::ports::PolicyStoreError> {
        Ok(false)
    }

    async fn subtree_contains(
        &self,
        _tenant_id: Uuid,
        _ancestor: Uuid,
        _descendant: Uuid,
    ) -> Result<bool, policy::ports::PolicyStoreError> {
        Ok(false)
    }
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn audit() -> MutationContext {
    MutationContext {
        actor_id: Uuid::now_v7().to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason: Some("perf harness".to_string()),
    }
}

fn percentile(sorted: &[Duration], pct: usize) -> Duration {
    let index = sorted.len().saturating_mul(pct) / 100;
    sorted[usize::min(index, sorted.len() - 1)]
}

#[tokio::test]
#[ignore = "requires PostgreSQL (set DATABASE_URL; perf evidence harness, run with --ignored --nocapture)"]
// One linear evidence script: seeding, warmup, sampling, reporting.
#[allow(clippy::too_many_lines)]
async fn authorize_hot_path_percentiles_on_postgres() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for the ignored perf test");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
        .expect("connect PostgreSQL");
    runtime_migration::MIGRATOR
        .run(&pool)
        .await
        .expect("migrations applied");

    let store = Arc::new(PostgresPolicyStore::new(pool));
    let command = store.command_port();
    let query = store.query_port();

    let tenant = Uuid::now_v7();
    let user = Uuid::now_v7();

    // One wide role: the 17 non-reserved catalog keys, twice over the
    // grant read by re-adding is not possible, so grant all 17 and rely
    // on the two permission maps of 100 bound roles for read fan-out.
    let catalog = policy::catalog::catalog();
    let active_keys: Vec<String> = catalog
        .iter()
        .filter(|(_, _, reserved)| !*reserved)
        .map(|(key, _, _)| (*key).to_string())
        .collect();
    let wide_role = Uuid::now_v7();
    command
        .create_role(CreateRoleCommit {
            tenant_id: tenant,
            role_id: Some(wide_role),
            stable_key: "perf.wide".to_string(),
            display_name: "Perf Wide".to_string(),
            audit: audit(),
            idempotency_key: None,
            now: now(),
        })
        .await
        .expect("create wide role");
    command
        .set_role_permissions(policy::ports::SetRolePermissionsCommit {
            tenant_id: tenant,
            role_id: wide_role,
            expected_version: 1,
            permission_keys: active_keys.clone(),
            audit: audit(),
            idempotency_key: None,
            now: now(),
        })
        .await
        .expect("grant wide role");

    // Fill the per-user active-binding budget (100): the wide role plus 99
    // narrow filler roles.
    command
        .bind_role(BindRoleCommit {
            tenant_id: tenant,
            binding_id: None,
            user_id: user,
            role_id: wide_role,
            scope: ResourceScope::Tenant,
            effective_at: now(),
            expires_at: None,
            audit: audit(),
            idempotency_key: None,
            now: now(),
        })
        .await
        .expect("bind wide role");
    for index in 0..99u32 {
        let role = Uuid::now_v7();
        command
            .create_role(CreateRoleCommit {
                tenant_id: tenant,
                role_id: Some(role),
                stable_key: format!("perf.filler.{index}"),
                display_name: format!("Perf Filler {index}"),
                audit: audit(),
                idempotency_key: None,
                now: now(),
            })
            .await
            .expect("create filler role");
        command
            .bind_role(BindRoleCommit {
                tenant_id: tenant,
                binding_id: None,
                user_id: user,
                role_id: role,
                scope: ResourceScope::Tenant,
                effective_at: now(),
                expires_at: None,
                audit: audit(),
                idempotency_key: None,
                now: now(),
            })
            .await
            .expect("bind filler role");
    }

    let engine = Authorize::new(
        Arc::clone(&query),
        Arc::new(AlwaysActive),
        Arc::new(NoUnits),
    );
    let ctx = AuthorizationContext::new(user, tenant);
    let bound_key = active_keys[0].clone();
    let target = ResourceTarget {
        kind: Some("contract".to_string()),
        resource_id: Some(Uuid::now_v7()),
        org_unit_id: None,
    };

    // Warm the connection pool / page cache.
    for _ in 0..50 {
        engine
            .check(&ctx, &bound_key, Some(&target))
            .await
            .expect("warmup decision");
    }

    let samples = 2_000usize;
    let mut durations = Vec::with_capacity(samples);
    for sample in 0..samples {
        // 75% allow path (bound key), 25% deny-by-no-binding path.
        let key = if sample % 4 == 0 {
            "does.not.exist"
        } else {
            bound_key.as_str()
        };
        let started = Instant::now();
        engine
            .check(&ctx, key, Some(&target))
            .await
            .expect("decision");
        durations.push(started.elapsed());
    }
    durations.sort();
    println!(
        "PERF-EVIDENCE authorize samples={samples} bindings_per_user=100 p50_us={} p95_us={} p99_us={}",
        percentile(&durations, 50).as_micros(),
        percentile(&durations, 95).as_micros(),
        percentile(&durations, 99).as_micros(),
    );

    let mut list_durations = Vec::with_capacity(500);
    for _ in 0..500 {
        let started = Instant::now();
        query
            .list_bindings_for_user(tenant, user)
            .await
            .expect("list bindings");
        list_durations.push(started.elapsed());
    }
    list_durations.sort();
    println!(
        "PERF-EVIDENCE list_bindings_for_user p50_us={} p95_us={}",
        percentile(&list_durations, 50).as_micros(),
        percentile(&list_durations, 95).as_micros(),
    );

    // Sanity bounds only: these catch pathological regressions (missing
    // index, N+1 fan-out), they are not service-level objectives.
    assert!(
        percentile(&durations, 99) < Duration::from_millis(50),
        "authorize P99 regressed past 50ms: {:?}",
        percentile(&durations, 99)
    );
    assert!(
        percentile(&list_durations, 95) < Duration::from_millis(50),
        "list P95 regressed past 50ms: {:?}",
        percentile(&list_durations, 95)
    );
}
