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
    let governance = GovernanceWorker::new(Arc::clone(&store), Arc::clone(&store));
    let rule_registry = Arc::new(governance.registry);
    let repair_handlers = Arc::new(
        runtime_governance::processing_repairs::ProcessingRepairRegistry::new(processing_store),
    );
    let lease_duration_secs = std::env::var("GOVERNANCE_REPAIR_LEASE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(30);
    let poll_interval_ms = std::env::var("GOVERNANCE_REPAIR_POLL_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1_000);
    let heartbeat_seconds = std::env::var("GOVERNANCE_REPAIR_HEARTBEAT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or((lease_duration_secs / 3).max(1));
    let batch_size = std::env::var("GOVERNANCE_REPAIR_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| (1..=1_000).contains(value))
        .unwrap_or(1);
    let once = std::env::var("GOVERNANCE_REPAIR_ONCE").as_deref() == Ok("true");
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = Arc::clone(&stop);
    let repair_worker = RepairWorker {
        persistence: Arc::clone(&store),
        handlers: repair_handlers,
        rule_registry: Some(rule_registry),
        worker_id: std::env::var("GOVERNANCE_WORKER_ID")
            .unwrap_or_else(|_| "governance-worker".to_string()),
        lease_duration_secs,
        heartbeat_seconds,
    };
    tracing::info!(
        poll_interval_ms,
        heartbeat_seconds,
        batch_size,
        once,
        "governance repair consumer configured"
    );
    let signal_task = if once {
        None
    } else {
        Some(tokio::spawn(async move {
            shutdown_signal().await;
            signal_stop.store(true, Ordering::Release);
        }))
    };
    repair_worker
        .run_loop(
            Duration::from_millis(poll_interval_ms),
            heartbeat_seconds,
            batch_size,
            once,
            stop,
        )
        .await?;
    if let Some(signal_task) = signal_task {
        signal_task.abort();
    }
    tracing::info!("governance-worker stopped claiming new work");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            let _ = signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
