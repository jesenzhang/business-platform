use std::net::SocketAddr;
use std::sync::Arc;

#[allow(dead_code)]
mod api_error;
#[allow(dead_code)]
mod api_response;
mod routes;
mod state;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = shared_kernel::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;

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

    let app = routes::create_router(state);

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    tracing::info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
