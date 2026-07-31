use std::sync::Arc;

use async_trait::async_trait;
use document::application::{CreateDocumentMetadata, GetDocumentMetadata, ListDocumentMetadata};
use sqlx::PgPool;

/// Application services injected by the composition root.
#[derive(Clone)]
pub struct DocumentServices {
    pub create: Arc<CreateDocumentMetadata>,
    pub get: Arc<GetDocumentMetadata>,
    pub list: Arc<ListDocumentMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessStatus {
    Ready,
    NotReady,
}

#[async_trait]
pub trait ReadinessProbe: Send + Sync {
    async fn check(&self) -> ReadinessReport;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessReport {
    pub status: ReadinessStatus,
    pub database: &'static str,
    pub migrations: &'static str,
}

/// HTTP application state. No handler receives a database pool.
#[derive(Clone)]
pub struct AppState {
    pub documents: DocumentServices,
    pub readiness: Arc<dyn ReadinessProbe>,
}

pub struct PostgresReadinessProbe {
    pool: PgPool,
    expected_migration: i64,
}

impl PostgresReadinessProbe {
    #[must_use]
    pub fn new(pool: PgPool, expected_migration: i64) -> Self {
        Self {
            pool,
            expected_migration,
        }
    }
}

#[async_trait]
impl ReadinessProbe for PostgresReadinessProbe {
    async fn check(&self) -> ReadinessReport {
        let database = sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| "available")
            .unwrap_or("unavailable");
        if database == "unavailable" {
            return ReadinessReport {
                status: ReadinessStatus::NotReady,
                database,
                migrations: "unknown",
            };
        }

        let migration_version =
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
                .fetch_one(&self.pool)
                .await
                .unwrap_or_default();
        let migrations = if migration_version >= self.expected_migration {
            "compatible"
        } else {
            "incompatible"
        };
        ReadinessReport {
            status: if migrations == "compatible" {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotReady
            },
            database,
            migrations,
        }
    }
}
