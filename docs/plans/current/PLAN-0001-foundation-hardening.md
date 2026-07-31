# PLAN-0001：初始服务基座加固

> 状态：Revision Required
> 日期：2026-07-30
> 来源：`REVIEW-2026-07-30-001`
> 目标分支：feat/PLAN-0001-foundation-hardening

## 1. 目标

在批量实现业务领域前，修正初始骨架中的安全、分层和可靠性阻断项，建立可编译、可测试、可运行的 Rust 服务基座。

## 2. 非目标

本计划不实现：

- 完整合同、客户、审批等业务；
- 正式 Agent Runtime；
- 完整生产集群；
- 复杂工作流产品能力；
- 多区域和容灾。

## 3. 前置规范

- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/standards/RUST_CODING_STANDARD.md`
- `docs/reviews/2026-07-30-initial-implementation-review.md`

## 4. 工作包

### WP-01：工具链与 CI

- 新增 `rust-toolchain.toml`；
- 新增 Workspace lints；
- 建立 GitHub Actions；
- 门禁：fmt、check、严格 Clippy、test；
- 固定 `Cargo.lock`。

验收：main 分支提交具有可见 CI 状态。

### WP-02：共享核心去框架化

- 从 `shared-kernel` 移除 Axum 和 SQLx；
- 拆分 Domain/Application/API/Infrastructure 错误；
- HTTP 错误映射移动到 API 层；
- SQLx 错误映射移动到 Repository。

验收：`shared-kernel` 无 Axum、SQLx、Reqwest 和外部 SDK 依赖。

### WP-03：应用组合和薄 API

- `AppState` 不再公开 `PgPool` 给业务 Handler；
- 定义应用服务/facade 注入方式；
- 健康检查使用独立依赖探针；
- 统一 `/health/live` 和 `/health/ready`；
- 增加优雅关闭。

验收：示例业务 Handler 只调用应用用例。

### WP-04：安全 HTTP 基线

- 移除默认 permissive CORS；
- 配置化 origin 白名单；
- 区分公开健康路由和受保护业务路由；
- 建立认证、租户上下文和安全错误响应骨架；
- 增加 body limit、timeout 和 request ID 测试。

验收：匿名访问受保护路由被拒绝；错误不泄漏底层细节。

### WP-05：配置和 Secret

- 按进程拆分配置；
- 引入脱敏 Secret 类型；
- 不再打印完整配置；
- 启动时验证连接池、URL、bucket、超时和生产必需项；
- 对齐多 bucket 配置。

验收：测试证明日志和 Debug 中不出现测试 Secret。

### WP-06：真实对象存储适配

- 删除或隔离当前未签名 S3 Stub；
- 使用成熟 S3 SDK；
- 支持 path-style、自定义 endpoint 和真实预签名 URL；
- 设计流式读写接口；
- 增加 MinIO 契约测试。

验收：私有 MinIO bucket 的 put/get/delete/head/presign 全部通过。

### WP-07：LocalStorage 安全

- 引入 `ObjectKey`；
- 拒绝 `..`、绝对路径、盘符和 UNC；
- 使用异步文件 API；
- 增加 Windows/Linux 路径测试。

验收：所有路径穿越用例被拒绝，目标始终位于根目录。

### WP-08：Migration 基座

- 实现 migration CLI 的 `up`、`status`；
- 从空库执行 migrations；
- readiness 校验兼容版本；
- 建立第一份基础 schema。

验收：空 PostgreSQL 可一条命令初始化，重复执行安全。

### WP-09：可靠 Outbox

- 增加状态、尝试、下次重试、claim/lease 和最后错误字段；
- 实现多 Worker 安全获取；
- 实现 Noop 以外的测试 Publisher；
- 建立重复、崩溃和恢复测试。

验收：两个 Worker 并发时不会同时持有同一 claim；重复发布不会产生重复业务副作用。

### WP-10：首个垂直切片

选择一个低风险只读/简单写入能力，贯穿：

```text
API
→ Application
→ Domain
→ Repository
→ PostgreSQL
→ Audit/Outbox
```

推荐使用 `document metadata` 或最小 `customer` 能力，不使用复杂合同流程。

验收：单元、Repository、API 和 E2E 测试证明分层可用。

## 5. 实施顺序

```text
WP-01
→ WP-02 / WP-05
→ WP-03 / WP-04
→ WP-06 / WP-07 / WP-08
→ WP-09
→ WP-10
```

WP-06 和 WP-08 可并行，但不得在真实对象存储和 Migration 未完成前声称基础设施可用。

## 6. 风险

- 一次修改过多 crate 导致难以定位回归；
- S3 SDK 增加编译时间和依赖体积；
- 认证设计过早复杂化；
- Outbox 方案在没有实际消息 Broker 时过度设计。

控制措施：

- 每个工作包独立提交和验收；
- 先建立端口和契约测试；
- 认证只完成边界，不一次实现完整组织权限；
- Outbox 先证明 PostgreSQL claim 和幂等，再接 NATS。

## 7. 回滚

每个工作包必须保持可独立回滚。数据库 migration 采用向前修复策略，不修改已发布 migration。新适配器通过 feature/config 切换，不在同一提交删除唯一可运行路径。

## 8. 完成定义

- [ ] 所有 WP 验收通过；
- [ ] CI 全绿；
- [ ] 审查报告中的 P0/P1 已关闭或有明确接受的 ADR；
- [ ] 总体架构、代码架构和代码实现一致；
- [ ] 基础设施契约测试可重复运行；
- [ ] 首个垂直切片证明 UI/API/Worker/未来 Agent 可复用应用层；
- [ ] 本计划移入 `docs/plans/archive/2026/` 并记录最终提交。（待 PR 合并后执行）

## 9. 候选提交

实施位于分支 `feat/PLAN-0001-foundation-hardening`，基线为 `696acfb` (main)。每个工作包对应一个独立提交：

| 提交 | 工作包 | 说明 |
|---|---|---|
| `73a2606` | WP-01 | ci: establish Rust workspace gates |
| `7bee85d` | WP-02 | refactor: decouple shared kernel from frameworks |
| `0458365` | WP-05 | security: protect configuration secrets |
| `76566f5` | WP-03 / WP-04 | security: establish HTTP security baseline and app composition |
| `6e64466` | WP-06 / WP-07 / WP-08 | feat: implement S3 adapter, ObjectKey security, and migration CLI |
| `fbe6300` | WP-09 | feat: make outbox claiming reliable for multi-worker delivery |
| `8fac234` | WP-10 | feat: add document metadata vertical slice |

实施审查见 [`../../reviews/2026-07-30-plan-0001-implementation-review.md`](../../reviews/2026-07-30-plan-0001-implementation-review.md)。

## 10. Revision

### 审查发现

- Rust 1.85 声明没有工具链证据且被当前锁定依赖拒绝；
- Document 核心混入 Axum、SQLx、对象存储和消息实现；
- Document 创建缺少 Audit 与 Idempotency 的原子写入；
- Outbox 完成/失败未校验 claim ownership/fencing；
- Outbox 新旧发布状态未向前协调；
- 对象存储默认整块读写，真实 MinIO/PostgreSQL 测试未进入 CI；
- readiness 泄漏数据库错误且 Handler 直接获取连接池。

### 修复提交

修复正在本分支按工具链、Document、Outbox、Migration、对象存储、
readiness/architecture、CI/文档主题形成提交；最终 SHA 在验证后登记。

### 验证证据

- `cargo +1.85.0 fmt --all -- --check`: PASS
- `cargo +1.85.0 check --workspace --all-targets --all-features`: FAIL
- `cargo +1.94.1 check --workspace --all-targets --all-features`（修订前基线）: PASS
- 修订后的全量本地、PostgreSQL、MinIO、E2E 与 CI：NOT RUN

### 剩余风险

- 真实基础设施和 GitHub Actions 未全部通过前，PLAN-0001 保持
  `Revision Required`，PR #3 保持 Draft。
