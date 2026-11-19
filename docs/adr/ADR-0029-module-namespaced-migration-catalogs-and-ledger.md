# ADR-0029：模块命名空间迁移 catalog 与平台迁移 ledger

> 状态：**Proposed**（本文不自行接受；接受与否由独立审阅与 Platform Architecture 决定）  
> 日期：2026-09-30  
> 决策所有者：Platform Runtime（migration runner）+ Platform Architecture  
> 关联文档：PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE、DATA_OWNERSHIP_AND_CONSISTENCY、SECURITY_ARCHITECTURE、DEPLOYMENT_ARCHITECTURE、SAAS_MODULE_STANDARD、ADR-0009、ADR-0025、PLAN-0014 Wave 5B-0 设计包  
> 替代：无  
> 被替代：无  
> 基线 SHA：`c73955ec30f46d839ff33d44776cef142b7d6d77`

**Revision 1（2026-09-30，起点 SHA `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`）**：按独立审阅
`docs/reviews/2026-09-30-wave5b-design-independent-review.md` 的 B1（共享 ledger 首次创建的锁保护与持锁连接
生命周期）与 B2（方案 B 时间线不可执行、且物理隔离不等于单模块冷安装）更正 3.2 / 3.3 / 3.5 / 3.7 与第 4、6
节；决策本身（冻结 baseline + namespace catalog + 平台 ledger + 单一 writer）未变，**状态仍为 Proposed**。

## 1. 背景

PostgreSQL 侧当前只有一个平面迁移 catalog：`migrations/001..019.sql` + 单一 `migrations/MANIFEST.sha256`，由 `crates/runtime-migration/src/lib.rs:6` 的 `sqlx::migrate!("../../migrations")` 整体 embed，执行记录只落在 sqlx 的全局 `_sqlx_migrations`。`migrations/019_identity_authorization_foundation.sql` 一个文件同时创建 identity-management（5 表）、organization（3 表）、policy（5 表）三套私有数据，并在 `:21-24` 重命名 `001` 创建的 v0 demo 表。

后果（PLAN-0014 Wave 2 审阅 G-203，`docs/reviews/2026-09-24-saas-align-module-packaging.md:316-320`）：模块的 `migration_namespace` 只是声明字段，多模块共存时无法证明"单模块迁移可独立演进/校验"。SQLite 侧已经有正确形状（每 crate 一个 `migrations/` + 自己的 `MANIFEST.sha256` + 自己的 `*_migrations` ledger），PostgreSQL 侧没有对应物。

同时存在两个真实约束：迁移历史不可变（`DATA_OWNERSHIP_AND_CONSISTENCY.md:318-325`），以及既有部署库已带 19 个 revision 与真实数据。任何"按模块重排历史"的做法都不可接受。

## 2. 决策驱动因素

- 历史不可变：`001..019` 与其 checksum 必须字节级保持；
- 所有权可见：每条迁移必须能指出所属 bounded context（`DATA_OWNERSHIP…:318-325`）；
- 单一 writer：同一版本不得由两种 runner 分别管理；
- 运行时可校验：namespace 的版本、checksum、缺口与孤儿必须能在运行时 fail-closed 判定，而不是只在构建期门禁；
- 新平台可变数据必须有唯一 owner（`DATA_OWNERSHIP…:21`）；
- 不破坏已 Accepted 契约：`MigrationRevision` 与 `identity.v1` 字段集合不得扩张（SP-1 明确未执行）；
- 渐进可交付：隔离能力不得以不可逆数据搬迁为前置条件。

## 3. 决策

### 3.1 host baseline 冻结

`migrations/*.sql`（当前 19 个文件）与其 `MANIFEST.sha256` 定位为 **host baseline catalog**，由架构门禁冻结：不得新增、删除、重命名文件，不得修改内容。执行记录继续使用既有的 `_sqlx_migrations`，不迁移、不补写、不改写 checksum。

### 3.2 模块 catalog 落在 namespace 子目录

模块自有迁移进入 `migrations/modules/<namespace>/NNN_*.sql`，`<namespace>` 必须等于该模块的 `data_owner`（沿用 G-203 / Wave 2 §16 D-02 的既有等价规则）。每个 namespace 目录自带一份 `MANIFEST.sha256`（沿用既有行语法 `<64 hex><两空格><文件名>`）与一份 `catalog.json`（`schema_version`、`namespace`、`owner`、`ledger`、`sql_schema`、`depends_on`；字段固定、**未知字段与未知 `schema_version` 一律拒绝**）。revision 在 namespace 内独立编号，从 001 起连续，不复用平面序列号。`schema_version` 是 catalog 元数据自身的版本，**不改变** `MigrationRevision` 或 `identity.v1` 的任何字段。

新 catalog 仍由共享 `runtime-migration` crate embed（满足 `PERSISTENCE…:91-96` 的单一 embed 点）；模块新增迁移只需新增目录与文件，不改核心代码。

### 3.3 新增平台 ledger 表 `platform_module_migrations`

```
platform_module_migrations(
  namespace TEXT, revision BIGINT, description TEXT,
  checksum CHAR(64), applied_at TIMESTAMPTZ, applied_by TEXT,
  PRIMARY KEY (namespace, revision))
```

- **唯一 owner：Platform Runtime（migration runner）**。业务模块的 SQL 永不写这张表，因此"谁能改迁移事实"只有一个答案（满足 `DATA_OWNERSHIP…:21`）。
- append-only，无 status 列（状态由行的存在性推导），`checksum` 存 LF 折叠后的 SHA-256 十六进制，执行前回读校验。
- **表 DDL 由 runner 在"平台 bootstrap 锁"内、专用事务中执行**（Revision 1 更正 B1）：该锁使用**与 namespace 无关的固定键**，所有 namespace 共用一把；因此两个不同 namespace 首次并发 `up` 不会同时进入共享表的创建。原方案"在每个 namespace 各自的 advisory lock 内 `CREATE TABLE IF NOT EXISTS`"被撤回——两把 namespace 锁互不相干，保护不了同一张共享表，且 `IF NOT EXISTS` 会静默接受形状不同的既存对象。本 ADR 不使用 `IF NOT EXISTS`：存在性检查与创建在同一把公共锁内完成，之后进入结构校验（列集合与类型、主键列序、CHECK 约束、`relkind='r'`、owner），不符即 fail closed 且不自动 `ALTER`；提交后再回读确认（设计包 §5.9）。
- 该表只记录 `migrations/modules/**` 的执行事实；`_sqlx_migrations` 只记录 `migrations/*.sql`。两个版本空间由 3.1 的冻结规则保证不重叠。

### 3.4 单一 writer 与只读校验

`apps/migration` 是唯一写 ledger 的进程（`up` 应用 baseline + 本二进制 embed 的全部 namespace；`--namespace` 子集仅供 pilot/测试）。API 与 worker 侧只做只读 verify，不执行迁移。该决策同时是 `PERSISTENCE…:96-98`（正式 API 进程不自动迁移）的落实路径，但对现有 `apps/business-api/src/composition.rs:126`、`apps/governance-worker/src/main.rs:50` 启动自动迁移的收敛属于独立决策项（Wave 5B-0 设计包 D-2），本 ADR 被接受不等于该变更已实施。

### 3.5 失败、并发、锁与恢复（Revision 1 重写）

- **事务原子性**：`BEGIN; <revision SQL>; INSERT ledger row; COMMIT;` 单事务，要么全成要么全无；不提供 down 脚本，回滚按 `DEPLOYMENT_ARCHITECTURE.md:338` 采用 forward-fix；不默认删除任何业务数据（`SAAS_MODULE_STANDARD.md:271-282` uninstall ≠ purge）。
- **两把不同职责的锁，均为事务级 `pg_advisory_xact_lock`**（与仓内既有 8 处实现同形）：公共 bootstrap 锁（单 bigint 键，固定文本常量）与 namespace 锁（`(classid, objid)` 二元组键）。两种键形态在 PostgreSQL 中属于不同锁标签，结构上不可能互相碰撞；namespace 内部 `hashtext` 若碰撞，后果只是**多**串行（安全方向），且 runner 启动时必须断言已 embed 的 namespace 键两两不同，否则 fail closed。
- **唯一锁序**：host baseline → bootstrap 单元（取公共锁、校验/创建、提交即释放）→ 按 `depends_on` 拓扑序逐 namespace（一次只持一把 namespace 锁）。**禁止**在持有 namespace 锁时再取公共锁，也禁止同时持两把 namespace 锁；死锁因此被结构排除，而不是靠超时兜底。
- **持锁连接生命周期**：每个受保护单元就是一个 `Transaction`，同一物理连接既执行锁语句又执行受保护 SQL（禁止经连接池对象执行）；锁随 COMMIT / ROLLBACK / 连接断开由数据库自动释放，因此不存在"持锁连接回池"或漏解锁。应用级取消 **不等于** 连接断开：runner 必须显式 ROLLBACK 并不再复用该连接。等锁用 `lock_timeout`（默认 30 秒）超时即 fail closed；**不设置** `statement_timeout`，避免合法慢 DDL 被会话默认值中途打断。
- **只读路径不创建表**：`verify` / `status` / readiness 不执行 DDL、不取 advisory 锁；ledger 表缺失映射为 `Empty` / `Pending`（与既有"status 只可忽略 migration table 缺失"的规则一致），权限不足、连接失败、表形状不符、checksum 漂移、缺口与孤儿一律 fail closed。
- **fail-closed 判定**：checksum 漂移、孤儿 revision、revision 缺口、ledger 形状不符、锁键碰撞、等锁超时一律退出码 1 且不自动修复。
- **协议边界**：advisory lock 只约束遵循本协议的 runner 进程，**不构成**数据库层 DDL 权限隔离；后者属角色与 grant 拆分（3.7，另需独立 ADR）。

### 3.6 与发布工件的绑定

`MigrationRevision` 保持 `(migration_namespace, set_digest)` 两字段不变，`plan-0014.migration-set.v1` 摘要公式不变。模块 spec 的 postgres set 改为自己 namespace 的文件（`module_owned = true`），并新增 `requires_host_baseline` 声明；baseline 文件仍作为 `set_digest` 输入，使工件摘要传递性地钉住所依赖的共享历史。后端评估拆为 `namespace_isolation` 与 `independent_install` 两个维度，后者在物理隔离完成前恒为 `BLOCKED`。

### 3.7 范围边界（明确不授权）

本 ADR **不**授权：创建 PostgreSQL `SCHEMA` 并把既有表迁入、`ALTER TABLE … SET SCHEMA`、任何 `DROP TABLE`（含 `v0_demo_*`）、修改 SQLite catalog 布局与 `*_migrations` ledger 命名、修改运行时连接角色与 grant 模型。以上各需独立 ADR 与运维授权。

**Revision 1 补充两条明确非目标（B2）**：

1. **物理 schema 隔离不是本 ADR 的一部分，且其设计尚未完成**。设计包 §6.3–§6.5 记录了撤回的原因（兼容视图指向尚未创建的表、旧名仍被旧表占用）与 7 项未决 OPEN（含"兼容视图上的 `ON CONFLICT` 写语义未证明"、"`search_path` 基线缺失"、"角色矩阵未设计"）。它已从 Wave 5B 的可执行实施步骤中移除，只作为未来 G-203b 的研究对象。
2. **本 ADR 不提供、也不声称提供"单模块在空库上独立安装"的能力**。搬移既有表的位置不会让空库少跑一步 host baseline；物理隔离（G-203b-i）与单模块冷安装（G-203b-ii）是两个必须分别设计、分别记账的验收目标（设计包 §6.6）。任何引用本 ADR 的工件都不得把 namespace 落地写成"独立安装已证明"。

## 4. 后果

正面：G-203 的字面关闭条件（namespace 子目录 + per-namespace manifest + `set_digest` 钉定 + 历史不重写 + 两类必需测试）可在不动既有数据的前提下达成并机器可验；模块新增迁移不再触碰共享目录；迁移执行事实首次具备运行时可校验的 namespace 维度。

负面与代价：PostgreSQL 侧出现两套 ledger（`_sqlx_migrations` 与 `platform_module_migrations`），必须靠门禁维持边界；namespace revision 依赖 host baseline revision 19，因此"identity 单独冷安装、不创建 organization/policy 表"仍未达成；identity / organization / policy 的既有 13 张表仍留在 `public`，其物理位置与权限隔离属 G-203b，需要 3.7 列出的授权动作才能关闭，**R2 在此之前继续 BLOCKED**；`runtime-migration` 需新增 `build.rs`（仓内首个构建脚本）以在编译期发现新目录。G-203 的判据拆分与四个记账子项见设计包 §7.1，均为 **Proposed**，不改变 G-203 / G-202 / A-12 的原分母。

## 5. 备选方案

| 方案 | 拒绝理由 |
|---|---|
| 直接按模块重写平面序列（把 019 拆成三个文件） | 违反迁移历史不可变，改变已应用库的 checksum，属不可逆且无必要 |
| 每 namespace 一个 `sqlx::migrate!` 静态 | 新增模块必须修改平台核心代码；且 sqlx 只认 `_sqlx_migrations`，无法表达 namespace 键 |
| 每模块一张 ledger 表（沿用 SQLite 命名法） | 跨 namespace 状态读取需动态表名；每张新表都要一次 DDL 授权；隔离收益不抵复杂度 |
| 运行时从文件系统读 catalog | 迁移 SQL 必须随镜像分发，且与 `runtime-migration` "embedded catalog" 定位冲突 |
| 立即做物理 schema 隔离（一步到位） | 需要 IAM 三适配器 73 处 SQL 命中点改写（grep 口径）、全仓首次引入 `search_path`、不可逆表搬迁与角色授权矩阵；应作为 G-203b 独立轮次。**且该方案当前设计未完成**（兼容视图写语义、`search_path` 基线、锁窗口与依赖对象清单均 OPEN，见 3.7 与设计包 §6.5），不能作为本决策的替代或补充立即执行 |
| 会话级 `pg_advisory_lock` + 手工释放（Revision 0 的形态） | 需要额外规定"锁未释放不得回池"与漏解锁路径；仓内既有 8 处均为事务级锁。改为事务级后，释放由事务/连接生命周期保证，且与公共 bootstrap 锁组成不同的键标签域（3.5） |

## 6. 验收

见 `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md` §9.3 验收矩阵（V1..V14，其中并发项在 Revision 1 拆为 V8a 同 namespace 双进程、V8b **不同 namespace 首次并发 bootstrap**、V8c bootstrap 持锁进程被终止后恢复）与 §7 / §7.1 的 G-203 逐子句核对和记账子项。V1–V4、V6–V9 需真实 PostgreSQL 授权；未获得授权时一律按 `NOT RUN` / `BLOCKED` 记录，不得以 SQLite 或离线证据替代，也不得以门禁脚本通过替代本 ADR 的接受判定。**截至 Revision 1（起点 `5c8355d`）本 ADR 未执行任何数据库操作，矩阵全部条目为 NOT RUN / BLOCKED。**
