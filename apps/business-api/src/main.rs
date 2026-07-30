use std::net::SocketAddr;
use std::sync::Arc;

use business_api::auth::AuthMiddlewareConfig;
use business_api::routes;
use business_api::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = shared_kernel::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;

    // Fail fast on invalid configuration before touching any infrastructure.
    if let Err(e) = config.validate() {
        eprintln!("Configuration validation failed:\n{e}");
        std::process::exit(1);
    }

    let _guard = observability::init_tracing(&config.observability)?;

    tracing::info!(service = %config.observability.service_name, "Starting business-api");

    // Database pool
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(std::time::Duration::from_secs(
            config.database.acquire_timeout_secs,
        ))
        .connect(&config.database.url)
        .await?;

    tracing::info!("Database connection established");

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
    });

    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled: config.auth.dev_auth_enabled,
        dev_secret: config
            .auth
            .dev_secret
            .as_ref()
            .map(|secret| secret.expose().clone()),
    };

    let app = routes::create_router(state, auth_config);

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
        // `expect` is acceptable here: installing the SIGTERM handler is a
        // startup-time static initialization that cannot fail in practice, and
        // a failure would mean the process can never be drained gracefully.
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received, draining connections");
}
