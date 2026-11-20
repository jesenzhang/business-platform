# PLAN-0018 独立评审报告（sqlx 0.9 迁移 / 关闭 RUSTSEC-2026-0459 风险接受）

> 文档类型：Independent Review（事实核对与评审意见；不修改 PLAN-0018，不授权编码/依赖升级/CI 配置/风险接受变更/激活）
> 评审日期：2026-11-20
> 评审者角色：独立评审者（非计划作者）
> 评审分支：`codex/plan-0018-sqlx09-review` @ `32b8453`（工作树 `E:\working\business-platform-wt\sqlx09-review`，基于 `main` @ `0f56fbe`；未改主工作区，未 push、未 merge）
> 评审对象：
> - `docs/plans/current/PLAN-0018-sqlx-09-migration.md`（121 行，Proposed / NOT ACTIVE / 待独立评审）
> - `docs/plans/README.md` 第 6 项与 PLAN-0017 状态措辞更新（`:28`、`:32`）
> 权威顺序：`AGENTS.md` §1（用户明确指令 → 已接受 ADR → 架构 Manifest/正文/Baseline → standards → 计划 → 实现）
> 关注优先级：按用户要求，优先核对"以 sqlx 0.9 迁移计划关闭 RUSTSEC-2026-0459 风险接受"这一首个关注项的事实依据与门禁诚实性。

## 1. Verdict

**PASS WITH COMMENTS（评审通过；PLAN-0018 必须保持 Proposed / NOT ACTIVE）**

- 计划中的**全部关键事实均已独立核对通过**：上游 CHANGELOG 的版本/日期/PR 号/MSRV/Breaking 列表、本地试验的 15 个第一波错误与 64 处盘点、`cargo audit` EXIT=0、仓库 Cargo.toml/audit.toml/BEGIN IMMEDIATE 调用点、PLAN-0018 编号未占用、PLAN-0017 已评审、CI run 全绿基线。**无 NOT VERIFIED 项。**
- 共 3 条发现，均为 C3（轻微/精度改进）：动态 SQL 盘点口径遗漏 1 个会编译失败的站点；WP2 "5 处 BEGIN IMMEDIATE" 表述混淆字面量与 TransactionManager 站点且未写明 `Cow → SqlStr` 签名变化；缺少显式"非目标"小节。**全部不阻塞「评审通过但计划仍 NOT ACTIVE」的结论**；WP4 的全工作区编译门禁可对前两条兜底。
- 结论口径：本报告只评价"计划是否可执行、边界是否正确、门禁是否诚实"。未把尚未实现的未来代码证据当计划缺陷，也未因试验成功就认为迁移完成；试验分支的改动仅作为证据，未被视为实现成果。

严重度定义：C0 = 关键/阻塞；C1 = 重大（通常阻塞）；C2 = 中等（不阻塞，评审冻结前建议修订）；C3 = 轻微（可选改进）。

## 2. 独立核对过的事实与证据

以下均为实际读取 CHANGELOG / 试验工作区 / 仓库 / 本地 advisory-db / gh 所得，**非仅读计划自述**。

### 2.1 上游事实（web_fetch，HTTP 200）

来源：<https://raw.githubusercontent.com/launchbadge/sqlx/main/CHANGELOG.md>

| 计划断言（PLAN-0018:30-31,53） | CHANGELOG 证据 | 结论 |
|---|---|---|
| `0.9.1` 日期 2026-11-17 | `## 0.9.1 - 2026-11-17` | ✓ |
| `#4502` 允许 libsqlite3-sys 0.39.x | Changed: `#4502 Bump the upper bound on libsqlite3-sys to allow 0.39.x` | ✓ |
| `#4525` 允许 0.40.x | Fixed: `#4525 sqlx-sqlite: bump the upper bound on libsqlite3-sys to allow 0.40.x` | ✓ |
| `0.9.0` 日期 2026-05-06、MSRV 1.94.0 | `## 0.9.0 - 2026-05-06`；Breaking: `supported Rust version ... 1.94.0` | ✓ |
| TransactionManager re-export 删除 | `#3821 ... Deleted re-export of unstable TransactionManager trait in sqlx` | ✓ |
| `SqlSafeStr` / `AssertSqlSafe` | `#3723 ... all query*() functions now take impl SqlSafeStr ... only implemented for &'static str and AssertSqlSafe` | ✓ |
| feature 拆分（组合 runtime+TLS 删除） | `#3821 ... Deleted deprecated combination runtime+TLS features`；0.9.1 Cargo.toml 有 `runtime-tokio`/`tls-rustls`，无 `runtime-tokio-rustls` | ✓ |
| `Migrate` trait 变化、`resolve_blocking()` 转 `#[doc(hidden)]` | `#3383 ... Significant changes to the Migrate trait` / `resolve_blocking() is now #[doc(hidden)]` | ✓ |
| `Migrator::set_ignore_missing/set_locking` 返回 `&mut Self` | `#3526` | ✓ |
| `RawSql` 增加 `DB` 参数 | `#3613 ... adds DB type parameter to all methods of RawSql` | ✓ |
| `derive(Type)` 自动生成 `PgHasArrayType` | `#4008` | ✓ |
| `PgConnectOptions::options()` 转义、`hostaddr` 语义 | `#3800`、`#4514`（0.9.1 Changed） | ✓ |
| `offline` feature 可选 | `#4077 breaking: make offline optional` | ✓ |

补充核对：本地 advisory-db `crates/libsqlite3-sys/RUSTSEC-2026-0459.md` 声明 `patched = [">= 0.39.0"]`、CVSS `AV:L/AC:L/PR:N/UI:P`，与 PLAN-0018:31 及 `.cargo/audit.toml:21-41` 一致；`sqlx-sqlite-0.8.6/Cargo.toml` 的 `libsqlite3-sys version = "0.30.1"` 证实 0.8 线无法在版本范围内修复。

### 2.2 本地试验事实（隔离工作区，只读）

试验工作区 `E:\working\business-platform-wt\sqlx09-trial` 位于 `codex/plan-0018-sqlx-09-migration`（HEAD `0f56fbe`），未提交改动为 `Cargo.toml`、`Cargo.lock`、`.cargo/audit.toml` 与未跟踪的 `sqlx09-check.log`；本次评审**未修改**该工作区。

| 计划断言（PLAN-0018:37-51） | 独立证据 | 结论 |
|---|---|---|
| `sqlx = "0.8"` → `"0.9"`、feature 拆分 | `git diff -- Cargo.toml`：`sqlx = { version = "0.9", features = ["runtime-tokio", "tls-rustls", ...] }` | ✓ |
| 解析到 sqlx 0.9.1 / sqlx-sqlite 0.9.1 | `sqlx09-check.log:9-15`；试验 `Cargo.lock` 中 sqlx/sqlx-sqlite 均 0.9.1 | ✓ |
| `cargo update --precise 0.40.0` 成功 | 试验 `Cargo.lock` 中 `libsqlite3-sys version = "0.40.0"` | ✓ |
| 移除 ignore 后 `cargo audit --deny warnings` EXIT=0 | 试验 `.cargo/audit.toml` ignore 仅剩 2023-0071/0543；评审者在试验工作区**只读重跑**：`Loaded 1550 security advisories`、`Scanning 511 crate dependencies`、`AUDIT_EXIT=0` | ✓ |
| `cargo check` EXIT=101，第一波 15 个错误集中在 document-processing-sqlite | `sqlx09-check.log:251-530`，末行 `due to 15 previous errors` | ✓ |
| 错误分类 1+4+8+2 | 日志逐条：E0432 `sqlx::TransactionManager`（1）；E0599 `begin/commit/rollback/start_rollback`（4）；E0277 `SqlSafeStr` 动态 SQL（8）；E0282 类型推断（2） | ✓ |
| 8 处动态 SQL 在 document-processing-sqlite | 日志命中源码行 390/425/1497/1531/1675/1998/2914/2981；与仓库 grep 的该文件 8 处一致 | ✓ |

证据口径说明：`sqlx09-check.log:157` 显示编译时仍为 `libsqlite3-sys v0.30.1`，而当前试验 lock 为 0.40.0——日志与 lock 来自试验的不同时刻。这不影响"15 个源码 API 错误"的结论（均为 sqlx facade/trait 变化，与 libsqlite3-sys 版本无关），但报告读者不应把该日志当作 0.40.0 已解析后的编译输出。

### 2.3 仓库事实

| 核对项 | 证据 | 结论 |
|---|---|---|
| 根依赖仍为 0.8 且用组合 feature | `Cargo.toml:99` `sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "sqlite", ...] }` | ✓ 计划 §2.2 属实 |
| audit ignore 三项与理由 | `.cargo/audit.toml:63-64` `ignore = ["RUSTSEC-2023-0071", "RUSTSEC-2026-0459", "RUSTSEC-2026-0543"]`；0459 理由含"0.9 源码迁移落地即删除"复审触发条件（`:35-40`） | ✓ WP6 删除时机与理由一致 |
| `BEGIN IMMEDIATE` / TransactionManager 调用点 | `crates/document-processing-sqlite/src/lib.rs:33-35`（`SqliteTransactionManager`、`TransactionManager` 导入）；字面量 `sqlx::query("BEGIN IMMEDIATE")` 在 `:797/:892/:918/:952/:1492`；trait 方法在 `:1645`(start_rollback)/`:1653`(begin)/`:1662`(commit)/`:2770`(rollback) | ✓ 计划 §4/§5 WP2 涉及的代码位置属实（计数精度见 F2） |
| 动态 SQL 盘点 64 处 / 12 文件 | 独立 grep（`sqlx::query*...(&var|&format!)`）：document-processing-sqlite 8、policy-postgres 8、policy-sqlite 8、document-processing-postgres 7、identity-postgres 7、identity-sqlite 7、organization-postgres 7、organization-sqlite 7、document-sqlite/tests/concurrency 2、apps/migration 1、apps/plan-0009-rehearsal 1、runtime-governance-sqlite 1 = **64 / 12** | ✓ 计划 §2.2:51 属实（口径问题见 F1） |
| PLAN-0018 编号未占用 | 全仓 `PLAN-0018` 仅出现在本计划及 README:32；`docs/plans/current/` 无其他 0018；0016 已归档、0017 合同切片 | ✓ |
| PLAN-0017 已评审 | `docs/reviews/2026-11-19-plan0017-independent-review.md:15` Verdict **PASS WITH COMMENTS**；README:28 措辞"独立评审 PASS WITH COMMENTS，NOT ACTIVE"与该报告一致 | ✓ |
| 基线 CI 10/10 全绿 | `gh run view 44482205617 --repo jesenzhang/business-platform`：10 作业全部 ✓（含 Security scanning、PostgreSQL + MinIO + E2E）；`git ls-remote origin refs/heads/main` = `0f56fbe...` | ✓ 计划头部:8 属实 |
| 风险接受第 1 项 | `docs/operations/CI_44310190041_RUSTSEC_DEPENDENCY_REMEDIATION.md:35` 表格第一项为 0459；恢复报告 `:34` 明确"应通过专门的 sqlx 0.9 迁移计划关闭" | ✓ 计划 §1 属实 |

### 2.4 计划质量核对（逐项）

- **目标与关闭条件（§1）**：六项关闭条件可测量（audit EXIT=0、libsqlite3-sys≥0.39 且 bundled 路径验证、门禁/CI 全绿、单写者与双后端契约无回归、64 处逐处处置、独立评审 PASS）。✓
- **边界与所有权不变（§3）/架构影响（§4）**：明确不新增/合并 Context、不改数据所有权与公开契约；依赖方向不变；`check-architecture.ps1` 继续作为门禁。未发现越权授权编码或改变边界。✓
- **SQL 安全（§4:64）**：明确"`AssertSqlSafe` 只是 speedbump，不是净化器"，仅允许常量 SQL 形态 + 绑定参数/受控占位符；外部输入先改绑定或白名单。与 sqlx-core-0.9.1 `SqlSafeStr` 语义及 `AGENTS.md` §10 一致。✓
- **BEGIN IMMEDIATE 不得静默降级（§4:63、§5 WP2、§6、§7）**：明确禁止改用默认 DEFERRED `begin()`；与 `AGENTS.md` §15、`ARCHITECTURE_FITNESS_FUNCTIONS.md:366-382` 的 SQLite 单写者门禁一致。✓（表述精度见 F2）
- **WP0–WP6 顺序与门禁（§5）**：WP1 暂不删 ignore，WP6 只能在 WP5 全绿后同一 PR 删除，禁止提前删除；执行规则 `:79` 明确任一 WP 失败先修复。✓
- **审计 ignore 删除时机（§5 WP1/WP6、§10）**：与 `.cargo/audit.toml` 复审触发条件一致；回滚要求同时恢复 ignore 与风险接受说明，禁止只回滚代码保留"已关闭"声明。✓
- **回滚与完成定义（§10）**：逐 WP 可回滚、DoD 明确"不得把 NOT RUN 写成 PASS"；全文未把试验或未来代码写成 PASS。✓
- **Fitness Functions 可执行性（§7）**：架构检查挂在已存在的 `scripts/check-architecture.ps1`（CI `ci.yml:68/301` 已调用，脚本已含 sqlx 依赖方向断言）；审计门禁挂在已存在的 CI Security job（`ci.yml:72-82`）；SQL-SAFE 注释与 `begin()` 门禁由 WP3/WP4 新增检查落地。均具备可执行落点，不属于空泛声明。✓（具体落点需在 WP3 冻结前确认，见开放问题 Q4）
- **AGENTS.md §13 必备要素**：目标 Context/所有者（§3）、架构影响（§4）、API/Event（§3/§6 "公开 API、事件、错误码、迁移文件、OpenAPI 零差异"）、一致性与安全（§4/§6）、质量属性（§6）、Fitness Functions（§7）、文档同步（§7/§10）、回滚与完成定义（§10）具备；"非目标"缺显式小节（见 F3）。
- **文档生命周期（DOCUMENT_MANAGEMENT.md §2.4/§6.2/§6.3）**：Proposed 措辞、编号不复用、状态迁移与归档要求在 README:22/32 与 §10 中体现。✓

## 3. 逐条发现

### F1 — C3（轻微，不阻塞）｜"64 处动态 SQL"盘点口径遗漏至少 1 个会在 0.9 下编译失败的站点

- 位置：`PLAN-0018...md:23`（关闭条件 5）、`:51`（"64 处 / 12 文件"）、`:74`（WP3"逐处处置 64 个调用点"）。
- 事实：`apps/plan-0009-rehearsal/src/stage4.rs:1053` 定义 `async fn bounded_count(pool: &SqlitePool, query: &str, ...)`，`:1056` 将 `query` 传入 `sqlx::query_scalar::<_, i64>(query)`。sqlx 0.9.1 的 `SqlSafeStr` 仅实现于 `&'static str`（`sqlx-core-0.9.1/src/sql_str.rs:47,52`）与 `AssertSqlSafe`/`SqlStr`；`query: &str` 的生命周期被函数签名擦除，不满足该约束。评审者在临时目录以 `sqlx = "=0.9.1"` 做独立探针，复现 `error[E0521]: borrowed data escapes outside of function ... argument requires that '1 must outlive 'static`（EXIT=101）。调用点 `:985-1011` 虽全传字面量，但函数体内仍会编译失败。
- 影响：WP3 若把"64 处"当作完备清单并据此宣告关闭条件 5 完成，会遗漏该站点；但 WP4 的 `cargo check --workspace --all-targets --all-features` 全绿门禁会捕获它，因此不阻塞计划通过。
- 建议替换 `:51` 该段为：
  `工作区静态盘点：按 &String/&format! 形态统计为 64 处非字面量 SQL，分布在 12 个文件：document-processing-sqlite 8、policy-postgres 8、policy-sqlite 8、document-processing-postgres 7、identity-postgres 7、identity-sqlite 7、organization-postgres 7、organization-sqlite 7、document-sqlite/tests/concurrency 2、apps/migration 1、apps/plan-0009-rehearsal 1、runtime-governance-sqlite 1。此外 apps/plan-0009-rehearsal/src/stage4.rs:1056 以 &str 形参传入 query_scalar，同样因 SqlSafeStr 只接受 &'static str 而失败，须一并纳入 WP3 清单；WP3 是否完成以 cargo check --workspace --all-targets --all-features 全绿为准，不以逐处数量声明为准。`
- 是否阻塞：**否**。

### F2 — C3（轻微，不阻塞）｜WP2"BEGIN IMMEDIATE 调用点（5 处）"混淆字面量与 Trait 站点，且未写明 `Cow → SqlStr` 的签名变化

- 位置：`PLAN-0018...md:62`（建议方案）、`:73`（WP2 行）。
- 事实：`crates/document-processing-sqlite/src/lib.rs` 中 `sqlx::query("BEGIN IMMEDIATE")` 是 5 处**字符串字面量**查询（`:797/:892/:918/:952/:1492`），`&'static str` 直接满足 `SqlSafeStr`，0.9 下本身可编译，不属于破坏点。真正破坏的是 `:34` 的 `TransactionManager` facade 导出，以及 `:1645/:1653/:1662/:2770` 四处 `SqliteTransactionManager` trait 方法（即"1 导入 + 4 方法"）。另外，sqlx 0.9.1 的 `TransactionManager::begin` 第二参数已改为 `Option<SqlStr>`（`sqlx-core-0.9.1/src/transaction.rs:14-18`），现码 `:1653` 的 `Some(Cow::Borrowed("BEGIN IMMEDIATE"))` 必须改为 `Some(SqlStr::from_static("BEGIN IMMEDIATE"))`（计划 `:62` 已提到 `SqlStr`，但未明确替换 `Cow` 及移除 `:35` 的 `use std::borrow::Cow`，否则 `clippy -D warnings` 会因未使用 import 失败）。
- 影响：实现者可能误改 5 处本可保留的字面量查询，或漏改 `Cow` 参数；WP4 编译/clippy 门禁可兜底，不阻塞。
- 建议替换 `:73` 的 WP2 行为：
  `| WP2 | 事务 API：将 :34 的 TransactionManager 改为从 sqlx_core::transaction 引入；修复 :1645/:1653/:1662/:2770 四处 SqliteTransactionManager 方法调用；:1653 第二参数由 Some(Cow::Borrowed("BEGIN IMMEDIATE")) 改为 Some(SqlStr::from_static("BEGIN IMMEDIATE")) 并移除不再使用的 Cow 导入；:797/:892/:918/:952/:1492 的字面量 BEGIN IMMEDIATE 查询保持不变 | 编译通过；SQLite 单写者、租约崩溃恢复与并发用例 PASS；禁止 DEFERRED 静默降级 |`
- 是否阻塞：**否**。

### F3 — C3（轻微，不阻塞）｜缺少显式"非目标"小节

- 位置：`PLAN-0018...md` 全文（§1–§10）。
- 事实：`docs/plans/README.md:17` 规则 2 要求计划必须有"目标、非目标、边界、步骤、风险、测试和完成定义"。计划 §1/§3/§4/§5/§9/§8/§10 分别覆盖目标、边界、步骤、风险、测试、完成定义，且头部 `:5`、§3、§10 多处声明"不授权改代码/依赖升级/CI 配置/风险接受变更""不改变 BC/数据所有权/公开契约"；但没有独立"非目标"小节。AGENTS.md §13 的 API/Event 要素已由 §3/§6"零差异"表述覆盖，不构成缺项。
- 建议：在 §1 之后新增：
  `## 1.1 非目标
  - 不授权任何编码、依赖升级落地、CI 配置修改或审计 ignore 的提前删除（WP6 前）；
  - 不改变 Bounded Context、数据所有权、公开 API/Event/错误码、迁移文件或 OpenAPI；
  - 不借迁移重构领域模型、不引入通用事务框架、不把 SQLite 用作生产多 Worker 权威；
  - 不改变 PLAN-0017 / PLAN-0014 / PLAN-0006 / PLAN-0009 的状态；不 push、不合并。`
- 是否阻塞：**否**。

## 4. 明确核查通过的关键门禁（无发现）

- **关闭 0459 的事实链完整**：advisory patched ≥0.39.0 → sqlx 0.9.1 + #4502/#4525 放开上限 → 试验 lock 锁定 0.40.0 → 移除 ignore 后 `cargo audit --deny warnings` 实测 EXIT=0 → WP6 同 PR 删除 ignore 并更新风险接受说明。链条每一环均有独立证据，不依赖计划自述。
- **无越权**：头部 `:5`、§3、§10 一致保持 Proposed / NOT ACTIVE / 不授权编码；未把试验分支改动当作已合入实现，未把 P0 CI 绿写成迁移完成。
- **边界/所有权**：§3 明确不变；未借依赖迁移重构领域模型或扩大为通用事务框架；SQLite 仅本地单进程、PG 多 Worker 权威的约束（AGENTS.md §15）被保留。
- **BEGIN IMMEDIATE 与单写者**：§4:63 明确禁止静默降级，§6/§7 有静态检查 + 契约测试双重门禁，与 FITNESS §24 一致。
- **SQL 安全**：`AssertSqlSafe` 被明确定义为 speedbump；§6 安全验收含"租户/用户可控输入进入 SQL 文本的处数 = 0"的负向测试与静态检查，符合最小权限与注入防护要求。
- **门禁诚实性**：§10 明确"未完成项如实记录，不得把 NOT RUN 写成 PASS"；全文没有把未运行的 CI/E2E 写成通过；试验仅称"证据，未合入 main"。
- **文档同步与生命周期**：README 第 6 项新增 PLAN-0018 条目、第 2 项 PLAN-0017 状态措辞与实际评审报告一致；编号 0018 未占用；Proposed → 归档的路径符合 DOCUMENT_MANAGEMENT.md。

## 5. 实现前必须回答的开放问题

计划 §9 已有 5 条，以下为独立评审认为**在进入 WP1 前必须冻结**的补充/复述项（保持 Proposed 正是诚实做法，不要求现在给出答案）：

1. **Q1（计划 §9.1）**：`sqlx-core = "=0.9.1"` 作为直接依赖是否可接受、由哪个 Owner 在 WP1 冻结？需确认它与 `sqlx` 版本严格同步的升级责任，避免未来版本漂移。
2. **Q2（计划 §9.2 + F1）**：64（或按 F1 修正后 65）处动态 SQL 中是否存在用户/租户可控拼接？WP3 必须逐处给出"常量形态/绑定参数/白名单"判定，不得默认全部用 `AssertSqlSafe` 包裹。
3. **Q3（新增）**：WP3/WP4 承诺的三项新静态检查（`// SQL-SAFE:` 注释、sqlx 与 sqlx-core 版本一致、`begin()` 无 BEGIN IMMEDIATE 拒绝）落在 `scripts/check-architecture.ps1` 还是新脚本、由哪个 CI job 执行？需在 WP3 冻结前明确，否则 §7 Fitness Functions 无法验收。
4. **Q4（计划 §9.5）**：`BEGIN IMMEDIATE` 是否封装为适配器内唯一入口？独立评审建议：**本次仅替换 API、保持现有调用结构**，不扩大为通用事务框架；若确需封装，另立计划/ADR。
5. **Q5（新增）**：WP6 删除 ignore 与风险接受文档更新必须在同一 PR；需明确 Security + Platform/Persistence Owner 谁核验"CI 10/10 全绿 + audit EXIT=0"作为删 ignore 的前置证据。
6. **Q6（计划 §9.4）**：实现必须从最新 `main` 重新起步，试验分支配置不得带入；WP0 需记录基线 SHA 与 CI run 作为对照。

## 6. 评审结论与后续

- **Verdict：PASS WITH COMMENTS。计划评审通过，但 PLAN-0018 必须保持 Proposed / NOT ACTIVE；本报告不构成编码、依赖升级、CI 配置、审计 ignore 删除、分支合并或 push 授权。**
- 3 条 C3 发现均不阻塞该结论；作者可在 Gate（WP0/WP1 Owner 冻结）前一并吸收 F1–F3 的建议文本，其中 F1/F2 建议在评审记录中同步修正以避免 WP3/WP2 执行偏差。
- 用户首个关注项（以 sqlx 0.9 迁移关闭 0459 风险接受）的**事实依据与门禁设计充分**：审计 EXIT=0 已在 0.40.0 下实测、删除时机被限制在 WP5 全绿后的同一 PR、回滚要求恢复 ignore 与风险接受说明。评审未发现需要 CHANGES REQUESTED 的事实错误或不可执行处。
- 本报告只做文档独立评审；未修改 PLAN-0018、README、架构基线、代码、权限、数据库、CI 配置或试验工作区；未 merge、未 push。

> 2026-11-20：评审在 `codex/plan-0018-sqlx09-review` 分支完成并提交本报告；工作区仅含本报告提交，无其他未提交改动。
