use shared_kernel::AppConfig;
use sqlx::PgPool;

/// 应用共享状态。
///
/// `pool` 是内部基础设施句柄，仅供健康探针（readiness）和未来依赖注入使用。
/// 业务 handler 不应直接持有 `PgPool`，而应接收 application 层服务
/// （use case / port），由 application 层编排数据库访问。当前尚无业务 handler，
/// 此处仅为结构化预留。
pub struct AppState {
    /// 内部数据库连接池：健康检查与未来 DI 容器使用，不直接暴露给业务 handler。
    pub pool: PgPool,
    pub config: AppConfig,
}
