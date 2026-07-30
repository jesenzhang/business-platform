//! Database migration CLI.
//!
//! Usage:
//!   cargo run -p migration -- up       Apply all pending migrations
//!   cargo run -p migration -- status   Show migration status
//!
//! The database is taken from `DATABASE_URL`, falling back to the local
//! development default. Migrations live in the workspace-root `migrations/`
//! directory and are embedded at compile time via `sqlx::migrate!`.

use anyhow::Context;
use sqlx::migrate::MigrateDatabase;
use sqlx::Row;

/// Fallback connection string for local development.
const DEFAULT_DATABASE_URL: &str =
    "postgres://postgres:postgres@localhost:5432/enterprise_platform";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());

    let command = std::env::args().nth(1).unwrap_or_default();

    match command.as_str() {
        "up" => run_up(&database_url).await,
        "status" => run_status(&database_url).await,
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

    // `sqlx::migrate!` requires a literal and resolves it relative to this
    // crate's manifest directory (`apps/migration`), so the workspace-root
    // `migrations/` directory is two levels up.
    let migrator = sqlx::migrate!("../../migrations");

    tracing::info!("Applying migrations");
    migrator
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

    let migrator = sqlx::migrate!("../../migrations");

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
    for migration in migrator.iter() {
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
