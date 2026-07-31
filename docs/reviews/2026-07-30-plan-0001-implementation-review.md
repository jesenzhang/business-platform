# PLAN-0001 实施审查报告

> 日期：2026-07-31
> 分支：feat/PLAN-0001-foundation-hardening
> PR：#3 (Draft)
> 状态：Revision Required

## 工作包状态

| WP | 名称 | 状态 | 当前证据 |
|---|---|---|---|
| WP-01 | 工具链与 CI | PASS | Rust 1.94.1 已固定；Rust 1.85 check 明确因 AWS SDK/Smithy MSRV 要求失败；1.94.1 fmt/check/clippy/test 通过 |
| WP-02 | 共享核心去框架化 | PASS | shared-kernel Cargo 依赖不含 Axum/SQLx/Reqwest/AWS SDK |
| WP-03 | 应用组合与薄 API | PASS | API Document Fake Port 测试 3 passed；Handler 使用组合根注入的应用服务 |
| WP-04 | HTTP 安全基线 | PASS | Security 测试 7 passed；错误响应 trace/request correlation 有覆盖 |
| WP-05 | 配置与 Secret | PASS | 配置与 Secret 测试 18 passed |
| WP-06 | 真实对象存储适配 | BLOCKED | 流式接口和 LocalStorage 已通过；8 个 MinIO 契约用例因本机无 MinIO 均为 dispatch failure |
| WP-07 | LocalStorage 安全 | PASS | LocalStorage/object-key 测试 11 passed，包含路径和 symlink 逃逸校验 |
| WP-08 | Migration 基座 | BLOCKED | 2 个静态 migration 测试通过；2 个 PostgreSQL 升级测试因 `PoolTimedOut` 未完成 |
| WP-09 | 可靠 Outbox | BLOCKED | 7 个 PostgreSQL ownership/fencing/并发测试因 `PoolTimedOut` 未完成 |
| WP-10 | document metadata 垂直切片 | BLOCKED | 核心与 API 单测通过；PostgreSQL 原子性和完整 E2E 需要真实依赖 |

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
| Rust 1.94.1 workspace fmt/check/clippy/test | PASS | 退出码均为 0；普通 workspace 测试 54 passed、17 ignored、0 failed |
| Architecture fitness | PASS | `scripts/check-architecture.ps1` 输出 `Architecture fitness: PASS` |
| Document API Fake Port tests | PASS | 3 passed, 0 failed, 0 ignored |
| Document core tests | PASS | 11 passed, 0 failed, 0 ignored |
| Security tests | PASS | 7 passed, 0 failed, 0 ignored |
| Config/Secret tests | PASS | 18 passed, 0 failed, 0 ignored |
| LocalStorage/object-key tests | PASS | 11 passed, 0 failed, 0 ignored |
| Messaging unit tests | PASS | 2 passed, 0 failed, 0 ignored |
| Migration static tests | PASS | 2 passed, 0 failed, 0 ignored |
| PostgreSQL migration upgrade | BLOCKED | 2 integration tests failed with `PoolTimedOut`; PostgreSQL unavailable |
| PostgreSQL Outbox reliability | BLOCKED | 7 tests failed with `PoolTimedOut`; PostgreSQL unavailable |
| MinIO contract | BLOCKED | 8 tests failed with AWS `dispatch failure`; MinIO unavailable |
| Document E2E | NOT RUN | 依赖 PostgreSQL/MinIO，未单独执行 |
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
