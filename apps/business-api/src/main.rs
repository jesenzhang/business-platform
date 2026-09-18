use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context;

use business_api::auth::{AuthMiddlewareConfig, ManagementPermission};
use business_api::bootstrap::{dev_auth_bootstrap_config, BootstrapComposition};
use business_api::config::{BusinessApiConfig, DatabaseBackend, StorageBackend};
use business_api::platform_authorization::{
    IdentitySubjectStatusBridge, IdentityTenantMembershipBridge, OrganizationScopeBridge,
};
use business_api::routes;
use business_api::state::{
    AccessServices, AdminServices, AppState, DocumentServices, GovernanceServices,
    PostgresReadinessProbe, ReadinessProbe, SqliteReadinessProbe, StorageServices,
};
use identity::application::{
    BootstrapAdministratorConfig, ChangeTenantMembershipStatus, CreateTenantMembership,
    GetTenantUser, ListMemberships, ListTenantUsers, ResolveAuthenticatedUser, TenantAccessChecker,
};
use identity::ports::{
    BootstrapLedgerPort, IdentityCommandPort, IdentityQueryPort, IdentityResolvePort,
};
use object_storage::{LocalStorageClient, ObjectStorageClient, S3Client};
use organization::application::{
    AddOrganizationMember, CreateOrganizationUnit, ListOrganizationTree, ListUnitMembers,
    MoveOrganizationUnit, RemoveOrganizationMember, UpdateOrganizationUnit,
};
use organization::ports::{OrganizationCommandPort, OrganizationQueryPort};
use policy::application::{
    Authorize, BindRole, CreateRole, ExplainDecision, RevokeRoleBinding, SetRolePermissions,
    UpdateRole,
};
use policy::ports::{OrganizationScopePort, PolicyCommandPort, PolicyQueryPort, SubjectStatusPort};

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
    AccessAdapters,
);

/// Raw identity/policy/organization ports for the platform-authorization
/// and IAM management compositions (PLAN-0013 §5/§7/§9). The use cases are
/// constructed from these exactly once, after the backend match.
struct AccessAdapters {
    resolve: Arc<dyn IdentityResolvePort>,
    command: Arc<dyn IdentityCommandPort>,
    ledger: Arc<dyn BootstrapLedgerPort>,
    identity_query: Arc<dyn IdentityQueryPort>,
    subject: Arc<dyn SubjectStatusPort>,
    org_scope: Arc<dyn OrganizationScopePort>,
    org_command: Arc<dyn OrganizationCommandPort>,
    org_query: Arc<dyn OrganizationQueryPort>,
    policy_query: Arc<dyn PolicyQueryPort>,
    policy_command: Arc<dyn PolicyCommandPort>,
}

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

    let log_format =
        observability::LogFormat::parse(&config.observability.log_format).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported observability.log_format: {}",
                config.observability.log_format
            )
        })?;
    let _guard = observability::init_tracing(
        &config.observability.service_name,
        &config.observability.log_level,
        log_format,
        config.observability.otlp_endpoint.as_deref(),
    )?;

    tracing::info!(service = %config.observability.service_name, "Starting business-api");

    let storage: Arc<dyn ObjectStorageClient> = match config.storage.backend {
        StorageBackend::S3 => Arc::new(S3Client::new(
            &config.storage.endpoint,
            &config.storage.access_key,
            &config.storage.secret_key,
            &config.storage.bucket,
            &config.storage.region,
        )),
        StorageBackend::Local => Arc::new(
            LocalStorageClient::new(&config.storage.local_path)
                .await
                .map_err(|_| anyhow::anyhow!("failed to initialize local object storage"))?,
        ),
    };

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
        access_adapters,
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
            // PLAN-0013: the identity/authorization tables (019) live in the
            // same catalog; like `governance-worker`, the API applies the
            // embedded catalog at startup (idempotent).
            runtime_migration::MIGRATOR
                .run(&pool)
                .await
                .context("apply runtime migrations")?;
            let identity_store =
                Arc::new(identity_postgres::PostgresIdentityStore::new(pool.clone()));
            let organization_store = Arc::new(
                organization_postgres::PostgresOrganizationStore::new(pool.clone()),
            );
            let policy_store = Arc::new(policy_postgres::PostgresPolicyStore::new(pool.clone()));
            let identity_query = identity_store.query_port();
            let org_query = organization_store.query_port();
            let access = AccessAdapters {
                resolve: identity_store.resolve_port(),
                command: identity_store.command_port(),
                ledger: identity_store.ledger_port(),
                identity_query: Arc::clone(&identity_query),
                subject: Arc::new(IdentitySubjectStatusBridge::new(Arc::new(
                    TenantAccessChecker::new(identity_query),
                ))),
                org_scope: Arc::new(OrganizationScopeBridge::new(Arc::clone(&org_query))),
                org_command: organization_store.command_port(),
                org_query,
                policy_query: policy_store.query_port(),
                policy_command: policy_store.command_port(),
            };
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
                access,
            )
        }
        DatabaseBackend::Sqlite => {
            let pool = document_sqlite::connect(
                config.database.url.expose(),
                config.database.max_connections,
            )
            .await?;
            document_processing_sqlite::run_migrations(&pool).await?;
            // Each adapter owns its migrations through an independent
            // version catalog, so all schemas coexist in the local file.
            identity_sqlite::run_migrations(&pool).await?;
            organization_sqlite::run_migrations(&pool).await?;
            policy_sqlite::run_migrations(&pool).await?;
            let identity_store = Arc::new(identity_sqlite::SqliteIdentityStore::new(pool.clone()));
            let organization_store = Arc::new(organization_sqlite::SqliteOrganizationStore::new(
                pool.clone(),
            ));
            let policy_store = Arc::new(policy_sqlite::SqlitePolicyStore::new(pool.clone()));
            let identity_query = identity_store.query_port();
            let org_query = organization_store.query_port();
            let access = AccessAdapters {
                resolve: identity_store.resolve_port(),
                command: identity_store.command_port(),
                ledger: identity_store.ledger_port(),
                identity_query: Arc::clone(&identity_query),
                subject: Arc::new(IdentitySubjectStatusBridge::new(Arc::new(
                    TenantAccessChecker::new(identity_query),
                ))),
                org_scope: Arc::new(OrganizationScopeBridge::new(Arc::clone(&org_query))),
                org_command: organization_store.command_port(),
                org_query,
                policy_query: policy_store.query_port(),
                policy_command: policy_store.command_port(),
            };
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
                access,
            )
        }
    };

    tracing::info!(backend = ?config.database.backend, "Database connection established");

    // Install the Prometheus recorder before bootstrap so startup bootstrap
    // outcomes are counted (installation is an idempotent `OnceLock`).
    business_api::metrics::install_metrics();

    // PLAN-0013 Stage 7: the platform-authorization use cases. Both
    // backends compose identically; only the adapter types differed.
    let access_services = AccessServices {
        resolve: Arc::new(ResolveAuthenticatedUser::new(Arc::clone(
            &access_adapters.resolve,
        ))),
        authorize: Arc::new(Authorize::new(
            Arc::clone(&access_adapters.policy_query),
            Arc::clone(&access_adapters.subject),
            Arc::clone(&access_adapters.org_scope),
        )),
        compat_enabled: config.auth.management_permission_compat_enabled,
        oidc_issuer: config.auth.issuer_url.clone(),
    };
    tracing::info!(
        compat_enabled = access_services.compat_enabled,
        "platform authorization composed (compat bridge = server-trusted claim grants, bounded to the seven governance keys)"
    );

    // PLAN-0013 Stage 8: the IAM management use cases. Both backends compose
    // identically; handlers receive these typed use cases only, never a
    // store, so no business rule lives in the delivery layer.
    let tenant_reader = Arc::new(IdentityTenantMembershipBridge::new(Arc::new(
        TenantAccessChecker::new(Arc::clone(&access_adapters.identity_query)),
    )));
    let admin_services = AdminServices {
        list_users: Arc::new(ListTenantUsers::new(Arc::clone(
            &access_adapters.identity_query,
        ))),
        get_user: Arc::new(GetTenantUser::new(Arc::clone(
            &access_adapters.identity_query,
        ))),
        list_memberships: Arc::new(ListMemberships::new(Arc::clone(
            &access_adapters.identity_query,
        ))),
        create_membership: Arc::new(CreateTenantMembership::new(Arc::clone(
            &access_adapters.command,
        ))),
        change_membership_status: Arc::new(ChangeTenantMembershipStatus::new(Arc::clone(
            &access_adapters.command,
        ))),
        policy_query: Arc::clone(&access_adapters.policy_query),
        list_org_units: Arc::new(ListOrganizationTree::new(Arc::clone(
            &access_adapters.org_query,
        ))),
        list_org_members: Arc::new(ListUnitMembers::new(Arc::clone(&access_adapters.org_query))),
        create_unit: Arc::new(CreateOrganizationUnit::new(
            Arc::clone(&access_adapters.org_command),
            Arc::clone(&access_adapters.org_query),
        )),
        update_unit: Arc::new(UpdateOrganizationUnit::new(Arc::clone(
            &access_adapters.org_command,
        ))),
        move_unit: Arc::new(MoveOrganizationUnit::new(
            Arc::clone(&access_adapters.org_command),
            Arc::clone(&access_adapters.org_query),
        )),
        add_org_member: Arc::new(AddOrganizationMember::new(
            Arc::clone(&access_adapters.org_command),
            Arc::clone(&access_adapters.org_query),
            tenant_reader,
        )),
        remove_org_member: Arc::new(RemoveOrganizationMember::new(Arc::clone(
            &access_adapters.org_command,
        ))),
        create_role: Arc::new(CreateRole::new(
            Arc::clone(&access_adapters.policy_command),
            Arc::clone(&access_adapters.policy_query),
        )),
        update_role: Arc::new(UpdateRole::new(
            Arc::clone(&access_adapters.policy_command),
            Arc::clone(&access_adapters.policy_query),
        )),
        set_role_permissions: Arc::new(SetRolePermissions::new(
            Arc::clone(&access_adapters.policy_command),
            Arc::clone(&access_adapters.policy_query),
        )),
        bind_role: Arc::new(BindRole::new(
            Arc::clone(&access_adapters.policy_command),
            Arc::clone(&access_adapters.policy_query),
            Arc::clone(&access_adapters.subject),
            Arc::clone(&access_adapters.org_scope),
        )),
        revoke_binding: Arc::new(RevokeRoleBinding::new(Arc::clone(
            &access_adapters.policy_command,
        ))),
        explain: Arc::new(ExplainDecision::new(
            Arc::clone(&access_adapters.policy_query),
            Arc::clone(&access_adapters.subject),
            Arc::clone(&access_adapters.org_scope),
        )),
    };

    // PLAN-0013 §7: startup-only bootstrap; HTTP never triggers it and no
    // request surface can enable it. Mode selection is server-side:
    // dev-auth deployments (production-forbidden by config validation) get
    // the dev-principal bootstrap; everything else uses the explicit
    // `[auth.bootstrap]` section.
    let bootstrap = BootstrapComposition::new(
        Arc::clone(&access_adapters.resolve),
        Arc::clone(&access_adapters.command),
        Arc::clone(&access_adapters.ledger),
        Arc::clone(&access_adapters.policy_query),
        Arc::clone(&access_adapters.policy_command),
        Arc::clone(&access_adapters.subject),
        Arc::clone(&access_adapters.org_scope),
    );
    if config.auth.dev_auth_enabled {
        let (Some(dev_tenant_id), Some(dev_user_id), Some(dev_subject)) = (
            config.auth.dev_tenant_id,
            config.auth.dev_user_id,
            config.auth.dev_subject.clone(),
        ) else {
            anyhow::bail!("dev auth bootstrap requires the configured dev identity");
        };
        if dev_subject.trim().is_empty() {
            anyhow::bail!("dev auth bootstrap requires a non-blank dev subject");
        }
        // Version is pinned at 1 for dev mode: changing dev tenant or
        // subject afterwards fails closed as `ConfigStale` (startup error,
        // no silent re-bind). Recovery in a throwaway dev database: delete
        // the `platform_bootstrap_executions` row for the dev principal
        // (or start a fresh database). Never reuse a dev ledger in a real
        // environment.
        let bootstrap_config = dev_auth_bootstrap_config(dev_tenant_id, dev_subject, 1);
        bootstrap
            .run_dev_auth(&bootstrap_config, dev_user_id)
            .await
            .map_err(|error| anyhow::anyhow!("dev-auth bootstrap failed: {error}"))?;
    } else if config.auth.bootstrap.enabled {
        // Config validation already requires explicit values when enabled;
        // this destructure is defensive, not the gate.
        let (Some(tenant_id), Some(issuer), Some(subject)) = (
            config.auth.bootstrap.tenant_id,
            config.auth.bootstrap.issuer.clone(),
            config.auth.bootstrap.subject.clone(),
        ) else {
            anyhow::bail!("auth.bootstrap is enabled but incomplete");
        };
        let bootstrap_config = BootstrapAdministratorConfig {
            enabled: true,
            tenant_id,
            issuer,
            subject,
            version: config.auth.bootstrap.version,
        };
        match bootstrap.run_production(&bootstrap_config).await {
            Ok(Some(outcome)) => {
                tracing::info!(?outcome, "bootstrap administrator ensured");
            }
            Ok(None) => {}
            Err(error) => return Err(anyhow::anyhow!("bootstrap administrator failed: {error}")),
        }
    }

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
        storage: Some(StorageServices { objects: storage }),
        access: Some(access_services),
        admin: Some(admin_services),
    });

    // PLAN-0012 M3: the OIDC validator is built whenever an issuer is
    // configured. Dev-mode requests take the static-token path and never touch
    // it; with dev auth disabled it is the only authentication boundary, and
    // config validation guarantees a non-empty issuer at that point.
    let oidc = if config.auth.issuer_url.trim().is_empty() {
        None
    } else {
        tracing::info!(
            issuer = %config.auth.issuer_url,
            "OIDC JWT validation enabled"
        );
        // Production identity material must not travel over plaintext HTTP:
        // the validator re-checks every fetched URL under this policy.
        Some(std::sync::Arc::new(
            business_api::oidc::OidcValidator::with_transport_policy(
                config.auth.issuer_url.clone(),
                config.auth.audience.clone(),
                config.auth.jwks_url.clone(),
                config.env == runtime_config::RuntimeEnvironment::Production,
            ),
        ))
    };

    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled: config.auth.dev_auth_enabled,
        dev_secret: config
            .auth
            .dev_secret
            .as_ref()
            .map(|secret| secret.expose().clone()),
        dev_permissions: config
            .auth
            .dev_permissions
            .iter()
            .map(|value| {
                ManagementPermission::from_str(value)
                    .map_err(|()| anyhow::anyhow!("invalid auth.dev_permissions value: {value}"))
            })
            .collect::<anyhow::Result<_>>()?,
        dev_tenant_id: config.auth.dev_tenant_id,
        dev_user_id: config.auth.dev_user_id,
        dev_subject: config.auth.dev_subject.clone(),
        dev_roles: config.auth.dev_roles.clone(),
        oidc,
    };

    // PLAN-0012 T4.2: install the Prometheus recorder before serving; the
    // /metrics endpoint renders from the installed handle.
    business_api::metrics::install_metrics();

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
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to receive Ctrl-C shutdown signal");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                if signal.recv().await.is_none() {
                    tracing::error!("SIGTERM signal stream closed before shutdown");
                }
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
