//! Document catalog, including compatibility with the historically shared audit schema.

use sqlx::migrate::{Migrate, MigrateError, Migration};

static CATALOG: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Keeps the existing `MIGRATOR.run(pool)` / `iter()` entry points. Migration 6
/// adopts audit columns already installed by the historical processing catalog.
pub struct DocumentMigrator {
    catalog: &'static sqlx::migrate::Migrator,
}

pub static MIGRATOR: DocumentMigrator = DocumentMigrator { catalog: &CATALOG };

impl DocumentMigrator {
    pub fn iter(&self) -> std::slice::Iter<'_, Migration> {
        self.catalog.iter()
    }

    pub async fn run(&self, pool: &sqlx::SqlitePool) -> Result<(), MigrateError> {
        let mut connection = pool.acquire().await?;
        connection.ensure_migrations_table().await?;
        if let Some(version) = connection.dirty_version().await? {
            return Err(MigrateError::Dirty(version));
        }
        let applied = connection.list_applied_migrations().await?;
        // Validate the entire existing history before changing schema.
        for previous in &applied {
            let migration = self
                .catalog
                .iter()
                .find(|migration| migration.version == previous.version)
                .ok_or(MigrateError::VersionMissing(previous.version))?;
            if migration.checksum != previous.checksum {
                return Err(MigrateError::VersionMismatch(previous.version));
            }
        }
        for migration in self.catalog.iter() {
            if applied
                .iter()
                .any(|previous| previous.version == migration.version)
            {
                continue;
            }
            if migration.version == 6 {
                // SQLite lacks ADD COLUMN IF NOT EXISTS. Reserve its single
                // writer before inspecting columns; SQLx applies the migration
                // and records its checksum in a nested transaction/savepoint.
                // Column adoption, backfill, indexes and ledger commit together.
                let mut transaction =
                    sqlx::Connection::begin_with(&mut *connection, "BEGIN IMMEDIATE").await?;
                let columns = sqlx::query_scalar::<_, String>(
                    "SELECT name FROM pragma_table_info('audit_events')",
                )
                .fetch_all(&mut *transaction)
                .await?;
                for (name, statement) in [
                    (
                        "stream_sequence",
                        "ALTER TABLE audit_events ADD COLUMN stream_sequence INTEGER",
                    ),
                    (
                        "recorded_at",
                        "ALTER TABLE audit_events ADD COLUMN recorded_at TEXT",
                    ),
                    (
                        "chain_version",
                        "ALTER TABLE audit_events ADD COLUMN chain_version INTEGER",
                    ),
                ] {
                    if !columns.iter().any(|column| column == name) {
                        sqlx::query(statement).execute(&mut *transaction).await?;
                    }
                }
                transaction.apply(migration).await?;
                transaction.commit().await?;
            } else {
                connection.apply(migration).await?;
            }
        }
        Ok(())
    }
}
