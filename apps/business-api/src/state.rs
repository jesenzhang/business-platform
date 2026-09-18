use std::sync::Arc;

use async_trait::async_trait;
use audit::AuditQuery;
use data_integrity::{IntegrityPersistencePort, IntegrityQueryPort};
use data_repair::{RepairHandlerRegistry, RepairPersistencePort};
use document::application::CreateDocumentMetadata;
use document::query::{DocumentDetailQuery, DocumentListQuery};
use document_processing::ports::{
    CandidateQuery, ProcessingExecutionUnitOfWork, ProcessingJobQuery, ProcessingStepQuery,
};
use identity::application::ResolveAuthenticatedUser;
use object_storage::ObjectStorageClient;
use policy::application::Authorize;
use runtime_governance::IntegrityScanPort;
use sqlx::PgPool;

/// Application services injected by the composition root.
#[derive(Clone)]
pub struct DocumentServices {
    pub create: Arc<CreateDocumentMetadata>,
    pub detail: Arc<dyn DocumentDetailQuery>,
    pub list: Arc<dyn DocumentListQuery>,
}

#[derive(Clone)]
pub struct ProcessingServices {
    pub queries: Arc<dyn ProcessingJobQuery>,
    pub candidate_queries: Arc<dyn CandidateQuery>,
    pub step_queries: Arc<dyn ProcessingStepQuery>,
    pub execution: Arc<dyn ProcessingExecutionUnitOfWork>,
}

/// Identity/Policy authorization services injected by the composition
/// root (PLAN-0013 Stage 7). Handlers and middleware call these
/// application use cases directly; no handler receives stores or
/// implements authorization rules.
#[derive(Clone)]
pub struct AccessServices {
    /// Identity resolve-or-provision use case (write-on-read).
    pub resolve: Arc<ResolveAuthenticatedUser>,
    /// Policy default-DENY evaluation use case.
    pub authorize: Arc<Authorize>,
    /// Locked compat bridge flag (`auth.management_permission_compat_enabled`).
    /// When false, the middleware carries an empty compat grant set and
    /// only `RoleBindings` can grant.
    pub compat_enabled: bool,
    /// Configured OIDC issuer; the resolver namespace for OIDC-authenticated
    /// requests (dev-auth uses the fixed `urn:business-api:dev-auth`).
    pub oidc_issuer: String,
}

/// Management-only governance ports.  Handlers receive typed ports rather
/// than database pools, SQL strings, or storage credentials.
#[derive(Clone)]
pub struct GovernanceServices {
    pub scans: Arc<dyn IntegrityScanPort>,
    pub integrity_queries: Arc<dyn IntegrityQueryPort>,
    pub integrity_persistence: Arc<dyn IntegrityPersistencePort>,
    pub repair_persistence: Arc<dyn RepairPersistencePort>,
    pub repair_handlers: Arc<dyn RepairHandlerRegistry>,
    pub audit_queries: Arc<dyn AuditQuery>,
}

pub struct SqliteReadinessProbe {
    pool: sqlx::SqlitePool,
}

impl SqliteReadinessProbe {
    #[must_use]
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReadinessProbe for SqliteReadinessProbe {
    async fn check(&self) -> ReadinessReport {
        let database = sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| "available")
            .unwrap_or("unavailable");
        let applied =
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
                .fetch_one(&self.pool)
                .await;
        let migrations = match applied {
            Ok(version) if version == document_sqlite::latest_migration_version() => "equal",
            Ok(0) => "empty",
            Ok(version) if version < document_sqlite::latest_migration_version() => "behind",
            Ok(_) => "ahead",
            Err(_) => "unknown",
        };
        ReadinessReport {
            status: if database == "available" && migrations == "equal" {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotReady
            },
            database,
            migrations,
        }
    }
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
pub struct StorageServices {
    pub objects: Arc<dyn ObjectStorageClient>,
}

#[derive(Clone)]
pub struct AppState {
    pub documents: DocumentServices,
    pub processing: Option<ProcessingServices>,
    pub governance: Option<GovernanceServices>,
    pub readiness: Arc<dyn ReadinessProbe>,
    pub storage: Option<StorageServices>,
    /// PLAN-0013 Stage 7 platform authorization services. `None` is a
    /// fail-closed misconfiguration: every protected request is rejected
    /// with a retryable 503 rather than bypassing identity resolution.
    pub access: Option<AccessServices>,
}

pub struct PostgresReadinessProbe {
    pool: PgPool,
}

impl PostgresReadinessProbe {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
                .await;
        let Ok(migration_version) = migration_version else {
            return ReadinessReport {
                status: ReadinessStatus::NotReady,
                database,
                migrations: "unknown",
            };
        };
        let compatibility = runtime_migration::classify(migration_version);
        let migrations = compatibility.as_str();
        ReadinessReport {
            status: if compatibility.is_ready() {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotReady
            },
            database,
            migrations,
        }
    }
}
