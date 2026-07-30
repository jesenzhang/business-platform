use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// 统一实体 ID 类型，基于 UUIDv7（时间有序）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(Uuid);

impl EntityId {
    /// 生成新的时间有序 ID
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// 从已有 UUID 构造
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// 从字符串解析
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// 获取内部 UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// 转为字符串
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for EntityId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<EntityId> for Uuid {
    fn from(id: EntityId) -> Self {
        id.0
    }
}
