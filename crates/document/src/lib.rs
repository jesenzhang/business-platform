//! 文档管理领域
//!
//! 负责文档元数据管理，包括创建、查询和列表功能。
//! 本模块遵循 DDD 分层：`domain` / `application` / `infrastructure` / `api`
//!
//! # Architecture
//!
//! - `domain`: 聚合根、领域错误、仓储端口（trait）
//! - `application`: 用例编排（创建、查询、列表）
//! - `infrastructure`: `PostgreSQL` 仓储实现
//! - `api`: Axum HTTP handler、请求/响应 DTO、路由

pub mod api;
pub mod application;
pub mod domain;
pub mod infrastructure;
