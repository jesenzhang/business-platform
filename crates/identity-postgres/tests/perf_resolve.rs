#![allow(clippy::expect_used)]
//! PLAN-0013 Stage 12 performance evidence — principal resolve overhead.
//!
//! Every authenticated request runs `resolve_or_provision` (OIDC →
//! principal path). This harness samples the two shapes of that call
//! against real `PostgreSQL`: repeat resolutions of an existing external
//! key (the per-request hot path) and cold provisioning of fresh subjects.
//! Results print as `PERF-EVIDENCE` lines for the Completion Audit.
//!
//! Run (fresh database):
//!   cargo run -p migration -- --backend postgres up
//!   cargo test -p identity-postgres --test `perf_resolve` -- --ignored --nocapture

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use uuid::Uuid;

use identity::application::EXTERNAL_IDENTITY_NAMESPACE;
use identity::ports::{MutationActorKind, MutationContext, ResolvePrincipalCommit};
use identity_postgres::PostgresIdentityStore;

fn audit() -> MutationContext {
    MutationContext {
        actor_id: Uuid::now_v7().to_string(),
        actor_kind: MutationActorKind::User,
        operation_id: Uuid::now_v7(),
        trace_id: None,
        reason: Some("perf harness".to_string()),
    }
}

fn resolve_commit(issuer: &str, subject: &str) -> ResolvePrincipalCommit {
    let deterministic = Uuid::new_v5(
        &EXTERNAL_IDENTITY_NAMESPACE,
        format!("{issuer}\u{1f}{subject}").as_bytes(),
    );
    ResolvePrincipalCommit {
        issuer: issuer.to_string(),
        subject: subject.to_string(),
        claimed_user_id: None,
        deterministic_user_id: deterministic,
        audit: audit(),
        now: Utc::now(),
    }
}

fn percentile(sorted: &[Duration], pct: usize) -> Duration {
    let index = sorted.len().saturating_mul(pct) / 100;
    sorted[usize::min(index, sorted.len() - 1)]
}

// One linear evidence script: seed, warmup, hot samples, cold samples.
#[allow(clippy::too_many_lines)]
#[tokio::test]
#[ignore = "requires PostgreSQL (set DATABASE_URL; perf evidence harness, run with --ignored --nocapture)"]
async fn resolve_hot_path_percentiles_on_postgres() {
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
    let store = Arc::new(PostgresIdentityStore::new(pool));
    let resolve = store.resolve_port();

    let issuer = format!("https://perf-resolve-{}.invalid", Uuid::now_v7());
    let warm_subject = "warm-principal";
    resolve
        .resolve_or_provision(resolve_commit(&issuer, warm_subject))
        .await
        .expect("seed warm principal");

    for _ in 0..50 {
        resolve
            .resolve_or_provision(resolve_commit(&issuer, warm_subject))
            .await
            .expect("warmup resolve");
    }

    let samples = 2_000usize;
    let mut hot = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        resolve
            .resolve_or_provision(resolve_commit(&issuer, warm_subject))
            .await
            .expect("hot resolve");
        hot.push(started.elapsed());
    }
    hot.sort();
    println!(
        "PERF-EVIDENCE resolve_existing samples={samples} p50_us={} p95_us={} p99_us={}",
        percentile(&hot, 50).as_micros(),
        percentile(&hot, 95).as_micros(),
        percentile(&hot, 99).as_micros(),
    );

    let cold_samples = 200usize;
    let mut cold = Vec::with_capacity(cold_samples);
    for index in 0..cold_samples {
        let subject = format!("cold-{index}-{}", Uuid::now_v7());
        let started = Instant::now();
        resolve
            .resolve_or_provision(resolve_commit(&issuer, &subject))
            .await
            .expect("cold provision");
        cold.push(started.elapsed());
    }
    cold.sort();
    println!(
        "PERF-EVIDENCE resolve_provision samples={cold_samples} p50_us={} p95_us={}",
        percentile(&cold, 50).as_micros(),
        percentile(&cold, 95).as_micros(),
    );

    // Sanity bounds only (see Stage 12 note in the Completion Audit).
    assert!(
        percentile(&hot, 99) < Duration::from_millis(50),
        "resolve P99 regressed past 50ms: {:?}",
        percentile(&hot, 99)
    );
    assert!(
        percentile(&cold, 95) < Duration::from_millis(100),
        "provision P95 regressed past 100ms: {:?}",
        percentile(&cold, 95)
    );
}
