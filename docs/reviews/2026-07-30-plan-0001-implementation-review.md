# PLAN-0001 实施审查报告

> 日期：2026-07-31
> 分支：feat/PLAN-0001-foundation-hardening
> PR：#3 (Draft)
> 状态：Revision Required

## 工作包状态

| WP | 名称 | 状态 | 当前证据 |
|---|---|---|---|
| WP-01 | 工具链与 CI | FAIL | Rust 1.85 被锁定依赖拒绝；已选择 1.94.1，修订 CI 待运行 |
| WP-02 | 共享核心去框架化 | PASS | shared-kernel Cargo 依赖不含 Axum/SQLx/Reqwest/AWS SDK |
| WP-03 | 应用组合与薄 API | NOT RUN | Document 服务已预构造注入，完整门禁待运行 |
| WP-04 | HTTP 安全基线 | NOT RUN | API 回归和 trace correlation 待全量运行 |
| WP-05 | 配置与 Secret | NOT RUN | 本轮未重新执行全量安全测试 |
| WP-06 | 真实对象存储适配 | NOT RUN | 流式接口已修订，真实 MinIO 契约待运行 |
| WP-07 | LocalStorage 安全 | NOT RUN | 本地测试已补充，尚未执行全量门禁 |
| WP-08 | Migration 基座 | NOT RUN | 004/005/006 向前 migration 已新增，升级测试待 PostgreSQL |
| WP-09 | 可靠 Outbox | NOT RUN | ownership/fencing 已修订，真实并发恢复测试待 PostgreSQL |
| WP-10 | document metadata 垂直切片 | NOT RUN | 核心已隔离，PostgreSQL 原子性/E2E 待运行 |

## Revision

### 审查发现

Document 分层、原子事务、Outbox fencing、旧发布状态、流式存储、
readiness 错误安全、架构门禁和真实基础设施 CI 均缺少完成证据。

### 修复提交

当前工作树包含本轮修订，验证和主题提交完成后登记最终 SHA。

### 验证证据

| 检查 | 状态 | 证据 |
|---|---|---|
| Rust 1.85 fmt | PASS | 2026-07-31 本地退出码 0 |
| Rust 1.85 workspace check | FAIL | `aws-sdk-s3 1.140.0`/Smithy 要求 Rust 1.94.1 |
| Rust 1.94.1 修订前 workspace check | PASS | 退出码 0 |
| Document API Fake Port tests | PASS | 3 passed, 0 failed, 0 ignored |
| 修订后 workspace fmt/check/clippy/test | NOT RUN | 待执行 |
| PostgreSQL/MinIO/Migration upgrade/E2E | NOT RUN | 待启动真实依赖 |
| GitHub Actions | NOT RUN | 待推送 |

### 剩余风险

- LocalStorage 无法跨平台消除 canonicalize/open 之间的 symlink race，
  仅允许受信开发环境；
- 生产 OIDC 认证仍是 fail-closed 骨架；
- 所有真实依赖门禁通过前，PR #3 不得转为 Ready。

## 回滚

代码提交可按主题 revert。已发布 migration 不回退，通过后续向前
migration 修复；对象存储和 Document adapter 可在 composition root
切换实现。不得删除已经应用的 migration 文件。
