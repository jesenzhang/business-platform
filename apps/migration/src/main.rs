//! Database migration CLI.
//!
//! Usage:
//!   cargo run -p migration -- --backend postgres up
//!   cargo run -p migration -- --backend sqlite status
//!
//! The database URL comes from process configuration (with the legacy
//! `DATABASE_URL` fallback). `PostgreSQL` uses the shared `runtime-migration`
//! catalog; `SQLite` owns an independent catalog in `document-sqlite`.

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
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (backend, command) = parse_arguments(&arguments)?;
    let schemes: &[&str] = match backend {
        DatabaseBackend::Postgres => &["postgres", "postgresql"],
        DatabaseBackend::Sqlite => &["sqlite"],
    };
    if let Err(error) = config.validate_scheme(schemes) {
        eprintln!("Migration configuration validation failed:\n{error}");
        std::process::exit(1);
    }

    match (backend, command) {
        (DatabaseBackend::Postgres, "up") => run_postgres_up(config.database.url.expose()).await,
        (DatabaseBackend::Postgres, "status") => {
            run_postgres_status(config.database.url.expose()).await
        }
        (DatabaseBackend::Sqlite, "up") => run_sqlite_up(config.database.url.expose()).await,
        (DatabaseBackend::Sqlite, "status") => {
            run_sqlite_status(config.database.url.expose()).await
        }
        _ => {
            eprintln!("Usage: migration --backend <postgres|sqlite> <up|status>");
            eprintln!("  up      Apply all pending migrations");
            eprintln!("  status  Show applied and pending migrations");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatabaseBackend {
    Postgres,
    Sqlite,
}

fn parse_arguments(arguments: &[String]) -> anyhow::Result<(DatabaseBackend, &str)> {
    if arguments.len() != 3 || arguments[0] != "--backend" {
        anyhow::bail!("Usage: migration --backend <postgres|sqlite> <up|status>");
    }
    let backend = match arguments[1].as_str() {
        "postgres" => DatabaseBackend::Postgres,
        "sqlite" => DatabaseBackend::Sqlite,
        value => anyhow::bail!("unsupported database backend: {value}"),
    };
    Ok((backend, arguments[2].as_str()))
}

/// Apply all pending migrations, creating the database if it does not exist.
async fn run_postgres_up(database_url: &str) -> anyhow::Result<()> {
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
async fn run_postgres_status(database_url: &str) -> anyhow::Result<()> {
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
        Err(error) if is_postgres_missing_migration_table(&error) => {
            println!("No migrations have been applied yet (migration table not found).");
        }
        Err(error) => return Err(error.into()),
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

async fn run_sqlite_up(database_url: &str) -> anyhow::Result<()> {
    if !sqlx::Sqlite::database_exists(database_url).await? {
        sqlx::Sqlite::create_database(database_url).await?;
    }
    let pool = sqlx::SqlitePool::connect(database_url)
        .await
        .context("failed to connect to SQLite database")?;
    document_sqlite::MIGRATOR
        .run(&pool)
        .await
        .context("failed to apply SQLite migrations")?;
    tracing::info!("All SQLite migrations applied successfully");
    Ok(())
}

async fn run_sqlite_status(database_url: &str) -> anyhow::Result<()> {
    let pool = sqlx::SqlitePool::connect(database_url)
        .await
        .context("failed to connect to SQLite database")?;
    let applied_versions =
        match sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
        {
            Ok(versions) => versions,
            Err(error) if is_sqlite_missing_migration_table(&error) => Vec::new(),
            Err(error) => return Err(error.into()),
        };
    println!("SQLite migrations:");
    for migration in document_sqlite::MIGRATOR.iter() {
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

fn is_postgres_missing_migration_table(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database_error)
            if database_error.code().as_deref() == Some("42P01")
    )
}

fn is_sqlite_missing_migration_table(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database_error)
            if is_missing_migration_table_message(database_error.message())
    )
}

fn is_missing_migration_table_message(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    normalized == "no such table: _sqlx_migrations"
        || normalized == "no such table _sqlx_migrations"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_explicit_backend() {
        assert!(parse_arguments(&["up".to_string()]).is_err());
        let arguments = [
            "--backend".to_string(),
            "sqlite".to_string(),
            "up".to_string(),
        ];
        let parsed = parse_arguments(&arguments);
        assert!(matches!(parsed, Ok((DatabaseBackend::Sqlite, "up"))));
    }

    #[test]
    fn only_missing_catalog_errors_are_suppressed() {
        assert!(is_missing_migration_table_message(
            "no such table: _sqlx_migrations"
        ));
        assert!(!is_missing_migration_table_message("permission denied"));
        assert!(!is_sqlite_missing_migration_table(&sqlx::Error::Protocol(
            "permission denied".to_string(),
        )));
        assert!(!is_postgres_missing_migration_table(
            &sqlx::Error::Protocol("connection refused".to_string(),)
        ));
    }
}
