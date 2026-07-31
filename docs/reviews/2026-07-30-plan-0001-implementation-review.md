# PLAN-0001 实施审查报告

> 日期：2026-07-31
> 分支：feat/PLAN-0001-foundation-hardening
> PR：#3 (Ready for Review)
> 状态：Implemented

## 工作包状态

| WP | 名称 | 状态 | 当前证据 |
|---|---|---|---|
| WP-01 | 工具链与 CI | PASS | Rust 1.94.1 已固定；Rust 1.85 check 明确因 AWS SDK/Smithy MSRV 要求失败；1.94.1 fmt/check/clippy/test 通过 |
| WP-02 | 共享核心去框架化 | PASS | shared-kernel Cargo 依赖不含 Axum/SQLx/Reqwest/AWS SDK |
| WP-03 | 应用组合与薄 API | PASS | API Document Fake Port 测试 3 passed；真实 PostgreSQL HTTP 流程通过 |
| WP-04 | HTTP 安全基线 | PASS | Security 测试 7 passed；错误响应 trace/request correlation 有覆盖 |
| WP-05 | 配置与 Secret | PASS | 配置与 Secret 测试 18 passed |
| WP-06 | 真实对象存储适配 | PASS | 8 个 MinIO 契约用例在 CI 私有 bucket 中通过 |
| WP-07 | LocalStorage 安全 | PASS | LocalStorage/object-key 测试 11 passed，包含路径和 symlink 逃逸校验 |
| WP-08 | Migration 基座 | PASS | 2 个静态和 2 个 PostgreSQL 升级测试在 CI 通过 |
| WP-09 | 可靠 Outbox | PASS | 7 个 PostgreSQL ownership/fencing/并发测试在 CI 通过 |
| WP-10 | document metadata 垂直切片 | PASS | 真实 PostgreSQL HTTP 流程验证原子行、幂等、冲突和跨租户隔离 |

## Revision

### 审查发现

Document 分层、原子事务、Outbox fencing、旧发布状态、流式存储、
readiness 错误安全、架构门禁和真实基础设施 CI 已补齐证据。

### 修复提交

当前 head 为 `7bc93ac`，包含真实 Document PostgreSQL HTTP 契约和顺序化基础设施测试。

### 验证证据

| 检查 | 状态 | 证据 |
|---|---|---|
| Rust 1.85 fmt | PASS | 2026-07-31 本地退出码 0 |
| Rust 1.85 workspace check | FAIL | `aws-sdk-s3 1.140.0`/Smithy 要求 Rust 1.94.1 |
| Rust 1.94.1 workspace fmt/check/clippy/test | PASS | 退出码均为 0；普通 workspace 测试 54 passed、18 ignored、0 failed |
| Architecture fitness | PASS | `scripts/check-architecture.ps1` 输出 `Architecture fitness: PASS` |
| Document API Fake Port tests | PASS | 3 passed, 0 failed, 0 ignored |
| Document core tests | PASS | 11 passed, 0 failed, 0 ignored |
| Security tests | PASS | 7 passed, 0 failed, 0 ignored |
| Config/Secret tests | PASS | 18 passed, 0 failed, 0 ignored |
| LocalStorage/object-key tests | PASS | 11 passed, 0 failed, 0 ignored |
| Messaging unit tests | PASS | 2 passed, 0 failed, 0 ignored |
| Migration static tests | PASS | 2 passed, 0 failed, 0 ignored |
| PostgreSQL migration upgrade | PASS | CI run `30595889016`：2 static + 2 integration passed |
| PostgreSQL Outbox reliability | PASS | CI run `30595889016`：7 ownership/fencing/concurrency passed |
| MinIO contract | PASS | CI run `30595889016`：8 private-bucket contract tests passed |
| Document PostgreSQL HTTP E2E | PASS | CI run `30595889016`：atomic/idempotent/tenant-scoped flow passed |
| GitHub Actions | PASS | All 6 required checks passed |

### 剩余风险

- LocalStorage 无法跨平台消除 canonicalize/open 之间的 symlink race，
  仅允许受信开发环境；
- 生产 OIDC 认证仍是 fail-closed 骨架，属于后续能力范围。

## 回滚

代码提交可按主题 revert。已发布 migration 不回退，通过后续向前
migration 修复；对象存储和 Document adapter 可在 composition root
切换实现。不得删除已经应用的 migration 文件。
