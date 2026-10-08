# PLAN-0014 Wave 5B-0：模块迁移隔离设计包

> Status: Proposed / **WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW**
> Revision: 1
> 文档 ID：PLAN-0014-WAVE5B-MIGRATION-ISOLATION-DESIGN-2026-09-30
> Date: 2026-09-30
> Revision 0 基线 SHA（§2 / §3 现状事实的取证基线）：`c73955ec30f46d839ff33d44776cef142b7d6d77`
> Revision 1 起点 SHA（本轮所有新增引用的取证基线）：`5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`
> 对应设计提案：SP-3、SP-4（`docs/reviews/2026-09-29-wave5a-implementation-record.md:382-383`）
> Revision 0 指令来源：`docs/reviews/2026-09-30-wave5a-repair2-independent-review.md` §4.4（十项必答）
> Revision 1 指令来源：`docs/reviews/2026-09-30-wave5b-design-independent-review.md` §2（B1/B2/B3）与 §3（唯一下一实施包）
> 关联准入矩阵：`docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md`
> 本轮边界：不改迁移代码、不执行任何数据库操作、不关闭 G-203、不关闭 A-12、不宣布 R2、不推送；
> 不实施 S1/S2/S3，不自判 ADR-0029 Accepted。

---

## 0. 结论先说

| 问题 | 结论（Revision 1） |
|---|---|
| 推荐方案 | **方案 A**：冻结 `migrations/001..019` 为 host baseline（历史与 `_sqlx_migrations` 一字不动），新增 `migrations/modules/<namespace>/` 每模块 catalog + 每 catalog `MANIFEST.sha256` + 一张平台 ledger 表 `platform_module_migrations`，由单一 runner（`apps/migration` + `runtime-migration` 新 API）按 namespace 写入。方案 A 本身仍是 **Proposed**，以 ADR-0029 的正式处理结果为前提（§10） |
| 方案 A 是否满足 G-203 字面关闭条件 | **满足五个字面子句**（`2026-09-24-saas-align-module-packaging.md:316-320`，逐条核对见 §7）；关闭判定权在独立审阅，本文不自判关闭 |
| 方案 A 是否满足"identity 可在 PostgreSQL 上单独冷安装"这种更强读法 | **不满足**。identity/organization/policy 的既有表全部由共享 `019` 创建，`019:21-24` 还重命名了 `001` 建的表 |
| 方案 B 是否满足"单模块冷安装" | **不满足，Revision 0 的"是"是错的**（B2）。方案 B 移动的是**既有表的位置**：空库上没有既有表可搬，搬迁后冷安装仍然必须先跑 host baseline 1..19。方案 B 描述的是既有库的物理位置/权限隔离，不是独立安装能力 |
| 单模块冷安装的当前证据 | **A、B 都没有**。它需要一份独立设计（模块自有初始化 catalog + 依赖对象最小清单，§6.6），且不以搬迁旧库为前提 |
| 方案 B 的设计完成度 | **未完成**（§6.3）。Revision 0 的 expand-2 时间线不可执行（视图指向尚未创建的表、旧名仍被旧表占用）；本轮给出修正后的候选顺序与中间态存在性表，但仍有 §6.5 的 7 项 OPEN，故已从 Wave 5B 可执行实施步骤中移除 |
| 必须新增的对象 | 1 张平台 ledger 表（owner = Platform Runtime）+ 1 个 Proposed ADR（ADR-0029，仍未接受）+ 目录与门禁扩展；**不需要**修改任何 Accepted 契约类型的字段 |
| 待决策 | 7 项（§10），其中 D-2（API/worker 启动是否继续自动迁移）是盘点暴露的真实 Baseline 偏差 |

### 0.1 修订记录

| Rev | 日期 | 基线 SHA | 内容与结论 |
|---|---|---|---|
| 0 | 2026-09-30 | `c73955e` | 首版设计包。独立审阅结论 **NEEDS REPAIR（设计包）**（`docs/reviews/2026-09-30-wave5b-design-independent-review.md:3`），阻止项 B1（共享 ledger 首次创建的锁保护）、B2（方案 B 时间线不可执行 + 冷安装判据混淆）、B3（验收命令互斥配置 + fixture 范围） |
| 1 | 2026-09-30 | `5c8355d` | 按该审阅 §3 修订：新增 §5.9（公共 bootstrap 锁 + namespace 锁 + 持锁连接生命周期，统一锁序与 fail-closed 读路径）；§6 方案 B 降为未完成研究方案并分离两个验收目标（§6.6）；§9.1/§9.2 拆分 CLI 与测试的环境作用域并逐 suite 盘点数据库范围；§7 增加四个子项记账与分母规则；§5.7/§8 补 S4 历史 v1 兼容要求；§11 给出收窄后的 S1 规格（文件清单、红绿反例、命令、停止条件）。逐项落点见 `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md` |
| — | 2026-09-30 | `6decf1a` | **S1 实施轮（不重启设计轮：Rev 仍为 1，本文判据与分母未改）**。按 docs/reviews/2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md §3 落地离线门禁，并落实其 §2 C1 历史摘要兼容规则；对本文的更正集中在 §5.2、§11.2、§11.3（OID 不是摘要 + C1 + 四方比对 + checkout 端字节稳定）、§11.4（实际落点），实施状态见 §12.1 |

本文判据的**唯一一次实施轮**（S1）状态记录在 §12.1；上表 Rev 列保持 1，实施不产生新的设计修订。
Revision 0 的历史结论按上述记录保留，未被改写；被 B1/B2 判定为错误的原文（ledger DDL 在 namespace 锁内创建、方案 B 冷安装"是"、expand-2 顺序、`V8` 单条并发用例、"V13 现在能验证新 mapping"）已在对应小节就地更正并标注 Revision 1。

**Revision 1 复审更正记录（2026-09-30，对交付提交 `7355c99` 的宿主复审；纯文档精度更正，不改变控制流、判据方向、分母或状态结论）**：① §3.2 索引计数 11 → 10（与 `019` 实测 8 条 `CREATE INDEX` + 2 条 partial unique 一致，亦与 §6.6"identity 3 索引"自洽）；② §9.1.2 结论 3 回落文件数 6 → 7（与 §9.1.3 的"上表 7 个文件"和 grep 硬编码回落 URL 的实测一致）；③ §5.9.3 第 7 步提交回读钉定为"同一物理连接 + 事务内记录的 backend pid"，消除可能空洞为真的断言；④ §7.1 G-203a-i 判据补齐 S4/V12 工件钉定半边，并与 §9.5 与准入矩阵 §4.1 口径对齐（S1 不实施工件钉定）；⑤ §9.1.2 逐 suite 表补 `apps/business-api/tests/plan_0008_postgres_minio.rs` 一行；⑥ §12 交付记录更正为"审阅文档已随父提交 `e55a5a0` 入库"。逐项映射见 `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md` §8。

---

## 1. 输入与本设计受约束的权威条文

| 权威条文 | 原文要点 | 本设计如何满足 |
|---|---|---|
| `docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md:91-98` | PostgreSQL 迁移只放 `/migrations`，由共享 `runtime-migration` embed；**正式 API 进程不自动迁移**；只有 `_sqlx_migrations` 缺失才视为空 catalog | 新 catalog 仍由共享 `runtime-migration` embed（§5.1）；写权限收敛到 `apps/migration`（§5.4、D-2）；空 catalog 判定只针对"表不存在"（§5.9.4） |
| `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md:318-325` | 迁移必须标明所属上下文；迁移历史不可变 | 每 namespace 目录 = 所属上下文；`001..019` 与 `MANIFEST.sha256` 冻结不改（§5.5、§11.3） |
| `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md:21` | 每份可变权威数据只有一个 owner | `platform_module_migrations` 的唯一 owner = Platform Runtime（migration runner），§5.3 显式命名并写入 ADR-0029 |
| `docs/architecture/SECURITY_ARCHITECTURE.md:251-260` | 迁移用独立高权限角色，运行时应用不得拥有随意 DDL | §5.6 角色矩阵 + D-2；本文不声称已满足，只把偏差登记为待决 |
| `docs/architecture/DEPLOYMENT_ARCHITECTURE.md:55-62, 333, 338` | migration 是独立部署单元，有自己权限；expand → migrate → switch → contract；回滚用 forward-fix | §5.5 安装时间线、§6.4 过渡候选顺序、§5.8 回滚策略 |
| `docs/standards/SAAS_MODULE_STANDARD.md:271-282` | 模块拥有独立迁移命名空间；只动 owner 的 schema/数据；uninstall ≠ purge | §4 namespace 定义、§5.8 无默认数据删除 |
| `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md:228-238` | 空库安装、重复运行安全、从上一版本升级 | §9.3 验收矩阵 V1/V3/V4/V5；空库安装的真实缺口见 §6.6，不以 namespace 落地冒充 |
| `docs/standards/SAAS_MODULE_STANDARD.md:370-383` + `PLAN-0014…md:809-822` | R2 需 C0=0、C1=0 且独立 reviewer 对固定 SHA PASS | 本设计不改变任何 C1 判定；G-203 仍 OPEN |
| `docs/reviews/2026-09-24-saas-align-module-packaging.md:316-320` | G-203 原关闭条件（逐字）：pilot 模块迁移落 `<namespace>` 子目录 + per-namespace `MANIFEST.sha256`，`MigrationRevision.set_digest` 钉定；平面序列不重写（历史保持）；必需测试 = per-namespace sha 校验 + release identity 钉定测试 | §7 逐子句核对 |
| `docs/reviews/2026-09-30-wave5b-design-independent-review.md:19-49`（B1/B2/B3） | 共享 ledger 首次创建必须共用固定锁并规定持锁连接生命周期；搬迁时间线不可执行且不能证明冷安装；验收命令同时设置互斥环境变量、fixture 范围未对齐 | §5.9、§6、§9.1–§9.2；逐项映射见 closeout |
| `AGENTS.md` §3（架构预检） | 关键项无法确定时先完成设计和计划，不得直接从实现开始 | 本轮只交付设计与规格，S1/S2/S3 不实施；ADR-0029 保持 Proposed |

---

## 2. 现状事实（只读盘点，基线 SHA `c73955e`；P-5/P-6 取证于 `5c8355d`）

### 2.1 六个 catalog 与 ledger

| Catalog | 文件 | 文件数 | 完整性记录 | 运行时 ledger | ledger 写入者 | checksum 是否回读校验 |
|---|---|---|---|---|---|---|
| PostgreSQL workspace | `migrations/001..019.sql` | 19 | `migrations/MANIFEST.sha256` | `_sqlx_migrations` | sqlx `Migrator::run`，被 `apps/migration/src/main.rs:87`、`apps/business-api/src/composition.rs:126`、`apps/governance-worker/src/main.rs:50` 三个进程调用 | sqlx 内部校验 |
| identity | `crates/identity-sqlite/migrations/001_identity_foundation.sql` | 1 | 同目录 `MANIFEST.sha256` | `identity_migrations(version,checksum,applied_at)` | `crates/identity-sqlite/src/lib.rs:37-63` | **否**：`checksum` 存 `include_bytes!` 原文（`:56`），只读 `MAX(version)`（`:43-47`） |
| organization | `crates/organization-sqlite/migrations/…` | 1 | 同上 | `organization_migrations` | `crates/organization-sqlite/src/lib.rs:39-68` | 否 |
| policy | `crates/policy-sqlite/migrations/…` | 1 | 同上 | `policy_migrations` | `crates/policy-sqlite/src/lib.rs:41-67` | 否 |
| document | `crates/document-sqlite/migrations/001..006` | 6 | 同上 | `_sqlx_migrations`（共享） | `crates/document-sqlite/src/migrations.rs:20-82` | **是**：`MigrateError::VersionMismatch`（`:34-36`）、`VersionMissing`（`:33`）、dirty 拒绝（`:23-25`） |
| document-processing | `crates/document-processing-sqlite/migrations/001..008` | 8 | 同上 | `document_processing_migrations` | `crates/document-processing-sqlite/src/lib.rs:145-375` | 否 |

`MANIFEST.sha256` 语法：`<64 位小写 hex><两个空格><NNN_名字.sql>`，只列文件名、不列自身、不自我哈希。校验只存在于 `scripts/check-architecture.ps1:610-676`（`Assert-MigrationManifest`，六处调用 `:678-683`）与 `tools/module-release-tool/src/migrations.rs:181-259`；**`crates/**` 内没有任何 Rust 代码读取 MANIFEST**。

### 2.2 共享 PostgreSQL catalog 的可执行面

- `crates/runtime-migration/src/lib.rs:6` `pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");` —— 全目录 embed，无过滤、无 namespace、无 verify-only、无自有错误类型；另有 `latest_version()`（`:34-40`）、`classify()`（`:43-53`）、`MigrationCompatibility{Empty,Behind,Equal,Ahead}`（`:8-14`）。
- CLI：`apps/migration/src/main.rs:63-73` 只接受 `--backend <postgres|sqlite> <up|status>`（严格 3 参数位置解析），没有 `--namespace` / `--dry-run` / `--target-version` / `verify`。`up` 在库不存在时会 `create_database`（`:77-80`）。`status` 只比版本号、不比 checksum，且落后也返回 0（`:98-144`）；"migration table 不存在"在 `:124` 被单独容忍为"尚未应用"，与 AGENTS §15（PLAN-0003）的 status 忽略规则一致。
- 兼容性读取：`apps/business-api/src/state.rs:233-254`（`SELECT COALESCE(MAX(version),0) FROM _sqlx_migrations` → `classify` → readiness）。
- 仓内既有 advisory lock 形态：全部 8 处均为**事务级** `pg_advisory_xact_lock(hashtextextended($1, 0))`，且都在 `pool.begin()` 之后的事务内获取——`crates/document-postgres/src/unit_of_work.rs:34-45`（并 `:438`）、`crates/identity-postgres/src/lib.rs:360,581`、`crates/policy-postgres/src/lib.rs:433`、`crates/organization-postgres/src/lib.rs:390`、`crates/audit-postgres/src/lib.rs:218`、`crates/document-processing-postgres/src/lib.rs:1383`。仓内**没有**任何会话级 `pg_advisory_lock` 用例（Revision 1 的 §5.9 沿用前者，理由见该节）。

### 2.3 盘点暴露的六条事实性偏差（本设计必须处理，不得当作正确现状）

| # | 事实 | 冲突条文 | 本文处置 |
|---|---|---|---|
| P-1 | `apps/business-api/src/composition.rs:126` 与 `apps/governance-worker/src/main.rs:50` 在进程启动时执行 `MIGRATOR.run` | `PERSISTENCE…:96-98`（正式 API 进程不自动迁移）、`SECURITY…:251-260`（运行时不得有随意 DDL） | 记为 D-2，方案 A 的"单 writer"命题依赖它；本文不擅自改代码 |
| P-2 | SQLite IAM ledger 的 `checksum` 列存 SQL 原文且从不回读 | `ARCHITECTURE_FITNESS_FUNCTIONS.md:228-238` 的完整性要求 | 新 PG ledger 存 64 位 hex 摘要并每次回读（§5.3/§5.4）；SQLite 侧不改动（SP-4 边界，§8） |
| P-3 | `MANIFEST.sha256` 只在构建期/门禁校验，运行时无人校验 | 同上 | 门禁 + runner 双侧校验（§5.4、§11） |
| P-4 | baseline catalog 被 embed **三次**：唯一合法的 `crates/runtime-migration/src/lib.rs:6`，加上测试里私自再 embed 的 `crates/messaging/tests/outbox_reliability.rs:27` 与 `crates/audit-postgres/tests/chain_pg.rs:36`；现有门禁 `check-architecture.ps1:52` 只扫 `apps/migration/src`，两处测试侧 embed 都在其视野之外 | `PERSISTENCE…:91-96` 单一 embed 点 | 纳入 §11 S1 的扫描范围扩展，并**同时修两处**（Revision 0 只列了一处，不完整） |
| P-5 | `MIGRATION__DATABASE__URL` 与 legacy `DATABASE_URL` **同时存在即 `ConfigLoadError`，值相同也拒绝**（`apps/migration/src/config.rs:17-21`） | 无（这是既有正确设计） | Revision 0 §9.2 前置同时设置两者 → CLI 在连接前就失败。Revision 1 拆成两条互斥的环境作用域（§9.2） |
| P-6 | `#[ignore]` PG 套件**并非**都只操作给定库：`apps/migration/tests/migration_test.rs:11-19` 会**丢弃给定库名**并派生 sibling 库 `plan2_<uuid-v7>` 后 `create_database`（`:21-30`），只在成功路径末尾 drop（`:221-224`、`:249-252`）；另有两条套件直接在给定库上 `MIGRATOR.run` 且**不创建也不清理**（`:132-155`、`:157-177`）；多条套件在未设变量时**静默回落到硬编码开发库**，回落名还互不一致（`business_platform` / `business_platform_test` / `enterprise_platform`，详见 §9.1.2） | 设计 §9.1 只授权 `plan0014_wave5b_<runid>` | Revision 1 逐 suite 盘点数据库范围（§9.1.2），新 namespace 测试限定显式授权库并禁止回落（§9.1.3）；旧 sibling-DB / 回落套件不进 Wave 5B lane，除非该轮授权清单逐条列出可新建与可删除的库名模式 |

---

## 3. `019_identity_authorization_foundation.sql` 对象归属表（§4.4 第 1 项）

文件头 `:2-6` 已声明三组 owner；`:7-8` 声明"跨上下文引用刻意使用裸 UUID，不建跨上下文外键"；`:9-11` 声明种子与 `crates/policy/src/catalog.rs` 同构。

### 3.1 表（13）

| 对象 | 行号 | 类型 | 所属 context（唯一 owner） |
|---|---|---|---|
| `platform_users` | 28-35 | 表 | identity-management |
| `external_identities` | 37-46 | 表（FK→`platform_users`） | identity-management |
| `tenant_memberships` | 48-59 | 表（FK→`platform_users`） | identity-management |
| `identity_idempotency` | 66-75 | 幂等 ledger | identity-management |
| `platform_bootstrap_executions` | 81-91 | 执行 ledger（PK `(tenant_id,issuer,subject,config_digest)`） | identity-management |
| `organization_units` | 98-110 | 表（自引用 FK） | **organization** |
| `organization_members` | 115-127 | 表（FK→`organization_units`） | **organization** |
| `organization_idempotency` | 132-141 | 幂等 ledger | **organization** |
| `permission_definitions` | 145-151 | 表 | policy |
| `roles` | 153-168 | 表 | policy |
| `role_permissions` | 175-179 | 关联表（FK→`roles`、`permission_definitions`） | policy |
| `role_bindings` | 181-214 | 表（FK→`roles`） | policy |
| `policy_idempotency` | 223-232 | 幂等 ledger | policy |

**FK 闭合性（Revision 1 新增，B2 的关键前提）**：上表全部 FK 的引用方与被引用方**都在同一 owner 内**（`external_identities`/`tenant_memberships`→`platform_users`；`organization_members`→`organization_units`；`role_permissions`→`roles`+`permission_definitions`；`role_bindings`→`roles`）。因此按 owner 搬移时不会出现"半条 FK 跨 schema"的形态——这是 §6.4 候选顺序在结构上成立的原因，也是 §3.4 裸 UUID 跨上下文引用**不需要**FK 的原因。

### 3.2 索引（10，Revision 1 复审更正：原记 11；`019` 实测 8 条 `CREATE INDEX` + 2 条 `CREATE UNIQUE INDEX`，`019:61,63,93,112,129,170,172,216,218,220`）

| 对象 | 行号 | owner |
|---|---|---|
| `ix_tenant_memberships_tenant_joined` / `ix_tenant_memberships_tenant_status` | 61-62 / 63-64 | identity-management |
| `ix_bootstrap_lookup` | 93-94 | identity-management |
| `ix_organization_units_tenant_parent` / `ix_organization_members_tenant_user` | 112-113 / 129-130 | **organization** |
| `ux_roles_system_key`（partial unique）/ `ux_roles_tenant_key`（partial unique） | 170-171 / 172-173 | policy |
| `ix_role_bindings_tenant_user` / `ix_role_bindings_tenant_role` / `ix_role_permissions_permission_key` | 216-217 / 218-219 / 220-221 | policy |

约束（CHECK / PRIMARY KEY / UNIQUE）随所在表同 owner，不单独记账。**本文件无触发器、无函数、无 `COMMENT ON`、无 schema 限定、无 `CREATE SCHEMA`。**

### 3.3 DML（唯一的写入语句）

| 语句 | 行号 | 写入对象 | 行数 | owner |
|---|---|---|---|---|
| `INSERT INTO permission_definitions … ON CONFLICT (stable_key) DO NOTHING` | 236-261 | `permission_definitions` | 24（其中 7 行 `reserved=TRUE`：`document.read`、`document.review`、`contract.read`、`contract.create`、`contract.update`、`contract.review`、`contract.archive`，`:254-260`） | policy |
| `INSERT INTO roles …` 两行固定 UUIDv5 id | 263-270 | `roles` | 2（`system.bootstrap-admin`、`system.platform-admin`） | policy |
| `INSERT INTO role_permissions … CROSS JOIN …` | 273-280 | `role_permissions` | 34（2 角色 × 17 非 reserved 权限） | policy |

核对：`crates/policy/src/catalog.rs` 现有 26 个 key，其中 2 个是角色 stable key（`:94-95`），其余 24 个与上述种子集合完全一致——**当前无漂移**；但今天新增一个权限 key 只能改这个已冻结文件或在 `migrations/` 追加新文件，这正是"未来 revision 必须落 namespace"的现实动机（§7）。

**upsert 面（Revision 1 新增，供 §6.5 OPEN-B2-1 记账）**：仓内已存在的写路径包含 `ON CONFLICT DO NOTHING` / `DO UPDATE`，例如 `crates/identity-postgres/src/lib.rs:489`（`INSERT INTO platform_users … ON CONFLICT (user_id) DO NOTHING`）与 `:1224`（`INSERT INTO platform_bootstrap_executions … ON CONFLICT (…) DO UPDATE SET …`）。这三张幂等/执行 ledger 与 policy 种子都是 upsert 密集对象，是方案 B 兼容视图写语义必须实测而不能假设的具体位置。

### 3.4 跨 context 引用与写入依赖（无 FK，单列）

| 列 | 行号 | 读取方 → 被引用 owner | 是否有 FK |
|---|---|---|---|
| `organization_members.user_id` | 118 | organization → identity-management | 否（裸 UUID） |
| `role_bindings.user_id` | 184 | policy → identity-management | 否（裸 UUID） |
| `role_bindings` 的 `scope_org_unit_id` scope 列 | 181-214 | policy → organization | 否（裸 UUID） |
| 全部 `tenant_id`（`50,100,117,133,155,183,224`） | — | 三 context → v0 `tenants`（`001_initial.sql:5`） | 否 |

`019` 不引用 `documents` / `document_*` / `contracts` / `outbox_events` / `audit_events`。**结论：跨 context 依赖是"值引用 + 应用层约束"，不是数据库约束**；因此任何 namespace 拆文件都不需要跨 schema 外键，但也不能宣称数据库层阻止跨模块写入（本来就不阻止）。

**适配器实际引用面（Revision 1 新增，供 §6.6 依赖对象最小清单）**：`crates/identity-postgres/src` 的 SQL 只出现 identity 自有 5 张表，不出现 `audit_events` / `outbox_events` / `tenants` / `roles` / `role_bindings`（grep 口径，`platform_users` 命中 `:262,489,917,1011,1092`，`identity_idempotency` `:387,424`，`platform_bootstrap_executions` `:1185,1224`）。即 identity 适配器的 **DDL 级**依赖只有它自己的表；对 `tenants` 的依赖是**语义/应用层**的（裸 `tenant_id` 值），不是数据库约束。该事实只说明"结构上可分"，**不构成**"已实现单模块冷安装"的证据（§6.6）。

### 3.5 破坏性 rename（必须在归属表中记账）

`019:21-24` 对 `001_initial.sql:29,39,46,53` 创建的 demo 表执行 `ALTER TABLE … RENAME TO v0_demo_*`（`roles`、`role_permissions`、`permissions`、`user_roles`）。含义：

1. `019` 不是自包含单元——单独执行 `019`（例如"只装 policy"）会重命名由 host baseline 的 `001` 建的表，且 `019` 的 `roles`/`role_permissions` 与 `001` 的同名表冲突；
2. 这两个对象的所有者是 **host baseline（legacy v0 demo）**，被 `019` 的 policy 段落改写；
3. 因此"identity 单独安装不创建 organization/policy 表"在今天的平面序列里**结构上不可能**，这是 §7 判据核对的硬事实。

### 3.6 Organization 的记账结论（评审明确要求）

`organization` 不是 Wave 5A pilot、没有 release spec，但它的 3 张表 + 2 索引今天被 `019` 顺带创建。本设计的记账方式（**不新增模块、不发明 owner**）：

- 归属：`organization` context（§3.1 三行显式列出），其 revision 记录 = host baseline `_sqlx_migrations` 的 version 19 行；
- 声明：`tools/module-release-tool/src/migrations.rs` 的 host baseline 记账条目必须列出 baseline catalog 内的 context→表映射（从 `019:2-6` 头注释 + `CREATE TABLE` 实际文本对账，测试保证两者一致，见 §8 S4），因此 identity/policy 工件里会**显式写出**"本模块与 organization 共享 host baseline revision 19"，而不是隐藏；
- 未来：organization 需要自己的 namespace 目录时才建（`migrations/modules/organization/`），不提前建空目录。

---

## 4. 三层"namespace"必须分开（§4.4 第 3 项）

| 层 | 名称 | 今天 | 目标 | 隔离强度 |
|---|---|---|---|---|
| L1 | **目录 namespace**（catalog 物理位置） | 无：`migrations/001..019` 平面序列 | `migrations/modules/<owner-context>/NNN_*.sql`，每目录独立 `MANIFEST.sha256` | 构建期可校验（sha + 门禁 + 工件钉定） |
| L2 | **ledger namespace**（执行记账键） | 无 PG 侧概念，只有全局 `_sqlx_migrations` | `platform_module_migrations(namespace, revision)` 复合主键 | 运行时可校验（版本、checksum、缺口、孤儿） |
| L3 | **物理 schema namespace**（PostgreSQL `SCHEMA`） | 无：全库零 `CREATE SCHEMA`、零 `public.` 限定、零 `search_path`（全仓仅 1 处散文命中） | 方案 A **不做**；方案 B 才做 | 数据库层强制（角色/授权/grant 边界） |

`MigrationRevision.migration_namespace`（`crates/business-module-contracts/src/release.rs:282-287`）今天只是声明字段。方案 A 把它同时变成 L1 + L2 的真名，**不**触及 L3。G-203 原文（`:316-320`）要求的正是 L1+L2；"独立演进/校验"由 L2 提供。

**Revision 1 补充（B2）**：L3 提供的是**权限与 DDL 边界**，不提供"空库单模块冷安装"能力。冷安装取决于**安装目录本身是否只含该模块的对象**（L1）加上**是否不再需要 host baseline**（当前无人满足），与旧表今天躺在哪个 schema 无关。把 L3 当成所有模块 R2 的唯一必选实现是不成立的：它可以是 G-203b 的一种落地方式，但不是关闭"独立安装"判据的充要条件。

---

## 5. 方案 A（推荐）：host baseline 冻结 + 每模块 catalog + 单一 runner

### 5.1 目录与 embed 布局

```
migrations/                         # host baseline（冻结，只读，19 个文件 + MANIFEST.sha256）
  001..019_*.sql
  MANIFEST.sha256
migrations/modules/
  identity-management/
    catalog.json                    # namespace 元数据（§5.2、§11.1）
    MANIFEST.sha256                 # 只覆盖本目录 SQL
    001_<owner>_<topic>.sql         # 第一条 identity-management 自有 revision（未来）
  policy/
    catalog.json
    MANIFEST.sha256
```

- 新 SQL **只允许**进 `migrations/modules/<ns>/`；`migrations/*.sql` 由新门禁冻结在现有 19 个文件名（§11 S1）。这保证 L2 的两个版本空间永不重叠：`_sqlx_migrations.version ∈ 1..19`，`platform_module_migrations.(namespace,revision)` 独立编号。
- embed 机制：`crates/runtime-migration` 新增 `build.rs`，构建期遍历 `migrations/modules/*/`（`std::fs`，无新依赖，`cargo:rerun-if-changed`），生成 `include_str!` 表 + 每文件 sha256；`MIGRATOR`（`:6`）保持不动。理由：模块新增迁移 = 新增目录，核心代码零改动；不把 `migrations/` 变成运行时文件系统依赖（`deploy/Dockerfile.rust-app:10` 仍只为 baseline 复制）。**前置事实（Revision 1 补充）**：仓内当前**没有任何** `build.rs`（`**/build.rs` → 0 文件），也没有编译期 `sqlx::query!` 宏或 `SQLX_OFFLINE`，即没有 crate 需要在编译期连数据库；因此新增 `build.rs` 是首个构建脚本先例，S2 必须先确认它在 CI（`.github/workflows/ci.yml`）与容器构建（`deploy/Dockerfile.rust-app`，只 COPY baseline 目录）下都能取到 `migrations/modules/`——这是 S2 的一条独立风险项，不属 S1。
- 拒绝项：每 namespace 一个 `sqlx::migrate!`（新增模块必须改核心代码）；运行时读目录（需要把 SQL 随镜像发布，且与 `runtime-migration` 的"embedded catalog"定位冲突）。

### 5.2 `catalog.json`（每 namespace 一份，最小字段）

| 字段 | 例 | 校验 |
|---|---|---|
| `schema_version` | `plan-0014.wave5b.catalog.v1` | **Revision 1 新增**：显式版本常量，未知值拒绝（不猜测、不按"看起来兼容"接受）；三方读取者共用同一常量 |
| `namespace` | `identity-management` | 必须等于目录名，且等于 `data_owner`（沿用 `specs/identity.json` 的 `migration_namespace` 规则，authority 见 `2026-09-24…md` §8 G-203 / §16 D-02） |
| `owner` | `identity-management` | 与 `BOUNDED_CONTEXT_MAP.md` 的 context 名一致 |
| `ledger` | `platform_module_migrations` | 固定常量，禁止 `_sqlx_migrations` |
| `sql_schema` | `public` | 方案 A 恒为 `public`；出现其他值即门禁失败（防止悄悄做 L3） |
| `depends_on` | `["platform-baseline@19"]` | 语法固定为 `name@revision`：`platform-baseline@N` 或 `<namespace>@N`，`N` 为无前导零的正整数；裸名 `platform-baseline`、`@0`、`@019`、空名一律拒绝（S1 复审更正：不猜测、不做"看起来等价"的规范化）。门禁另检查：无环、同一目标不重复声明、被依赖 namespace 在本 checkout 确有该 revision、baseline 版本 ≥ 声明值 |

不引入新格式标准：`catalog.json` 只被 `check-architecture.ps1`、`tools/module-release-tool`、`runtime-migration/build.rs` 三方读取，三方共用同一组 schema 校验测试。**未知字段策略、字段固定顺序与反例见 §11.1**（Revision 1 按审阅 §3.5 收窄为 S1 规格）。`catalog.json` 的 `schema_version` **不改变** `identity.v1` / `MigrationRevision` 的任何字段（§5.7）。

### 5.3 新 ledger 表（唯一新增可变权威数据）

```sql
-- 由 runner 在**平台 bootstrap 锁**内的专用事务中创建（§5.9），不属于任何业务模块。
-- Revision 1：不是 CREATE TABLE IF NOT EXISTS，且不在 namespace 锁内创建。
CREATE TABLE public.platform_module_migrations (
  namespace   TEXT        NOT NULL CHECK (namespace ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
  revision    BIGINT      NOT NULL CHECK (revision >= 1),
  description TEXT        NOT NULL,
  checksum    CHAR(64)    NOT NULL CHECK (checksum ~ '^[0-9a-f]{64}$'),
  applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  applied_by  TEXT        NOT NULL,
  PRIMARY KEY (namespace, revision)
);
```

- **owner（`DATA_OWNERSHIP…:21` 要求显式命名）**：Platform Runtime（migration runner）。业务模块 SQL **永远不写**这张表；因此"谁能改 ledger"有唯一答案。
- 无 `status` 列：状态由存在性推导（缺 = pending，有 = applied），避免状态列与实际对象漂移；**append-only**，无 `UPDATE`/`DELETE` 路径。
- `checksum` 存 LF 折叠后的 64 位 hex（对齐 `tools/module-release-tool/src/canonical.rs:77 fold_line_endings` 与 `src/migrations.rs:203-225` 的 `digest_form` 判定顺序），修正 P-2 存原文的做法。
- 与 SQLite 侧关系：`identity_migrations` 等保持不动（SP-4 明确把 SQLite catalog 重命名列为边界提案，§8 排除项）；两套 ledger 不共享键空间。
- **Revision 1 更正（B1）**：Revision 0 把这张表的创建放在"各 namespace 自己的 advisory lock 内"，是两个不同 namespace 首次 `up` 并发时的 DDL 竞争（`crates/messaging/tests/inbox_idempotency.rs:18-22` 已记录 `CREATE TABLE IF NOT EXISTS` 在 PG 下并发不安全）。表的创建/校验改由**所有 namespace 共用的固定 bootstrap 锁**保护，并去掉 `IF NOT EXISTS`：在公共锁内"存在性检查 + 创建"是原子的，而 `IF NOT EXISTS` 会静默接受一个形状不同的既存同名对象，把 fail-closed 变成静默通过。锁与连接生命周期见 §5.9。

### 5.4 runner 职责矩阵（§4.4 第 4 项）

保留 workspace composition root：`apps/migration` 是唯一可执行迁移的进程，`runtime-migration` 提供纯逻辑与 catalog。

| 职责 | 承担者 | 细节 |
|---|---|---|
| 谁写 `_sqlx_migrations` | 仅 sqlx `MIGRATOR`（host baseline） | 今天被 3 个进程调用（P-1）；sqlx 自带 advisory lock 保证同版本不被双写，D-2 把入口收敛到 1 个 |
| 谁写 `platform_module_migrations` | **仅 `apps/migration`**（`runtime_migration::apply_namespace`） | API / worker 只读校验，见下 |
| 两个 ledger 会不会管同一版本 | 结构上不会 | `_sqlx_migrations` 只承载 `migrations/*.sql`（冻结 1..19）；`platform_module_migrations` 只承载 `migrations/modules/**`；S1 门禁禁止向 baseline 追加文件 |
| namespace 选择 | `apps/migration` 参数 | `up` = baseline + 本二进制 embed 的**全部** namespace（fail-closed：二进制带的都要装上）；`--namespace <ns>`（可重复）= 子集，仅用于 pilot/测试，日志显式打印 restricted namespace run。`--namespace` 与 `verify` **当前未实现**（`apps/migration/src/main.rs:63-73`），属 S3 新增，见 §9.3 |
| 版本状态 | `runtime_migration::namespace_status()` | 每 namespace 返回 `{head_applied, pending, expected_head}`；复用 `MigrationCompatibility`（`lib.rs:8-14`）语义 `Empty/Behind/Equal/Ahead` |
| checksum 校验 | runner | 执行前：对 ledger 每一行重算嵌入文件摘要并比较；执行后写入本次摘要。不匹配 → `NamespaceError::ChecksumMismatch{namespace,revision}`，**不自动修复** |
| 依赖排序 | runner | 先 host baseline（`MIGRATOR.run`），再 namespace 拓扑序（`depends_on`），namespace 内 revision 升序 |
| 并发锁 | runner | **Revision 1 重写**：两把不同职责的事务级 advisory lock（公共 bootstrap 锁 + namespace 锁），锁键域、顺序、持锁连接、超时与取消语义见 **§5.9**；沿用仓内既有 `pg_advisory_xact_lock` 模式而非会话级锁 |
| dirty / 失败拒绝 | 事务原子性 + 三条 fail-closed 判定 | ① ledger 有行但 catalog 无对应文件 → `OrphanRevision`；② revision 不连续（有 2 无 1）→ `RevisionGap`；③ checksum 漂移 → `ChecksumMismatch`。三条都 exit 1 且不改任何对象。另有 `LedgerSchemaMismatch` / `LockKeyCollision` / `LockTimeout`（§5.9） |
| 恢复 | runner | 无 dirty 标记：PostgreSQL DDL 事务化，`BEGIN; <revision SQL>; INSERT ledger row; COMMIT;` 要么全成要么全无；重跑从第一个缺失 revision 继续 |
| verify-only | `runtime_migration::verify_namespace()`（只读） | **不执行任何 DDL、不取 advisory 锁**；表不存在返回 `Empty`/`Pending`，权限/连接/形状/checksum 问题一律 fail closed（§5.9.4）。`status` 升级为**按 namespace**报告并把"落后"变成非 0 退出码（今天 `main.rs:98-144` 落后也返回 0） |

### 5.5 两条时间线（§4.4 第 2 项：历史兼容）

**A. 空库全新安装**（Revision 1 更正第 2 步）

1. `MIGRATOR.run` → `_sqlx_migrations` 写入 1..19（与今天逐字节一致，checksum 由 sqlx 生成，无人改写）；
2. **公共 bootstrap 事务**：同一物理连接取公共 bootstrap 锁 → 校验或创建 `public.platform_module_migrations` → 结构校验 → COMMIT（锁随事务自动释放）→ 提交后回读确认（§5.9.2）；
3. 按 `depends_on` 拓扑序逐 namespace：`BEGIN` + namespace 锁 → 回读该 namespace ledger 状态 → revision SQL + `INSERT` ledger 行 → `COMMIT`；同一时刻只持有一个 namespace 锁；
4. 结果：`_sqlx_migrations` 19 行 + 每 namespace N 行。**不补写任何成功记录**：baseline 的真实记录仍在 `_sqlx_migrations`，新 ledger 只记它真正执行过的东西。
5. 诚实边界：这条时间线**不证明**单模块冷安装——第 1 步仍是全量 host baseline（§6.6）。

**B. 既有宿主升级**（今天的历史状态：migrations 1–19、43 张 public 表）

1. `MIGRATOR.run` 校验 1..19 checksum 全部 `Equal`，**不重放**；任一漂移即失败（sqlx 行为，本设计不绕过）；
2. 公共 bootstrap 事务首次创建 ledger 表（这是 baseline 之外唯一的 runner 自有 DDL，§5.3/§5.9.2）；
3. 只应用 namespace revision；
4. `_sqlx_migrations` 内容前后完全不变（验收矩阵 V4 用前后 dump 比对断言这点）。

**C. 空库单模块独立安装（Revision 1：本设计不覆盖，见 §6.6）**：方案 A 与方案 B 都不提供这条时间线；它需要模块自有 init catalog，属独立后续设计范围。任何验收矩阵条目都不得把它记在 A 或 B 名下。

**明确不做**：不把 001..019 复制进 namespace、不做"补记/adopt 旧 revision"、不改 `migrations/MANIFEST.sha256`、不引入 `CREATE TABLE IF NOT EXISTS` 式的条件采纳（那会让同一张表有两个定义源，只能靠额外 diff 门禁勉强兜住，拒绝）。

### 5.6 兼容与权限（§4.4 第 6 项）

| 面 | 现状（实测） | 方案 A 影响 |
|---|---|---|
| repository SQL | 全部裸表名，零 `public.` 限定。表名出现次数（grep 口径，含列名/别名噪声，**不是**精确语句数）：`identity-postgres` 21、`organization-postgres` 19、`policy-postgres` 33、`document-postgres` 13、`document-processing-postgres` 87、`audit-postgres` 6、`messaging` 8、`runtime-governance-postgres` 93，合计 280 | **零改动**（模块 DDL 仍落 `public`，在任何角色默认 `search_path` 内可见） |
| `search_path` | 全仓 0 处设置（仅 1 处散文命中），无 `PgConnectOptions`、无 `set_config` | 方案 A 不引入；方案 B 必须引入（§6.5 OPEN-B2-2） |
| 连接角色 | 单一连接串（`MIGRATION__DATABASE__URL`，legacy `DATABASE_URL`，二者互斥，`apps/migration/src/config.rs:17-21`）；API 能自动迁移意味着应用角色当前持有 DDL 权限 | 方案 A 不新增权限；提出目标角色矩阵 `bp_migration`（`CREATE`/`ALTER` on `public`，ledger owner）与 `bp_app`（DML，ledger 只 `SELECT`），作为 D-5，不在 5B-1 强推（牵动部署 runbook） |
| 既有依据 | `SECURITY_ARCHITECTURE.md:251-260`、`DEPLOYMENT_ARCHITECTURE.md:55-62,333` | 本文只登记差距，不声称已达成 |

**Revision 1 澄清（B1 边界）**：advisory lock 只约束**遵循本协议的 runner 进程**。它不能替代数据库层的角色/授权隔离（D-5），也不能阻止一个不取锁的连接直接 DDL。因此本文任何小节都不得把"有 advisory lock"写成"DDL 权限已隔离"。

### 5.7 与发布工件的绑定（§4.4 第 7 项）

| 对象 | 变化 | 不变（硬约束） |
|---|---|---|
| `MigrationRevision`（`release.rs:282-287`） | 无 | **仍是 2 字段** `(migration_namespace, set_digest)`；`identity.v1` 8 字段成员集合不扩（SP-1 明确未执行，`PLAN-0014…md:979`） |
| `set_digest`（`canonical.rs:242-260`：`sha256("plan-0014.migration-set.v1\nnamespace=…\n<backend> <file> <sha256>\n…")`） | 输入集合改变：pilot 的 postgres set 从"共享 `019`（`module_owned=false`）"变为"本 namespace 文件（`module_owned=true`）+ 声明的 host baseline 依赖" | 算法 id `plan-0014.migration-set.v1` 与摘要公式不改 |
| host baseline 依赖表达 | `ReleaseSpec` 新增 `requires_host_baseline`（spec schema id 升 `…release-spec.v2`，`lib.rs:63`），baseline 文件仍进 `set_digest` 输入 ⇒ **工件摘要传递性地钉住它依赖的共享历史** | 不新增 `ModuleReleaseIdentity` 字段 |
| `MigrationFileRecord`（`migrations.rs:41-61`） | 字段不变；`module_owned` 对新 namespace catalog 为 `true`（沿用"必须整目录全声明"规则 `:239-256`） | 不改 JSON 字段名/含义 |
| 后端评估（`migrations.rs:307-338`，现对 `postgres && !module_owned` 直接 `BLOCKED`） | 拆成两个维度：`namespace_isolation`（新 catalog 落地 = `MAPPED`）与 `independent_install`（G-203b 关闭前恒 `BLOCKED`）；mapping schema 升 `…migration-mapping.v2`，v1 保留为历史 | 不得因为 namespace 落地就把"独立安装已证明"写进工件 |
| `package_digest` / `contract_digest` | 预期**不变**（迁移映射不属于契约面；`verify_release_identity` 钉的是 namespace，不是 `set_digest`）；`artifact_integrity_digest` 会变（mapping 字节变了） | 该预期在 5B-1 实测确认，本文不作为既成事实 |

**HC-1..HC-5 历史候选兼容要求（Revision 1 新增，B2/S4 的硬约束；本轮只设计不实现）**

升 v2 不得让已固定候选失去 verify 能力。当前的单常量比较就是断点：

- `tools/module-release-tool/src/spec.rs:215` 把 `spec.spec_version` 与**单一**常量 `RELEASE_SPEC_SCHEMA_VERSION`（`lib.rs:63` = `plan-0014.wave5a.release-spec.v1`）比较，不等即报错；而派生与校验是**按固定提交读取** spec 的（`migrations.rs:340-347` 走 `inputs.revision()` + `inputs.text()`，即 `git show <rev>:<path>`，实测日志 `docs/reviews/2026-09-30-wave5a-repair2-gates/green/20-green-source-derivation.log`）。常量一旦直接改成 v2，`c22939f` 提交内的 v1 spec 就会被拒。
- `packaging.rs:1583` 同样把工件内 `manifest.schema_version` 与单一 `ARTIFACT_MANIFEST_SCHEMA_VERSION` 比较，`verify` 入口在 `packaging.rs:246`；常量升级会让已归档工件 `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/{identity,policy}/` 直接校验失败。

| # | 要求 | 验收方式（S4，未来） |
|---|---|---|
| HC-1 | **版本分派**：读取/校验路径接受**已登记的历史版本集合**（`release-spec.v1` 与 `v2`、`migration-mapping.v1` 与 `v2`、`artifact-manifest.v1` 与后继），由分派表而非单一常量决定用哪个解析器；**生成**路径只写当前版本 | 单测：v1 spec 与 v2 spec 各能解析；生成结果仍是当前版本 |
| HC-2 | **未知版本拒绝**：不在登记集合内的 `spec_version` / `schema_version` 一律 fail closed，不猜测、不按"前缀像 v1"接受 | 负例：`…release-spec.v99` → exit 1 并指名版本 |
| HC-3 | **历史候选仍可 verify**：在实现 v2 之后的 HEAD 上，对 `c22939f0a51e3c7fa2ec424f3819c6e7113a9b66` 固定候选重跑 `module-release verify`，两模块 12 项检查必须仍 exit 0，四类摘要与准入矩阵 §2 表逐字相同 | 回归测试 `pinned_wave5a_candidate_still_verifies_after_schema_v2`（未来新增；对应 `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/13-verify-*.log`） |
| HC-4 | **mapping 字节口径不变**：v1 mapping 的重算路径仍产出同一 `set_digest`；v2 只在评估维度上扩展，不改 `plan-0014.migration-set.v1` 摘要公式 | `set_digest` 回归 + `…migration-mapping.v1` 样本 golden |
| HC-5 | **不覆盖历史工件**：v2 的新工件写到新输出目录；`5c8355d` 已归档的 Wave 5A 证据目录与 `MANIFEST.sha256` 逐字节不动 | `git diff --check` + 归档目录 diff 为空的门禁 |

### 5.8 失败与回滚（§4.4 第 8 项）

- 失败：revision 事务回滚 → 无 ledger 行、无残留对象 → 直接重跑；三种脏状态 fail-closed（§5.4）。
- 回滚：**不提供 down 脚本**，遵循 `DEPLOYMENT_ARCHITECTURE.md:338` forward-fix：加列/加索引/加表/加种子用新增 revision 撤销语义（新 revision 置 `reserved`、写补偿事实，而不是 `DROP`）。
- **不做任何默认数据删除**（`SAAS_MODULE_STANDARD.md:278-279` uninstall ≠ purge）。
- 不可逆动作清单（= 将来的授权门禁，本设计一轮也不做）：
  1. `DROP TABLE` 任何业务表（含 `v0_demo_*`）；
  2. 把既有表 `ALTER TABLE … SET SCHEMA`（方案 B 的搬迁）；
  3. 删除/重命名 `001..019` 或改其 checksum；
  4. 在授权库以外的任何库执行上述任一。
  每一项都需要：独立 ADR（先 Proposed）+ 该 Wave 计划条款 + 运维显式授权 + 备份验证。

### 5.9 平台 bootstrap 锁、namespace 锁与持锁连接生命周期（Revision 1，B1）

设计目标：任何数量的 runner 进程、任意 namespace 组合、任意首次到达顺序下，共享 ledger 表**最多被创建一次**、结构**永不漂移**、且**不存在两个入口以相反顺序取锁**。

#### 5.9.1 锁键域与锁形式

| 锁 | PostgreSQL 形式 | 键 | 生命周期 | 保护对象 |
|---|---|---|---|---|
| **L-B**（平台 bootstrap 锁） | `pg_advisory_xact_lock(bigint)`，参数 `hashtextextended('plan0014.module-migration.bootstrap#v1', 0)` | **固定常量文本，与 namespace 无关**，所有 namespace 共用同一把 | 仅 bootstrap 事务（COMMIT/ROLLBACK 自动释放） | `public.platform_module_migrations` 的创建与结构校验 |
| **L-N**（namespace 锁） | `pg_advisory_xact_lock(int, int)`，`classid = 780014`（`PLAN0014_MIGRATION_NAMESPACE_LOCK_CLASS` 常量）、`objid = hashtext(namespace)` | 每 namespace 一把 | 仅该 revision 事务 | `(namespace, revision)` 记账与 revision SQL |
| **L-H**（host baseline） | sqlx `Migrator` 内置 | sqlx 内部 | sqlx 内部 | `_sqlx_migrations` |

选择说明：

- **锁键域不相交**：PostgreSQL 的 advisory lock 有两种互不相同的 tag 形态（单 bigint 与 `(classid, objid)` 二元组），因此 L-B 与 L-N 的键空间在数据库层面**结构上不可能重合**；无需再为"bootstrap 键会不会被某个 namespace 撞到"追加规则。
- **统一采用事务级锁**：仓内 8 处既有实现全部是 `pg_advisory_xact_lock` 并在事务内获取（§2.2 列表，最完整示例 `crates/document-postgres/src/unit_of_work.rs:34-45`）。事务级锁在 COMMIT / ROLLBACK / 连接断开时由 PostgreSQL 自动释放，因此本设计**没有**"锁仍持有而连接已回池"这一类泄漏；Revision 0 的会话级 `pg_advisory_lock` + "进程被杀由 PG 释放"改为事务级并把释放写成结构性事实。
- **L-N 必须显式在事务内获取**：`pg_advisory_xact_lock` 在无显式事务的自动提交语句下会随语句结束立即释放，等于没有锁。runner 的每个受保护单元都从 `pool.begin()` 开始，锁语句与后续 SQL 全部通过 `&mut *transaction` 执行，**禁止**用 `&pool` 执行（`&pool` 会在连接池里任取连接，正是审阅指出的"从 pool 任意连接执行 SQL"反例）。
- **hash 碰撞的保守处理**：`hashtext(namespace)` 只有 32 位，两个 namespace 可能撞到同一 `objid`。方向是安全的——碰撞只会让两个 namespace **多**串行（互斥过度），绝不会让本该互斥的两方同时进入。尽管如此，runner 启动时仍必须枚举本二进制 embed 的全部 namespace，断言其 `objid` **两两不同**且 `classid` 常量未被其它用途占用；碰撞 → `NamespaceError::LockKeyCollision{a, b}`，exit 1，不降级为"只用目录顺序串行"。
- 文本键 `…bootstrap#v1` 带版本后缀：将来若 bootstrap 协议本身改变（例如 ledger 换形状），新旧进程不会在同一把锁下互相误解，必须另立 ADR 后改用 `#v2` 常量。

#### 5.9.2 唯一允许的锁获取顺序

```
host baseline (sqlx MIGRATOR.run，L-H)              ← 不持 L-B / L-N
      │
      ▼
[bootstrap 单元]  BEGIN → L-B → 读表存在性 → (不存在) CREATE TABLE → 结构校验 → COMMIT(释放 L-B)
      │                                    (已存在) → 结构校验 → COMMIT(释放 L-B)
      ▼ COMMIT 后回读确认（§5.9.3）
[namespace 单元，按 depends_on 拓扑序，一次一个]
      BEGIN → L-N(ns) → 回读该 ns 的 ledger 行 → 校验 checksum/缺口/孤儿
            → revision SQL → INSERT ledger 行 → COMMIT(释放 L-N)
```

硬性规则（任何实现与测试都必须可判定）：

1. **不在持有 L-N 时再取 L-B**。bootstrap 单元永远在 namespace 循环之前完成并释放，namespace 循环内部不得调用 bootstrap。这条规则连同"同一时刻最多持有一把 L-N、且按固定拓扑序"共同排除锁序倒置型死锁——不是靠超时兜底，而是结构上不存在两种取锁顺序。
2. 一个 namespace 的**每个 revision 一个事务**（每事务一把 L-N），不跨 revision 持锁；不把多个 namespace 的 revision 塞进同一事务。
3. 不做 `pg_try_advisory_lock` + 自旋重试的"抢锁"策略：等锁用 `lock_timeout`（§5.9.5）超时即 fail closed，不静默降级。
4. `up` 不带 `--namespace` 时对全部 embed namespace 依次执行 namespace 单元；`--namespace` 子集只是缩短这个循环，**不改变** bootstrap 单元（共享表不能因为只装一个模块就少一次校验）。

#### 5.9.3 bootstrap 的校验内容（含提交结果回读）

bootstrap 事务内，取得 L-B 之后：

1. 查 `information_schema.tables`（`table_schema='public' AND table_name='platform_module_migrations'`）。
2. **不存在** → 执行 §5.3 的 `CREATE TABLE`（无 `IF NOT EXISTS`）。
3. **已存在** → 不创建、不 `ALTER`，直接进入形状校验。
4. 形状校验（对 `information_schema.columns` / `table_constraints` / `key_column_usage` / `pg_class.relkind`）：
   - `relkind = 'r'`（视图 / 物化视图 / 外部表 / 分区父表都不是本表的合法形态）；
   - 列集合恰为 `{namespace, revision, description, checksum, applied_at, applied_by}`，且类型与 NOT NULL 逐列一致（`checksum` 为 `char(64)`）；
   - 主键列恰为 `(namespace, revision)` 且顺序一致；
   - 两个 CHECK 约束（namespace 形状、checksum 十六进制形状）存在；
   - 表 owner 等于当前迁移角色。
5. 任一项不符 → `NamespaceError::LedgerSchemaMismatch{detail}`，ROLLBACK，exit 1，日志只打印期望/实际形状差异，不打印连接串。**不尝试 `ALTER` 修复**（修复旧库 ledger 形状属未来独立 ADR）。
6. COMMIT。
7. **提交结果回读**（只读，**必须在与 bootstrap 单元相同的物理连接上执行**）：确认表可见、可 `SELECT`，且 `SELECT count(*) FROM public.platform_module_migrations` 成功；同时回读 `pg_locks` 中断言不再存在该 bootstrap 的 advisory 锁（`locktype='advisory'` 且 `database = 当前库` 且 `pid = <记录值>` 为 0 行）。两处钉定（Revision 1 复审更正）：**其一**，断言的 pid 必须是 bootstrap 事务内在同一连接上执行 `pg_backend_pid()` 记录下来的值，而不是"执行回读的连接自己的 pid"——若回读被连接池派给另一条物理连接，"本连接不持锁"恒真，断言空洞失效，这正是 §5.9.5 禁止经 `&pool` 执行受保护语句的同一条理由；实现上 bootstrap 单元取单条 `Connection`，事务 COMMIT 后在同一连接上执行回读再归还。**其二**，回读整体纳入 fail-closed：记录的 pid 在回读时仍持有 advisory 锁行，或表不可见 / 不可查，即失败退出 1。"我创建成功了"必须由读回证明，不能由语句未报错推定。

#### 5.9.4 只读路径不创建表

`verify_namespace()`、`status`、`apps/business-api` readiness（`state.rs:233-254` 的扩展点）**一律只读**：

- 不执行 `CREATE`/`ALTER`/`INSERT`，不取 L-B / L-N（读路径由 MVCC 快照保护，与并发 bootstrap 竞争无害）。
- ledger 表不存在 → 返回明确状态 `Empty`（namespace 从未应用），`status` 打印 "no module revisions applied"，退出码 0；这与仓内既有规则一致：status 只可忽略"ledger 表缺失"（`apps/migration/src/main.rs:124` 的 `is_postgres_missing_migration_table`；AGENTS §15 PLAN-0003 的"status 只可忽略明确的 migration table 缺失"）。
- 必须 fail closed（**不得**映射成 Empty/Pending）：连接失败、权限不足（42501）、表存在但列缺失/类型不符（走只读 catalog 检查）、checksum 漂移、`revision` 缺口、孤儿行。
- readiness 语义：`Empty` 且二进制不带任何 namespace 时可视为就绪；二进制带了 namespace 而库里是 `Empty` → `Behind` → `NotReady`（fail closed，不做"自动补装"）。

#### 5.9.5 持锁连接、异常、取消与超时

| 场景 | 规定行为 | 依据/理由 |
|---|---|---|
| 连接来源 | 一个受保护单元 = 一个 `Transaction`；sqlx 保证一个事务自始至终占用的就是那一条物理连接，因此"同一物理连接既执行锁语句又执行受保护 SQL"由构造保证。该连接在 COMMIT/ROLLBACK 返回前不会回池 | 与 `unit_of_work.rs:34-45` 一致 |
| 正常释放 | 由 COMMIT/ROLLBACK 触发，**没有**手写 `pg_advisory_unlock`，因此不存在漏解锁路径 | 事务级锁语义 |
| 语句报错 | 事务进入 aborted 状态 → runner 显式 `ROLLBACK` → 单元失败退出；不使用被污染的连接继续下一单元 | 25P02 之后同事务内任何语句都会失败，复用会掩盖首个真实错误 |
| 应用级取消（Ctrl-C / select! 取消 future） | **取消不等于断开**：runner 捕获取消后必须完成 ROLLBACK（若连接尚可用），随后丢弃该连接（不回池复用）。锁的释放不依赖这一步，因为事务结束即释放 | 审阅 §3.2 明确要求区分"应用级取消"与"连接断开" |
| 连接断开 / 进程被 kill | PostgreSQL 在后端退出时回滚未完成事务并释放其全部 advisory 锁；DDL 事务化 ⇒ 表要么完整存在要么不存在，**不会有半成品**；重跑按 §5.9.2 重新 bootstrap | 这是唯一的进程级故障恢复手段，测试用例 V8c（§9.3）必须覆盖 |
| 超时 | bootstrap 连接在取锁前设置 `SET LOCAL lock_timeout = <database.lock_timeout_secs>`（新配置字段，默认 30 秒，S2 落地）；超锁 → `NamespaceError::LockTimeout` → exit 1。**不设置 `statement_timeout`**：一条合法的慢 DDL 不应被会话默认值中途打断，而取消慢 DDL 的代价（回滚 + 重试）比等待更高 | 保持"要么全成要么全无"的恢复语义简单 |
| 连接池上限 | runner 使用小池（建议 `max_connections(2)`）；受保护单元串行，不依赖并发换吞吐 | 减少同一进程内自相等待 |
| 日志 | 只打 redacted URL（`crates/runtime-config/src/secret_url.rs:80-101`）、锁键的**文本常量**（不打 hash 值以免误读为凭据）、namespace、revision、退出原因 | `SECURITY_ARCHITECTURE.md` 的日志面 |
| 协议边界 | advisory lock 只协调遵循本协议的进程；数据库层 DDL 权限隔离属 D-5（§5.6 澄清） | 防止把锁当授权 |

#### 5.9.6 错误类型（S2 计划，本轮不实现）

`NamespaceError { ChecksumMismatch, OrphanRevision, RevisionGap, BaselineNotApplied, LedgerSchemaMismatch, LockKeyCollision, LockTimeout }`，每个变体的 `Display` 必须给出**具名**判据字符串（供验收矩阵按名字断言，例如 `checksum-mismatch:policy:3`、`ledger-schema-mismatch:missing-column:applied_by`、`lock-key-collision:a|b`），且不得包含连接串、SQL 正文或文件绝对路径。

---

## 6. 方案 B：物理 schema 隔离（Revision 1：**未完成的研究方案，不在 Wave 5B 可执行步骤内**）

### 6.1 定义

每 context 一个 PostgreSQL `SCHEMA`（`identity_management`、`policy`、…），既有表从 `public` 迁入，adapter SQL 全部 schema 限定或每连接 `SET search_path`，角色按 schema 授权。

### 6.2 实测影响面

| 项 | 数量 |
|---|---|
| 仅 IAM 三适配器需改写的 SQL 命中点（grep 口径，见 §5.6 注） | 21 + 19 + 33 = **73**（全仓同类命中合计 280） |
| 需新增的连接会话设置 | 全仓当前 **0** 处 `search_path`，即从 0 到"每个连接池都设" |
| 需迁移的表 | identity 5 + organization 3 + policy 5 = 13（索引/约束随之移动） |
| 跨 context 裸 UUID 引用 | 4 类（§3.4），跨 schema 后仍不能加 FK（会违反 `DATA_OWNERSHIP` 的裸 UUID 设计），故隔离收益只在 DDL/授权面 |
| 角色授权 | 每 schema `USAGE` + 表级 grant，需 `SECURITY…:251-260` 的双角色拆分先落地 |

### 6.3 Revision 1 状态判定：设计未完成

Revision 0 的 §6.3 把方案 B 写成"完整过渡步骤"，其中 **expand-2 不可执行**：它要求在 `public.platform_users` 仍是旧表、`identity_management.platform_users` 尚不存在时先建同名兼容视图——目标关系不存在，且视图名与既有表名在同一 schema 内冲突。B2 判定成立。

Revision 1 的处理（对应审阅 §3.3 第 3 条）：

1. **保留**方案 B 作为对比对象与未来研究方向，并给出修正后的候选事务顺序与中间态对象存在性表（§6.4），供未来独立 ADR 使用；
2. **移除**其"可执行实施步骤"身份：Wave 5B-1 及后续 S1..S5 切片都不包含 §6.4 的任何一步，G-203b 保持 OPEN；
3. 未完成的部分**明确留 OPEN**（§6.5），不用"步骤齐全"的表象掩盖；
4. **撤销** Revision 0 的对比表中"方案 B 单模块冷安装 = 是"这一格（见 §6.6、§6.7）。

### 6.4 候选事务顺序与中间态对象存在性表（未验证、未授权、不可执行）

以下为**候选**顺序：修正了 Revision 0 的视图/搬移次序错误（同一事务内**先移动表、后创建旧名视图**），但未经真实库验证，也不构成执行授权。

前置（只读，不属于迁移）：目标库必须处于 host baseline revision 19；`<ns>.*` 目标名全部不存在；对象定义、行数、索引与约束清单已 dump 并做备份验证。

- **T1（独立事务）**：`CREATE SCHEMA <ns>;` + `GRANT USAGE …`。中间态：schema 存在且为空，对现有客户端零影响。
- **T2（每 context 一个事务，全成或全无）**，顺序固定：
  1. `ALTER TABLE public.<t> SET SCHEMA <ns>`，按 owner 一次做完（该表获得 ACCESS EXCLUSIVE 锁）；
  2. 全部表移动完成后，在同事务内 `CREATE VIEW public.<t> AS SELECT * FROM <ns>.<t>`（旧名此时已释放，视图与表不再同名冲突）；
  3. 视图与底层表的 `GRANT`；
  4. 任何一步失败 → 整个 T2 回滚 → 回到 T1 前的形态。
- **T3（代码发布，不是迁移）**：适配器改用 schema 限定名或建立 `search_path` 基线；跑 §9.3 全矩阵。
- **T4（contract）**：verify 全绿并经过一个发布周期后才删兼容视图。**自 T2 起不可逆**（回退需再次 `SET SCHEMA`，且期间经视图写入的路径要人工核对）。

中间态对象存在性表（identity 为例；organization / policy 同构；`T0`=前置，`T2*`=T2 事务内**提交后**才对外可见）：

| 对象位置 | T0 | T1 后 | T2 事务内（未提交，外部不可见） | T2 提交后 | T3 | T4 |
|---|---|---|---|---|---|---|
| `public.platform_users` | 表 | 表 | 事务内已移动并建有同名视图 | **视图** | 视图 | 删除（若做 contract） |
| `identity_management.platform_users` | 不存在 | 不存在 | 表 | 表 | 表 | 表 |
| `identity_management.external_identities` / `tenant_memberships` / `identity_idempotency` / `platform_bootstrap_executions` | 在 `public` | 在 `public` | 同事务移动 | 在 `identity_management` | 同 | 同 |
| 上述表的 FK（§3.1，全部同 owner） | 同 schema 内 | 同 schema 内 | 同事务一起移动，FK 按 OID 绑定不断裂 | 同 schema 内 | 同 | 同 |
| `public.v0_demo_roles` 等 4 张 rename 遗留表 | 表（host baseline 所有） | 表 | 不动 | 不动 | 不动 | 不在本方案范围 |
| `public.permission_definitions` / `roles` / … | 表 | 表 | policy 的 T2 事务处理 | 移到 `policy` + `public` 同名视图 | 同 | 同 |
| `_sqlx_migrations` | 19 行 | 19 行 | 不写 | **19 行，逐字节不变** | 同 | 同 |
| `platform_module_migrations` | 可能存在（方案 A 已落地） | 同 | 不写（搬迁是 revision，由 runner 记账） | 多 1 行 | 同 | 同 |

搬迁本身写成 `migrations/modules/<ns>/NNN` 新 revision（baseline `001..019` 依然不动），且每步都必须能在独立库上重放。

### 6.5 未完成项（G-203b 的前置，全部 OPEN）

| # | OPEN 项 | 为什么不能靠推理关闭 |
|---|---|---|
| OPEN-B2-1 | **兼容视图上的写语义未证明**：`INSERT … ON CONFLICT DO NOTHING/DO UPDATE`、`RETURNING`、DEFAULT/序列行为在 simple view 上是否等价于直写基表 | 现有代码大量使用 upsert：`crates/identity-postgres/src/lib.rs:489,1224`、`019:236-280` 的 policy 种子。必须在真实 PG 上按 adapter 契约套件实测，不能按"简单视图可自动更新"推定 |
| OPEN-B2-2 | **`search_path` 基线缺失**：全仓 0 处设置；限定名与 `SET search_path` 是两种不同方案，后者还会改变 `CREATE` 的目标 schema 语义 | 需要先决定连接配置基线（属 D-5/部署面），否则 T3 无定义 |
| OPEN-B2-3 | **角色与 grant 矩阵未设计**（D-5）：没有 `bp_migration` / `bp_app` 拆分，schema 隔离只剩"换个名字"，没有权限收益 | `SECURITY_ARCHITECTURE.md:251-260` 要求双角色，当前应用角色持有 DDL |
| OPEN-B2-4 | **锁与停机窗口未测量**：`SET SCHEMA` 对每张表取 ACCESS EXCLUSIVE，真实数据量下的窗口、维护窗口编排与回退点未知 | 只能在授权库上测量；本轮无数据库授权 |
| OPEN-B2-5 | **依赖对象清单不完整**：owned sequences / 视图 / 规则 / 扩展运算符 / collation / `pg_statistic`（搬迁后需 ANALYZE）在 `SET SCHEMA` 下的实际跟随行为未逐项核实 | 需真实库探测；本文不猜测 PostgreSQL 行为 |
| OPEN-B2-6 | **`v0_demo_*` 与 host baseline 表的归属边界**：这 4 张表由 `019` rename 而来但 owner 是 host baseline，是否需要在 `public` 保留同名视图未定 | 涉及 host baseline 冻结承诺（§3.5） |
| OPEN-B2-7 | **G-203b 与"单模块冷安装"的判据关系未定**：物理隔离不是冷安装的充分条件（§6.6） | 属判据裁决，判定权在独立审阅与计划 owner |

**本节的结论**：方案 B 在 Revision 1 结束时仍是**未完成设计**；上述任一项都足以阻止它进入实施。任何后续轮次要动它，必须先关闭 §6.5 全部项 + 独立 ADR + 运维授权。

### 6.6 两个必须分开的验收目标（Revision 1 新增，B2 的核心更正）

| 目标 | 内容 | 前置条件 | 谁能推进 | 当前证据 |
|---|---|---|---|---|
| **目标 P：既有库物理隔离（G-203b-升级）** | 把 13 张既有表从 `public` 移到各 context schema，并配 schema/grant 边界 | 备份 + 停机窗口 + §6.5 全部 OPEN 关闭 + 独立 ADR + 运维授权 | 未来 G-203b 轮 | **零**（本轮无库授权，未执行任何 DDL） |
| **目标 I：空库单模块独立安装（G-203b-冷安装）** | 在一台空库上只装 identity（或 policy），不创建他人表 | 模块**自有初始化 catalog**（含该模块全部对象的初建 DDL）+ 依赖对象最小清单 + 与 legacy 历史**分别记账** | 独立后续设计（不属于 Wave 5B 任何切片） | **零** |

关键更正：**目标 I 不以目标 P 为前提**。搬移旧库的表不会让空库少跑一步 host baseline；反过来，即使不搬任何旧表，只要模块有一份自有的、自包含的初建 catalog 并且记账与 legacy 分离，"空库单模块安装"在原则上也可成立。因此：

- Revision 0 的"方案 B 单模块冷安装 = 是"是错的，已撤销（§6.7）。
- 目标 I 的最小依赖对象清单（**结构事实，非实现证据**）：以 identity 为例，其 adapter SQL 只引用自有 5 张表 + 3 个索引（§3.4 末），对 `tenants` 只有裸 `tenant_id` 值引用（§3.4 表）——故 DDL 级依赖为"自有对象 + 由平台保证的租户命名空间语义"，DB 级依赖为空。这条事实只说明"结构上可分"，**不**说明"已能独立安装"。
- **记账纪律**：目标 I 的历史与 legacy 历史**分别记账**，模块自有 init catalog 是它自己的 revision 1..N，绝不把 `001..019` 复制进去冒充模块历史；"目录已存在 / manifest 为空"只能记为**预留**，不得记为安装成功（§7、§11.2）。

### 6.7 A vs B 对比（Revision 1 修正）

| 维度 | 方案 A | 方案 B |
|---|---|---|
| 满足 G-203 字面条件 | 是（C1..C5 逐子句，§7） | 是（超额） |
| **既有表的位置/权限隔离** | 否（表仍在 `public`） | **这是 B 的实际内容**（但设计未完成，§6.3/§6.5） |
| **单模块冷安装（不建他人表）** | 否（依赖 host baseline revision 19） | **否，且未证明**。搬迁只改变旧表位置，空库仍要跑 host baseline 1..19（§6.6 目标 I） |
| 触及既有数据 | 否 | 是（不可逆搬迁） |
| 改动 adapter SQL | 0 处 | 73 处 grep 命中（仅 IAM 三适配器，§5.6 口径） |
| 需要新授权 | 表创建权限（已有） | schema + grant + 停机窗口 |
| 与 `search_path` 缺位现状的关系 | 无 | 需先建立连接配置基线（OPEN-B2-2） |
| 可分轮交付 | 是（S1..S5） | 只能在 A 之后，且 §6.5 未闭合前不可排期 |
| 建议 | **Wave 5B-1 做这个**（仍须 ADR-0029 先被正式处理） | **未完成研究方案**：留 G-203b，独立 ADR + 独立 Wave，不进入本轮实施步骤 |

---

## 7. 最小方案能否关闭 G-203：逐子句核对与记账规则（§4.4 第 5 项）

原文（`docs/reviews/2026-09-24-saas-align-module-packaging.md:316-320`）拆成 5 个子句：

| # | 原关闭条件（逐字子句） | 方案 A 的判据 | 可关闭轮次 |
|---|---|---|---|
| C1 | "pilot 模块的迁移落 `<namespace>` 子目录" | `migrations/modules/identity-management/`、`migrations/modules/policy/` 存在且被门禁与工件引用；`migrations/*.sql` 冻结 | Wave 5B-1 |
| C2 | "+ per-namespace `MANIFEST.sha256`" | 每目录一份，`Assert-MigrationManifest` 逐个调用；runner 运行时二次校验 | Wave 5B-1 |
| C3 | "`MigrationRevision.set_digest` 钉定" | 工件 `release.json.migration_revision.set_digest` 覆盖 namespace 文件 + 声明的 baseline 依赖（§5.7） | Wave 5B-1 |
| C4 | "平面序列不重写（历史保持）" | `001..019` 与 `migrations/MANIFEST.sha256` 字节不变；V4 用前后 dump 断言 | Wave 5B-1（结构性满足） |
| C5 | "必需测试：per-namespace sha 校验 + release identity 钉定测试" | §9.3 V11/V12 + §8 测试清单 | Wave 5B-1 |
| 影响句 | "多模块共存时无法证明单模块迁移可独立演进/校验" | namespace revision 的**安装/升级/校验**独立成立；但**既有 13 张表**仍由共享 revision 19 创建（§3.5 硬事实），且空库单模块安装无人提供（§6.6） | **部分** |

判定：**方案 A 满足 C1..C5 的字面要求，但不满足影响句中"单模块可独立安装"的强读法。**

### 7.1 判据拆分提案（**Proposed，不自采纳**）与四个子项记账

| 记账子项 | 内容 | 状态词汇 | 判据 |
|---|---|---|---|
| **G-203a-i 目录隔离** | namespace 目录 + per-namespace `MANIFEST.sha256` + `catalog.json` + 工件钉定 + 静态门禁 | Proposed 子项（**不标 Accepted**） | §11 S1 全绿 **＋ S4 工件钉定半边（V12：`namespace_isolation` 双维度评估、`host_baseline_dependency_is_pinned_in_set_digest` 与 HC-3 回归）**——工件钉定不在 S1 范围内（§11.6 明列 S1 不升 release spec），单靠 S1 不得关闭本子项（Revision 1 复审更正） |
| **G-203a-ii ledger 隔离** | 平台 ledger + 单一 writer + 锁/恢复语义 + fail-closed 判据 + 真实 PG 行为证据 | Proposed 子项 | §8 S2/S3 + §9.3 V1–V3、V6–V9 |
| **G-203b-i 既有库升级（物理 schema 搬迁）** | §6.4 序列在授权库上完成 + §6.5 全部 OPEN 关闭 | Proposed 子项，**未设计完成** | 独立 ADR + 运维授权后 |
| **G-203b-ii 独立冷安装（空库单模块）** | §6.6 目标 I：模块自有 init catalog + 空库安装证据 | Proposed 子项，**未设计** | 独立后续设计 + 授权库证据 |

**分母规则（审阅 §3.3 第 5 条，硬性）**：

1. 上述四个子项是**记账粒度**的细化，**不新增也不降低**任何原条目的分母：**G-203 原文条目保持 OPEN 且整体判定不变**，G-202、A-12 的判定面与分母同样不变。
2. 子项状态一律 **Proposed / OPEN**，除非独立审阅与计划 owner 明确裁决；本设计不自判任何子项 Accepted。
3. 不得把"目录存在""manifest 文件为空""门禁通过"记为**独立安装成功**；目录为空只可记为"预留"（§6.6、§11.2）。
4. 整体 G-203 关闭仍需原条件 + 强读法同时满足；在此之前 `independent_install` 评估恒为 `BLOCKED`，**R2 继续 BLOCKED**。
5. 不允许的写法：以方案 A 宣称"G-203 已关闭"或"identity 已可独立安装"；以拆分子项宣称"原 G-203 的未满足部分已被更小的等价条件替代"。

---

## 8. 逐文件实施清单与提交切片（§4.4 第 9 项）

> 每个切片 = 一个本地提交，切片内测试先红后绿。**各切片的准入与停止条件互相独立**（Revision 1 按审阅 §3.3 第 7 条分开），且除 S5 外全部离线可跑。SP-1/SP-2/SP-5/SP-6/SP-7 **不混入**（评审 `:96`）。

| 切片 | 唯一职责 | 准入条件（开始前提） | 停止条件（本轮不做） |
|---|---|---|---|
| **S1** | 离线 catalog 验证 + 静态门禁（**零运行时行为变更**） | 独立审阅放行 Revision 1 设计包 + ADR-0029 的正式处理结果记录在案 | 不创建平台表、不接 CLI `up`、不改启动迁移、不升 release spec（详见 §11.6） |
| **S2** | runner 行为：namespace catalog、bootstrap 与 namespace 锁、ledger 读写、fail-closed 判据 | S1 通过；`runtime-migration` API 形状评审通过；§5.9 被接受 | 不执行生产库；`#[ignore]` PG 用例仅在授权库上跑 |
| **S3** | CLI 参数与 readiness / 启动迁移收敛（D-2） | S2 通过；D-2 有明确决策 | 不在未授权库上跑 `up`；不改 compose/runbook 之外的部署面 |
| **S4** | 发布工件 schema 与历史兼容（HC-1..HC-5） | S1 通过（catalog 存在）；历史候选回归清单确认 | 不覆盖已归档工件；不扩 `identity.v1` 字段 |
| **S5** | 真实 PostgreSQL 验收轮 | **显式数据库授权** + §9.1 范围确认 | 未授权即 BLOCKED，不得用 SQLite / 离线证据替代 |
| **S6** | 文档同步 | 上述任一切片合入 | 不宣布任何 gap 关闭 |

**S1** 的完整规格（文件清单、字段、红绿反例、命令、停止条件）见 **§11**，本轮不实施。

**S2 namespace catalog 与 ledger 内核**

| 文件 / 符号 | 变更 | 测试 |
|---|---|---|
| `crates/runtime-migration/build.rs`（新） | 遍历 `migrations/modules/*/`，生成嵌入表 + sha256 | `catalog_entries_are_ordered_and_unique_per_namespace`、`manifest_covers_every_embedded_file`、`namespace_names_match_catalog_json` |
| `crates/runtime-migration/src/lib.rs` | 新增 `NamespaceCatalog`（含**测试可构造**入口，§11.5）、`namespaces()`、`enum NamespaceError`（§5.9.6）、`bootstrap_ledger()`、`apply_namespace()`、`verify_namespace()`；`MIGRATOR` / `classify` 保持不动 | 离线：`gap_in_revisions_is_rejected`、`unknown_namespace_is_rejected`、`lock_keys_are_pairwise_distinct` |
| `crates/runtime-migration/tests/namespace_ledger.rs`（新，`#[ignore = "requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored)"]`） | PG 行为 | `empty_ledger_records_every_revision`、`rerun_is_a_no_op`、`checksum_drift_fails_closed`、`orphan_row_fails_closed`、`revision_gap_fails_closed`、`concurrent_runners_apply_once`、`failed_revision_leaves_no_row`（fail-after 注入仅测试可用，生产 fail-closed，AGENTS §15 PLAN-0004 规则） |
| `crates/runtime-migration/tests/ledger_bootstrap_concurrency.rs`（新，`#[ignore]`） | §9.3 V8a/V8b/V8c 三条并发与崩溃恢复 | 见 §9.3 与 §11.5 fixture 规范 |

**S3 CLI 与 readiness**

| 文件 / 符号 | 变更 |
|---|---|
| `apps/migration/src/main.rs` | `parse_arguments`（`:63-73`）支持 `--namespace`（可重复）与 `verify`；`up` 语义 = baseline + 全部 embed namespace；`run_postgres_status`（`:98-144`）按 namespace 输出并把落后改为非 0 退出；新增与 `:247-287` 同风格的 fail-closed 谓词。`--namespace` / `verify` 在合入前**不存在**，任何文档与命令不得当作已有 CLI 使用（§9.3 列标"现在能执行？"） |
| `apps/migration/tests/namespace_selection.rs`（新） | ignored PG 套件：`identity_only_selection_writes_only_identity_rows`、`unknown_namespace_is_refused`、`baseline_below_required_head_is_refused`。仅操作 §9.1 显式授权库，且缺变量即 fail（不得回落到硬编码 URL，§9.1.3） |
| `apps/business-api/src/state.rs:233-254` | readiness 追加每 namespace `verify_namespace` 结果；不一致 → `NotReady`（只读，不迁移，§5.9.4） |
| （D-2 通过时）`apps/business-api/src/composition.rs:126`、`apps/governance-worker/src/main.rs:50` | 移除启动 `MIGRATOR.run`，改只读 verify；复核 `deploy/demo/docker-compose.yml:48-54,90,113,135` 已先跑 migration 单元 |

**S4 发布工件绑定**（受 §5.7 HC-1..HC-5 约束）

| 文件 / 符号 | 变更 |
|---|---|
| `tools/module-release-tool/src/spec.rs` | `ReleaseSpec.requires_host_baseline`（结构 `:142-184`）+ `RELEASE_SPEC_SCHEMA_VERSION` → `…release-spec.v2`（`lib.rs:63`）**并引入 §5.7 HC-1 的版本分派表**；`validate_against`（`:214-273`）按版本分派校验，未知版本拒绝（HC-2） |
| `tools/module-release-tool/specs/identity.json` / `policy.json` | postgres `migration_sets[]` 改指 `migrations/modules/<ns>`（`module_owned: true`、`ledger_table: platform_module_migrations`）+ `requires_host_baseline: 19`；SQLite set 原样保留；**不修改**已归档候选所在提交的 spec |
| `tools/module-release-tool/src/migrations.rs` | `MIGRATION_MAPPING_SCHEMA_VERSION` → `…migration-mapping.v2`（`:34`）且 reader 仍可读 v1（HC-4）；`BackendAssessment`（`:65-74`）拆 `namespace_isolation` / `independent_install`（改写 `:307-338`）；新增 host baseline context 记账常量 + 测试 `host_baseline_accounting_matches_tables_created_by_baseline_catalog`（从 `019` 文本对账，使 organization 显式记账，§3.6） |
| `tools/module-release-tool/src/packaging.rs` | `:1583` 的 schema 常量比较改版本分派（HC-1/HC-2），未知 `schema_version` 拒绝 |
| `tools/module-release-tool/tests/*` | `namespace_catalog_is_claimed_in_full`、`shared_baseline_claim_still_refuses_module_owned`（保留现 `pilot_candidates.rs:306,329` 语义）、`host_baseline_dependency_is_pinned_in_set_digest`、`independent_install_stays_blocked`、`package_and_contract_digest_unchanged_by_namespace_split`、`pinned_wave5a_candidate_still_verifies_after_schema_v2`（HC-3）、`unknown_schema_version_is_refused`（HC-2） |

**S5 真实 PostgreSQL 验收轮（需授权）**：执行 §9.3，产出 `docs/reviews/2026-10-XX-wave5b-gates/`。

**S6 文档同步**：`ARCHITECTURE_STATUS.md`、`docs/README.md`、`PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`（迁移单元与 ledger 段落，AGENTS §14 要求同变更更新）、`ADR-0029` 状态推进、准入矩阵 G-203 子项行。

**明确排除**：SQLite catalog 目录化/重命名（SP-4 后半）、`search_path` 与角色拆分（D-5）、`platform_module_migrations` 之外的新表、任何 `DROP`、`SAAS_MODULE_STANDARD` 文本修订（SP-7）、SP-1/SP-2/SP-5/SP-6/SP-7、**§6.4 的任何搬迁步骤（方案 B，未完成设计）**、**任何单模块冷安装实现（§6.6 目标 I）**。

---

## 9. 真实验收矩阵与命令（§4.4 第 10 项）

### 9.1 fixture、数据库授权范围与环境作用域

#### 9.1.1 授权范围

- 只允许在**运维显式授权**的本地 PostgreSQL 服务上创建专用空库 `plan0014_wave5b_<runid>`（沿用 Wave 3 `plan0014_wave3_r2`、Wave 4 `plan0014_wave4` 惯例），只允许 DROP `plan0014_wave5b_%`。
- 禁止触碰 `contract_management_*`；禁止猜测凭证（`config/default.toml:12-14` 只是开发默认值，不作为连接依据）。legacy `TEST_POSTGRESQL_URL` / `TEST_POSTGRESQL_RESTORED_URL` **在今天没有任何代码读取**（仅出现在历史文档散文中），因此它们不是"可复用的现成通道"，只是命名残留；本设计仍然显式禁止把它们指向旧目标。
- CI 的 PostgreSQL 面使用服务库 `business_platform`（`.github/workflows/ci.yml:169-181` 的 `POSTGRES_DB`、`:199` 的 `DATABASE_URL`），并在 `:262` 以 `--include-ignored` 跑 `migration_test`——即 **CI 本身就会在该服务器上创建 `plan2_<uuid>` sibling 库**。这不构成本轮授权：本地 Wave 5B lane 仍需单独授权，且不得默认指向 CI 同名的库。
- 未取得授权 ⇒ 本节全部 **BLOCKED**，不得用 SQLite / 离线证据替代。**本轮（Revision 1）无任何数据库授权，§9.3 一条命令都未执行。**

#### 9.1.2 逐 suite 数据库范围盘点（Revision 1 新增，B3；取证 `5c8355d`）

计数口径：一个 `#[ignore]` 属性 = 一条 ignored 测试；"目标库"指该测试**实际**连接/创建/删除的库，不是命令行的 URL。下表为设计相关的 PostgreSQL 套件；分类事实另见 `docs/reviews/2026-09-30-wave5a-repair2-gates/ignored-tests-classified.md:11-17`（38 PG / 3 PG+对象存储 / 28 S3-MinIO / 1 内网模型 provider）。

| Suite | 读哪个变量 | 缺变量时 | 实际数据库范围 | 清理 | 能否算"只操作给定库" |
|---|---|---|---|---|---|
| `apps/migration/tests/migration_test.rs::apply_migrations_from_empty`（`:132-155`） | `DATABASE_URL` | **静默回落到硬编码 `…localhost:5432/enterprise_platform`**（`:136-138`） | 在**给定库上直接** `MIGRATOR.run`，不建 sibling | **不清理**，给定库被永久改动 | 是，但会改动给定库本体 |
| 同文件 `migrations_are_idempotent`（`:157-177`） | `DATABASE_URL` | 同上回落（`:161-163`） | 同上，跑两遍 | 不清理 | 是，同上 |
| 同文件 `document_integrity_upgrade_preserves_legal_legacy_rows_…`（`:179-225`） | `DATABASE_URL`（派生） | 回落基址为 `…/business_platform`（`:12-14`） | **创建 sibling 库 `plan2_<uuid v7>`**（`:11-19`、`:21-30`） | `pool.close()` + `drop_database`（`:221-224`）**仅在成功路径** | **否**：会新建并删除给定库之外的库 |
| 同文件 `document_integrity_upgrade_fails_closed_for_invalid_legacy_rows`（`:227-253`） | 同上 | 同上 | 同上 `plan2_<uuid>` | `:249-252`，仅成功路径 | **否** |
| `crates/identity-postgres/tests/contract.rs`（`:19-26`） | `DATABASE_URL`（`.expect` 缺即失败，`:21-22`） | fail | 在给定库跑 `MIGRATOR.run` + 全套 IAM 契约，**要求 fresh 或已清 identity 表**（文件头 `:4-9`） | 不清理 | 是，但**要求专用空库**，不可与别的套件共用同一库 |
| `crates/identity-postgres/tests/perf_resolve.rs:60`、`crates/policy-postgres/tests/perf_authorize.rs:91` | `DATABASE_URL`（`.expect`） | fail | 给定库 | 不清理 | 是 |
| `crates/policy-postgres/tests/contract.rs:21`、`organization-postgres/tests/contract.rs:20`、`document-postgres/tests/{query_contracts.rs:8,tenant_fence_pg.rs:45}`、`apps/business-api/tests/documents_postgres.rs:26`、`e2e_admin_bootstrap_postgres.rs:20,27`（经 `tests/harness/mod.rs:266`）、`governance-worker/tests/postgres_governance.rs:20`（不自跑迁移，假定库已 up） | `DATABASE_URL` | **`.expect(...)` fail closed** | 给定库 | 仅行级清理，且只在成功路径（`query_contracts.rs:41` 的 delete 位于 `:39` 的断言**之后**，失败即跳过） | 是，但会永久改动给定库的对象与行 |
| `crates/messaging/tests/inbox_idempotency.rs:10-11`、`crates/document-processing-postgres/tests/{contracts.rs:37,230,323,cancel_factuality_pg.rs:181,211,258,306,353}`、`apps/business-worker/tests/postgres_minio_processing.rs:44-45` | `DATABASE_URL` | **回落到硬编码 `…/business_platform`**（`postgres_minio_processing` 还需 `MINIO_*`） | 给定库 | 行级清理，成功路径 | 是，但**回落不是 fail closed**（§9.1.3） |
| `crates/messaging/tests/outbox_reliability.rs:14-15`、`crates/audit-postgres/tests/chain_pg.rs:23-24` | `DATABASE_URL` | **回落到 `…/business_platform_test`**（与 CI 的 `business_platform` 都不同名） | 给定库；`inbox_idempotency.rs:18-22` 记录 `CREATE TABLE IF NOT EXISTS` 并发不安全，故进程内只建一次 | 行级清理，成功路径 | 是，且这两条还各自私自 embed 第二份 catalog（P-4） |
| `apps/business-api/tests/plan_0008_postgres_minio.rs:215,316`（Revision 1 复审补入） | `DATABASE_URL`（`:71-72` `.expect`，缺失即 fail）+ `MINIO_*`（`:87-90` 仅对象存储侧有硬编码默认，DB 侧无回落） | DB 侧 fail closed | 在**给定库上直接** `MIGRATOR.run`（`:78-81`）+ MinIO 桶对象读写 | 仅成功路径删上传对象（`:312`）；库内行与 `MIGRATOR.run` 的改动不清理 | 是（不新建库），但属 PG + 对象存储面，纯 PG 的 Wave 5B lane 不含它 |
| `deploy/operations/drill-backup-restore.sh`（运维演练，**不是 cargo 测试**） | `DATABASE_URL`（`:167` 缺失即 fail） | fail closed，并拒绝指向 `postgres`（`:196`）或 `*_drill_*`（`:197`） | **创建 sibling 库 `<live_db>_drill_<UTC stamp>_<rand>`**（`:199-200`，DDL 走 `…/postgres` 维护库，`:187-191`、`:293-294`），另建 S3 桶 `<bucket>-restore-<stamp>-<rand>`（`:204`） | `trap cleanup EXIT`（`:234`）→ `DROP DATABASE IF EXISTS`（`:214`），**失败路径也执行**；drop 失败仅记日志（`:215`） | **否**：会新建/删除给定库之外的对象。这是仓内唯一"失败也清理"的 sibling 先例，可作为 §9.1.3 清理语义的参照 |

盘点结论（必须如实写进未来 gate 记录）：

1. **cargo 测试侧**只有一个地方创建给定 URL 之外的库：`apps/migration/tests/migration_test.rs`（sibling `plan2_<uuid-v7>`，两条测试各建一个）。其余 PG 套件都只操作**给定库本体**。另有两处**非测试**代码会创建 sibling 库：CLI 的 `main.rs:77-79`（创建的是 URL 自己指向的库，不算 sibling）与 `deploy/operations/drill-backup-restore.sh`（`<live_db>_drill_…`，属运维演练面，不在 Wave 5B lane）。其余 `create_database` / `drop_database` 命中（10 处）全是 SQLite 临时文件，不在本清单。
2. 因此"两条命令都指向同一个目标库"**不能**当作"测试只操作给定库"的证明：`plan2_*` 的建/删落在 §9.1.1 授权范围之外；`migration_test.rs:11-19` 更是**丢弃给定库名**，即使 URL 指向授权库，实际新建的也是 `plan2_*`。
3. 更普遍的问题是**回落**：7 个文件在 `DATABASE_URL` 缺失时静默连到硬编码库（与 §9.1.3"上表 7 个文件"同一集合；Revision 1 复审更正，原记 6），且名字互不相同（`migration_test.rs` 内部就同时用 `business_platform`（`:13`）与 `enterprise_platform`（`:137`、`:162`）；`outbox_reliability.rs:15` 与 `chain_pg.rs:24` 用 `business_platform_test`）。这些套件在未设变量时"看起来通过"，实际改动的是另一个库。
4. **清理普遍只在成功路径**：`migration_test.rs:221-224` / `:249-252` 的 `drop_database` 是函数最后两条语句，前面有十余处可 panic 的 `.expect`/`assert!`，任何一次失败都永久泄漏一个 `plan2_*` 库；行级清理同理（`query_contracts.rs:41` 在断言之后）。仓内**没有**任何枚举或回收 `plan2_%` 残留的步骤。唯一具备失败路径清理的是演练脚本的 `trap cleanup EXIT`。
5. 结论性约束：**新的 namespace 测试（S2/S3）不得创建 sibling 库**，只允许在显式授权的 `plan0014_wave5b_%` 上运行；`migration_test.rs` 的两条 sibling 套件如确需运行，必须先在该轮授权清单里**逐条列出允许新建的库名模式与删除权限**，否则不运行。
6. 未来清理策略：新套件采用 try/finally 语义（成功与 panic 路径都执行清理，参照 `drill-backup-restore.sh:210-234` 的 trap 模型），并在结束时断言 `pg_database` 中不再有本轮 `plan0014_wave5b_%` 残留；sibling 套件维持现状但被排除在 Wave 5B lane 之外。

#### 9.1.3 fail-closed 前置（Revision 1 新增）

- 新套件**禁止**在变量缺失时回落到硬编码 URL：必须 `.expect("…")` 或以 `EnvVarMissing` 失败退出（对齐 `identity-postgres/tests/contract.rs:21-22`、`governance-worker/tests/postgres_governance.rs:20-21` 的做法，反对 `migration_test.rs:12-14`、`outbox_reliability.rs:14-15`、`chain_pg.rs:23-24` 的回落）。
- 现有带回落的套件（上表 7 个文件：`migration_test.rs`、`inbox_idempotency.rs`、`outbox_reliability.rs`、
  `chain_pg.rs`、`postgres_minio_processing.rs`、document-processing 的 `contracts.rs` 与
  `cancel_factuality_pg.rs`）在 Wave 5B lane 中**不启用**；启用它们等价于连到"某个默认开发库"，不可接受。若将来要收敛它们，属独立小改动（把 `unwrap_or_else` 改 `expect`），与本设计解耦，且会改变 ignored 集合的语义而不改变其数量——如实施，须按 §9.4 对账说明。
- 新套件的清理必须覆盖失败路径（第 6 条），并把"本轮结束后无 `plan0014_wave5b_%` 残留"作为验收断言的一部分。

### 9.2 两条互斥的环境作用域（Revision 1 重写，B3）

`apps/migration/src/config.rs:17-21`：`MIGRATION__DATABASE__URL` 与 `DATABASE_URL` **同时存在即 `ConfigLoadError`，值相同也拒绝**。Revision 0 的前置同时设置两者，会让 CLI 在连接前就失败。Revision 1 拆成两条**互斥**的 lane，各自 try/finally 恢复"存在/不存在及原值"，日志不落明文 URL。

```powershell
# ───────────────────────── LANE 1：CLI（apps/migration）─────────────────────────
# 只设 MIGRATION__DATABASE__URL；必须清除 legacy DATABASE_URL（config.rs:17-21 互斥）。
# $cliUrl 由操作员从授权来源注入；本脚本与日志中不出现真实 URL。
$cliUrl = Read-Wave5bSecret -Scope Cli          # 操作员自备，绝不硬编码
$prevProc  = Get-Item Env:MIGRATION__DATABASE__URL -ErrorAction SilentlyContinue
$prevLegacy = Get-Item Env:DATABASE_URL            -ErrorAction SilentlyContinue
try {
    Set-Item -Path Env:MIGRATION__DATABASE__URL -Value $cliUrl
    Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue

    # 以下三条命令在 S2/S3 合入前不存在，属于未来命令（见 §9.3「现在能执行？」列）
    cargo run -p migration -- --backend postgres --namespace identity-management up   # 未来（V1）
    cargo run -p migration -- --backend postgres up                                   # 现在可执行，但只跑 baseline
    cargo run -p migration -- --backend postgres verify                               # 未来（S3）
}
finally {
    # 精确恢复"存在/不存在 + 原值"，不假设调用前环境
    if ($null -ne $prevProc)   { Set-Item Env:MIGRATION__DATABASE__URL $prevProc.Value }
    else                       { Remove-Item Env:MIGRATION__DATABASE__URL -ErrorAction SilentlyContinue }
    if ($null -ne $prevLegacy) { Set-Item Env:DATABASE_URL $prevLegacy.Value }
    else                       { Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue }
}
```

```powershell
# ───────────────────────── LANE 2：#[ignore] 测试套件 ─────────────────────────
# 只设 DATABASE_URL；必须清除 MIGRATION__DATABASE__URL，否则同一条 cargo 命令下
# 任何读取 MigrationConfig 的进程都会 ConfigLoadError（config.rs:17-21）。
$testUrl = Read-Wave5bSecret -Scope Test         # 显式授权的 plan0014_wave5b_<runid>
$prevProc  = Get-Item Env:MIGRATION__DATABASE__URL -ErrorAction SilentlyContinue
$prevLegacy = Get-Item Env:DATABASE_URL            -ErrorAction SilentlyContinue
try {
    Set-Item -Path Env:DATABASE_URL -Value $testUrl
    Remove-Item Env:MIGRATION__DATABASE__URL -ErrorAction SilentlyContinue

    cargo test -p runtime-migration --all-features --target-dir <隔离目录> `
        -- --include-ignored --test-threads=1 2>&1 |
        ForEach-Object { "$_" -replace '://[^@/\s]+@', '://***:***@' } |
        Tee-Object -FilePath <证据目录>\v8-concurrency.log
}
finally {
    if ($null -ne $prevProc)   { Set-Item Env:MIGRATION__DATABASE__URL $prevProc.Value }
    else                       { Remove-Item Env:MIGRATION__DATABASE__URL -ErrorAction SilentlyContinue }
    if ($null -ne $prevLegacy) { Set-Item Env:DATABASE_URL $prevLegacy.Value }
    else                       { Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue }
}
```

纪律：两条 lane **不得嵌套或同时开启**；`cargo run -p migration` 只出现在 LANE 1，`cargo test --include-ignored` 只出现在 LANE 2；归档前必须过 URL 脱敏过滤（`://user:pass@` → `://***:***@`，与 `crates/runtime-config/src/secret_url.rs:80-101` 的 redaction 同口径）；本轮两条示例都**未执行**（无授权）。

### 9.3 矩阵（Revision 1 重写；V8 拆成三条，V13/V14 更正）

| # | 验收 | 命令（标注是否当前存在） | 期望 | 现在能执行？ | 诚实依赖 |
|---|---|---|---|---|---|
| V1 | 单 namespace 子集安装 | LANE 1：`cargo run -p migration -- --backend postgres --namespace identity-management up` | exit 0；ledger 只有 `identity-management` 行；`_sqlx_migrations` 1..19 | **否**：`--namespace` 未实现（`main.rs:63-73`），需 S2/S3 | 依赖 baseline revision 19，须在工件中如实声明（§6.6） |
| V2 | policy 子集安装 | LANE 1：同上 `--namespace policy` | 同上（policy 段落只引用自身表） | 否（需 S2/S3） | 同 V1 |
| V3 | 共存 + 幂等重跑 | LANE 1：`cargo run -p migration -- --backend postgres up`（连续两次） | 两次 exit 0；第二次零写入；两 namespace 行互不覆盖；第二次不再取 L-B 之外的 DDL | 否（需 S2/S3） | 需 `depends_on` 拓扑序与 §5.9.2 锁序 |
| V4 | 既有宿主升级不动历史 | base SHA 建库 → 候选 SHA `up` → 比对 `_sqlx_migrations` dump | version / description / checksum 集合完全相同 | 否（需 S3 + 授权） | 需两个 SHA 的 worktree |
| V5 | 空库全量安装 | LANE 1：空库 `up` | 全部 baseline 表 + namespace 表存在 | **部分**：今天 `up` 已能装 baseline（`apps/migration/tests/migration_test.rs:132-155` 为 ignored 证明，但它改动给定库） | **只证明 baseline 安装**，不证明 namespace，更不证明单模块冷安装（§6.6） |
| V6 | checksum 漂移 fail-closed | LANE 1：`UPDATE platform_module_migrations SET checksum = repeat('0',64) WHERE namespace='policy'` 后 `up` | exit 1，报 `checksum-mismatch:<ns>:<rev>`，不改对象 | 否（需 S2） | ledger 表须先存在 |
| V7 | 孤儿 revision / 缺口 fail-closed | LANE 1：向 ledger 插 `(policy,900,…)` / 跳号插入 | exit 1，`orphan-revision` / `revision-gap` | 否（需 S2） | 用 fixture namespace（§11.5），不污染正式模块 |
| **V8a** | **同 namespace 双进程** | LANE 1：同一库上同时起两个 `--namespace policy up` 进程 | 两者 exit 0（或后者等待）；每 `(ns,revision)` **恰一行**；结束后 `pg_locks` 中 `locktype='advisory'` 为 0 行；无遗留临时对象 | 否（需 S2 + 授权） | 启动屏障：两进程连接后各插一行 gate 记录，轮询到两行齐备才进入取锁阶段（gate 表由 harness 建/删，runner 不建任何非 ledger 对象）。专用库 |
| **V8b** | **不同 namespace 首次并发 bootstrap（B1 的核心反例）** | 库刚完成 host baseline 1..19、**尚无** `platform_module_migrations`；同时起 `--namespace identity-management up` 与 `--namespace policy up` | 两者 exit 0；库里**只有一张** `platform_module_migrations`（`pg_class` 计数 = 1）；两条 namespace 行都存在；日志中无 `already exists` / `tuple concurrently updated` / `duplicate key value violates unique constraint "pg_type_typname_nsp_index"`；重复 20 轮结论一致 | 否（需 S2 + 授权） | 证明 L-B 公共锁的作用；单 namespace 的并发（V8a）**不足以**覆盖本条。专用空库 |
| **V8c** | **bootstrap 持锁进程被终止后另一进程恢复** | 进程 A 取到 L-B 后在 COMMIT 前被强制终止（测试钩子 `--test-hook-hold-bootstrap`，见依赖列）→ `taskkill /F`；随后进程 B 正常 `up` | A 被杀后 ledger 表要么不存在要么**结构完整**（不得有半成品/无列定义对象）；A 未留下任何 ledger 行；B 完成 bootstrap 并应用自己的 revision，exit 0；结束后 `pg_locks` advisory 计数 0 | 否（需 S2 + 授权） | 钩子必须是**非默认 cargo feature**（`test-hooks`）编译期隔离，且生产构建下环境变量出现该键即 fail-closed 退出（AGENTS §15 PLAN-0004 的测试钩子规则）。专用库 |
| V9 | 中断恢复（revision 事务） | fail-after 注入使某 revision 中途失败 → 重跑 | 第一次 exit 1 且无残留对象与 ledger 行；第二次成功且只补该 revision | 否（需 S2） | 注入仅测试可用（同上 feature 纪律） |
| V10 | 契约 / 租户 / 幂等复跑 | LANE 2：`cargo test -p identity-postgres --all-features --target-dir <隔离目录> -- --include-ignored --test-threads=1`；`-p policy-postgres`、`-p organization-postgres` 同 | 全绿，证据入 `wave5b-gates/`；identity 套件要求专用空库（§9.1.2） | 是（有授权即可） | 关闭的是 adapter 证据，不是 G-203b |
| V11 | per-namespace sha 门禁 | `pwsh scripts/check-architecture.ps1`；`pwsh scripts/test-plan0014-wave5b.ps1` | 全 PASS，含 baseline 冻结与新目录断言 | **否**（需 S1，脚本尚不存在） | — |
| V12 | release identity 钉定 | `cargo test -p module-release-tool --all-features` | mapping v2 全绿；postgres 拆双维度评估；**HC-3 历史候选回归同时绿** | 否（需 S4） | `independent_install` 必须仍 `BLOCKED` |
| V13 | Wave 5A 工件在新 HEAD 上仍可验证 | `pwsh scripts/plan0014-wave5a.ps1 -SkipOutsideConsumer`（现存在） | 两模块 verify 全 PASS，且钉定候选仍是 `c22939f0…`，四类摘要与准入矩阵 §2 逐字相同 | **是**（脚本已在仓） | **只证明 Wave 5A 的固定候选仍自洽**；S1/S4 合入后必须**另起一条命令**验证新 mapping（HC-3 的回归测试），不得说"V13 验证了新 mapping" |
| V14 | 工作区回归 | `cargo test --workspace --all-features --target-dir <隔离目录>` | 见 §9.4 的增量对账规则（**不要求**保持 864 / 70 / 191 不变） | 是 | 默认 target 目录本机 `os error 5`，须隔离 |

### 9.4 V14 与 ignored 的分母规则（Revision 1 新增，B3）

基线（`c22939f0`，`docs/reviews/2026-09-30-wave5a-repair2-gates/ignored-tests-classified.md:5,11-17`）：**864 passed / 0 failed / 70 ignored，191 个测试套件**；ignored 四类 = PostgreSQL 38 / PostgreSQL+对象存储 3 / 仅 S3-MinIO 28 / 内网模型 provider 1。计数口径与 `§9.1.2` 一致：**一个 `#[ignore]` 属性 = 一条 ignored 测试**（仓内 `#[ignore` 文本命中 73，其中 3 处是 `//!` / `///` 散文，余 70 条全部独占一行且都紧贴在 `#[test]` / `#[tokio::test]` 之下）。

**当前 70 条的逐文件分解（Revision 1 新增，作为未来逐项对账的分母；不是新的执行证据）**：

| 类别 | 文件（条数） | 小计 |
|---|---|---|
| 仅 PostgreSQL（38） | `apps/migration/tests/migration_test.rs` 4、`crates/messaging/tests/outbox_reliability.rs` 7、`crates/messaging/tests/inbox_idempotency.rs` 4、`crates/audit-postgres/tests/chain_pg.rs` 4、`crates/document-processing-postgres/tests/cancel_factuality_pg.rs` 5、`…/contracts.rs` 3、`crates/document-postgres/tests/query_contracts.rs` 1、`…/tenant_fence_pg.rs` 1、`crates/identity-postgres/tests/contract.rs` 1、`…/perf_resolve.rs` 1、`crates/organization-postgres/tests/contract.rs` 1、`crates/policy-postgres/tests/contract.rs` 1、`…/perf_authorize.rs` 1、`apps/governance-worker/tests/postgres_governance.rs` 1、`apps/business-api/tests/documents_postgres.rs` 1、`…/e2e_admin_bootstrap_postgres.rs` 2 | 38 |
| PostgreSQL + 对象存储（3） | `apps/business-api/tests/plan_0008_postgres_minio.rs` 2（`:215`、`:316`）、`apps/business-worker/tests/postgres_minio_processing.rs` 1（`:41`） | 3 |
| 仅 S3 / MinIO（28） | `crates/object-storage/tests/minio_contract.rs` 10、`s3_contract_lane.rs` 6、`s3_lane_cleanup.rs` 7、`s3_lane_fault_injection.rs` 5 | 28 |
| 内网模型 provider（1） | `apps/ai-worker/src/extractor.rs:556`（`AI_SMOKE_BASE_URL`） | 1 |

两个必须记住的事实：(1) 上述 PG 文件里的测试 **100% 都是 `#[ignore]`**，因此 `cargo test --workspace` 永不触库，PG 面证据只存在于 `--include-ignored`；(2) `crates/runtime-governance-postgres` 没有 tests 目录，其 PG 路径由 `postgres_governance.rs` 覆盖——新增 namespace 测试时不要误以为它已被覆盖。

V14 的期望写法（**不得写成"与基线一致"或"保持 864/191 不变"**）：

1. `passed = 864 + N_new`、`suites = 191 + M_new`、`ignored = 70 + K_new`，三个增量必须**逐条**对账到新增测试名与所属套件（照 `ignored-tests-classified.md:25-26` 的既有做法：Repair 1→2 的 +9 / +1 就是对账样例）。
2. **任何未预期的减少都算失败**：`passed` 少于 `864 + N_new`、或既有测试名消失、或类别漂移，一律判失败，不得用"总数差不多"通过。
3. **新增 PostgreSQL `#[ignore]` 会让分母增大，这是预期的、必须如实解释**（S2/S3 会新增 §9.3 V8a/V8b/V8c 等 ignored 用例），因此**禁止**要求"ignored 必须仍为 70"这类机械固定；但每条新增 ignored 必须写明所属套件、原因文本、四类归属与"本轮未执行"。
4. ignored 分类沿用既有规则（同时提 PostgreSQL 与 MinIO/S3 → PG+对象存储；只提 PostgreSQL → PostgreSQL；只提 MinIO/S3 → 对象存储；其余 → 外部模型 provider），并**逐条**输出清单与机读摘要，不接受只报合计。
5. 一条 ignored 都没跑就照实记 **NOT RUN**；不得因为"总数没变"而声称 PostgreSQL 面有进展。

### 9.5 修订后的关闭判据提案（连同 §7 一起送审）

`G-203a` 关闭 = V1、V2、V3、V4、V6、V7、V8a、V8b、V8c、V9、V11、V12 全绿（其中 V1–V4、V6–V9 需数据库授权；子项判据按 §7.1：**G-203a-i = V11 + V12 的工件钉定半边（S4）**，S1 单独不足以关闭，**G-203a-ii = V1–V4、V6–V9**，两者并集即本清单——Revision 1 复审更正，与 §7.1 对齐）；`G-203b` 保持 OPEN 直至 §6.5 全部 OPEN 关闭且 §6.4 序列在授权库完成；`G-203b-ii`（独立冷安装）另需 §6.6 目标 I 的独立设计与证据；`G-02` 只有 V5/V10 在授权库上真实执行后才可能收窄（ignored 分母按 §9.4 规则，不机械固定 70）；A-12 的 PostgreSQL 部分继续 `NOT RUN`。**四个子项的记账不改变 G-203 / G-202 / A-12 的原分母（§7.1）。**

---

## 10. 待决策项（本轮不擅自决定）

| # | 决策 | 选项 | 建议 |
|---|---|---|---|
| D-1 | 采用方案 A / B / A→B | — | **A 先行**，B 作为 G-203b 独立轮（且 §6.5 未闭合前不排期） |
| D-2 | API 与 governance-worker 启动自动迁移是否移除（P-1，违反 `PERSISTENCE…:96-98`） | ① 保持现状；② 改只读 verify；③ 完全移除 | **②**：既对齐 Baseline，又让"namespace ledger 只有一个 writer"成为可证事实；改动小、可回退 |
| D-3 | G-203 是否按 §7 拆成 a/b（并按 §7.1 四子项记账） | 拆 / 不拆 | **拆**；不拆则 G-203 只能保持 OPEN，5B 的隔离工作无处记账。拆分**不改变**原条目分母 |
| D-4 | 新 ledger 用单表 `(namespace,revision)` 还是每模块一张表 | 单表 / 多表 | **单表**：跨 namespace 状态一次可读、无动态表名、无 bootstrap 悖论；模块 SQL 不写 ledger 已保证隔离 |
| D-5 | 迁移角色 / 应用角色拆分（`SECURITY…:251-260`） | 本轮做 / 独立轮 | **独立轮**：牵动 runbook 与 compose，不与隔离设计捆绑；但它是 G-203b 与"DDL 权限已隔离"结论的前置（§5.6、OPEN-B2-3） |
| D-6 | ADR-0029 范围 | 只覆盖 catalog+ledger+runner / 也覆盖 schema 搬迁 | **只覆盖前者**：方案 B 含不可逆授权，应另立 ADR |
| D-7 | 锁实现形态（Revision 1 新增） | ① 会话级 `pg_advisory_lock`（Revision 0）；② 事务级 `pg_advisory_xact_lock` + 双锁键域（本 Revision 建议） | **②**：与仓内既有 8 处实现一致，释放由事务/连接生命周期保证，消除"持锁连接回池"与漏解锁两类风险 |

---

## 11. S1 规格：下一轮可编码包（Revision 1 新增；**本轮不实施**）

S1 只做**离线 catalog 验证 + 静态门禁**，零运行时行为变更。ADR-0029 仍是 Proposed：S1 通过**不等于** ADR 被接受，也不激活生产 catalog（§11.6）。

### 11.1 元数据字段与未知字段策略

- `catalog.json` 字段固定为 §5.2 的 6 项（`schema_version`、`namespace`、`owner`、`ledger`、`sql_schema`、`depends_on`），**不多不少**；解析使用 `deny_unknown_fields` 等价的严格模式，未知字段、缺失字段、`schema_version` 非当前常量 → 全部拒绝，错误消息点名具体字段/版本。
- `MANIFEST.sha256` 语法沿用 `<64 位小写 hex><两空格><NNN_名字.sql>`；不列自身、不自我哈希（与既有六个 catalog 一致）。
- **不改变** `identity.v1` / `MigrationRevision` / `ModuleReleaseIdentity` 的任何字段（§5.7）。
- `catalog.json` 的 `schema_version` 常量由 S1 引入，三方读取者（`check-architecture.ps1`、`tools/module-release-tool`、`runtime-migration/build.rs`）共用同一值并有共同的 schema 校验测试；S1 阶段 `build.rs` 尚未存在，因此 S1 只落两方 + 一份共享 fixture。

### 11.2 核心验证内容（每一条都必须可判定）

| # | 检查 | 判定 |
|---|---|---|
| 1 | 目录名 == `catalog.namespace` == `catalog.owner` == `BOUNDED_CONTEXT_MAP` 中的 context 名 | 不等 → FAIL |
| 2 | `sql_schema == "public"`；`ledger == "platform_module_migrations"` | 其他值 → FAIL（防止悄悄做 L3 / 换 ledger） |
| 3 | **路径逃逸与符号链接拒绝**：catalog 目录解析后的绝对路径必须仍在 `migrations/modules/` 之下；拒绝 `..`、绝对路径条目、指向仓库外的 symlink/junction（Windows 下含 reparse point）；拒绝大小写重复目录（`Policy` 与 `policy`） | 违反 → FAIL |
| 4 | **manifest ↔ SQL 双向集合相等**：目录内每个 `*.sql` 都在 MANIFEST 中且摘要一致；MANIFEST 中每个条目都在目录中存在（多余条目 = 幽灵文件，FAIL） | 单向子集不算通过 |
| 5 | **重复 revision 号**（`001_a.sql` + `001_b.sql`）→ FAIL | |
| 6 | **revision 缺口**（`001`、`003` 无 `002`）→ FAIL | |
| 7 | **`depends_on` 未知依赖 / 环**：引用不存在的 namespace 或 baseline 版本号非正、依赖图含环 → FAIL | |
| 8 | **checksum 重算**：按 `canonical.rs:77` 的 LF 折叠口径重算并与 MANIFEST、与 spec 的 `set_digest` 输入一致 | 漂移 → FAIL |
| 9 | **空目录语义**：`MANIFEST.sha256` 为 0 行的 catalog 只能记为 `RESERVED`；任何"安装成功 / 已隔离 / 可独立安装"的断言都不得由空 catalog 触发 | 打印必须区分 `RESERVED` 与 `INSTALLED` |
| 10 | **正式 SQL owner 白名单扫描（静态 fitness）**：模块 catalog 内 SQL 只能引用**该 owner 拥有**的表；owner 表集合 = （§3 归属表中归属该 owner 的 host baseline 表）∪（该 namespace **更早 revision** 自身创建的表） | 引用他人表 → FAIL；引用未声明的新表 → FAIL |
| 11 | 单一 embed 点扫描扩展：`sqlx::migrate!("../../migrations")` 只允许出现在 `crates/runtime-migration/src/lib.rs`；把现 `check-architecture.ps1:52` 的扫描范围从 `apps/migration/src` 扩到 `apps/**` 与 `crates/**`（SQLite 侧的 `sqlx::migrate!("./migrations")` 是各自 crate 自有 catalog，不在禁止范围），同时**修两处**既有违规：`crates/messaging/tests/outbox_reliability.rs:27` 与 `crates/audit-postgres/tests/chain_pg.rs:36` 改用 `runtime_migration::MIGRATOR`（修 P-4） |

**#10 的授权扩展机制（必须存在，不能永远只认 019 的 13 张旧表）**：新表通过**创建它的那条 revision 自身的声明头**进入 owner 集合——形如 `-- @bp-owned: <table_name>` 的首部注释，门禁解析后把该表加入该 owner 的已授权集合，供后续 revision 引用；声明但本文件未创建 → FAIL；声明的表名归属其他 owner → FAIL。这样"合法同 owner 新表"有明确、可审计的扩充路径，而不是硬编码 13 张旧表。

**#10 的诚实边界（不得夸大）**：这是对 SQL **文本**的静态 fitness function，用于阻止"顺手把别人的表写进自己的迁移"。它**不证明**数据库层的权限隔离：真正的隔离来自角色与 grant 边界（D-5 / OPEN-B2-3）。任何文档与日志都不得把这条门禁描述成"跨模块写入已被阻止"。

**S1 实施补充（2026-09-30 S1 复审更正；不新增判据方向，只把 §11.2 已列规则做到可判定）**：

- **a. 依赖语法与可得性**：`depends_on` 按上表语法精确解析；namespace 依赖的 revision 必须在同一 checkout 的目标 catalog 中存在（不足 → `requires revision N of '<name>' but that catalog … here`），否则"声明了依赖"仍可能在 S2 运行时才发现无法满足。
- **b. 重复依赖按目标名判重**：`policy@1` + `policy@2` 是两个不同 revision 号但同一目标；判重键是**目标名**，不是整串文本。
- **c. 路径包含用文件系统解析后的真实路径**：`migrations/modules`、每个 namespace 目录、freeze 的每个条目都先 `canonicalize` 再断言仍在根内，因此**祖先目录**被替换为 junction/symlink 时同样拒绝（只做字符串前缀比较会在祖先被换走时放行）；每次读目录后二次校验该路径仍是普通目录，压缩 read 与 walk 之间的 TOCTOU 窗口。
- **d. 无法归因的 SQL 结构点名拒绝**：整表 `RENAME TO`、`CREATE TABLE … (LIKE …)`、`CREATE TABLE … PARTITION OF …`、dollar-quoted 块、引号/注释不闭合，全部给出具名失败（"outside the supported SQL subset"），而不是悄悄跳过——跳过正是外来表引用溜过的方式。列改名 `RENAME COLUMN … TO …` 不受影响。
- **e. host baseline 表禁止 schema 级销毁**：`DROP TABLE` / `TRUNCATE` 命中 §3.1 归属表中的 13 张 host 表即拒绝，**即使声明者就是其 owner**（否则 owner 可以删掉冻结 baseline 里的表）；数据语句（`INSERT`/`UPDATE`/`DELETE`）仍按 #10 的 owner 规则允许。
- **f. 每条读取失败都带 check 名**：文件/目录读取、摘要比较、embed 扫描的错误消息一律形如 `refused by the [<check>] check: …`，使负例断言能钉住"是哪条规则拒绝的"，而不是只看到一句失败。

### 11.3 历史保护（文件集合 + 固定历史内容/摘要）

Revision 0 只断言"恰好 19 个文件名"，这**不足以**阻止同时修改 SQL 与 `MANIFEST.sha256`（两者可一起改而门禁仍绿）。S1 必须同时覆盖：

1. **文件集合**：`migrations/*.sql` 的文件名集合等于固定 19 项清单，且无额外子目录混入 baseline。
2. **固定历史内容**：逐个 `git rev-parse <PINNED_SHA>:migrations/<file>` 取 **git blob** 的 SHA-256，与门禁内固化的 19 项摘要表逐项比对；`migrations/MANIFEST.sha256` 自身同样按 blob 比对。`PINNED_SHA` 在 S1 中固定为本设计的 Revision 1 起点 `5c8355d…`（此后仅在 baseline 合法演进时经独立裁决更新），并在日志中打印所钉的 SHA。
3. **交叉一致**：`MANIFEST.sha256` 声明的摘要必须与 git blob 重算一致（防"改 SQL 不改 MANIFEST"与"两者同改但 blob 变"两种情形）。
4. **Windows EOL 口径（必须写明）**：对比基准是 **git blob 的字节**（仓库内规范形态），不是工作树字节——`core.autocrlf` / `.gitattributes` 可能让工作树呈现 CRLF，直接哈希工作树会因开发者配置不同而假红或假绿。因此门禁走 `git show <sha>:<path>` 取字节；需要哈希工作树（例如本地快速模式）时，必须先按 `canonical.rs:77 fold_line_endings` 做 LF 折叠，且日志必须标明用的是 blob 模式还是工作树模式。

5. **S1 复审更正（C1：`git` 对象 id 不是内容摘要；历史容错不得外溢）**：

   - 本条第 2 点原文写的 `git rev-parse <PINNED_SHA>:<path>` 返回的是 **40 位 git 对象 id**，它是 Git 的对象寻址键，**不是**内容摘要。S1 的比对对象是 `git cat-file blob <sha>:<path>` 的**字节**再做 SHA-256（64 位小写 hex）。反例可复现：`001_initial.sql` 在 `5c8355d` 的 blob OID 是 40 hex，`validate_sha256_hex` 直接拒绝——把 OID 记进摘要表会当场得到一个"看起来像 hash 但不是 SHA-256"的值。测试 `a_git_object_id_is_never_a_content_digest` 钉住这条。
   - **历史 manifest 的三形容错保留**：`migrations/MANIFEST.sha256` 19 条中**恰有 7 条**只与 CRLF 形摘要相符（`002`、`003`、`004`、`005`、`006`、`007`、`008`；`001`/`018` 在盘上是 LF，其余 11 条与 LF 形一致）。S1 **不改写**任何历史文件与历史行；容错之所以安全，是因为每条同时被该文件的 **blob 摘要**钉住——改内容必然改 blob。
   - **容错不得外溢**：新建 namespace catalog 的 `MANIFEST.sha256` 只接受**规范 LF 摘要**（`fold_line_endings` 后 SHA-256），且 manifest 文件本身不得含 CR；这条规则独立实现，不复用历史 manifest 的旧断言（旧断言继续只管 `migrations/MANIFEST.sha256`）。
   - **四方比对**：钉住的 blob ↔ `HEAD` blob ↔ 索引 blob ↔ 工作树（LF 折叠后）。只比 HEAD 会漏掉"只改了索引"和"只改了工作树"两种漂移。
   - **checkout 端字节稳定**：S1 新增 path 作用域的 `.gitattributes`（仅 `tools/module-release-tool/tests/fixtures/wave5b/**` 与 `migrations/modules/**`，`text eol=lf`），使摘要 fixture 不随 `core.autocrlf` 漂移；**历史 migrations 故意不列入**——历史文件不改写，其 CRLF 兼容由 blob 摘要承担。

### 11.4 准确文件清单与命令

| 文件 | 变更 | 红→绿 |
|---|---|---|
| `migrations/modules/identity-management/catalog.json`、`…/policy/catalog.json` | 新增（§11.1），初始无 SQL | 绿：字段齐全、`schema_version` 正确 |
| `migrations/modules/identity-management/MANIFEST.sha256`、`…/policy/MANIFEST.sha256` | 新增（当前各 0 行） | 绿：0 行合法，状态记 `RESERVED` |
| `scripts/check-architecture.ps1` | §11.2 的 11 条 + §11.3 的历史保护 + `Assert-MigrationManifest` 接两个新目录（`:678-683` 续） | 每条一个负例 |
| `scripts/test-plan0014-wave5b.ps1`（新） | 上述每条的正/负例驱动，负例用临时目录副本，不改动仓库真实 catalog | exit 0 全 OK；任一负例被"顺手放宽"即 exit 1 |
| `crates/messaging/tests/outbox_reliability.rs:27`、`crates/audit-postgres/tests/chain_pg.rs:36` | 两处都改用 `runtime_migration::MIGRATOR`（修 P-4 的两处，Revision 0 只列了一处） | 红：新扫描命中第二、三处 `sqlx::migrate!` → 绿：改完 |

命令（三条现在都可执行，S1 合入前第 1、2 条会因新目录缺失而 FAIL，即红阶段）：

```powershell
pwsh scripts/check-architecture.ps1                     # 静态门禁（V11 的一部分）
pwsh scripts/test-plan0014-wave5b.ps1                    # 门禁自身的正/负例回归
cargo test -p messaging -p audit-postgres --all-features   # P-4 两处修复后仍全绿（两条 PG 用例仍是 ignored，离线）
```

**S1 实施落地补充（2026-09-30；基线 `6decf1a`，候选 `b25e2d4`）**——只登记实际落点，不改动上表的计划文本：

| 文件 | 实际变更 |
|---|---|
| `tools/module-release-tool/src/catalog.rs`（新）、`src/lib.rs`、`src/main.rs` | 四个 check：`module-catalog` / `pinned-baseline-history` / `baseline-embed`（§3.7 单一 embed 扫描，计划表原缺此行）/ `baseline-candidate-freeze`（**仅 `--only` 可选**，默认集合不含）；`migrations/modules` 两份 catalog 的 6 字段严格解析与全部 §11.2 规则 |
| `tools/module-release-tool/tests/fixtures/wave5b/catalogs/**` + `EXPECTATIONS.json`（新） | 50 个 case 目录（期望表 50 行），**两个读取者共用同一张期望表**；`history/pinned-baseline-5c8355d….json`（新）是钉住的 20 条摘要表（`include_str!` 内嵌，测试断言内嵌值 == 磁盘文件） |
| `tools/module-release-tool/tests/wave5b_catalog.rs`（新） | 第二个读取者（19 个用例）：fixture 全表、钉住历史、C1 七条、OID≠摘要反例、13 表归属 ↔ `019` 对账、默认 check 集合、仓库自身状态、embed 唯一点 |
| `scripts/test-plan0014-wave5b.ps1`（新） | 第一个读取者：catalog / history / embed / freeze / path 五个家族；负例全部在 scratch 副本上跑，不动仓库真实 catalog |
| `.gitattributes`（新，path 作用域） | 见 §11.3 第 5 点：只钉 fixture 与 `migrations/modules/**`，历史 migrations 不列入 |
| `Cargo.lock`、`crates/audit-postgres/Cargo.toml` | 两处测试改用 `runtime_migration::MIGRATOR` 所需的 dev-dependency 及其锁定结果 |

`cargo test -p messaging -p audit-postgres --all-features` 的实际口径修正：两条 PostgreSQL 用例保持 `#[ignore]`，本轮**未连接数据库**，因此上面第三条只证明**能编译**（`--no-run`），不证明 PG 行为；PG 行为仍为 NOT RUN。

### 11.5 fixture 规范：空 catalog ≠ 安装证明（Revision 1 新增）

- 测试目录必须提供两种 fixture：**(a) 空 catalog**（0 条 revision）、**(b) 含真实 revision 的 catalog**（至少一条能真实建对象的 SQL，例如在专用库创建 `wave5b_fixture_<ns>_t1` 临时表）。**(a) 只能证明"预留"**，安装 / 升级 / 失败事务 / 崩溃恢复路径**必须**由 (b) 覆盖——空 catalog 下这些路径不可达。
- 未来 fixture revision 通过 `runtime-migration` 暴露的**测试可构造** catalog 入口注入（内存构造 `NamespaceCatalog`，或 `#[cfg(any(test, feature = "test-hooks"))]` 的 fixture 目录），**不在 `migrations/modules/` 的正式模块目录里造无业务意义的迁移**，也不在 `migrations/` baseline 里加测试文件。
- fixture 建出的对象名必须带 `wave5b_fixture_` 前缀，测试结束时删除，并在结束时断言专用库里无该前缀残留。
- V8a/V8b/V8c 三条并发用例（§9.3）各自要求专用库，启动屏障由 harness 建立（gate 表由 harness 建删，runner 永不创建 ledger 之外的对象）。

### 11.6 S1 的边界与停止条件

S1 **不做**：创建 `platform_module_migrations`（属 S2 的 bootstrap）、接 CLI `--namespace` / `verify`（S3）、改 `apps/business-api` / `apps/governance-worker` 的启动迁移（S3 + D-2）、升级 release spec / mapping（S4）、任何数据库连接（S5）、`build.rs`（S2）。

**生产 catalog 激活与 baseline 全局冻结的前置**：二者都以 ADR-0029 的**正式处理结果**为准（由独立审阅与 Platform Architecture 裁决 Accepted / 需修改 / 拒绝）。**离线脚本通过不构成接受**，也不构成"G-203a-i 关闭"——那仍需独立审阅对固定 SHA 的判定。

---

## 12. 本轮执行状态（Revision 1）

- 本轮修改文件（严格限于审阅 §3.1 许可清单 + 归档本轮所依据的审阅文档）：
  1. 本文（Revision 0 → 1）；
  2. `docs/adr/ADR-0029-module-namespaced-migration-catalogs-and-ledger.md`（同步 B1/B2，**保持 Proposed**）；
  3. `docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md`（修正独立安装判据与真实执行范围）；
  4. `docs/plans/current/PLAN-0014-reference-conformance-and-r2-module-readiness.md`、`docs/architecture/ARCHITECTURE_STATUS.md`、`docs/README.md`（追加本轮审阅状态与修订指针，历史证据未改）；
  5. 新建 `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md`；
  6. 归档本轮所依据的 `docs/reviews/2026-09-30-wave5b-design-independent-review.md`（原为未跟踪文件；**已先随父提交 `e55a5a0` 单独入库，交付提交本身不含该文件**，归档做法与 `7180eec7` / `f1984544` 一致——Revision 1 复审更正）。
- 未修改：`migrations/*.sql`、任何 `MANIFEST.sha256`、`crates/runtime-migration/src/lib.rs`、`apps/migration/**`、任何模块适配器、release spec、`scripts/check-architecture.ps1`、Wave 5A 证据目录、`docs/adr/README.md` 索引行（其描述在本轮后仍准确：Proposed、不含物理 schema 搬迁）。
- 未连接、未执行任何数据库操作（含只读 SQL）；§9 的 LANE 1 / LANE 2 命令与 V1..V14 全部 **NOT RUN / BLOCKED**。
- Rust workspace 门禁：**本轮未运行，产品代码无改动**（`cargo fmt --check` / `cargo check` / `clippy -D warnings` / `cargo test` 均沿用 `c22939f0` / `c73955e` 的既有记录，不冒充本轮执行，也不借用 Wave 5A 的通过数字）。
- `git diff --check`：见 `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md` 的交付记录。
- 未自判 ADR-0029 Accepted；未实施 S1/S2/S3；未改 main；未 push / merge / tag / publish / deploy；未重开 Wave 5A Repair 2。
- 状态：**WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW；WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；R2 BLOCKED**。

### 12.1 S1 实施状态（2026-09-30；实施轮，不是设计修订）

- 基线与候选：起点 `6decf1ad9ea05233d66dd3539594ff210afd99f2`，实现候选 `b25e2d4a9b455b776934495b8f3dfd2b71a13fe2`（本地，未集成）；钉住的 baseline 提交仍是 `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`（revision 19）。
- 已落地：`catalog-check` 四个 check（默认 `module-catalog` / `pinned-baseline-history` / `baseline-embed`，`baseline-candidate-freeze` 仅显式 `--only`）；`migrations/modules/{identity-management,policy}` 两份 catalog 打印 `RESERVED`；`scripts/check-architecture.ps1` 调默认 check + namespace manifest canonical-LF-only 断言；一份 `EXPECTATIONS.json` 由 PowerShell 驱动与 Rust 测试两个读取者共同消费；`tools/module-release-tool/scripts/` 收录 fixture 生成器、钉住历史生成器、门禁驱动与 closeout 对账器。
- 对本文的更正落点：§5.2（`depends_on` 精确语法与两条附加规则）、§11.2 实施补充 a–f（含"owner 也不得销毁 host baseline 表"）、§11.3 第 5 点（C1 完整口径：`git` 对象 id 不是摘要 / 7 条 CRLF-only 容错保留但每条另被 blob 摘要钉住 / 新 catalog 只认 canonical LF / 四方比对 / `.gitattributes` 只作用两条新路径）、§11.4（实际文件清单，含本文原计划缺的 `baseline-embed` 与 `.gitattributes` 两行）。
- 与本文计划的一处口径修正：§11.4 第三条命令 `cargo test -p messaging -p audit-postgres --all-features` 本轮**只做到 `--no-run`**（两条 PG 用例仍 `#[ignore]`，未连接数据库），因此它证明能编译，不证明 PG 行为。
- 门禁与缺口：逐条日志 `docs/reviews/2026-09-30-wave5b-s1-gates/`；PG 全部 **NOT RUN**（无授权），`P2-catalog-symlink` **BLOCKED**（无特权 + exFAT）。S1 不做的事（§11.6）一件都没做：无 ledger 表、无 runner、无 CLI `--namespace` / `verify`、无启动迁移改动、无 release spec 升级。
- 状态：**WAVE 5B-S1 READY FOR INDEPENDENT REVIEW / NOT INTEGRATED；R2 BLOCKED**。§12 的 Revision 1 记录保留原文；G-203 的四个记账子项与全部 C1 一律未关闭，ADR-0029 仍 Proposed——离线门禁通过不构成接受（§11.6）。
