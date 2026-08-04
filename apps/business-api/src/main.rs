use std::net::SocketAddr;
use std::sync::Arc;

use business_api::auth::AuthMiddlewareConfig;
use business_api::config::{BusinessApiConfig, DatabaseBackend};
use business_api::routes;
use business_api::state::{
    AppState, DocumentServices, GovernanceServices, PostgresReadinessProbe, ReadinessProbe,
    SqliteReadinessProbe,
};

type PersistenceAdapters = (
    Arc<dyn document::ports::CreateDocumentUnitOfWork>,
    Arc<dyn document::query::DocumentDetailQuery>,
    Arc<dyn document::query::DocumentListQuery>,
    Arc<dyn ReadinessProbe>,
    Arc<dyn document_processing::ports::ProcessingJobQuery>,
    Arc<dyn document_processing::ports::CandidateQuery>,
    Arc<dyn document_processing::ports::ProcessingStepQuery>,
    Arc<dyn document_processing::ports::ProcessingExecutionUnitOfWork>,
    GovernanceServices,
);

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    let config =
        BusinessApiConfig::load().map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;

    // Fail fast on invalid configuration before touching any infrastructure.
    if let Err(e) = config.validate() {
        eprintln!("Configuration validation failed:\n{e}");
        std::process::exit(1);
    }

    let _guard = observability::init_tracing(
        &config.observability.service_name,
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;

    tracing::info!(service = %config.observability.service_name, "Starting business-api");

    let (
        unit_of_work,
        detail,
        list,
        readiness,
        processing_queries,
        processing_candidate_queries,
        processing_step_queries,
        processing_execution,
        governance,
    ): PersistenceAdapters = match config.database.backend {
        DatabaseBackend::Postgres => {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(config.database.max_connections)
                .min_connections(config.database.min_connections)
                .acquire_timeout(std::time::Duration::from_secs(
                    config.database.acquire_timeout_secs,
                ))
                .connect(config.database.url.expose())
                .await?;
            let processing_store = Arc::new(
                document_processing_postgres::PostgresProcessingStore::new(pool.clone()),
            );
            let governance_store = Arc::new(
                runtime_governance_postgres::PostgresGovernanceStore::new(pool.clone()),
            );
            let audit_store = Arc::new(audit_postgres::PostgresAuditStore::new(pool.clone()));
            let scanner = Arc::new(runtime_governance::ExplicitIntegrityScanner::new(
                governance_store.clone(),
                governance_store.clone(),
            ));
            let repair_handlers = Arc::new(
                runtime_governance::processing_repairs::ProcessingRepairRegistry::new(
                    processing_store.clone(),
                ),
            );
            (
                Arc::new(document_postgres::PostgresCreateDocumentUnitOfWork::new(
                    pool.clone(),
                )),
                Arc::new(document_postgres::PostgresDocumentDetailQuery::new(
                    pool.clone(),
                )),
                Arc::new(document_postgres::PostgresDocumentListQuery::new(
                    pool.clone(),
                )),
                Arc::new(PostgresReadinessProbe::new(pool.clone())),
                processing_store.clone(),
                processing_store.clone(),
                processing_store.clone(),
                processing_store,
                GovernanceServices {
                    scans: scanner,
                    integrity_queries: governance_store.clone(),
                    integrity_persistence: governance_store.clone(),
                    repair_persistence: governance_store,
                    repair_handlers,
                    audit_queries: audit_store,
                },
            )
        }
        DatabaseBackend::Sqlite => {
            let pool = document_sqlite::connect(
                config.database.url.expose(),
                config.database.max_connections,
            )
            .await?;
            document_processing_sqlite::run_migrations(&pool).await?;
            let processing_store = Arc::new(
                document_processing_sqlite::SqliteProcessingStore::new(pool.clone()),
            );
            let governance_store = Arc::new(runtime_governance_sqlite::SqliteGovernanceStore::new(
                pool.clone(),
            ));
            let audit_store = Arc::new(audit_sqlite::SqliteAuditStore::new(pool.clone()));
            let scanner = Arc::new(runtime_governance::ExplicitIntegrityScanner::new(
                governance_store.clone(),
                governance_store.clone(),
            ));
            let repair_handlers = Arc::new(
                runtime_governance::processing_repairs::ProcessingRepairRegistry::new(
                    processing_store.clone(),
                ),
            );
            (
                Arc::new(document_sqlite::SqliteCreateDocumentUnitOfWork::new(
                    pool.clone(),
                )),
                Arc::new(document_sqlite::SqliteDocumentDetailQuery::new(
                    pool.clone(),
                )),
                Arc::new(document_sqlite::SqliteDocumentListQuery::new(pool.clone())),
                Arc::new(SqliteReadinessProbe::new(pool.clone())),
                processing_store.clone(),
                processing_store.clone(),
                processing_store.clone(),
                processing_store,
                GovernanceServices {
                    scans: scanner,
                    integrity_queries: governance_store.clone(),
                    integrity_persistence: governance_store.clone(),
                    repair_persistence: governance_store,
                    repair_handlers,
                    audit_queries: audit_store,
                },
            )
        }
    };

    tracing::info!(backend = ?config.database.backend, "Database connection established");
    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(document::application::CreateDocumentMetadata::new(
                unit_of_work,
            )),
            detail,
            list,
        },
        processing: Some(business_api::state::ProcessingServices {
            queries: processing_queries,
            candidate_queries: processing_candidate_queries,
            step_queries: processing_step_queries,
            execution: processing_execution,
        }),
        governance: Some(governance),
        readiness,
    });

    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled: config.auth.dev_auth_enabled,
        dev_secret: config
            .auth
            .dev_secret
            .as_ref()
            .map(|secret| secret.expose().clone()),
    };

    let app = routes::create_router(state, auth_config, &config.server);

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    tracing::info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Resolve the process shutdown signal (Ctrl-C or SIGTERM).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                let _ = signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "failed to install SIGTERM handler");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received, draining connections");
}
