# PLAN-0014 Wave 5B-0 Revision 1 收口 — B1/B2/B3 的落点、推演与待执行验收

> Closeout ID: CLOSEOUT-PLAN-0014-WAVE5B-DESIGN-REVISION1-2026-09-30
> 状态：**WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW；WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；R2 BLOCKED**
> Date: 2026-09-30
> 起点（本轮所有事实与行号的取证基线）：`5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`
> 起点分支 / 工作树：`codex/plan-0014-wave5a-local-release-pilot`，起点干净，唯一未跟踪文件是本轮所依据的
> `docs/reviews/2026-09-30-wave5b-design-independent-review.md`（本轮随文档包一并入库，与 `7180eec7` /
> `f1984544` 的"归档审阅文档"做法一致）
> 执行依据：`docs/reviews/2026-09-30-wave5b-design-independent-review.md` §3（唯一下一实施包）
> 本轮性质：**纯文档轮**。未实施 S1/S2/S3，未连接任何数据库，未改 Rust / SQL / Cargo / 脚本 / 旧 MANIFEST，
> 未自判 ADR-0029 Accepted，未 push / merge / tag / publish / deploy，未改 main，未重开 Wave 5A Repair 2。

## 0. 本轮做了什么（一句话）

按独立审阅的 B1/B2/B3 把 Wave 5B-0 设计包从"方向对但控制流不可执行"改为**可分别送审 S1/S2 的实施规格**：
锁与持锁连接生命周期成为一等公民（§5.9），物理隔离与单模块冷安装被拆成两个独立目标（§6.6），验收命令按
环境变量互斥性拆成两条 lane 并逐 suite 盘点了真实数据库范围（§9.1-§9.2），并附一份收窄后的 S1 可编码规格
（§11）与 S4 历史候选兼容要求（§5.7 HC-1..HC-5）。**没有写一行产品代码。**

## 1. 交付清单

| # | 文件 | 变更 | 对应审阅 §3.1 许可项 |
|---|---|---|---|
| 1 | `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md` | Revision 0 到 **1**；新增 §0.1 修订记录、§5.9（锁与持锁连接）、§6.3-§6.7（方案 B 降级与两个目标分离）、§7.1（四子项与分母规则）、§9.1.2/§9.1.3（数据库范围盘点）、§9.2（两条环境 lane）、§9.4（V14 增量对账）、§10 D-7、§11（S1 规格）；更正 §2.3 P-4/P-5/P-6、§5.3、§5.4、§5.5、§5.7、§8、§9.3 | 许可项 1（保留历史修订说明） |
| 2 | `docs/adr/ADR-0029-module-namespaced-migration-catalogs-and-ledger.md` | **仍 Proposed**；同步 B1（3.3 表 DDL 与 3.5 锁/连接/超时/取消全节重写）、B2（3.7 新增两条非目标 + 第 4 节代价段 + 第 5 节新增两行被拒方案 + 第 6 节验收指针）；3.2 增 `schema_version` 与未知字段拒绝 | 许可项 2 |
| 3 | `docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md` | 新增 §4.1（四个记账子项，全 Proposed/OPEN/零证据 + B2 判据更正记录）、§4.2（真实执行范围边界）；更正 G-203 行的剩余动作与 A-12 的分母口径；§7 增 8-11 条禁止结论；§8 状态更新。**§2/§3/§5 的历史证据、固定 SHA 与 10 PASS + 1 PARTIAL + 1 NOT RUN 计数一字未改** | 许可项 3 |
| 4 | `docs/plans/current/PLAN-0014-reference-conformance-and-r2-module-readiness.md` | 末尾追加一节状态与指针（历史小节未改） | 许可项 4 |
| 5 | `docs/architecture/ARCHITECTURE_STATUS.md` | 末尾追加一节状态（历史小节未改） | 许可项 4 |
| 6 | `docs/README.md` | 末尾追加导航节 | 许可项 4 |
| 7 | `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md` | 本文件 | 许可项 5 |
| 8 | `docs/reviews/2026-09-30-wave5b-design-independent-review.md` | 归档（原未跟踪） | §3.6"检查本报告引用"所需；沿用既有归档惯例 |

未改且刻意不改：`docs/adr/README.md`（ADR-0029 索引行的描述在本轮后仍然准确：Proposed、不含物理 schema
搬迁）、`migrations/**`、任何 `MANIFEST.sha256`、`crates/**`、`apps/**`、`tools/**`、`scripts/**`、
`docs/reviews/2026-09-30-wave5a-repair2-gates/**`。

## 2. B1 — 共享 ledger 首次创建的锁保护与持锁连接生命周期

### 2.1 原问题（审阅 §2 B1）

Revision 0 让每个 namespace 各取**自己的** advisory lock，再去执行同一张 `public.platform_module_migrations`
的 `CREATE TABLE IF NOT EXISTS`。两把锁键不同 ⇒ 互不排斥；刚跑完 host baseline、尚无模块 ledger 的库上，
两个不同 namespace 的首次 `up` 会同时进入建表。另外没有规定"同一物理连接持锁并执行相关事务"，从池里任取
连接执行 SQL 再回池是允许的解释。

### 2.2 修正条款（设计包落点）

| 修正 | 位置 | 内容要点 |
|---|---|---|
| 两把锁职责分离 | §5.9.1 | **L-B 公共 bootstrap 锁**（单 bigint 键，`hashtextextended('plan0014.module-migration.bootstrap#v1',0)`，所有 namespace 共用）保护共享表的创建与结构校验；**L-N namespace 锁**（`(classid=780014, objid=hashtext(ns))` 二元组键）保护 `(namespace,revision)` 记账 |
| 键域不相交 | §5.9.1 | PostgreSQL advisory lock 的两种键形态是不同锁标签，L-B 与 L-N **结构上不可能**撞键；namespace 内部 `hashtext` 碰撞的后果只是**多**串行（安全方向），并额外要求 runner 启动时断言已 embed 的 namespace 键两两不同，否则 `LockKeyCollision` fail closed |
| 统一锁序 | §5.9.2 | host baseline 到 bootstrap 单元（COMMIT 即释放）到按 `depends_on` 拓扑序一次一把 L-N；**禁止**持 L-N 时再取 L-B，禁止同时持两把 L-N，锁序倒置型死锁被结构排除，不依赖超时兜底 |
| 去掉 `IF NOT EXISTS` | §5.3、§5.9.3 | 锁内"存在性检查 + 创建"是原子的；`IF NOT EXISTS` 会静默接受形状不同的既存对象，把 fail-closed 变成静默通过 |
| 结构校验 + 提交回读 | §5.9.3 | `relkind='r'`、六列集合与类型/NOT NULL、主键列序、两个 CHECK、owner；不符 `LedgerSchemaMismatch`，不 `ALTER` 修复；COMMIT 后回读确认表可见可查且本连接不再持锁 |
| 持锁连接生命周期 | §5.9.5 | 一个受保护单元就是一个 `Transaction`（sqlx 保证事务自始至终占同一条连接），全部语句经 `&mut *transaction`，禁止经 `&pool`；锁随 COMMIT/ROLLBACK/断开自动释放，因此"持锁连接回池"不可能发生；**应用级取消不等于断开**，须显式 ROLLBACK 并弃用连接 |
| 超时 | §5.9.5 | `SET LOCAL lock_timeout`（新配置 `database.lock_timeout_secs`，默认 30s）超时即 `LockTimeout` fail closed；**不设** `statement_timeout`（合法慢 DDL 不应被会话默认值中途打断） |
| 只读路径不建表 | §5.9.4 | `verify`/`status`/readiness 不执行 DDL、不取锁；表缺失返回 `Empty`/`Pending`（与 `apps/migration/src/main.rs:124` 及 AGENTS §15 PLAN-0003 的"只可忽略 migration table 缺失"一致）；权限、连接、形状、checksum、缺口、孤儿一律 fail closed |
| 会话级锁被否决 | §10 D-7、ADR §5 | 改为事务级锁，与仓内既有实现一致；ADR 备选表新增该行拒绝理由 |
| 协议边界 | §5.6、§5.9.5、ADR §3.5 | advisory lock 只协调遵循本协议的进程，**不构成**数据库层 DDL 权限隔离（属 D-5） |

### 2.3 静态推演（本轮无库，未执行）

1. **两个不同 namespace 首次并发 `up`（B1 的反例场景）**：A(`identity-management`) 与 B(`policy`) 都在
   bootstrap 单元请求 L-B。两进程请求的是**同一键**的同一锁标签，后者阻塞至前者 COMMIT；后者进入后看到表
   已存在，走"已存在则结构校验后 COMMIT"分支，不执行 DDL。**只可能建表一次**。
   若 A 在建表后被强杀：事务未提交，PostgreSQL 回滚并释放锁，表**不存在**（不会半张表），B 重新建表。
2. **锁序倒置**：唯一取锁序列是 `L-B 然后 L-N`，且 L-N 一次一把、按固定拓扑序；不存在 `L-N 到 L-B` 的代码
   路径，两进程之间不可能形成环路等待。
3. **持锁连接回池**：锁是事务级，事务结束时锁必然已释放，而 sqlx 只在事务结束后归还连接；"锁仍持有但连接已
   回池"在该模型下没有可达状态。若语句报错使事务进入 aborted（25P02），runner 显式 ROLLBACK 并弃用连接，
   不用被污染的连接继续下一单元。
4. **表存在但形状不同**（例如人工先建了同名视图或列缺失）：`relkind` 或列集合校验失败，`LedgerSchemaMismatch`
   退出 1，不改对象、不 `ALTER`。这正是去掉 `IF NOT EXISTS` 带来的可判定性。
5. **只读 readiness**：并发 bootstrap 与 `verify` 同时发生时无需互斥，`verify` 只读，读到"表不存在"返回
   `Empty`；二进制带了 namespace 而库里是 `Empty` 则 `Behind` 再 `NotReady`（不会自动补装）。

推演依赖的既有仓内事实（本轮核对）：`pg_advisory_xact_lock` 是仓内 8 处的统一形态
（`crates/document-postgres/src/unit_of_work.rs:34-45`、`crates/identity-postgres/src/lib.rs:360,581`、
`crates/policy-postgres/src/lib.rs:433`、`crates/organization-postgres/src/lib.rs:390`、
`crates/audit-postgres/src/lib.rs:218`、`crates/document-processing-postgres/src/lib.rs:1383`）；
`CREATE TABLE IF NOT EXISTS` 的并发不安全已由 `crates/messaging/tests/inbox_idempotency.rs:18-22` 记录在案。

### 2.4 未来测试映射（全部 NOT RUN，需数据库授权）

| 场景 | 用例 | 位置 |
|---|---|---|
| 同 namespace 两进程 | **V8a**（启动屏障 = harness gate 表；断言每 `(ns,revision)` 恰一行、`pg_locks` advisory 归零、无残留对象） | 设计包 §9.3 V8a + `crates/runtime-migration/tests/ledger_bootstrap_concurrency.rs`（S2 新增） |
| **不同 namespace 首次并发 bootstrap** | **V8b**（库刚过 baseline 且无 ledger 表；断言 `pg_class` 中该表计数为 1、两 namespace 均有行、无 `already exists` / `tuple concurrently updated`；重复 20 轮） | 同上 |
| bootstrap 持锁进程被终止后恢复 | **V8c**（`test-hooks` feature 钩子 + 强制杀进程；断言无半成品表、无 A 的 ledger 行、B 成功、锁归零；生产构建下该钩子键存在即 fail closed） | 同上 |
| revision 事务中断 | V9（fail-after 注入后重跑只补该 revision） | 同上 |
| 锁键碰撞拒绝 | 离线单测 `lock_keys_are_pairwise_distinct` | S2 |
| 结构不符 fail closed | 计划新增 `ledger_shape_mismatch_fails_closed` | S2 |

## 3. B2 — 方案 B 的搬迁时间线与"单模块冷安装"判据

### 3.1 原问题（审阅 §2 B2）

expand-2 要求在 `public.platform_users` 仍是旧表、`identity_management.platform_users` 尚未创建时先建同名兼容
视图：目标不存在且名称被占，时间线不可执行。此外把共享 019 的 13 张表搬到各 schema 并不改变"冷安装必须先跑
host baseline 1..19"的事实，而对比表把"单模块冷安装"记为方案 B"是"。

### 3.2 修正条款

| 修正 | 位置 | 内容 |
|---|---|---|
| 方案 B 降级 | §6.3 | **未完成的研究方案**，从 Wave 5B 可执行实施步骤中移除；§8 的"明确排除"列出"§6.4 的任何搬迁步骤" |
| 时间线更正 | §6.4 | 候选顺序：T1 独立事务 `CREATE SCHEMA`；T2 每 context 一个事务，**先 `ALTER TABLE … SET SCHEMA` 移动全部表，再 `CREATE VIEW public.<t> AS SELECT * FROM <ns>.<t>`**（旧名此时已释放）；T3 代码发布；T4 verify 全绿一个周期后删视图 |
| 中间态存在性表 | §6.4 | 按 T0 / T1 后 / T2 事务内 / T2 提交后 / T3 / T4 列出 `public.platform_users`（表到视图到删除）、`identity_management.platform_users`（不存在到表）、FK 闭合性、`v0_demo_*` 不动、`_sqlx_migrations` 逐字节不变、ledger 多 1 行 |
| FK 闭合性发现 | §3.1 末 | 13 张表的**全部 FK 都跨不出 owner**（`external_identities`/`tenant_memberships` 指向 `platform_users`；`organization_members` 指向 `organization_units`；`role_permissions` 指向 `roles`+`permission_definitions`；`role_bindings` 指向 `roles`），按 owner 搬移不会出现"半条 FK 跨 schema"，这是候选顺序在结构上成立的真正原因（也是 §3.4 跨上下文裸 UUID 不需要 FK 的原因） |
| 判据拆分 | §6.6 | **目标 P**（既有库物理隔离）与**目标 I**（空库单模块冷安装）分开：前置条件、推进路径、当前证据各自记账；明确 **I 不以 P 为前提**，且 I 需要模块自有 init catalog + 依赖对象最小清单 + 与 legacy 分别记账 |
| 撤销错误断言 | §0、§6.7、§4（L3 段） | "方案 B 单模块冷安装 = 是" 改为 **否，且未证明**；并写明 L3 提供的是权限/DDL 边界，不是安装能力；物理 schema 不是所有模块 R2 的唯一必选实现 |
| 分母保护 | §7.1、矩阵 §4.1、矩阵 §7 第 9 条 | 四个子项（G-203a-i 目录隔离 / G-203a-ii ledger 隔离 / G-203b-i 既有库升级 / G-203b-ii 独立冷安装）全 Proposed、全 OPEN；**G-203 / G-202 / A-12 原分母不变**；空目录只记 `RESERVED`，不记安装成功 |
| S1..S5 准入/停止分离 | §8 表 | S1 离线 catalog 与静态门禁；S2 runner 行为；S3 CLI/readiness；S4 工件 schema；S5 真实 PG；各自准入条件与"本轮不做"分列 |
| S4 历史兼容 | §5.7 HC-1..HC-5 | 见 §3.3 |

### 3.3 S4 历史 v1 候选兼容（本轮新增的硬要求）

断点是具体代码：`tools/module-release-tool/src/spec.rs:215` 把 `spec.spec_version` 与**单一**常量
`RELEASE_SPEC_SCHEMA_VERSION`（`src/lib.rs:63` 等于 `plan-0014.wave5a.release-spec.v1`）比较，且派生/校验按固定
提交读取 spec（`src/migrations.rs:340-347` 经 `inputs.revision()` 与 `inputs.text()`，即 `git show rev:path`，
实测日志 `docs/reviews/2026-09-30-wave5a-repair2-gates/green/20-green-source-derivation.log`）；
`src/packaging.rs:1583` 同样把工件 `schema_version` 与单一常量比较，`verify` 入口 `packaging.rs:246`。
**直接把常量升到 v2 会让 `c22939f` 固定候选（v1 spec 与 v1 工件）在后续 HEAD 上直接校验失败。**

因此 HC-1..HC-5 要求：读取与校验按**分派表**接受已登记的历史版本集合、生成只写当前版本；未知版本 fail closed；
在实现 v2 后的 HEAD 上对 `c22939f` 固定候选重跑 `module-release verify`，两模块 12 项检查必须仍 exit 0 且四类
摘要与准入矩阵 §2 逐字相同；v1 mapping 重算必须产出同一 `set_digest`（摘要公式 `plan-0014.migration-set.v1`
不变）；新工件写到新目录，已归档 Wave 5A 证据与 `MANIFEST.sha256` 逐字节不动。**本轮只设计，未实现。**

### 3.4 留 OPEN 的研究结论（明确未完成，不外推）

| # | OPEN 项 | 状态 |
|---|---|---|
| OPEN-B2-1 | 兼容视图上的写语义（`INSERT … ON CONFLICT DO NOTHING/DO UPDATE`、`RETURNING`、DEFAULT/序列）是否等价于直写基表。**未证明**：现有代码大量 upsert（`crates/identity-postgres/src/lib.rs:489,1224`、`migrations/019:236-280`） | 只能在授权库上按 adapter 契约套件实测 |
| OPEN-B2-2 | `search_path` 基线缺失（全仓 0 处）；限定名与每连接 `SET search_path` 是两种不同方案 | 未定，属部署面（与 D-5 绑定） |
| OPEN-B2-3 | `bp_migration` / `bp_app` 角色与 grant 矩阵未设计 | 未设计（D-5，独立轮） |
| OPEN-B2-4 | `SET SCHEMA` 的 ACCESS EXCLUSIVE 窗口与停机编排未测量 | 需真实库 |
| OPEN-B2-5 | owned sequences、视图、规则、扩展运算符、collation、`pg_statistic` 在 `SET SCHEMA` 下的跟随行为未逐项核实 | 不猜测数据库行为，需探测 |
| OPEN-B2-6 | `v0_demo_*`（由 `019` rename 而来、owner 是 host baseline）是否需要保留 `public` 视图 | 未定 |
| OPEN-B2-7 | G-203b 与"单模块冷安装"的判据关系（物理隔离非充分条件） | 属判据裁决，判定权在独立审阅 |
| OPEN-I-1 | 目标 I 的模块自有 init catalog 设计与依赖对象最小清单 | **未设计**（本轮只给出 §3.4 末的结构事实：identity 适配器 SQL 只引用自有 5 表，对 `tenants` 是裸值引用无 FK；这只说明"结构上可分"，**不是**可独立安装的证据） |

### 3.5 未来测试映射

| 判据 | 用例 / 命令 | 状态 |
|---|---|---|
| S4 历史候选仍可 verify | `pinned_wave5a_candidate_still_verifies_after_schema_v2`；现存的 `scripts/plan0014-wave5a.ps1`（V13）只证明 `c22939f` 候选自洽 | 未来；V13 现在可执行但本轮未跑 |
| 未知 schema 版本拒绝 | `unknown_schema_version_is_refused` | 未来（S4） |
| 独立安装仍为 BLOCKED | `independent_install_stays_blocked` | 未来（S4） |
| 目标 I 与目标 P | 无：两者均未设计完成，**不排期、不写验收** | OPEN |

## 4. B3 — 验收命令的互斥配置与 fixture 范围

### 4.1 原问题（审阅 §2 B3）

`apps/migration/src/config.rs:17-21` 在两个 URL 环境变量同时存在时直接 `ConfigLoadError`（值相同也拒绝），
而 Revision 0 的前置同时设置两者，按原文执行会在连接前失败。`apps/migration/tests/migration_test.rs:11-19`
还会按 `DATABASE_URL` 派生 sibling 库 `plan2_<uuid>`，与设计只允许 `plan0014_wave5b_<runid>` 的清单不一致；
"same target"不能作为"测试只操作给定库"的证明。

### 4.2 修正条款

| 修正 | 位置 | 内容 |
|---|---|---|
| 两条互斥 lane | §9.2 | LANE 1 只设 `MIGRATION__DATABASE__URL` 并 `Remove-Item Env:DATABASE_URL`；LANE 2 反之。两者都用 `Get-Item Env:…` 先快照、`finally` 精确恢复"存在或不存在加原值"；输出经 `://user:pass@` 到 `://***:***@` 过滤后 `Tee-Object` 归档；规定 `cargo run -p migration` 只出现在 LANE 1、`--include-ignored` 只出现在 LANE 2 |
| 逐 suite 数据库范围盘点 | §9.1.2 | PostgreSQL 侧 18 个测试文件（含 1 个共享 helper 模块 `apps/business-api/tests/harness/mod.rs`）全部纳入盘点，设计包表格按行为分组登记：读哪个变量、缺变量时的行为（**HARD `.expect` 与 SOFT 硬编码回落**）、实际连接的库、清理是否在失败路径执行、能否算"只操作给定库" |
| fail-closed 前置 | §9.1.3 | 新套件禁止回落；带回落的既有套件不进 Wave 5B lane；清理必须覆盖失败路径（参照 `deploy/operations/drill-backup-restore.sh:210-234` 的 `trap cleanup EXIT`），并断言本轮结束后无 `plan0014_wave5b_%` 残留 |
| V14 分母规则 | §9.4 | `passed = 864 + N` / `suites = 191 + M` / `ignored = 70 + K`，三个增量逐条对账到测试名与套件；未预期的减少即失败；**新增 PG ignored 增大分母是预期的、必须逐条解释，禁止机械固定 70**；附当前 70 条的逐文件分解表（38 / 3 / 28 / 1）作为对账分母 |
| 命令真实性 | §9.3 | 每条命令标注是否当前存在；`--namespace` 与 `verify` 标注"未实现（`apps/migration/src/main.rs:63-73`）"；V13 取消"能验证新 mapping"的歧义（只证明 `c22939f` 固定候选自洽） |

### 4.3 盘点得到的硬事实（本轮核对，含对本设计自身的更正）

1. **sibling 库只有一处来自 cargo 测试**：`apps/migration/tests/migration_test.rs` 的 `plan2_<uuid-v7>`
   （`:18` 用 `rsplit_once('/')` **丢弃给定库名**；`:23` 建库；`:222` 与 `:250` 只在成功路径 drop）。
2. **CI 本身就会建 sibling**：`.github/workflows/ci.yml:262` 对 CI 服务库 `business_platform`
   （`:169-181`、`:199`）跑 `migration_test --include-ignored`。这不构成本轮或未来 Wave 5B 的授权。
3. **非测试侧另有一处**：`deploy/operations/drill-backup-restore.sh:199` 创建
   `<live_db>_drill_<stamp>_<rand>`，且它是仓内**唯一**在失败路径也清理的示例（`:214` 加 `:234`）。
4. **回落互不一致**：`migration_test.rs` 内部同时用 `business_platform`（`:13`）与 `enterprise_platform`
   （`:137`、`:162`）；`outbox_reliability.rs:15` 与 `chain_pg.rs:24` 回落到 `business_platform_test`，
   与 CI 库名都不同。未设变量时这些套件"看起来通过"却改了另一个库。
5. **更正本设计 Revision 0 的 P-4**：第二份 embed 不止一处，除
   `crates/messaging/tests/outbox_reliability.rs:27` 外还有 `crates/audit-postgres/tests/chain_pg.rs:36`。
   §11.2 第 11 条与 §11.4 已改为**同时修两处**。
6. **ignored 计数口径已确定**：仓内 `#[ignore` 文本命中 73，其中 3 处是文档注释散文，余 **70** 条
   与归档清单（`docs/reviews/2026-09-30-wave5a-repair2-gates/ignored-tests-classified.md:11-17`）一致；
   且所有 PG 文件里的测试 **100% 被 ignore**，故 `cargo test --workspace` 永不触库。
7. **EOL 口径是真实风险**：本机 `core.autocrlf = true` 且仓库**无** `.gitattributes`；本轮重写设计包时 git
   即报 "LF will be replaced by CRLF"。§11.3 因此把历史保护钉在 **git blob 字节**，并规定工作树模式必须先
   按 `canonical.rs:77 fold_line_endings` 折叠。

### 4.4 未来测试映射

| 项 | 用例 / 命令 | 状态 |
|---|---|---|
| CLI lane 命令 | V1 / V2 / V3 / V6 / V7（`--namespace`、`verify` 待 S2/S3） | 未来 |
| 测试 lane 命令 | V8a/b/c、V9、V10（`--include-ignored --test-threads=1`，隔离 target-dir） | 未来；有授权即可 |
| V14 对账 | `cargo test --workspace --all-features --target-dir <隔离目录>` 加逐条增量清单 | 本轮未跑（纯文档轮） |

## 5. S1 规格交付（设计包 §11，本轮不实施）

已按审阅 §3.5 逐项覆盖：固定字段与 `deny_unknown_fields` 等价的严格模式、显式 `schema_version`（不改
`identity.v1`）；owner/namespace/目录三者一致；路径逃逸与 symlink/junction 拒绝（含 Windows reparse point 与
大小写重复目录）；manifest 与 SQL **双向**集合相等；重复 revision、缺口、未知依赖、环、checksum 重算；空目录
只能记 `RESERVED`；历史保护同时覆盖文件集合与**钉定 commit（`5c8355d`）的 git blob 摘要表**并写明 EOL 口径；
owner 白名单只是**静态 fitness**（不声称证明数据库权限隔离），授权扩展机制是"创建该表的 revision 自身声明
`-- @bp-owned: <table>`"从而 owner 集合可审计增长；准确文件清单、红/绿反例与三条命令；停止条件明列
S1 不建平台表、不接 CLI、不改启动迁移、不升 release spec。

**验收分母规则（审阅 §3.5 末句与 §3.3 第 5 条）已在三处落地**：设计包 §7.1、准入矩阵 §4.1、准入矩阵 §7 第 9
条。生产 catalog 激活与 baseline 全局冻结**以 Proposed ADR 的正式处理结果为准**，离线脚本通过不构成接受，
也不构成任何子项关闭。

## 6. 门禁与交付记录

| 项 | 结果 |
|---|---|
| 起点 branch / HEAD / status | `codex/plan-0014-wave5a-local-release-pilot` / `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601` / 干净，仅未跟踪的独立审阅文档 |
| `git diff --check`（本轮执行） | **exit 0**。首轮曾报 3 处 ADR 头部 trailing whitespace（Markdown 双空格硬换行），已改为段落后复跑为 0 |
| 本轮 diff 规模 | 6 改 1 新，`+722 / -134`（`git diff --stat`，不含本 closeout 与归档审阅文档） |
| 行副作用告警（非失败） | `warning: in the working copy of '…wave5b-migration-isolation-design.md', LF will be replaced by CRLF`。本机 `core.autocrlf=true` 且无 `.gitattributes`；索引内仍为 LF，与仓库既有 blob 一致；已作为 §11.3 EOL 口径的实测依据记录 |
| Rust workspace 门禁（fmt / check / clippy -D warnings / test） | **NOT RUN，本轮无产品代码改动**。不冒充本轮执行，也不借用 Wave 5A 在 `c22939f0` / `a98e45c` 的通过数字（864 / 70 / 191 只作为 §9.4 的**对账基线**引用，并注明其来源） |
| 设计包 §9 全部数据库项（含 V5 / V10 / V13 / V14） | **BLOCKED 或 NOT RUN，无数据库授权**；本轮未执行任何 SQL（含只读），未猜测凭据 |
| 架构门禁 `scripts/check-architecture.ps1` | **NOT RUN**（本轮未改 catalog 与门禁，跑了也不覆盖本轮内容；不冒充） |
| 本地提交 | 本轮文档包以本地提交交付，父提交 `5c8355d`；本文件不内嵌自身提交 SHA（自引用不可能），SHA 以本轮交付回复与 `git log --oneline -1` 为准 |
| 未做 | ADR Accepted 自判、S1/S2/S3 实施、数据库连接、`main` 改动、push / merge / tag / publish / deploy、重开 Wave 5A Repair 2 |

## 7. 送审项（判定权在独立审阅）

1. §5.9 的锁键域、锁序、持锁连接与 fail-closed 读路径是否可判定；V8b/V8c 是否足以覆盖 B1 的反例。
2. §6.3-§6.7 是否已把方案 B 从"可执行"彻底降为"未完成研究"，以及"物理隔离不等于冷安装"的拆分是否可作为 G-203
   判据裁决的基础。
3. §7.1 四子项记账是否满足"不降低原分母"的要求（G-203 / G-202 / A-12 保持原样）。
4. §9.1.2 / §9.2 / §9.4 的命令与盘点是否足以作为未来 S5 的真实执行范围声明。
5. §11 的 S1 是否可作为下一轮可编码包（其准入仍依赖本设计被接受与 ADR-0029 的正式处理结果）。

**状态：WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW；WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；R2 BLOCKED。**
