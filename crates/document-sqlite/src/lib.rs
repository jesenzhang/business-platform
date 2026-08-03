//! `SQLite` adapters for local development and single-process tests.

mod detail_query;
mod list_query;
mod mapper;
mod unit_of_work;

pub use detail_query::SqliteDocumentDetailQuery;
pub use list_query::SqliteDocumentListQuery;
pub use unit_of_work::SqliteCreateDocumentUnitOfWork;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const MAX_SQLITE_CONNECTIONS: u32 = 4;

pub fn validate_pool_size(max_connections: u32) -> Result<(), String> {
    if (1..=MAX_SQLITE_CONNECTIONS).contains(&max_connections) {
        Ok(())
    } else {
        Err(format!(
            "SQLite pool max_connections must be between 1 and {MAX_SQLITE_CONNECTIONS}"
        ))
    }
}

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

    validate_pool_size(max_connections).map_err(|message| {
        sqlx::Error::Configuration(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            message,
        )))
    })?;
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_pool_size_is_positive_and_bounded() {
        assert!(validate_pool_size(1).is_ok());
        assert!(validate_pool_size(MAX_SQLITE_CONNECTIONS).is_ok());
        assert!(validate_pool_size(0).is_err());
        assert!(validate_pool_size(MAX_SQLITE_CONNECTIONS + 1).is_err());
    }
}
