# PLAN-0018 — sqlx 0.9 迁移（关闭 RUSTSEC-2026-0459 风险接受）

> 文档 ID：PLAN-0018
> 版本：0.1（评审稿）
> 状态：**Proposed / NOT ACTIVE / 待独立评审**。本文件是文档计划，不授权改代码、依赖升级、CI 配置或风险接受变更。
> 日期：2026-11-20
> 责任：Platform / Persistence Owners；协同：Security、CI Owner、Document Management、Identity/Organization/Policy 各持久化 Owner
> 计划基线：`main` @ `0f56fbe`（[CI run 44482205617](https://github.com/jesenzhang/business-platform/actions/runs/44482205617) 10/10 全绿）
> 权威输入（冲突时按 `AGENTS.md` §1）：`PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`、`SERVER_BACKEND_ARCHITECTURE.md`、`DATA_OWNERSHIP_AND_CONSISTENCY.md`、`RUST_CODING_STANDARD.md`、`QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`、`ARCHITECTURE_FITNESS_FUNCTIONS.md`、`DOCUMENT_MANAGEMENT.md`、`.cargo/audit.toml`
> 关联：[CI 恢复与计划状态报告](../reports/2026-11-19-P0-CI-RESTORATION-AND-PLAN-STATUS.md)、[依赖修复说明](../operations/CI_44310190041_RUSTSEC_DEPENDENCY_REMEDIATION.md)、PLAN-0017（合同切片，独立推进）、PLAN-0014（R2 对齐，独立推进）
> 编号说明：0018 未占用；0016 已归档（Astryx），0017 为合同切片计划。

## 1. 目标与关闭条件

把工作区从 `sqlx 0.8.6` 迁移到 `sqlx 0.9.1`，将 `libsqlite3-sys` 提升到 `>= 0.39.0`，并在**同一个 PR** 中删除 `.cargo/audit.toml` 的 `RUSTSEC-2026-0459` ignore，从而关闭 [风险接受](../operations/CI_44310190041_RUSTSEC_DEPENDENCY_REMEDIATION.md) 第 1 项。

关闭条件（全部满足才算关闭）：

1. `cargo audit --deny warnings` 在 **不含** `RUSTSEC-2026-0459` ignore 的审计配置下退出 0，且没有新增任何 ignore；
2. `libsqlite3-sys` 锁定版本 >= 0.39.0 且 bundled SQLite 实际编译路径仍被验证；
3. `AGENTS.md` §12 的 fmt/check/clippy/test、架构门禁与 CI 10 作业全绿；
4. SQLite 单写者语义（`BEGIN IMMEDIATE`）与 PostgreSQL/SQLite 双后端契约、长任务崩溃恢复 E2E 无回归；
5. 64 处动态 SQL 全部完成逐处处置并留下可审阅的理由与测试；
6. 独立评审 PASS，且审计说明/风险接受文档同步更新。

## 2. 为什么现在可做（已核实事实）

### 2.1 上游与漏洞

- [sqlx CHANGELOG](https://raw.githubusercontent.com/launchbadge/sqlx/main/CHANGELOG.md)：`0.9.1`（2026-11-17）明确包含 `#4502`（允许 `libsqlite3-sys` 0.39.x）与 `#4525`（允许 0.40.x）；`0.9.0`（2026-05-06）MSRV 为 Rust 1.94.0，工作区 CI 固定 1.94.1，满足。
- `RUSTSEC-2026-0459` 的修复版本为 `libsqlite3-sys >= 0.39.0`；sqlx 0.8.6 的 `sqlx-sqlite` 固定 `^0.30.1`，无法在 0.8 线解决。

### 2.2 本地可复现试验（trial，不在 main）

在隔离分支 `codex/plan-0018-sqlx-09-migration` 上做了只读性质的升级试验（未合入 main）：

| 试验动作 | 观察结果 |
|---|---|
| 根 `Cargo.toml`：`sqlx = "0.8"` → `"0.9"`，feature `runtime-tokio-rustls` → `runtime-tokio` + `tls-rustls` | 解析到 `sqlx 0.9.1` / `sqlx-sqlite 0.9.1` |
| `cargo update -p libsqlite3-sys --precise 0.40.0` | 成功，lock 从 0.30.1 → 0.40.0 |
| 临时移除 audit ignore 后 `cargo audit --deny warnings` | **EXIT=0**（1550 条 advisory，无漏洞/警告触发） |
| `cargo check --workspace --all-targets --all-features` | EXIT=101；第一波 15 个错误全部集中在 `crates/document-processing-sqlite/src/lib.rs` |

第一波错误分类：

- `sqlx::TransactionManager` 不再从 facade 导出（1）；
- `SqliteTransactionManager::{begin,commit,rollback,start_rollback}` 因 trait 不可见而不可用（4）；
- `query*` 的动态字符串需 `SqlSafeStr`/`AssertSqlSafe`（8）；
- 由上述错误导致的类型推断失败（2）。

工作区静态盘点：**64 处** `sqlx::query*` 调用传入非字面量 SQL，分布在 12 个文件：`document-processing-sqlite` 8、`policy-postgres` 8、`policy-sqlite` 8、`document-processing-postgres` 7、`identity-postgres` 7、`identity-sqlite` 7、`organization-postgres` 7、`organization-sqlite` 7、`document-sqlite/tests/concurrency` 2、`apps/migration` 1、`apps/plan-0009-rehearsal` 1、`runtime-governance-sqlite` 1。

其他破坏性变更（来自 CHANGELOG，需要在 WP4 逐项核实）：`Migrate` trait 变化、`resolve_blocking()` 转 `#[doc(hidden)]`、`Migrator::set_ignore_missing/set_locking` 返回 `&mut Self`、`RawSql` 增加 `DB` 参数、`derive(Type)` 自动生成 `PgHasArrayType`、`PgConnectOptions::options()` 转义与 `hostaddr` 语义变化、`offline` feature 可选。仓库当前未使用编译期 `query!()` 宏、`RawSql`、`.options()` 或 sqlx-cli 作为构建步骤（已 grep 核实），但仍需编译与契约验证。

## 3. 目标 Bounded Context 与数据所有者

不改变 Bounded Context、数据所有权或公开契约。涉及的持久化适配器仍属于各自上下文：Document/Document Intelligence（document-sqlite、document-processing-sqlite/postgres）、Identity/Organization/Policy（*-sqlite/postgres）、Runtime Governance、Messaging、Audit；`apps/migration` 仍为 Composition Root 的迁移入口。所有 SQL 与事务行为改动必须保持原上下文的不变量，禁止借迁移之机重构领域模型。

## 4. 架构影响与约束

- **依赖方向不变**：只允许在 Composition Root/基础设施适配器使用 sqlx；Domain/Application 不得新增 sqlx 依赖。现有 `scripts/check-architecture.ps1` 必须继续通过。
- **新增直接依赖**：sqlx 0.9 facade 不再导出 `TransactionManager`；为保留 `BEGIN IMMEDIATE`，建议在根 `Cargo.toml` 增加与 sqlx **完全同版本** 的直接依赖 `sqlx-core = "=0.9.1"`，并在 `document-processing-sqlite` 中使用 `sqlx_core::transaction::TransactionManager` + `sqlx::SqlStr::from_static("BEGIN IMMEDIATE")`。
  - 备选（需独立评审）：用 `Connection::begin()`（默认 DEFERRED）并自行实现单写者保护；**不得**为了编译通过而静默降级 `BEGIN IMMEDIATE`，这违反 `AGENTS.md` §15 与 SQLite 单进程写语义。
- **SQL 安全**：`AssertSqlSafe` 只是 speedbump，不是净化器。仅允许用于“SQL 形态由代码常量决定、变量只提供绑定参数或受控占位符数量”的调用点；任何把租户/用户/外部输入拼进 SQL 文本的调用点必须先改成绑定参数或白名单，再考虑迁移。
- **事务与幂等**：迁移不得改变现有本地事务边界、幂等键、Outbox 原子性、lease/fence 与版本冲突语义。

## 5. 工作包与顺序

| WP | 内容 | 主要证据 / 门禁 |
|---|---|---|
| WP0 | 固定基线：记录当前 `main` SHA、CI run、audit 输出、SQLite 并发与 PG 契约结果 | 基线清单；不写代码 |
| WP1 | 依赖与 feature：workspace `sqlx 0.9.1`、feature 拆分、`sqlx-core` 直依赖、`libsqlite3-sys 0.40.0`；**暂不删 audit ignore** | `cargo metadata`/`cargo tree` 证明版本；`cargo audit` 显示仅 0459 仍被 ignore |
| WP2 | 事务 API：`BEGIN IMMEDIATE` 调用点（5 处）改用 `sqlx_core::transaction::TransactionManager` + `SqlStr` | 编译通过；SQLite 单写者、租约崩溃恢复与并发用例 PASS；禁止 DEFERRED 静默降级 |
| WP3 | 动态 SQL：逐处处置 64 个调用点，按 §4 分类为 `AssertSqlSafe`（附 `// SQL-SAFE:` 理由）或绑定参数/白名单重构 | 逐处清单（文件:行、分类、理由、测试）；新增/扩展架构检查防止无理由 `AssertSqlSafe` |
| WP4 | 其他 API 兼容：`migrate!`/`Migrator`、feature 与 provider 行为差异逐项核对；修复新出现的编译错误 | `cargo check/clippy --workspace --all-targets --all-features` 全绿；迁移 status 只忽略明确的 migration table 缺失 |
| WP5 | 完整验证：workspace 门禁 + CI 10 作业 + 真实 PG/MinIO 契约 + SQLite 单进程 E2E + 长任务崩溃/重复/恢复 | CI run ID 与逐项结果；性能记录为 evidence，不虚构 SLO |
| WP6 | 关闭风险接受：同一 PR 删除 `RUSTSEC-2026-0459` ignore，更新依赖修复说明与本报告，`git diff --check` | `cargo audit --deny warnings` EXIT=0 且 ignore 列表仅剩 2023-0071/0543；独立评审 PASS |

执行规则：WP1–WP4 在独立分支提交；任一 WP 失败先修复再推进；WP6 只能在 WP5 全绿后进行，禁止提前删除 ignore。

## 6. 质量属性与可测量验收

| 属性 | 验收 |
|---|---|
| 安全 | 审计 ignore 中不含 0459；64 处动态 SQL 逐处有分类与理由；租户/用户可控输入进入 SQL 文本的处数 = 0（负向测试 + 静态检查） |
| 并发/恢复 | `BEGIN IMMEDIATE` 调用点与迁移前一一对应；SQLite 单写者、`BEGIN IMMEDIATE` 拒绝抛错、worker crash/lease reclaim/fence 用例全部 PASS |
| 一致性 | PostgreSQL 与 SQLite 双后端共享契约测试无新增忽略；Outbox/审计/幂等用例无回归 |
| 兼容性 | 现有公开 API、事件、错误码、迁移文件、OpenAPI 零差异；sqlx 只作为基础设施依赖 |
| 性能 | 记录迁移前后关键路径（List/Detail/Apply、worker claim、SQLite 写入）实测值；无 Owner 确认的性能退化不得合并 |
| 可回滚 | 单个 PR 可 `git revert`；回退时同时恢复 0459 ignore 与风险接受记录，避免审计门禁裸露失败 |

## 7. Fitness Functions

- 架构检查：`check-architecture.ps1` 继续禁止 Domain/Application 直接依赖 sqlx；新增检查确保 `sqlx` 与 `sqlx-core` 版本一致。
- SQL 安全：新增静态检查，要求每处 `AssertSqlSafe` 紧邻 `// SQL-SAFE:` 注释并给出常量/白名单理由；对拼接外部输入的 `format!` 进入 `sqlx::query*` 直接失败。
- 审计门禁：`cargo audit --deny warnings` 且 `.cargo/audit.toml` 中 `RUSTSEC-2026-0459` 不存在；新增任意 ignore 必须带理由与复审触发条件。
- 事务门禁：静态检查 `document-processing-sqlite` 中不得出现 `begin()` 无 `BEGIN IMMEDIATE` 参数的新调用；SQLite 契约测试覆盖并发写冲突。
- 文档同步：依赖修复说明、状态报告、本计划与审计配置在同一 PR 更新。

## 8. 测试策略

1. 单元测试：事务包装函数与 SQL 构造函数的常量性/白名单行为。
2. Application + Fake Ports：不改变，作为回归基线。
3. 真实 PostgreSQL + MinIO / SQLite 契约：CI `--include-ignored` 全量；SQLite 单进程 `BEGIN IMMEDIATE` 与多 worker 拒绝用例。
4. 安全测试：跨租户 IDOR、注入型 SQL 片段负例、`AssertSqlSafe` 使用审查。
5. 长任务测试：崩溃、重复投递、租约过期、fence 拒绝、Outbox 重放。
6. 审计与许可证：cargo audit、gitleaks、trivy。

## 9. 风险与开放问题

1. `sqlx-core` 作为直接依赖是否可接受（版本必须严格等于 sqlx，升级需同步）；由 Owner 在 WP1 评审确认。
2. 64 处动态 SQL 中是否存在实际的用户可控拼接；需 WP3 逐处判定，不能默认全部用 `AssertSqlSafe` 包裹。
3. sqlx 0.9.1 之后若出现新的传递依赖漏洞，本计划关闭 0459 的同时不得引入新 ignore。
4. 试验分支 `codex/plan-0018-sqlx-09-migration` 的修改仅为证据，未合入 main；实现时应从最新 main 重新开始，避免带入试验配置。
5. 是否需要把 SQLite 的 `BEGIN IMMEDIATE` 封装为适配器内部唯一入口（而非 5 处散落调用）以减少 future drift；由独立评审决定，但不得扩大为通用事务框架。

## 10. 回滚与完成定义

- 回滚：按 WP 逐段可回滚；WP6 回滚必须同时恢复 ignore 与风险接受说明。禁止只回滚代码而保留“已关闭”声明。
- 完成定义：§1 六项关闭条件全部满足；独立评审 PASS；计划状态由 Proposed 迁移并按文档生命周期归档；未完成项如实记录，不得把 NOT RUN 写成 PASS。
- 本计划不授权实现，也不改变 PLAN-0017 / PLAN-0014 / PLAN-0006 的状态；在主干 CI 全绿基线上独立推进。
