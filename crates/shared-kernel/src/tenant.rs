use serde::{Deserialize, Serialize};

/// 租户上下文，贯穿整个请求生命周期
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantContext {
    /// 租户 ID
    pub tenant_id: String,
    /// 当前用户 ID
    pub user_id: String,
    /// 用户角色列表
    pub roles: Vec<String>,
    /// 认证级别（用于高风险操作二次认证判断）
    #[serde(default = "default_auth_level")]
    pub authentication_level: AuthLevel,
}

/// 认证级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthLevel {
    /// 标准认证（密码/OIDC）
    #[default]
    Standard,
    /// 增强认证（MFA/二次验证）
    Elevated,
}

impl TenantContext {
    pub fn new(tenant_id: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            user_id: user_id.into(),
            roles: Vec::new(),
            authentication_level: AuthLevel::Standard,
        }
    }

    /// 检查是否具有指定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

fn default_auth_level() -> AuthLevel {
    AuthLevel::Standard
}
