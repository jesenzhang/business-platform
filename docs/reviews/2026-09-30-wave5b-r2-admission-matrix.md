# PLAN-0014 Wave 5B-0 前置 — R2 准入矩阵（G-01 / G-201 / G-02 / G-202 / G-203 / A-01..A-12）

> Review ID: REVIEW-PLAN-0014-WAVE5B-ADMISSION-2026-09-30
> Status: 事实盘点（inventory / admission matrix），不是验收，不改规范
> Date: 2026-09-30（Revision 1：按 `docs/reviews/2026-09-30-wave5b-design-independent-review.md` 修正"独立安装"判据与真实执行范围；§2–§6 的历史证据、固定 SHA 与计数一律照录未改）
> 起点：Revision 0 = `c73955e`；Revision 1 = `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`
> Related plan: PLAN-0014（Active / Review-first）
> 来源指令：`docs/reviews/2026-09-30-wave5a-repair2-independent-review.md` §4.3
> 配套设计：`docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md`（Revision 1）

## 1. 判定纪律（本矩阵遵循的计数规则）

1. **G-01 与 G-201 是承接关系，不是两条独立成功项**。Wave 2 review §8 明写 G-201 = 「Wave 1 G-01
   重编号承接」（`docs/reviews/2026-09-24-saas-align-module-packaging.md:300`）。本矩阵把两者记成
   同一行账（`G-01 → G-201`），关闭判定只记一次。
2. **foundation 的两个纯 crate 不等于两个试点**。`business-module-contracts` 与
   `business-application-compiler` 同属一个 platform foundation；若日后登记为模块只是**一个**模块
   （`ModuleReleaseBundle.crate_versions` 映射两个 crate）。试点仍是且只有 `identity` 与 `policy`。
3. **针对本地工具的 PASS 不得扩大为模块 R2 admission**。`module-release-tool` 的 12 项检查证明的是
   「候选工件自洽且来自固定提交」，不是「模块可独立安装 / 升级」。
4. **旧日志不冒充新执行**。每行标注证据年代与产生该证据的固定 SHA；跨轮引用必须写明是哪一轮。
5. **判定权在独立审阅**。本轮（实现侧）给出的「有证据 / 残项收窄」不是关闭；
   G-01 / G-02 / G-201 / G-202 / G-203 本轮**一律未关闭**。
6. **最终结论：R2 BLOCKED**。只要适用 C1 与真实验收（真实 PostgreSQL、单模块安装 / 升级 / 回滚）
   未被独立关闭，`identity` / `policy` / 两个纯 crate 维持 **R1 / R2 BLOCKED / NOT CLAIMED**。

## 2. 起点与固定 SHA 基线

| 用途 | 值 |
|---|---|
| 分支 | `codex/plan-0014-wave5a-local-release-pilot`（未 push、未 merge、未改 main） |
| 本轮起点 = 独立审阅 HEAD | `c73955ec30f46d839ff33d44776cef142b7d6d77` |
| Repair 2 代码候选（pilot 的 `source_revision`） | `c22939f0a51e3c7fa2ec424f3819c6e7113a9b66` |
| Repair 2 文档 closeout HEAD | `6c63ba1598d38e71913ad633eb8a46002b35c7d3` |
| Repair 1 代码候选 | `69d9a6b01184db7d9f3d4aa3b688fb569387b2fc` |
| Wave 5A 首轮代码候选 | `8036bc65ba7c77666e48afaa6439911ba7c083d7` |
| Wave 2 Round 2 代码候选（A-01..A-12 原始证据） | `1e3a25b3dff62a336ce32ba7bd9f2aaf763f6e9b` |
| Wave 1 审阅（G-01 / G-02 出处，2026-09-23） | `docs/reviews/2026-09-23-saas-align-identity-organization-policy.md` |
| Wave 2 审阅（G-201..G-203 / A-01..A-12 出处，2026-09-24） | `docs/reviews/2026-09-24-saas-align-module-packaging.md` |
| Repair 2 独立审阅结论 | **PASS（Wave 5A Repair 2 有界修复） / NOT INTEGRATED**（`docs/reviews/2026-09-30-wave5a-repair2-independent-review.md`） |
| 本轮起点工作树 | 干净；唯一未跟踪文件是上述独立审阅文档，本轮随文档包一并入库（与 `7180eec7` / `f1984544` 两次「归档审阅文档」提交的做法一致） |

两试点四类摘要（`c22939f0…` 实测，与 Repair 1、Wave 5A 首轮逐字相同；随提交变化的只有
`source_revision`）：

| 模块 | contract_digest | package_digest | artifact_integrity_digest | migration_set_digest | 成员 |
|---|---|---|---|---|---|
| `identity` | `3fed976b…1107` | `b9dfa42c…3c70` | `68deba81…c700` | `ef43cfa9…ae03` | 21 |
| `policy` | `52e788ff…68c8` | `37e95a19…4b6` | `f8add9e0…b243` | `1a5a43f8…a597` | 23 |

## 3. 原关闭条件（逐字，作为判定标尺）

| ID | 原关闭条件（verbatim） | 出处 |
|---|---|---|
| G-01（→ G-201） | “**R2 BLOCKED**; close in packaging/R2 pilot with manifest, digest, migration map, compatibility fixture and artifact” | `2026-09-23-saas-align-identity-organization-policy.md:109` |
| G-02 | “Current Wave 1 records `NOT RUN`; close with CI/staging PostgreSQL lane, not fake substitution” | 同文件 `:110` |
| G-201 残项 | “残留关闭条件：真实模块 manifest 的 release identity 由 release tool 实际组装并钉入 pilot 工件（Wave 5 关闭）” | `2026-09-24-saas-align-module-packaging.md:307-308` |
| G-202 | “关闭条件：pilot 工件带 release identity 且外部工程仅依赖发布产物编译/测试通过。必需测试：§10 条目 A-10 的 workspace 外消费者部分与 A-12（NOT RUN）” | 同文件 `:312-313` |
| G-202 校准（2026-09-27 外部复审） | “**G-202**（独立发布工件 + 外部消费者）**仍未关闭**，缺的是授权数据库上的单模块独立安装/升级与运维回滚证据” | `PLAN-0014:560-561` |
| G-203 | “pilot 模块的迁移落 `<namespace>` 子目录 + per-namespace `MANIFEST.sha256`，`MigrationRevision.set_digest` 钉定；平面序列不重写（历史保持）。必需测试：per-namespace sha 校验 + release identity 钉定测试（Wave 5）” | `2026-09-24-saas-align-module-packaging.md:318-320` |
| G-203 校准（2026-09-27） | “…`G-203`（PostgreSQL 共享 `019` 单文件、同 …” 继续有效 | `PLAN-0014:561` |

> 读法：G-202 的**原文**关闭条件已被 Wave 5A 证据面覆盖；2026-09-27 的外部校准把「授权数据库上的
> 单模块独立安装 / 升级与运维回滚」并入本 gap 判定面。两条都列，不用较小口径宣布关闭；同时
> 原文条件也**不包含** publish / 注册中心，本矩阵不擅自加高门槛。

## 4. 主矩阵 — G 项

状态词汇：`CLOSED（局部）` = 原条件的某个明确子句已被当前证据满足；`有证据 / 残项收窄` = 残项有了
对应证据但全条件仍未满足；`OPEN` = 无当前证据；`NOT RUN` = 未执行；`BLOCKED` = 缺授权 / 环境。

| ID | 承接 | 状态 | 实际执行范围（只写真正跑过的） | 当前源码 / 固定 SHA | 证据路径 | 证据年代 | 剩余动作 | owner |
|---|---|---|---|---|---|---|---|---|
| **G-01 → G-201**（canonical module release identity 缺失，C1） | Wave 1 G-01 由 Wave 2 G-201 重编号承接，**同一件事，只记一次** | 值契约 / 声明一致性：**CLOSED（局部）**；真实工件组装：**有证据 / 残项收窄**；整体 **未关闭** | ① `cargo test -p business-module-contracts -p business-application-compiler --all-features` → 113 passed（`69d9a6b0`）；② `cargo test --workspace --all-features` 在独立 `--target-dir` 下 exit 0，**864 passed / 0 failed / 70 ignored，191 suites**（`c22939f0`；默认构建目录被两个被占用 bin 阻塞，`os error 5`，两份日志都归档）；③ `module-release verify` 两模块各 12 项检查 exit 0（候选钉 `c22939f0`，HEAD `c73955e` 上复验仍 PASS）；④ 独立审阅重跑两个保留反例各 exit 1，命中具名检查 | `1e3a25b3`（值契约与 `verify_release_identity`）→ `c22939f0`（工具组装 + 钉入工件） | `2026-09-24-saas-align-module-packaging.md` §8 / §10；`2026-09-30-wave5a-repair2-gates/F1-closure.md` §6；`…/pilot/artifacts/{identity,policy}/`；审阅 §2 / §3 | 2026-09-24 / 09-29 / 09-30 | ① 「由 release tool 实际组装并钉入 pilot 工件」在 `c22939f0` 上已有证据 → 待独立审阅记为残项关闭；② G-01 原文的 “compatibility fixture”：spec 自述 compat 窗口「未经评审」、无上界 → 未满足；③ 迁移映射在 PostgreSQL 上 `module_owned = false`（见 G-203） | 平台架构 + 模块 owner（`identity` / `policy`）；判定：独立审阅者 |
| **G-02**（本候选的真实 PostgreSQL 证据缺失，C1 evidence blocker） | 独立项，不被任何工件 / SQLite 证据替代 | **NOT RUN / BLOCKED** | 未连库。Repair 2 归档逐行记录：`PostgreSQL 迁移执行、单模块独立安装/升级/rollback` = **BLOCKED（无数据库授权）**；70 个 `#[ignore]` 一条未执行 | 无（无数据库执行 ⇒ 无候选可钉） | `2026-09-30-wave5a-repair2-gates/F1-closure.md` §7 第 3–4 行；`…/pilot/assembly-log.md` 末行 `BLOCKED (G-02 / G-203, no database authorisation in this environment)` | 2026-09-30 | 需一次**授权**的 PostgreSQL lane：专用空测试库 + 显式 TEST URL + 已授权角色；命令与判据见设计包 §9.1 / §9.2。原条件还要求 CI / staging lane，本地一次通过不等于关闭 | 平台架构 + Wave 5B 实施者；数据库授权：用户 / 运维 |
| **G-202**（无独立发布工件与 workspace 外消费者证据，C1，R2 BLOCKER） | 独立项 | **有证据 / 残项收窄**；整体 **OPEN** | (a) pilot 工件带 release identity：`release.json` / `release-bundle.json` / `verification.json` 在 `c22939f0` 上 verify exit 0（12 项检查）；(b) **workspace 外消费者工程**：`tools/module-release-tool/consumers/{identity,policy}` 自带空 `[workspace]` 表、path 指向 packed artifact，实际 `build` + `test`（identity 1 例、policy 2 例，均为默认拒绝方向）+ 缓存预热后 `--locked --offline` 复建 + `module-release guard` 读完整 `cargo metadata` 图判定落点；(c) **仓库目录之外复建**：Repair 1 做过一次（`3*-outside-*`），Repair 2 按审阅指令 `-SkipOutsideConsumer` ⇒ **NOT RUN** | (a)(b) `c22939f0`；(c) 仅 `69d9a6b0` | `…/pilot/assembly-log.md`、`…/pilot/25-consumer-*-cargo-metadata.json`、`…/pilot/negatives/*/case.md`（19 例全 OK）；(c) `2026-09-29-wave5a-repair1-gates/3*-outside-*` | (a)(b) 2026-09-30；(c) 2026-09-29 | ① (c) 需在当前候选上重做一次才算当前证据；② 2026-09-27 校准要求的「授权库上单模块安装 / 升级 + 运维回滚」→ 设计包 §9.2 / §6.3；③ 不新增 publish / 注册中心要求（原文无此项） | 平台架构 + Wave 5B 实施者；判定：独立审阅者 |
| **G-203**（migration 所有权仍是 workspace 平面序列，C1） | 独立项；本设计包的目标 | 声明层：**CLOSED（局部）**；物理 catalog：**OPEN**；执行 ledger：**OPEN**；安装 / 升级隔离：**BLOCKED** | 已执行且可复核的是**离线只读映射**：`migration-mapping.json` 逐文件比对 catalog `MANIFEST.sha256` 与提交内字节（先 LF 折叠、后 raw），module-owned catalog 必须全量声明，`set_digest` 由 `plan-0014.migration-set.v1` 计算并被 `MigrationRevision` 钉住；虚构文件与「把共享 catalog 声称为模块独有」均被拒。**SQLite 侧已有真 ledger 事实**：`identity_migrations` / `organization_migrations` / `policy_migrations` / `document_processing_migrations` 逐目录记账。**PostgreSQL 侧未做任何事**：`_sqlx_migrations` 单账本、`migrations/MANIFEST.sha256` 单目录、`runtime-migration` 全量 embed | `c22939f0`（映射与 `set_digest`）；SQL / catalog 自 `6c63ba15` 起未变，本轮零改动 | `…/pilot/artifacts/{identity,policy}/migration-mapping.json`；`tools/module-release-tool/src/migrations.rs`；`scripts/check-architecture.ps1:610-683`（六个 catalog 的 sha 门禁）；审阅 §4 前言（“声明 namespace 和 mapping/checksum 已有证据；物理 catalog、执行 ledger、安装升级隔离继续 OPEN”） | 2026-09-30 | 按 `PLAN-0014-wave5b-migration-isolation-design.md` **Revision 1** 实施：per-namespace 目录 / manifest、per-namespace ledger、单 runner 归属、**公共 bootstrap 锁 + namespace 锁与持锁连接生命周期**、真实 PG 验收。**Revision 0 与 1 两轮都是零代码改动**；且必须区分两个不同验收目标——既有库物理隔离与空库单模块冷安装（设计包 §6.6），当前**两者证据均为零** | 平台架构（设计）→ Wave 5B 实施者 → 独立审阅者 |

### 4.1 G-203 的四个记账子项（Revision 1 新增；**Proposed，全部 OPEN，零证据**）

拆分的动机是**记账粒度**，不是降低门槛：**G-203 原条目仍是一条、仍 OPEN，分母不变**；G-202 与 A-12 的判定面同样不因拆分而改变（设计包 §7.1 分母规则）。子项状态由独立审阅裁决，本矩阵不标 Accepted。

| 子项（Proposed） | 内容 | 本矩阵状态 | 实际执行范围（只写真正跑过的） | 关闭还需要什么 |
|---|---|---|---|---|
| G-203a-i 目录隔离 | `migrations/modules/<ns>/` + per-namespace `MANIFEST.sha256` + `catalog.json` + 工件钉定 + 静态门禁 | **OPEN** | 无：目录与门禁尚未创建（S1 未实施） | S1 落地 + S4 工件钉定（V12——工件钉定不在 S1 范围，判据见设计包 §7.1；Revision 1 复审补） + 独立审阅对固定 SHA 判定 |
| G-203a-ii ledger 隔离 | 平台 ledger + 单一 writer + 公共 bootstrap 锁 / namespace 锁 + fail-closed 判据 | **OPEN** | 无：无任何 runner 代码、无任何库操作 | S2/S3 + 授权库上 V1–V4、V6–V9 全绿 |
| G-203b-i 既有库物理隔离（升级） | 13 张表从 `public` 迁到各 schema | **OPEN，且设计未完成**（设计包 §6.3–§6.5 列 7 项 OPEN；原 expand 顺序已撤回） | 无：未执行任何 DDL | 关闭全部 OPEN + 独立 ADR + 运维授权 + 备份验证 |
| G-203b-ii 单模块冷安装 | 空库上只装一个模块，不建他人表 | **OPEN，且未设计** | 无 | 独立的模块自有 init catalog 设计与依赖对象最小清单（设计包 §6.6），**不以 G-203b-i 为前提**，也不得以复制旧迁移的方式冒充模块历史 |

**判据更正记录（B2）**：Revision 0 的设计曾把"单模块冷安装"记为方案 B 已满足（"是"）。该断言已撤销——方案 B 移动的是**既有表的位置**，空库上没有既有表可搬，搬迁后冷安装仍需先跑 host baseline `1..19`。方案 A 与方案 B 在此判据上的当前状态都是**未满足、无证据**。

### 4.2 真实执行范围的边界（Revision 1 新增，B3）

本矩阵引用的 PostgreSQL 证据面必须与 `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md` §9.1.2 的**逐 suite 数据库范围盘点**一起读，不得只看命令行 URL：

- `#[ignore]` 的 PostgreSQL 套件**并非**都只操作给定库：`apps/migration/tests/migration_test.rs` 会丢弃给定库名并新建 `plan2_<uuid>` sibling 库（只建不保证清理），另有两条直接在给定库上跑迁移且不清理。
- 多条套件在 `DATABASE_URL` 缺失时**静默回落**到硬编码开发库，且回落名互不一致（`business_platform` / `business_platform_test` / `enterprise_platform`）。
- `MIGRATION__DATABASE__URL` 与 `DATABASE_URL` 互斥（同时设置即配置错误），因此"一条命令同时满足 CLI 与测试"的写法不成立；两条 lane 必须分开设置环境并各自恢复。
- 结论：任何未来轮次声称"真实执行"时，必须写明**本轮实际新建 / 改动 / 删除了哪些数据库**；只写命令与 URL 不构成执行范围证明。



### 4.3 两试点的 C0 / C1 现状

| 模块 | C0 | 未清零 C1（⇒ R2 BLOCKED） | 当前可主张的事实 | 不可主张 |
|---|---|---|---|---|
| `identity` | 0（Wave 1 无 C0，未被推翻） | G-01→G-201、G-02、G-202、G-203 | 有独立版本 `0.1.0-rc.1` 的本地候选工件；声明、契约面、迁移映射、四类摘要一致且来自固定提交；packed crate 可独立编译并被外部消费者调用公开契约 | 单模块在 PostgreSQL 上独立安装 / 升级 / 回滚；R2；已发布 |
| `policy` | 0 | 同上四条 | 同上（23 成员；`019` 的权限 seed 与 `crates/policy/src/catalog.rs` 同源，已在映射内） | 同上；另 `permission_definitions` 内含 6 个 `reserved` 的 document / contract 键，跨上下文记账语义未定（见 §6） |
| `organization`（非试点，被 `019` 顺带安装） | — | 随 G-203 | 只有一个 SQLite ledger（`organization_migrations`） | 任何 PostgreSQL 侧独立演进；**不得在两试点迁移中被顺带安装且不记账**（设计包 §3 所有权表单列） |
| foundation（两个纯 crate） | 0 | 无独立 gap 行；证据面随 G-201 / G-202 | 113 项测试 + 依赖白名单 fitness | **不计为第二个试点** |

## 5. Wave 2 R2 packaging acceptance — A-01..A-12

原验收与原始证据：`2026-09-24-saas-align-module-packaging.md` §10（代码 `1e3a25b3`，2026-09-24）。
「本轮复现」列只写**实际重跑过**的层级。Repair 2 独立审阅者明确未重跑 Rust workspace 门禁
（审阅 §3 末行），因此工作区级证据来自实现归档 `gates/38-test-workspace-isolated-target.log`，
不是审阅者复算。

| ID | 原验收（缩写） | 原始证据（测试 / 命令） | 原始判定 | 本轮（`c22939f0`）复现范围 | 证据年代 | 当前状态 | 剩余动作 / owner |
|---|---|---|---|---|---|---|---|
| A-01 | 同输入置换 → 同 contract digest | `release_surface_digest.rs::contract_digest_ignores_input_order_of_the_same_contract_surface`、`…_is_stable_across_repeated_invocations` | PASS | 经归档 workspace 测试覆盖（独立 target 864 passed） | 09-24 / 09-30 | PASS（不扩大） | — |
| A-02 | 发布元数据变 → contract digest 不变、package digest 变 | 同文件两例 | PASS | 同上 | 09-24 / 09-30 | PASS | — |
| A-03 | 契约面变（含已发布 agent capability ID / 版本）→ contract digest 变 | 同文件两例（Round 2 补 F-1） | PASS | 同上 | 09-24 / 09-30 | PASS | — |
| A-04 | 重复 ID 被拒 | compiler + contracts 既有测试 | PASS | 同上 | 09-24 / 09-30 | PASS | — |
| A-05 | 未知依赖 / 未知 endpoint 被拒 | compiler tests | PASS | 同上 | 09-24 / 09-30 | PASS | — |
| A-06 | 版本不兼容（Downgrade 拒绝、compat 窗口） | compiler / planning tests | PASS | 同上 | 09-24 / 09-30 | PASS | 试点 spec 的 compat 窗口自述「未经评审」→ 记在 G-01→G-201 残项，不在本条加判 |
| A-07 | 依赖环被拒 | `DependencyCycle` tests | PASS | 同上 | 09-24 / 09-30 | PASS | — |
| A-08 | 非法 platform dependency / 跨模块 private ref / 未知字段 | contracts integration 22 + `release.rs` unit 11 | PASS | 同上 | 09-24 / 09-30 | PASS | — |
| A-09 | **release 一致性验证**：`verify_release_identity` 对 `compile()` 输出重算双摘要并核对字段；6 类单字段错配负向；release 层 SemVer | `release_surface_digest.rs` 9 例 + `package_fingerprint_matches_the_planner_per_module_digest` + `release.rs` 2 例 | PASS（范围 = **声明一致性**；Round 1 高估已由 Round 2 纠正） | 同上；工具侧另有 `release-identity-consistency` 检查在候选上 exit 0 | 09-24 / 09-30 | PASS（限定词必须保留） | 不得再扩大为「工件字节一致性」——那是工具侧 `artifact_integrity_digest`（SP-1 未执行） |
| A-10 | 独立消费者无 concrete adapter 编译 / 测试 | 内部：依赖白名单 fitness + 113 测试只用纯 contract 类型；外部：Wave 5 fixture | **PARTIAL**（内部 PASS / 外部 NOT RUN） | **外部消费者已执行**：两消费者独立 `[workspace]` + path 指向 packed artifact，build / test / `--locked --offline` / guard（`c22939f0`）。**仓库目录之外复建 NOT RUN**（Repair 2 `-SkipOutsideConsumer`） | 09-24 / 09-29（目录外历史） / 09-30 | PARTIAL → **接近 MET**（外部消费者有当前证据；目录外复建 NOT RUN） | Wave 5B 首轮在当前候选上补做一次目录外复建即可提请关闭；owner：Wave 5B 实施者 |
| A-11 | live consumer 移除阻断、uninstall ≠ purge | planning tests（`BlockedRemoval`、`RemoveModule data_retained`） | PASS | 经归档 workspace 测试覆盖 | 09-24 / 09-30 | PASS | 迁移侧「回滚不得默认 drop 业务数据」由设计包 §5.8 承接，不属于本条证据 |
| A-12 | 真实模块 release 工件 + PG 迁移 lane | 工件：Wave 5 pilot；PG：未执行 | **NOT RUN** | 工件部分已有候选（21 / 23 成员，verify 12 项 PASS）；**PG lane NOT RUN / BLOCKED**：无授权；70 个 `#[ignore]` 未执行，四类分母保留（PostgreSQL 38 / PostgreSQL 且对象存储 3 / 仅 S3-MinIO 对象存储 28 / 内网模型 provider 1） | 09-30 | **NOT RUN（真实 PostgreSQL 部分继续）** | 设计包 §9.3 验收矩阵 + 一次授权 lane；ignored 分母按设计包 §9.4 的**增量对账**规则处理（新增 PostgreSQL ignored 会让分母增大，须如实逐条解释，**禁止**机械要求"必须仍为 70"，也禁止要求 passed/suites 恒等于 864/191）；owner：Wave 5B 实施者 + 数据库授权方 |

**计数一致性核对**：A-01..A-12 = 12 条。Wave 2 原始口径 = 10 PASS（A-01..A-09、A-11）
+ 1 PARTIAL（A-10）+ 1 NOT RUN（A-12）。本轮**不把总数改写为「更多 PASS」**：A-10 的外部子项虽有
当前证据，其「目录外复建」仍 NOT RUN，故 A-10 继续记未关闭；A-12 继续 NOT RUN。
**本轮关闭数 0 / 12**（A-01..A-09、A-11 的 PASS 是 Wave 2 验收面的条目状态，不等于 R2 admission 关闭）。

## 6. 与本设计相关的其他开放项（不作为试点关闭判据，但约束迁移设计）

| ID | 级别 | 内容（缩写） | 为什么进本矩阵 | 出处 |
|---|---|---|---|---|
| D-W4-02 | C1 | `document_idempotency` 缺复合外键 | 未来要新增 PostgreSQL DDL ⇒ 必须落到新的 namespace catalog，而不是再写进 `001..019` 平面序列 | `PLAN-0014:949` |
| D-W4-06 | C1 | 文档资源无 policy 授权调用路径 | 跨 context（document ↔ policy）读边界，设计包 §3.4 单列 | 同上 |
| D-W4-08 + ST-W4-05 | C1 | 无元数据 ↔ 对象一致性扫描 | 需新扫描 / 投影表 ⇒ 同一归属问题 | 同上 |
| D-W4-09 | C1 | PostgreSQL 缺 SQLite 已有的 evidence↔revision/artifact 绑定触发器 | 未来 DDL | 同上 |
| D-W4-11 | C1 | `document_links` 无 revision 绑定 | 未来 DDL | 同上 |
| OBS-W4-05 / 06 / 07、N-G05 | C1 | 无服务端关联身份 / 无 Collector / 日志侧无脱敏 / outbox relay 未接线 | 不在迁移面，仅登记，防止本轮被误当作已处理 | 同上 |
| G-204 / G-206 / G-09 | C2 | Manifest Registry / install executor、per-tenant 启用态、roster↔binding 证明 | 设计包明确**不**实现它们（不发明表结构），只在 §4 / §5.3 划清 namespace 与 ledger 的边界 | `2026-09-24-saas-align-module-packaging.md:324-329` |
| permission 键预留 | 观察 | `019` seed 以 `reserved = TRUE` 写入 `document.read` / `contract.*` 等键 | policy 的 catalog 替别的 context 预留键 ⇒ 跨 context 记账问题，设计包 §3 单列 | `migrations/019_identity_authorization_foundation.sql:254-260` |
| SP-1 / SP-2 / SP-5 / SP-6 / SP-7 | 提案 | 工件摘要入 identity.v2、compiler seam、resource kind、capability 目录、工件格式标准 | 审阅 §4.4.9 明令「其他提案不混入本轮最小包」 | `2026-09-29-wave5a-implementation-record.md:380-386` |

## 7. 不得从本矩阵得出的结论

1. 不得说「两个模块已通过 R2 打包验收」：本轮**关闭 0 条**，A-10 / A-12 未关闭，G 项全部未关闭。
2. 不得把 `module-release-tool` 的 12 项检查、61 个测试或 19 个负例当作模块 R2 admission 证据；
   它们只覆盖候选工件的来源与自洽。
3. 不得用 Repair 1 的仓库外 lane 日志充当 Repair 2 或 Wave 5B 的当前证据。
4. 不得用 SQLite catalog 的 per-namespace ledger 事实推断 PostgreSQL 已隔离：PostgreSQL 仍是
   `_sqlx_migrations` 单账本 + `migrations/` 全量 embed。
5. 不得把「工件存在」说成「已发布 / 已安装 / 已升级 / 已回滚」：publish / install / upgrade /
   rollback 全部 NOT RUN。
6. 不得把 Wave 4 的 OBS-W4-* / N-G05 与迁移面混为一谈，也不得因为它们未修而宣称本设计无效。
7. 不得宣称 Rust workspace 门禁在「当前 HEAD」通过：本轮为文档包，未运行产品门禁；Repair 2 归档
   的门禁属于 `c22939f0` / `a98e45c`，与本设计包提交不是同一输入。
8. （Revision 1）**不得把方案 A 或方案 B 说成"单模块冷安装已具备"**：两者在此判据上都是未满足、
   零证据；物理 schema 搬迁改变的是既有表的位置，不会让空库少跑一步 host baseline（§4.1）。
9. （Revision 1）**不得把 G-203 拆成子项当作门槛降低**：G-203 原条目仍是一条、仍 OPEN，
   G-202 / A-12 的判定面与分母不变；四个子项只是记账粒度，且全部 Proposed / OPEN，未标 Accepted。
10. （Revision 1）**不得把"目录已建 / `MANIFEST.sha256` 为空 / 门禁脚本通过"记为安装或隔离成功**：
    空 catalog 只表示预留；离线门禁通过也不等于 ADR-0029 被接受。
11. （Revision 1）**不得用命令行 URL 推断测试的实际数据库范围**：见 §4.2 的逐 suite 盘点
    （sibling 库 `plan2_<uuid>`、缺失变量时回落到硬编码开发库）。

## 8. 状态

**WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW；WAVE 5A REPAIR 2 REVIEWED（PASS）/ NOT INTEGRATED；R2 BLOCKED。**
`identity` / `policy` / 两个纯 crate 维持 **R1 / R2 BLOCKED / NOT CLAIMED**；
G-01 / G-02 / G-201 / G-202 / G-203 一律未关闭（G-203 的四个记账子项全部 Proposed / OPEN，见 §4.1）；
Wave 4 的 C1 与观察项继续有效；
ADR-0027 / ADR-0028 仍 Proposed / NOT ACCEPTED；本包新增的 ADR-0029 为 **Proposed / NOT ACCEPTED**（Revision 1
已按设计审阅 B1/B2 同步锁模型与范围边界，但**未**自行改变状态）。
Revision 1 起点 `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`，纯文档轮：未连接数据库、未实施 S1/S2/S3、
未 push / merge / tag / deploy、未改 main。

#

## 9. 追加记录（2026-09-30 Wave 5B-S1 实施轮）：静态门禁已落地，**本矩阵判定不变**

起点 `6decf1ad9ea05233d66dd3539594ff210afd99f2`，本地候选 `b25e2d4a9b455b776934495b8f3dfd2b71a13fe2`（未集成）。上文 §1–§8（含 §4.1 的"实际执行范围=无"与 §4.2）是 Revision 1 的原始记录，**一字未改**；本节只登记 S1 带来的新事实与其明确不能证明的东西。

- 新增事实：`migrations/modules/{identity-management,policy}` 两份 `catalog.json` + 空 `MANIFEST.sha256` 已入库并打印 `RESERVED`；`tools/module-release-tool` 的 `catalog-check`（默认 `module-catalog` / `pinned-baseline-history` / `baseline-embed`）与 `scripts/check-architecture.ps1` 的 namespace manifest canonical-LF-only 断言已成为静态门禁；baseline embed 点收敛为唯一一处。
- **G-203a-i 仍 OPEN**：S1 交付的是目录、manifest、静态门禁与钉住的历史摘要，**不含** S4 的工件钉定（V12），也不含独立审阅对固定 SHA 的判定；§4.1 中"实际执行范围"一栏对本轮的正确读法是"新增静态门禁，数据库执行范围仍为零"。
- **G-203a-ii 仍 OPEN**：无 `platform_module_migrations`、无 runner、无 bootstrap 锁代码（S2 未实施）。
- **G-203b-i / G-203b-ii 仍 OPEN**：未执行任何 DDL；单模块冷安装仍无证据，且两份 RESERVED catalog **不得**被读成冷安装能力。
- G-01 / G-02 / G-201 / G-202 / G-203 与 Wave 4 的 C1 **一律未关闭**；A-01..A-12 判定面不变；分母未降。
- 数据库执行范围：本轮**未连接任何数据库**（含只读）；PG 相关 suite NOT RUN（无授权），文件 symlink 路径用例 BLOCKED（无特权 + 仓库卷 exFAT）。
- ADR-0029 **仍 Proposed / NOT ACCEPTED**；离线门禁通过不构成接受，也不构成任何子项关闭。
