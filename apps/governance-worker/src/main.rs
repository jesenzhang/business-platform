//! Runtime Governance worker composition root.
//!
//! The process is deliberately idle until an explicit scan or approved repair
//! command wakes it. It is not a generic scheduler.

use std::sync::Arc;

use anyhow::Context;
use governance_worker::{GovernanceWorker, RepairWorker};
use runtime_governance_postgres::PostgresGovernanceStore;
use sqlx::postgres::PgPoolOptions;

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
    let _worker = GovernanceWorker::new(Arc::clone(&store), Arc::clone(&store));
    let repair_handlers = Arc::new(
        runtime_governance::processing_repairs::ProcessingRepairRegistry::new(processing_store),
    );
    let repair_worker = RepairWorker {
        persistence: Arc::clone(&store),
        handlers: repair_handlers,
        worker_id: std::env::var("GOVERNANCE_WORKER_ID")
            .unwrap_or_else(|_| "governance-worker".to_string()),
        lease_duration_secs: 30,
    };
    if std::env::var("GOVERNANCE_REPAIR_ONCE").as_deref() == Ok("true") {
        repair_worker.execute_one().await?;
    }
    tracing::info!("governance-worker ready; waiting for explicit management commands");
    shutdown_signal().await;
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
