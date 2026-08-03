//! Database migration CLI.
//!
//! Usage:
//!   cargo run -p migration -- up       Apply all pending migrations
//!   cargo run -p migration -- status   Show migration status
//!
//! The database is taken from `DATABASE_URL`, falling back to the local
//! development default. Migrations live in the workspace-root `migrations/`
//! directory and are embedded once by the shared `runtime-migration` catalog.

use anyhow::Context;
use sqlx::migrate::MigrateDatabase;
use sqlx::Row;

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::MigrationConfig::load()
        .map_err(|error| anyhow::anyhow!("failed to load migration configuration: {error}"))?;
    if let Err(error) = config.validate() {
        eprintln!("Migration configuration validation failed:\n{error}");
        std::process::exit(1);
    }

    let command = std::env::args().nth(1).unwrap_or_default();

    match command.as_str() {
        "up" => run_up(config.database.url.expose()).await,
        "status" => run_status(config.database.url.expose()).await,
        _ => {
            eprintln!("Usage: migration <up|status>");
            eprintln!("  up      Apply all pending migrations");
            eprintln!("  status  Show applied and pending migrations");
            std::process::exit(1);
        }
    }
}

/// Apply all pending migrations, creating the database if it does not exist.
async fn run_up(database_url: &str) -> anyhow::Result<()> {
    if !sqlx::Postgres::database_exists(database_url).await? {
        tracing::info!("Creating database");
        sqlx::Postgres::create_database(database_url).await?;
    }

    let pool = sqlx::PgPool::connect(database_url)
        .await
        .context("failed to connect to database")?;

    tracing::info!("Applying migrations");
    runtime_migration::MIGRATOR
        .run(&pool)
        .await
        .context("failed to apply migrations")?;

    tracing::info!("All migrations applied successfully");
    Ok(())
}

/// Print applied migrations (from `_sqlx_migrations`) and the full set of
/// migrations known to the binary, labelling those not yet applied as pending.
async fn run_status(database_url: &str) -> anyhow::Result<()> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .context("failed to connect to database")?;

    let applied = sqlx::query(
        "SELECT version, description, installed_on FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(&pool)
    .await;

    let mut applied_versions: Vec<i64> = Vec::new();

    match applied {
        Ok(rows) => {
            println!("Applied migrations:");
            if rows.is_empty() {
                println!("  (none)");
            }
            for row in rows {
                let version: i64 = row.get("version");
                let description: String = row.get("description");
                applied_versions.push(version);
                println!("  [applied] {version} {description}");
            }
        }
        Err(_) => {
            println!("No migrations have been applied yet (migration table not found).");
        }
    }

    println!("\nAvailable migrations:");
    for migration in runtime_migration::MIGRATOR.iter() {
        let state = if applied_versions.contains(&migration.version) {
            "applied"
        } else {
            "pending"
        };
        println!(
            "  [{state}] {} {}",
            migration.version, migration.description
        );
    }

    Ok(())
}
