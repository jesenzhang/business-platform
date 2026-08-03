//! `SQLite` adapters for local development and single-process tests.

mod detail_query;
mod list_query;
mod mapper;
mod repository;
mod unit_of_work;

pub use detail_query::SqliteDocumentDetailQuery;
pub use list_query::SqliteDocumentListQuery;
pub use repository::SqliteDocumentQueryRepository;
pub use unit_of_work::SqliteCreateDocumentUnitOfWork;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[must_use]
pub fn latest_migration_version() -> i64 {
    MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

pub async fn connect(
    database_url: &str,
    max_connections: u32,
) -> Result<sqlx::SqlitePool, sqlx::Error> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use std::time::Duration;

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
}
