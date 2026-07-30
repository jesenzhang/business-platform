# PLAN-0001 实施审查报告

> 日期：2026-07-30
> 分支：feat/PLAN-0001-foundation-hardening
> 基线：696acfb (main)

## 工作包完成状态

| WP | 名称 | 状态 | 提交 |
|---|---|---|---|
| WP-01 | 工具链与 CI | PASS | 73a2606 |
| WP-02 | 共享核心去框架化 | PASS | 7bee85d |
| WP-05 | 配置与 Secret | PASS | 0458365 |
| WP-03 | 应用组合与薄 API | PASS | 76566f5 |
| WP-04 | HTTP 安全基线 | PASS | 76566f5 |
| WP-06 | 真实对象存储适配 | PASS | 6e64466 |
| WP-07 | LocalStorage 安全 | PASS | 6e64466 |
| WP-08 | Migration 基座 | PASS | 6e64466 |
| WP-09 | 可靠 Outbox | PASS | fbe6300 |
| WP-10 | document metadata 垂直切片 | PASS | 8fac234 |

## 关键架构变化

1. shared-kernel 不再依赖 axum/sqlx/reqwest，仅保留纯类型
2. 错误分为 Domain/Application/Infrastructure/API 四层
3. HTTP 错误映射位于 business-api (api_error.rs)
4. 认证中间件：开发模式静态 Token，生产 fail-closed
5. 对象存储使用 aws-sdk-s3（Signature V4）
6. ObjectKey 值对象阻止路径穿越
7. Outbox 使用 FOR UPDATE SKIP LOCKED + lease 机制
8. document metadata 贯穿全部 DDD 分层

## 安全修复

- 移除 CorsLayer::permissive()
- Secret<T> 脱敏类型（Debug/Display 输出 [REDACTED]）
- 生产环境禁止 dev_auth_enabled
- ObjectKey 拒绝 ..、绝对路径、UNC、盘符
- 错误响应不泄漏 SQL/URL/凭证/堆栈
- 多租户查询强制 tenant_id 条件

## 新增 ADR

- ADR-0001: S3 SDK 选择 (aws-sdk-s3)
- ADR-0002: Outbox claim/retry 设计

## 数据库 Migration

- 001_initial.sql: tenants, users, roles, permissions, outbox_events, audit_events
- 002_document_metadata.sql: documents 表
- 003_outbox_reliability.sql: outbox 状态机升级

## 测试

- 56 个测试通过（单元 + 集成）
- 3 个忽略（需要 PostgreSQL/MinIO）
- 7 个安全测试
- 9 个 ObjectKey 测试
- 7 个 document domain 测试
- 8 个 document API 测试
- 18 个 shared-kernel 测试
- 6 个 outbox backoff 测试
- 1 个 migration 编译测试

## 未完成项

- MinIO 契约测试需要 Docker（标记 #[ignore]）
- PostgreSQL 集成测试需要运行数据库（标记 #[ignore]）
- 生产 OIDC 认证尚未实现（fail-closed 骨架）
- business-worker/ai-worker/agent-adapter 仍为占位

## 已接受风险

- aws-sdk-s3 增加编译时间（ADR-0001 记录）
- 开发模式使用静态 Token（仅 dev config 启用，生产禁止）

## 回滚方式

每个 WP 独立提交，可按提交粒度 revert。
Migration 采用向前修复策略，不修改已发布文件。
