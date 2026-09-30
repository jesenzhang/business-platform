# PLAN-0014 Wave 5B-0：模块迁移隔离设计包

> Status: Proposed / **DESIGN READY FOR REVIEW**
> Revision: 0
> 文档 ID：PLAN-0014-WAVE5B-MIGRATION-ISOLATION-DESIGN-2026-09-30
> Date: 2026-09-30
> Start SHA（本文所有行号与事实的基线）：`c73955ec30f46d839ff33d44776cef142b7d6d77`
> 对应设计提案：SP-3、SP-4（`docs/reviews/2026-09-29-wave5a-implementation-record.md:382-383`）
> 指令来源：`docs/reviews/2026-09-30-wave5a-repair2-independent-review.md` §4.4（十项必答）
> 关联准入矩阵：`docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md`
> 本轮边界：不改迁移代码、不执行任何数据库操作、不关闭 G-203、不关闭 A-12、不宣布 R2、不推送。

---

## 0. 结论先说

| 问题 | 结论 |
|---|---|
| 推荐方案 | **方案 A**：冻结 `migrations/001..019` 为 host baseline（历史与 `_sqlx_migrations` 一字不动），新增 `migrations/modules/<namespace>/` 每模块 catalog + 每 catalog `MANIFEST.sha256` + 一张平台 ledger 表 `platform_module_migrations`，由单一 runner（`apps/migration` + `runtime-migration` 新 API）按 namespace 写入 |
| 方案 A 是否满足 G-203 字面关闭条件 | **满足**（`2026-09-24-saas-align-module-packaging.md:316-320` 的 5 个子句逐条可测，见 §7） |
| 方案 A 是否满足"identity 可在 PostgreSQL 上单独冷安装"这种更强读法 | **不满足**，且当前代码下任何方案都不可能在不动数据的前提下满足：identity/organization/policy 的既有表全部由共享 `019` 创建，`019:21-24` 还重命名了 `001` 建的表 |
| 因此提出的判据修订 | 把 G-203 拆成 **G-203a**（namespace catalog / manifest / ledger / runner 隔离 + 模块**未来** revision 可独立安装升级：Wave 5B 可关闭）与 **G-203b**（既有共享表的物理 schema 隔离与真正单模块冷安装：不可逆数据搬迁，需独立 ADR + 运维授权，R2 继续 BLOCKED）。**本文不自判关闭**，判定权在独立审阅 |
| 必须新增的对象 | 1 张平台 ledger 表（需命名 owner）+ 1 个 Proposed ADR（ADR-0029）+ 目录与门禁扩展；**不需要**修改任何 Accepted 契约类型的字段 |
| 待决策 | 6 项（§10），其中 D-2（API/worker 启动是否继续自动迁移）是本轮盘点暴露的真实 Baseline 偏差 |

---

## 1. 输入与本设计受约束的权威条文

| 权威条文 | 原文要点 | 本设计如何满足 |
|---|---|---|
| `docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md:91-98` | PostgreSQL 迁移只放 `/migrations`，由共享 `runtime-migration` embed；**正式 API 进程不自动迁移**；只有 `_sqlx_migrations` 缺失才视为空 catalog | 新 catalog 仍由共享 `runtime-migration` embed（§5.1）；写权限收敛到 `apps/migration`（§5.4、D-2）；空 catalog 判定只针对"表不存在"（§5.4） |
| `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md:318-325` | 迁移必须标明所属上下文；迁移历史不可变 | 每 namespace 目录 = 所属上下文；`001..019` 与 `MANIFEST.sha256` 冻结不改（§5.5） |
| `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md:21` | 每份可变权威数据只有一个 owner | `platform_module_migrations` 的唯一 owner = Platform Runtime（migration runner），§5.3 显式命名并写入 ADR-0029 |
| `docs/architecture/SECURITY_ARCHITECTURE.md:251-260` | 迁移用独立高权限角色，运行时应用不得拥有随意 DDL | §5.6 角色矩阵 + D-2；本文不声称已满足，只把偏差登记为待决 |
| `docs/architecture/DEPLOYMENT_ARCHITECTURE.md:55-62, 333, 338` | migration 是独立部署单元，有自己权限；expand → migrate → switch → contract；回滚用 forward-fix | §5.5 安装时间线、§6.3 过渡步骤、§5.8 回滚策略 |
| `docs/standards/SAAS_MODULE_STANDARD.md:271-282` | 模块拥有独立迁移命名空间；只动 owner 的 schema/数据；uninstall ≠ purge | §4 namespace 定义、§5.8 无默认数据删除 |
| `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md:228-238` | 空库安装、重复运行安全、从上一版本升级 | §9.2 验收矩阵 V1/V3/V4/V5 |
| `docs/standards/SAAS_MODULE_STANDARD.md:370-383` + `PLAN-0014…md:809-822` | R2 需 C0=0、C1=0 且独立 reviewer 对固定 SHA PASS | 本设计不改变任何 C1 判定；G-203 仍 OPEN |
| `docs/reviews/2026-09-24-saas-align-module-packaging.md:316-320` | G-203 原关闭条件（逐字）：pilot 模块迁移落 `<namespace>` 子目录 + per-namespace `MANIFEST.sha256`，`MigrationRevision.set_digest` 钉定；平面序列不重写（历史保持）；必需测试 = per-namespace sha 校验 + release identity 钉定测试 | §7 逐子句核对 |

---

## 2. 现状事实（只读盘点，基线 SHA `c73955e`）

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
- CLI：`apps/migration/src/main.rs:63-73` 只接受 `--backend <postgres|sqlite> <up|status>`（严格 3 参数位置解析），没有 `--namespace` / `--dry-run` / `--target-version`。`up` 在库不存在时会 `create_database`（`:77-80`）。`status` 只比版本号、不比 checksum，且落后也返回 0（`:98-144`）。
- 兼容性读取：`apps/business-api/src/state.rs:233-254`（`SELECT COALESCE(MAX(version),0) FROM _sqlx_migrations` → `classify` → readiness）。

### 2.3 盘点暴露的四条事实性偏差（本设计必须处理，不得当作正确现状）

| # | 事实 | 冲突条文 | 本文处置 |
|---|---|---|---|
| P-1 | `apps/business-api/src/composition.rs:126` 与 `apps/governance-worker/src/main.rs:50` 在进程启动时执行 `MIGRATOR.run` | `PERSISTENCE…:96-98`（正式 API 进程不自动迁移）、`SECURITY…:251-260`（运行时不得有随意 DDL） | 记为 D-2，方案 A 的"单 writer"命题依赖它；本文不擅自改代码 |
| P-2 | SQLite IAM ledger 的 `checksum` 列存 SQL 原文且从不回读 | `ARCHITECTURE_FITNESS_FUNCTIONS.md:228-238` 的完整性要求 | 新 PG ledger 存 64 位 hex 摘要并每次回读（§5.3/§5.4）；SQLite 侧不改动（SP-4 边界，§8） |
| P-3 | `MANIFEST.sha256` 只在构建期/门禁校验，运行时无人校验 | 同上 | 门禁 + runner 双侧校验（§5.4、§8 S1） |
| P-4 | `crates/messaging/tests/outbox_reliability.rs:27` 自己再 embed 一份 `sqlx::migrate!("../../migrations")`；现有门禁 `check-architecture.ps1:52` 只扫 `apps/migration/src` | `PERSISTENCE…:91-96` 单一 embed 点 | 纳入 §8 S1 的扫描范围扩展 |

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

### 3.2 索引（11）

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

### 3.4 跨 context 引用与写入依赖（无 FK，单列）

| 列 | 行号 | 读取方 → 被引用 owner | 是否有 FK |
|---|---|---|---|
| `organization_members.user_id` | 118 | organization → identity-management | 否（裸 UUID） |
| `role_bindings.user_id` | 184 | policy → identity-management | 否（裸 UUID） |
| `role_bindings` 的 `scope_org_unit_id` scope 列 | 181-214 | policy → organization | 否（裸 UUID） |
| 全部 `tenant_id`（`50,100,117,133,155,183,224`） | — | 三 context → v0 `tenants`（`001_initial.sql:5`） | 否 |

`019` 不引用 `documents` / `document_*` / `contracts` / `outbox_events` / `audit_events`。**结论：跨 context 依赖是"值引用 + 应用层约束"，不是数据库约束**；因此任何 namespace 拆文件都不需要跨 schema 外键，但也不能宣称数据库层阻止跨模块写入（本来就不阻止）。

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

`MigrationRevision.migration_namespace`（`crates/business-module-contracts/src/release.rs:282-287`）今天只是声明字段。方案 A 把它同时变成 L1 + L2 的真名，**不**触及 L3。G-203 原文（`:316-320`）要求的正是 L1+L2；"独立演进/校验"由 L2 提供；"单模块冷安装不碰别人表"才需要 L3。

---

## 5. 方案 A（推荐）：host baseline 冻结 + 每模块 catalog + 单一 runner

### 5.1 目录与 embed 布局

```
migrations/                         # host baseline（冻结，只读，19 个文件 + MANIFEST.sha256）
  001..019_*.sql
  MANIFEST.sha256
migrations/modules/
  identity-management/
    catalog.json                    # namespace 元数据（§5.2）
    MANIFEST.sha256                 # 只覆盖本目录 SQL
    001_<owner>_<topic>.sql         # 第一条 identity-management 自有 revision（未来）
  policy/
    catalog.json
    MANIFEST.sha256
```

- 新 SQL **只允许**进 `migrations/modules/<ns>/`；`migrations/*.sql` 由新门禁冻结在现有 19 个文件名（§8 S1）。这保证 L2 的两个版本空间永不重叠：`_sqlx_migrations.version ∈ 1..19`，`platform_module_migrations.(namespace,revision)` 独立编号。
- embed 机制：`crates/runtime-migration` 新增 `build.rs`，构建期遍历 `migrations/modules/*/`（`std::fs`，无新依赖，`cargo:rerun-if-changed`），生成 `include_str!` 表 + 每文件 sha256；`MIGRATOR`（`:6`）保持不动。理由：模块新增迁移 = 新增目录，核心代码零改动；不把 `migrations/` 变成运行时文件系统依赖（`deploy/Dockerfile.rust-app:10` 仍只为 baseline 复制）。
- 拒绝项：每 namespace 一个 `sqlx::migrate!`（新增模块必须改核心代码）；运行时读目录（需要把 SQL 随镜像发布，且与 `runtime-migration` 的"embedded catalog"定位冲突）。

### 5.2 `catalog.json`（每 namespace 一份，最小字段）

| 字段 | 例 | 校验 |
|---|---|---|
| `namespace` | `identity-management` | 必须等于目录名，且等于 `data_owner`（沿用 `specs/identity.json` 的 `migration_namespace` 规则，authority 见 `2026-09-24…md` §8 G-203 / §16 D-02） |
| `owner` | `identity-management` | 与 `BOUNDED_CONTEXT_MAP.md` 的 context 名一致 |
| `ledger` | `platform_module_migrations` | 固定常量，禁止 `_sqlx_migrations` |
| `sql_schema` | `public` | 方案 A 恒为 `public`；出现其他值即门禁失败（防止悄悄做 L3） |
| `depends_on` | `["platform-baseline@19"]` | runner 与门禁都检查：无环、baseline 版本 ≥ 声明值 |

不引入新格式标准：`catalog.json` 只被 `check-architecture.ps1`、`tools/module-release-tool`、`runtime-migration/build.rs` 三方读取，三方共用同一组 schema 校验测试。

### 5.3 新 ledger 表（唯一新增可变权威数据）

```sql
-- 由 runner 在 namespace advisory lock 内创建；不属于任何业务模块
CREATE TABLE IF NOT EXISTS public.platform_module_migrations (
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

### 5.4 runner 职责矩阵（§4.4 第 4 项）

保留 workspace composition root：`apps/migration` 是唯一可执行迁移的进程，`runtime-migration` 提供纯逻辑与 catalog。

| 职责 | 承担者 | 细节 |
|---|---|---|
| 谁写 `_sqlx_migrations` | 仅 sqlx `MIGRATOR`（host baseline） | 今天被 3 个进程调用（P-1）；sqlx 自带 advisory lock 保证同版本不被双写，D-2 把入口收敛到 1 个 |
| 谁写 `platform_module_migrations` | **仅 `apps/migration`**（`runtime_migration::apply_namespace`） | API / worker 只读校验，见下 |
| 两个 ledger 会不会管同一版本 | 结构上不会 | `_sqlx_migrations` 只承载 `migrations/*.sql`（冻结 1..19）；`platform_module_migrations` 只承载 `migrations/modules/**`；S1 门禁禁止向 baseline 追加文件 |
| namespace 选择 | `apps/migration` 参数 | `up` = baseline + 本二进制 embed 的**全部** namespace（fail-closed：二进制带的都要装上）；`--namespace <ns>`（可重复）= 子集，仅用于 pilot/测试，日志显式打印 restricted namespace run |
| 版本状态 | `runtime_migration::namespace_status()` | 每 namespace 返回 `{head_applied, pending, expected_head}`；复用 `MigrationCompatibility`（`lib.rs:8-14`）语义 `Empty/Behind/Equal/Ahead` |
| checksum 校验 | runner | 执行前：对 ledger 每一行重算嵌入文件摘要并比较；执行后写入本次摘要。不匹配 → `NamespaceError::ChecksumMismatch{namespace,revision}`，**不自动修复** |
| 依赖排序 | runner | 先 host baseline（`MIGRATOR.run`），再 namespace 拓扑序（`depends_on`），namespace 内 revision 升序 |
| 并发锁 | runner | `SELECT pg_advisory_lock(hashtextextended('plan0014.module-migration:' \|\| $ns, 0))`，会话级，**在 `CREATE TABLE IF NOT EXISTS` ledger 之前**获取（`crates/messaging/tests/inbox_idempotency.rs:18-21` 已记录 `CREATE TABLE IF NOT EXISTS` 在 PG 下并发不安全）；进程被杀由 PG 自动释放锁。沿用仓内既有 `hashtextextended` 锁模式（`crates/identity-postgres/src/lib.rs:360,581` 等） |
| dirty / 失败拒绝 | 事务原子性 + 三条 fail-closed 判定 | ① ledger 有行但 catalog 无对应文件 → `OrphanRevision`；② revision 不连续（有 2 无 1）→ `RevisionGap`；③ checksum 漂移 → `ChecksumMismatch`。三条都 exit 1 且不改任何对象 |
| 恢复 | runner | 无 dirty 标记：PostgreSQL DDL 事务化，`BEGIN; <revision SQL>; INSERT ledger row; COMMIT;` 要么全成要么全无；重跑从第一个缺失 revision 继续 |
| verify-only | `runtime_migration::verify_namespace()`（只读） | 供 readiness 与部署后检查使用；`status` 升级为**按 namespace**报告并把"落后"变成非 0 退出码（今天 `main.rs:98-144` 落后也返回 0） |

### 5.5 两条时间线（§4.4 第 2 项：历史兼容）

**A. 空库全新安装**

1. `MIGRATOR.run` → `_sqlx_migrations` 写入 1..19（与今天逐字节一致，checksum 由 sqlx 生成，无人改写）；
2. 取各 namespace advisory lock，`CREATE TABLE IF NOT EXISTS platform_module_migrations`；
3. 按 `depends_on` 拓扑序 + revision 升序应用 namespace revision；
4. 结果：`_sqlx_migrations` 19 行 + 每 namespace N 行。**不补写任何成功记录**：baseline 的真实记录仍在 `_sqlx_migrations`，新 ledger 只记它真正执行过的东西。

**B. 既有宿主升级**（今天的历史状态：migrations 1–19、43 张 public 表）

1. `MIGRATOR.run` 校验 1..19 checksum 全部 `Equal`，**不重放**；任一漂移即失败（sqlx 行为，本设计不绕过）；
2. ledger 表由 runner 首次创建（这是 baseline 之外唯一的 DDL，§5.3）；
3. 只应用 namespace revision；
4. `_sqlx_migrations` 内容前后完全不变（验收矩阵 V4 用前后 dump 比对断言这点）。

**明确不做**：不把 001..019 复制进 namespace、不做"补记/adopt 旧 revision"、不改 `migrations/MANIFEST.sha256`、不引入 `CREATE TABLE IF NOT EXISTS` 式的条件采纳（那会让同一张表有两个定义源，只能靠额外 diff 门禁勉强兜住，拒绝）。

### 5.6 兼容与权限（§4.4 第 6 项）

| 面 | 现状（实测） | 方案 A 影响 |
|---|---|---|
| repository SQL | 全部裸表名，零 `public.` 限定。表名出现次数（grep 口径，含列名/别名噪声，**不是**精确语句数）：`identity-postgres` 21、`organization-postgres` 19、`policy-postgres` 33、`document-postgres` 13、`document-processing-postgres` 87、`audit-postgres` 6、`messaging` 8、`runtime-governance-postgres` 93，合计 280 | **零改动**（模块 DDL 仍落 `public`，在任何角色默认 `search_path` 内可见） |
| `search_path` | 全仓 0 处设置（仅 1 处散文命中），无 `PgConnectOptions`、无 `set_config` | 方案 A 不引入；方案 B 必须引入（§6.2） |
| 连接角色 | 单一连接串（`MIGRATION__DATABASE__URL`，legacy `DATABASE_URL`，二者互斥，`apps/migration/src/config.rs:15-26`）；API 能自动迁移意味着应用角色当前持有 DDL 权限 | 方案 A 不新增权限；提出目标角色矩阵 `bp_migration`（`CREATE`/`ALTER` on `public`，ledger owner）与 `bp_app`（DML，ledger 只 `SELECT`），作为 D-5，不在 5B-1 强推（牵动部署 runbook） |
| 既有依据 | `SECURITY_ARCHITECTURE.md:251-260`、`DEPLOYMENT_ARCHITECTURE.md:55-62,333` | 本文只登记差距，不声称已达成 |

### 5.7 与发布工件的绑定（§4.4 第 7 项）

| 对象 | 变化 | 不变（硬约束） |
|---|---|---|
| `MigrationRevision`（`release.rs:282-287`） | 无 | **仍是 2 字段** `(migration_namespace, set_digest)`；`identity.v1` 8 字段成员集合不扩（SP-1 明确未执行，`PLAN-0014…md:979`） |
| `set_digest`（`canonical.rs:242-260`：`sha256("plan-0014.migration-set.v1\nnamespace=…\n<backend> <file> <sha256>\n…")`） | 输入集合改变：pilot 的 postgres set 从"共享 `019`（`module_owned=false`）"变为"本 namespace 文件（`module_owned=true`）+ 声明的 host baseline 依赖" | 算法 id `plan-0014.migration-set.v1` 与摘要公式不改 |
| host baseline 依赖表达 | `ReleaseSpec` 新增 `requires_host_baseline`（spec schema id 升 `…release-spec.v2`，`lib.rs:63`），baseline 文件仍进 `set_digest` 输入 ⇒ **工件摘要传递性地钉住它依赖的共享历史** | 不新增 `ModuleReleaseIdentity` 字段 |
| `MigrationFileRecord`（`migrations.rs:41-61`） | 字段不变；`module_owned` 对新 namespace catalog 为 `true`（沿用"必须整目录全声明"规则 `:239-256`） | 不改 JSON 字段名/含义 |
| 后端评估（`migrations.rs:307-338`，现对 `postgres && !module_owned` 直接 `BLOCKED`） | 拆成两个维度：`namespace_isolation`（新 catalog 落地 = `MAPPED`）与 `independent_install`（G-203b 关闭前恒 `BLOCKED`）；mapping schema 升 `…migration-mapping.v2`，v1 保留为历史 | 不得因为 namespace 落地就把"独立安装已证明"写进工件 |
| `package_digest` / `contract_digest` | 预期**不变**（迁移映射不属于契约面；`verify_release_identity` 钉的是 namespace，不是 `set_digest`）；`artifact_integrity_digest` 会变（mapping 字节变了） | 该预期在 5B-1 实测确认，本文不作为既成事实 |

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

---

## 6. 方案 B：物理 schema 隔离（对比对象 + 完整过渡步骤）

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

### 6.3 过渡步骤（expand → migrate → switch → contract，`DEPLOYMENT…:333`）

1. **G0 授权**：ADR-0029 由 Proposed → Accepted，且只接受到"允许做 expand 阶段"；搬迁本身另立 ADR。
2. **expand-1**：`CREATE SCHEMA <ns>`；ledger 已在方案 A 落地。
3. **expand-2**：建兼容视图（如 `CREATE VIEW public.platform_users AS SELECT * FROM identity_management.platform_users` + INSTEAD OF 触发器），使未改写的 SQL 仍可工作。
4. **expand-3**：新代码路径改用 schema 限定名；不做双写（同一物理对象只有一条路径）。
5. **migrate**：`ALTER TABLE public.platform_users SET SCHEMA identity_management`（每 context 一个维护窗口，表级锁，需停机估算与回退点）。
6. **switch**：adapter 全部切到限定名 / `search_path`；跑 §9.2 全矩阵。
7. **verify**：`information_schema` 对账（对象计数、约束、索引、权限），tenant / version / idempotency 契约套件全绿。
8. **contract**：只有 verify 全绿且经过一个发布周期，才删兼容视图。**步骤 5 起即不可逆**（回退需再次 `SET SCHEMA`，且期间经视图写入的路径要人工核对）。
9. 每步都必须能在独立库上重放（§9.1 fixture 规范），且 baseline `001..019` 依然不动——搬迁写成 `migrations/modules/<ns>/NNN` 新 revision，不是改历史。

### 6.4 A vs B 对比

| 维度 | 方案 A | 方案 B |
|---|---|---|
| 满足 G-203 字面条件 | 是 | 是（超额） |
| 单模块冷安装（不建他人表） | 否（依赖 host baseline revision 19，如实声明） | 是 |
| 触及既有数据 | 否 | 是（不可逆搬迁） |
| 改动 adapter SQL | 0 处 | 73 处 grep 命中（仅 IAM 三适配器，§5.6 口径） |
| 需要新授权 | 表创建权限（已有） | schema + grant + 停机窗口 |
| 与 `search_path` 缺位现状的关系 | 无 | 需先建立连接配置基线 |
| 可分轮交付 | 是（S1..S6） | 只能在 A 之后，且每步需授权 |
| 建议 | **Wave 5B-1 做这个** | 记为 G-203b，独立 ADR + 独立 Wave |

---

## 7. 最小方案能否关闭 G-203：逐子句核对（§4.4 第 5 项）

原文（`docs/reviews/2026-09-24-saas-align-module-packaging.md:316-320`）拆成 5 个子句：

| # | 原关闭条件（逐字子句） | 方案 A 的判据 | 可关闭轮次 |
|---|---|---|---|
| C1 | "pilot 模块的迁移落 `<namespace>` 子目录" | `migrations/modules/identity-management/`、`migrations/modules/policy/` 存在且被门禁与工件引用；`migrations/*.sql` 冻结 | Wave 5B-1 |
| C2 | "+ per-namespace `MANIFEST.sha256`" | 每目录一份，`Assert-MigrationManifest` 逐个调用；runner 运行时二次校验 | Wave 5B-1 |
| C3 | "`MigrationRevision.set_digest` 钉定" | 工件 `release.json.migration_revision.set_digest` 覆盖 namespace 文件 + 声明的 baseline 依赖（§5.7） | Wave 5B-1 |
| C4 | "平面序列不重写（历史保持）" | `001..019` 与 `migrations/MANIFEST.sha256` 字节不变；V4 用前后 dump 断言 | Wave 5B-1（结构性满足） |
| C5 | "必需测试：per-namespace sha 校验 + release identity 钉定测试" | §9.2 V11/V12 + §8 测试清单 | Wave 5B-1 |
| 影响句 | "多模块共存时无法证明单模块迁移可独立演进/校验" | namespace revision 的**安装/升级/校验**独立成立；但**既有 13 张表**仍由共享 revision 19 创建（§3.5 硬事实） | **部分** |

判定：**方案 A 满足 C1..C5 的字面要求，但不满足影响句中"单模块可独立安装"的强读法。** 按评审 §4.4 第 5 项"不能就给出完整过渡步骤，不把较小方案直接叫闭环"：

- 提出判据修订（**由独立审阅 / 计划 owner 裁决，本文不自采纳**）：
  - **G-203a 模块迁移 namespace 隔离**：C1..C5 + §9.2 V1–V12 全绿 ⇒ 可关闭；
  - **G-203b 既有共享表的物理隔离与单模块冷安装**：§6.3 全序列 + 授权 + 独立 ADR ⇒ 在此之前 identity/policy 的 `independent_install` 评估恒为 `BLOCKED`，**R2 继续 BLOCKED**。
- 不允许的写法：以方案 A 宣称"G-203 已关闭"或"identity 已可独立安装"。

---

## 8. 逐文件实施清单与提交切片（§4.4 第 9 项）

> 每个切片 = 一个本地提交，切片内测试先红后绿；除 S5 外全部离线可跑。SP-1/SP-2/SP-5/SP-6/SP-7 **不混入**（评审 `:96`）。

**S1 catalog 布局与门禁（零运行时行为变更）**

| 文件 | 变更 |
|---|---|
| `migrations/modules/identity-management/catalog.json`、`…/policy/catalog.json` | 新增（§5.2），初始无 SQL 文件 |
| `migrations/modules/identity-management/MANIFEST.sha256`、`…/policy/MANIFEST.sha256` | 新增（当前各 0 行，语法沿用 `<hex64><2 spaces><file>`） |
| `scripts/check-architecture.ps1` | ① `Assert-MigrationManifest` 增两个目录（接 `:678-683`）；② 新门禁：`migrations/*.sql` 文件名集合必须等于固定 19 项清单（禁新增）；③ 每 namespace `catalog.json` 的 schema / `sql_schema=public` / `owner` 合法性；④ `Assert-NotContains`（现 `:52`）扩到 `crates/**/tests` 禁第二处 `sqlx::migrate!`，同时把 `crates/messaging/tests/outbox_reliability.rs:27` 改为复用 `runtime_migration::MIGRATOR`（修 P-4）；⑤ module-owned catalog 内 SQL 禁止出现非 owner 表名（白名单来自 §3 归属表） |
| `scripts/test-plan0014-wave5b.ps1`（新） | 对上述 5 条各一个正/负例 |

**S2 namespace catalog 与 ledger 内核**

| 文件 / 符号 | 变更 | 测试 |
|---|---|---|
| `crates/runtime-migration/build.rs`（新） | 遍历 `migrations/modules/*/`，生成嵌入表 + sha256 | `catalog_entries_are_ordered_and_unique_per_namespace`、`manifest_covers_every_embedded_file`、`namespace_names_match_catalog_json` |
| `crates/runtime-migration/src/lib.rs` | 新增 `NamespaceCatalog`、`namespaces()`、`enum NamespaceError{ChecksumMismatch,OrphanRevision,RevisionGap,BaselineNotApplied}`、`apply_namespace()`、`verify_namespace()`；`MIGRATOR` / `classify` 保持不动 | 离线：`gap_in_revisions_is_rejected`、`unknown_namespace_is_rejected` |
| `crates/runtime-migration/tests/namespace_ledger.rs`（新，`#[ignore = "requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored)"]`） | PG 行为 | `empty_ledger_records_every_revision`、`rerun_is_a_no_op`、`checksum_drift_fails_closed`、`orphan_row_fails_closed`、`revision_gap_fails_closed`、`concurrent_runners_apply_once`、`failed_revision_leaves_no_row`（fail-after 注入仅测试可用，生产 fail-closed，AGENTS §15 PLAN-0004 规则） |

**S3 CLI 与 readiness**

| 文件 / 符号 | 变更 |
|---|---|
| `apps/migration/src/main.rs` | `parse_arguments`（`:63-73`）支持 `--namespace`（可重复）与 `verify`；`up` 语义 = baseline + 全部 embed namespace；`run_postgres_status`（`:98-144`）按 namespace 输出并把落后改为非 0 退出；新增与 `:247-287` 同风格的 fail-closed 谓词 |
| `apps/migration/tests/namespace_selection.rs`（新） | ignored PG 套件：`identity_only_selection_writes_only_identity_rows`、`unknown_namespace_is_refused`、`baseline_below_required_head_is_refused` |
| `apps/business-api/src/state.rs:233-254` | readiness 追加每 namespace `verify_namespace` 结果；不一致 → `NotReady`（只读，不迁移） |
| （D-2 通过时）`apps/business-api/src/composition.rs:126`、`apps/governance-worker/src/main.rs:50` | 移除启动 `MIGRATOR.run`，改只读 verify；复核 `deploy/demo/docker-compose.yml:48-54,90,113,135` 已先跑 migration 单元 |

**S4 发布工件绑定**

| 文件 / 符号 | 变更 |
|---|---|
| `tools/module-release-tool/src/spec.rs` | `ReleaseSpec.requires_host_baseline`（结构 `:142-184`）+ `RELEASE_SPEC_SCHEMA_VERSION` → `…release-spec.v2`（`lib.rs:63`）+ `validate_against`（`:214-273`）新增校验 |
| `tools/module-release-tool/specs/identity.json` / `policy.json` | postgres `migration_sets[]` 改指 `migrations/modules/<ns>`（`module_owned: true`、`ledger_table: platform_module_migrations`）+ `requires_host_baseline: 19`；SQLite set 原样保留 |
| `tools/module-release-tool/src/migrations.rs` | `MIGRATION_MAPPING_SCHEMA_VERSION` → `…migration-mapping.v2`（`:34`）；`BackendAssessment`（`:65-74`）拆 `namespace_isolation` / `independent_install`（改写 `:307-338`）；新增 host baseline context 记账常量 + 测试 `host_baseline_accounting_matches_tables_created_by_baseline_catalog`（从 `019` 文本对账，使 organization 显式记账，§3.6） |
| `tools/module-release-tool/tests/*` | `namespace_catalog_is_claimed_in_full`、`shared_baseline_claim_still_refuses_module_owned`（保留现 `pilot_candidates.rs:306,329` 语义）、`host_baseline_dependency_is_pinned_in_set_digest`、`independent_install_stays_blocked`、`package_and_contract_digest_unchanged_by_namespace_split` |

**S5 真实 PostgreSQL 验收轮（需授权）**：执行 §9.2，产出 `docs/reviews/2026-10-XX-wave5b-gates/`。

**S6 文档同步**：`ARCHITECTURE_STATUS.md`、`docs/README.md`、`PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`（迁移单元与 ledger 段落，AGENTS §14 要求同变更更新）、`ADR-0029` 状态推进、准入矩阵 G-203a/G-203b 行。

**明确排除**：SQLite catalog 目录化/重命名（SP-4 后半）、`search_path` 与角色拆分（D-5）、`platform_module_migrations` 之外的新表、任何 `DROP`、`SAAS_MODULE_STANDARD` 文本修订（SP-7）、SP-1/SP-2/SP-5/SP-6/SP-7。

---

## 9. 真实验收矩阵与命令（§4.4 第 10 项）

### 9.1 fixture 规范（无授权即 BLOCKED）

- 只允许在**运维显式授权**的本地 PostgreSQL 服务上创建专用空库 `plan0014_wave5b_<runid>`（沿用 Wave 3 `plan0014_wave3_r2`、Wave 4 `plan0014_wave4` 惯例），只允许 DROP `plan0014_wave5b_%`。
- 禁止触碰 `contract_management_*`；禁止复用 legacy `TEST_POSTGRESQL_URL` / `TEST_POSTGRESQL_RESTORED_URL` 目标；禁止猜测凭证（`config/default.toml:12-14` 只是开发默认值，不作为连接依据）。
- 连接方式：`MIGRATION__DATABASE__URL`（`apps/migration/src/config.rs:15-26`，与 legacy `DATABASE_URL` 互斥）与 `#[ignore]` 测试惯例的 `DATABASE_URL`（`apps/migration/tests/migration_test.rs:11-19` 的 sibling-DB 模式）；两者只以 redacted 形式进日志（`crates/runtime-config/src/secret_url.rs:80-101`）。
- 未取得授权 ⇒ 本节全部 **BLOCKED**，不得用 SQLite / 离线证据替代。

### 9.2 矩阵

```powershell
# 前置（操作员执行，本轮未执行）
$env:MIGRATION__DATABASE__URL = '<operator-authorized postgres url for plan0014_wave5b_<runid>>'
$env:DATABASE_URL            = '<same target, 供 #[ignore] 测试>'
```

| # | 验收 | 命令 | 期望 | 现在能执行？ | 诚实依赖 |
|---|---|---|---|---|---|
| V1 | 单 namespace 子集安装 | `cargo run -p migration -- --backend postgres --namespace identity-management up` | exit 0；ledger 只有 `identity-management` 行；`_sqlx_migrations` 1..19 | 否（需 S2/S3） | 依赖 baseline revision 19，须在工件中如实声明 |
| V2 | policy 子集安装 | `cargo run -p migration -- --backend postgres --namespace policy up` | 同上（policy 段落只引用自身表） | 否（需 S2/S3） | 同 V1 |
| V3 | 共存 + 幂等重跑 | `cargo run -p migration -- --backend postgres up`（连续两次） | 两次 exit 0；第二次零写入；两 namespace 行互不覆盖 | 否（需 S2/S3） | 需 `depends_on` 拓扑序 |
| V4 | 既有宿主升级不动历史 | base SHA 建库 → 候选 SHA `up` → 比对 `_sqlx_migrations` dump | version / description / checksum 集合完全相同 | 否（需 S3 + 授权） | 需两个 SHA 的 worktree |
| V5 | 空库全量安装 | 空库 `up` | 全部 baseline 表 + namespace 表存在 | 部分（今天 `up` 已能装 baseline；`apps/migration/tests/migration_test.rs:132-155` 为 ignored 证明） | 今天只能证明 baseline，不证明 namespace |
| V6 | checksum 漂移 fail-closed | `UPDATE platform_module_migrations SET checksum = repeat('0',64) WHERE namespace='policy'` 后 `up` | exit 1，报 `checksum-mismatch:<ns>:<rev>`，不改对象 | 否（需 S2） | ledger 表须先存在 |
| V7 | 孤儿 revision / 缺口 fail-closed | 向 ledger 插 `(policy,900,…)` / 跳号插入 | exit 1，`orphan-revision` / `revision-gap` | 否（需 S2） | 用 fixture namespace |
| V8 | 并发单 writer | 两个 `up` 进程同时起 | 都 exit 0（或后者等待）；每 `(ns,revision)` 恰一行 | 否（需 S2） | advisory lock 必须在 ledger DDL 之前 |
| V9 | 中断恢复 | fail-after 注入使某 revision 中途失败 → 重跑 | 第一次 exit 1 且无残留；第二次成功且只补该 revision | 否（需 S2） | 注入仅测试可用，生产 fail-closed |
| V10 | 契约 / 租户 / 幂等复跑 | `cargo test -p identity-postgres --all-features -- --include-ignored --test-threads=1`；`-p policy-postgres` 同；`-p organization-postgres` 同 | 全绿，证据入 `wave5b-gates/` | 是（有授权即可） | 关闭的是 adapter 证据，不是 G-203b |
| V11 | per-namespace sha 门禁 | `pwsh scripts/check-architecture.ps1`；`pwsh scripts/test-plan0014-wave5b.ps1` | 全 PASS，含 baseline 冻结与新目录断言 | 否（需 S1） | — |
| V12 | release identity 钉定 | `cargo test -p module-release-tool --all-features` | mapping v2 全绿；postgres 拆双维度评估 | 否（需 S4） | `independent_install` 必须仍 `BLOCKED` |
| V13 | 工件重建与验证 | Wave 5B 版 pilot 脚本（现 `scripts/plan0014-wave5a.ps1 -SkipOutsideConsumer` 的继任） | 两模块 verify 全 PASS；digest 变化面与 §5.7 预期一致 | 是（脚本已在仓） | 需 S1/S4 后才能比对新 digest |
| V14 | 工作区回归 | `cargo test --workspace --all-features --target-dir <隔离目录>` | 与 `c73955e` 基线一致（864 passed / 0 failed / 70 ignored / 191 suites，ignored 四类分母保持） | 是 | 默认 target 目录本机 `os error 5`，须隔离 |

### 9.3 修订后的关闭判据提案（连同 §7 一起送审）

`G-203a` 关闭 = V1、V2、V3、V4、V6、V7、V8、V9、V11、V12 全绿（其中 V1–V4、V6–V9 需数据库授权）；`G-203b` 保持 OPEN 直至 §6.3 全序列完成；`G-02` 只有 V5/V10 在授权库上真实执行后才可能收窄（70 个 `#[ignore]` 分母：PostgreSQL 38 / PG+对象存储 3 / S3-MinIO 28 / 内部模型 1）；A-12 的 PostgreSQL 部分继续 `NOT RUN`。

---

## 10. 待决策项（本轮不擅自决定）

| # | 决策 | 选项 | 建议 |
|---|---|---|---|
| D-1 | 采用方案 A / B / A→B | — | **A 先行**，B 作为 G-203b 独立轮 |
| D-2 | API 与 governance-worker 启动自动迁移是否移除（P-1，违反 `PERSISTENCE…:96-98`） | ① 保持现状；② 改只读 verify；③ 完全移除 | **②**：既对齐 Baseline，又让"namespace ledger 只有一个 writer"成为可证事实；改动小、可回退 |
| D-3 | G-203 是否按 §7 拆成 a/b | 拆 / 不拆 | **拆**；不拆则 G-203 只能保持 OPEN，5B 的隔离工作无处记账 |
| D-4 | 新 ledger 用单表 `(namespace,revision)` 还是每模块一张表 | 单表 / 多表 | **单表**：跨 namespace 状态一次可读、无动态表名、无 bootstrap 悖论；模块 SQL 不写 ledger 已保证隔离 |
| D-5 | 迁移角色 / 应用角色拆分（`SECURITY…:251-260`） | 本轮做 / 独立轮 | **独立轮**：牵动 runbook 与 compose，不与隔离设计捆绑 |
| D-6 | ADR-0029 范围 | 只覆盖 catalog+ledger+runner / 也覆盖 schema 搬迁 | **只覆盖前者**：方案 B 含不可逆授权，应另立 ADR |

---

## 11. 本轮执行状态

- 本文与 `docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md`、`docs/adr/ADR-0029-module-namespaced-migration-catalogs-and-ledger.md`（Proposed）为纯文档交付；未修改 `migrations/*.sql`、`MANIFEST.sha256`、`crates/runtime-migration/src/lib.rs`、`apps/migration/**`、任何模块适配器、release spec、`scripts/check-architecture.ps1` 或 Wave 5A 证据目录。
- 未连接、未执行任何数据库操作（含只读 SQL）。
- Rust workspace 门禁：**本轮未运行，产品代码无改动**（`cargo fmt --check` / `cargo check` / `clippy -D warnings` / `cargo test` 均沿用 `c73955e` 的记录，不冒充本轮执行）。
- §9.2 全部真实数据库项：**BLOCKED（无数据库授权）**；V5、V10、V13、V14 本轮同样 **NOT RUN**（本轮不做仓库范围构建/测试重跑，纯文档变更）。
- 状态：**WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；WAVE 5B-0 DESIGN READY FOR REVIEW；R2 BLOCKED**。
