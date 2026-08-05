//! Runtime Governance worker composition root.
//!
//! The process is deliberately idle until an explicit scan or approved repair
//! command wakes it. It is not a generic scheduler.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Context;
use governance_worker::{GovernanceWorker, RepairWorker};
use runtime_governance_postgres::PostgresGovernanceStore;
use sqlx::postgres::PgPoolOptions;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = std::env::var("GOVERNANCE_DATABASE_BACKEND")
        .context("GOVERNANCE_DATABASE_BACKEND is required")?;
    let url =
        std::env::var("GOVERNANCE_DATABASE_URL").context("GOVERNANCE_DATABASE_URL is required")?;
    if backend != "postgres" {
        anyhow::bail!("governance-worker production mode requires PostgreSQL");
    }
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .context("connect governance database")?;
    runtime_migration::MIGRATOR
        .run(&pool)
        .await
        .context("apply runtime migrations")?;
    let processing_store = Arc::new(document_processing_postgres::PostgresProcessingStore::new(
        pool.clone(),
    ));
    let store = Arc::new(PostgresGovernanceStore::new(pool));
    let governance = GovernanceWorker::new(Arc::clone(&store), Arc::clone(&store))
        .context("register governance integrity rules")?;
    let rule_registry = Arc::new(governance.registry);
    let repair_handlers = Arc::new(
        runtime_governance::processing_repairs::ProcessingRepairRegistry::new(processing_store),
    );
    let lease_duration_secs = positive_i64("GOVERNANCE_LEASE_DURATION_SECS", 30)?;
    let poll_interval_ms = positive_u64("GOVERNANCE_POLL_INTERVAL_MILLIS", 1_000)?;
    let heartbeat_seconds = positive_i64("GOVERNANCE_HEARTBEAT_INTERVAL_SECS", 10)?;
    let concurrency = positive_u32("GOVERNANCE_CONCURRENCY", 1)?;
    let max_attempts = positive_u32("GOVERNANCE_MAX_ATTEMPTS", 3)?;
    if heartbeat_seconds >= lease_duration_secs {
        anyhow::bail!("GOVERNANCE_LEASE_DURATION_SECS must be greater than GOVERNANCE_HEARTBEAT_INTERVAL_SECS");
    }
    if concurrency != 1 {
        anyhow::bail!("GOVERNANCE_CONCURRENCY currently supports only 1");
    }
    let worker_id =
        std::env::var("GOVERNANCE_WORKER_ID").unwrap_or_else(|_| "governance-worker".to_string());
    if worker_id.trim().is_empty() {
        anyhow::bail!("GOVERNANCE_WORKER_ID must not be empty");
    }
    let once = std::env::var("GOVERNANCE_REPAIR_ONCE").as_deref() == Ok("true");
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = Arc::clone(&stop);
    let repair_worker = RepairWorker {
        persistence: Arc::clone(&store),
        handlers: repair_handlers,
        rule_registry: Some(rule_registry),
        worker_id,
        lease_duration_secs,
        heartbeat_seconds,
        max_attempts,
    };
    tracing::info!(
        poll_interval_ms,
        heartbeat_seconds,
        concurrency,
        max_attempts,
        once,
        "governance repair consumer configured"
    );
    let signal_task = if once {
        None
    } else {
        Some(tokio::spawn(async move {
            if let Err(error) = shutdown_signal().await {
                tracing::error!(error = %error, "governance shutdown signal listener failed");
            }
            signal_stop.store(true, Ordering::Release);
        }))
    };
    repair_worker
        .run_loop(
            Duration::from_millis(poll_interval_ms),
            heartbeat_seconds,
            concurrency,
            once,
            stop,
        )
        .await?;
    if let Some(signal_task) = signal_task {
        signal_task.abort();
        if let Err(error) = signal_task.await {
            if !error.is_cancelled() {
                tracing::error!(error = %error, "governance shutdown signal task join failed");
            }
        }
    }
    tracing::info!("governance-worker stopped claiming new work");
    Ok(())
}

fn positive_i64(name: &str, default: i64) -> anyhow::Result<i64> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| anyhow::anyhow!("{name} must be > 0")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(anyhow::anyhow!("{name} is invalid: {error}")),
    }
}

fn positive_u64(name: &str, default: u64) -> anyhow::Result<u64> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| anyhow::anyhow!("{name} must be > 0")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(anyhow::anyhow!("{name} is invalid: {error}")),
    }
}

fn positive_u32(name: &str, default: u32) -> anyhow::Result<u32> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| anyhow::anyhow!("{name} must be > 0")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(anyhow::anyhow!("{name} is invalid: {error}")),
    }
}

async fn shutdown_signal() -> anyhow::Result<()> {
    let ctrl_c = async { tokio::signal::ctrl_c().await.map_err(anyhow::Error::from) };
    #[cfg(unix)]
    let terminate = async {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(anyhow::Error::from)?;
        signal
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("termination signal stream ended"))
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<anyhow::Result<()>>();
    tokio::select! {
        result = ctrl_c => result,
        result = terminate => result,
    }
}
