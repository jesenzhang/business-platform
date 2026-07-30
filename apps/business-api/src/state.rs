use shared_kernel::AppConfig;
use sqlx::PgPool;

/// 应用共享状态
pub struct AppState {
    pub pool: PgPool,
    pub config: AppConfig,
}
