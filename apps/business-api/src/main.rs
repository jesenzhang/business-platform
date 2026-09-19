use std::net::SocketAddr;

use business_api::composition::build_app;
use business_api::config::BusinessApiConfig;

#[tokio::main]
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

    // PLAN-0013 Stage 10: the entire application composition (storage,
    // database adapters for either backend, the authorization and IAM use
    // graphs, startup-only bootstrap, OIDC, and the router) lives in
    // `composition::build_app`, which the end-to-end tests run verbatim.
    let app = build_app(&config).await?;

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
