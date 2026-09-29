# PLAN-0014 Wave 5A — 本地 R2 发布候选工件：实施记录（编码前）

> 文档 ID：REVIEW-PLAN0014-WAVE5A-2026-09-29
> 类型：执行计划 + 设计边界记录（**编码前形成**；执行结果见 closeout 提交与本文件 §13）
> 分支：`codex/plan-0014-wave5a-local-release-pilot`
> 起点（已核实）：`9b05ccddb4964b363b0c9ae001c4303c1b7393db`
> 独立审阅栏：`<pending>`（实现者不自行填写 PASS）

## 0. 起点核实与本轮边界

| 项 | 实测值 |
|---|---|
| 起点 HEAD | `9b05ccddb4964b363b0c9ae001c4303c1b7393db`（＝任务预期基线，逐字符一致） |
| 起点分支 | `codex/plan-0014-wave4-reference-conformance`（Wave 4 Round 3 closeout 所在分支） |
| 工作树 | `git status --porcelain` 无输出（干净），无 stash |
| 本轮分支 | `codex/plan-0014-wave5a-local-release-pilot`，由上述 HEAD 直接创建 |
| 工具链 | `cargo 1.94.1 (29ea6fb6a 2026-03-24)`、`rustc 1.94.1 (e408947bf)`、`rust-toolchain.toml` channel `1.94.1` |
| 主机日期 | 2026-09-29，早于 PLAN-0014 §6 Wave 5 原日程（2026-10-18）；按「Wave 5A 提前启动」记录，不改写日程表 |

本轮**只做**：本地候选工件、真实构建与验证、文档与证据。

本轮**不做**（结果再好也不越界）：

- 不 `push`、不 `merge`、不 `publish`、不打正式发布 tag、不部署；
- 不把任何模块成熟度改为 R2（`SAAS_MODULE_STANDARD` §18 与 `ARCHITECTURE_STATUS` 保持 R1 判定，只追加候选事实）；
- 不接受 ADR-0027 / ADR-0028（保持 Proposed / NOT ACCEPTED），不激活 PLAN-0006；
- 不实现 Document ACL、Notification 渠道、通用日志脱敏；
- 不修改 `crates/business-module-contracts` 与 `crates/business-application-compiler` 的任何契约、字段、摘要算法或公开签名；
- 不修改历史迁移、不复制脱离正式 catalog 的 SQL；
- 不修改 Accepted ADR，不修改 `SAAS_MODULE_STANDARD`（Accepted 标准文本修订属设计提案，§12）。

## 1. 承前独立审阅结论的记录（不得扩大）

操作者交给本轮的上一轮独立审阅结论，原文记录：

> **Round 3 修复通过；Wave 4 有界收口；既有 C1、R2 阻塞与未执行项继续有效。**

使用方式与限定：

1. 该结论属 **Wave 4 Round 3**（Wave 4 起点 HEAD `7fb9f735a7e0a787666d7b62592f08ac5161245a`，代码候选 `19fa60238e316da386209d83e92c04c920760420`）。本轮不把它扩大为仓库级、发布级、生产级或 R2 级验收。
2. 「既有 C1、R2 阻塞与未执行项继续有效」在本轮的具体含义：Wave 1 的 **G-01 / G-02**、Wave 2 的 **G-201 / G-202 / G-203**（C1）与 Wave 4 的 **D-W4-02/06/08/09/11、OBS-W4-05/06/07、N-G05**（C1）全部仍然开放。本轮不宣称任何一条关闭；G-201/G-202/G-203 只允许记「新增证据 / 残项收窄」，判定权在独立审阅。
3. 本轮不在任何评审文档的独立评审栏填 PASS；Wave 4 两份评审与本记录自身的独立栏均为 `<pending>`。

## 2. 权威依据与约束力最强的既有规则

已读：`AGENTS.md`（§1/§4/§9/§12/§14/§16）、`PLAN-0014` §4 P0-A/P0-B、§6 Wave 5、§8 T0–T7、§9、§10、§13、`SAAS_MODULE_STANDARD`（§3/§4/§5/§8/§9/§13/§14/§17/§18）、`INTER_MODULE_COMMUNICATION_STANDARD`（§6/§11）、`docs/reviews/2026-09-24-saas-align-module-packaging.md`（§1/§4/§5/§6/§7/§8/§9/§10/§11/§12/§14/§16）、`docs/reviews/2026-09-23-saas-align-identity-organization-policy.md`（§7/§11）、ADR-0020/0021/0022/0025（Accepted）与 ADR-0027/0028（Proposed）、`crates/business-module-contracts/src/{lib,manifest,release}.rs`、`crates/business-application-compiler/src/lib.rs`、`scripts/check-architecture.ps1`、`migrations/MANIFEST.sha256`、`crates/{identity,policy,organization}-sqlite/migrations/MANIFEST.sha256`。

| # | 出处 | 原文要点 | 本轮遵守 |
|---|---|---|---|
| A1 | `SAAS_MODULE_STANDARD` §8 Package digest | 编译/发布产物必须有 deterministic digest，部署/升级/审计引用 digest 而非可变 tag | §4.3/§4.4 定义工件字节摘要 + 重复构建一致性 |
| A2 | Wave 2 review §5 表 + §16 F-5 | `package_digest` = **compiled declaration fingerprint**，「不是 tarball/发布工件字节；工件字节完整性 digest 属 Wave 5」 | `release.json.package_digest` 保持声明指纹语义；工件字节摘要**另立名字**，不塞进 `ModuleReleaseIdentity` |
| A3 | Wave 2 review §5 规则 3（closeout 修正） | 「contract_digest 相同仅表示纳入摘要的公开声明面相同；compatibility、dependencies、module_version 仍须通过 compile/dry_plan 的独立兼容性校验，不能据摘要相同推断可互换」 | 兼容性一律由 `compile`/`dry_plan` 真实判定（§8.1 C 组） |
| A4 | `release.rs` 模块注释 + review §5 | `ModuleReleaseIdentity::validate` 只是字段形状校验，**不得描述为一致性验证**；一致性入口是 `verify_release_identity` | 工具的一致性判定只走 `verify_release_identity`；文档不把 shape validate 写成校验通过 |
| A5 | review §8 G-201 残项 | 「真实模块 manifest 的 release identity 由 release tool 实际组装并钉入 pilot 工件（Wave 5 关闭）」 | 工具从真实模块声明组装 identity 并写入工件 |
| A6 | review §8 G-202（R2 BLOCKER） | 关闭条件「pilot 工件带 release identity 且外部工程仅依赖发布产物编译/测试通过」 | §4.8 外部消费者 + §8.2 真实构建 |
| A7 | review §8 G-203 + D-02 | pilot 迁移落 `<namespace>` 子目录 + per-namespace `MANIFEST.sha256`，`MigrationRevision.set_digest` 钉定；**平面序列不重写（历史保持）** | §6 只读映射 + 校验归属/顺序/checksum；不改写历史 |
| A8 | review §1/§9/§16 | 三层证据禁止互相推断；需真实发布工具、workspace 外消费者、真实 PostgreSQL 的验收项「一律记 NOT RUN，不得写 PASS」；「SQLite 与 mock 不替代任何 PostgreSQL 结论」 | §13 NOT RUN 表；本轮不执行 PG lane，也不冒充 |
| A9 | PLAN-0014 §10 第 4/9 条 | R2 admission 需 C0=0、C1=0 且独立 reviewer PASS | identity/policy 均带未关闭 C1 → 本轮**不得**宣布 R2 |
| A10 | PLAN-0014 §8 T7 | Release compatibility = package build、manifest/digest、old/new contract fixture、migration upgrade、rollback/forward-only、consumer compile/test | §8 按这六行逐项对齐（§14 记录 Wave 2 文档的 T 级标签错位） |
| A11 | `AGENTS.md` §16 / ADR-0020 | 两个纯 crate 不得依赖文件系统、打包、数据库、供应商 | 工具落在新 crate（`tools/`），二者零改动；fitness 增加纯度守卫（§7 N 项） |
| A12 | `AGENTS.md` §12 | 无法运行时明确记录 NOT RUN / BLOCKED | §13 逐条绑定原因 |

### 2.1 权威未定义、本轮认领新定义的部分

`2026-09-24-saas-align-module-packaging.md` **没有**定义任何确定性打包规则（成员顺序、路径规范化、时间戳、mode/uid、字节级可复现性）；其确定性主张全部停留在「对规范化声明求摘要」层（§5「哈希输入是规范化声明，不是工件字节」）。`AGENTS.md` §16「输出必须稳定排序并带 canonical digest」约束 compiler 输出，不外推到工件字节。

故 §4.4 的规范化规则属**本轮新定义**：先在工具与本记录落地并接受审阅；若被接受，再按 §12 SP-7 提案并入 `SAAS_MODULE_STANDARD` §13。本轮不自填 Accepted 标准文本。

## 3. 试点选择表

PLAN-0014 §6 pilot 候选三个：`identity`、`policy`、`business-module-contracts`/compiler release bundle。逐项核对 Wave 5 的 R2 条件（独立版本、manifest、contract artifact、package digest、compatibility、migration mapping if persistent、changelog、contract tests、release artifact、consumer builds without concrete adapter dependency）。

### 3.1 事实基线（只列实测，not found 明确标注）

| 维度 | identity | policy | contracts/compiler bundle |
|---|---|---|---|
| 模块 crate | `crates/identity`（bc/data_owner `identity-management`，layer `domain-and-application`） | `crates/policy`（bc/data_owner `policy`） | 两个 crate 同属一个 platform foundation |
| 核心 crate 依赖 | chrono / uuid / async-trait / thiserror / sha2 | chrono / uuid / async-trait / thiserror | semver / serde / thiserror（+ serde_json / sha2） |
| 含 sqlx / axum / 供应商 SDK | 否 | 否 | 否 |
| 适配器 | `identity-sqlite`（`migration_catalog=identity-sqlite`）、`identity-postgres`（`runtime-migration`） | `policy-sqlite`（`policy-sqlite`）、`policy-postgres`（`runtime-migration`） | 无 |
| 独立版本现状 | `version.workspace = true` → `0.1.0`；非 workspace 版本 **not found** | 同 | 同 |
| 真实 `BusinessModuleManifest` | **not found**（全仓只有 synthetic / `sales` / `module-core` fixture） | **not found** | not found（它是 manifest 的**类型**权威，自身无 manifest） |
| 公开契约载体 | 模块 crate 自身（`ports`/`application`/`domain`）；`identity-authorization-contracts` 是跨 identity/organization/policy 的**适配器行为契约套件**，仅作 dev-dependency | 同 | Rust API 即公开面 |
| 已发布事件 | **not found**（grep 无匹配） | **not found** | 无 |
| 持久事实 | `platform_users`、`external_identities`、`tenant_memberships`、`identity_idempotency`、`platform_bootstrap_executions` | `permission_definitions`、`roles`、`role_permissions`、`role_bindings`、`policy_idempotency` | 无 |
| SQLite 迁移 | `crates/identity-sqlite/migrations/001_identity_foundation.sql` + 独立 `MANIFEST.sha256` + 独立 ledger 表 `identity_migrations` | `crates/policy-sqlite/migrations/001_policy_foundation.sql` + 独立 `MANIFEST.sha256` + `policy_migrations` | 不适用（非持久化 → `migration_revision: None`） |
| PostgreSQL 迁移 | 共享 `migrations/019_identity_authorization_foundation.sql`（同文件含 identity/organization/policy 三段，同 `public` schema） | 同一文件 | 不适用 |
| 未关闭 C1 | G-01、G-02、G-201（残项）、G-202、G-203 | 同 | G-201、G-202、G-203 |
| 是否被既有契约/ADR 阻塞 | **否**（无 Accepted ADR 或既有契约禁止为其声明 manifest / 组装工件） | **否** | 无阻塞，但见 §3.2 第 3 条计数纪律 |

### 3.2 结论

1. **两个试点 = `identity` 与 `policy`**，满足 PLAN-0014 §6「At least two modules」。理由：核心 crate 纯 Rust、无 DB/供应商依赖（可真实打包 + 真实编译 + 真实调用公开契约）；均有真实持久事实（可真实检验迁移映射，而非演示）；正是 G-201/G-202/G-203 需要证据的形状。
2. **不需要改用 contracts/compiler bundle 顶替**：本轮未发现任何既有契约或 Accepted ADR 阻塞这两个模块的 manifest 声明与工件组装（§3.1 末行）。
3. **bundle 计数纪律**：`business-module-contracts` 与 `business-application-compiler` 是同一 foundation 的两个 crate；若日后登记为模块，那是**一个**模块（`ModuleReleaseBundle.crate_versions` 映射两个 crate），**不得**计作两个独立模块。本轮不为它出工件，避免用 bundle 凑「两个模块」。
4. **`organization` 不入本轮**：crate 形状与 identity 同构，但不是 PLAN-0014 §6 列出的候选；扩范围只会稀释试点证据。
5. **两试点仍不满足 R2**：R2 admission（PLAN-0014 §10）要求 C1=0 + 独立 reviewer PASS；两者各带 G-01/G-02/G-201/G-202/G-203。本轮产出是**候选工件 + 证据**，终态 §11。
6. **预先声明的结构性限制**：PostgreSQL（生产权威，`SAAS_MODULE_STANDARD` §9）上 identity 与 policy 的表位于同一共享文件 `019`、同 `public` schema，且 `runtime-migration` embed 全目录 → 「单模块在 PostgreSQL 上独立安装/升级」本轮**不可能**证明；如实产出映射并保留 R2 BLOCKED（§6.4），不伪造 per-module SQL。

## 4. 候选工件格式（编码前冻结）

### 4.1 工具位置与纯度

- 新工具 crate：`tools/module-release-tool`（package `module-release-tool`，binary `module-release`），注册为 workspace 成员 → 受同一 fmt/check/clippy/test 门禁覆盖。
- 位置理由：任务允许「外层 scripts/tools 或合适的工具 crate」；`scripts/` 只承载 PowerShell/Shell，放入 Rust crate 会让脚本目录变成混合语义，故取「外层工具 crate」形态置于 `tools/`；置于 `crates/` 之外以免被 Platform Core 目录规则误分类。
- 依赖：`business-module-contracts`、`business-application-compiler`、`serde`、`serde_json`、`sha2`、`semver` + `std::fs`。**不**引入 sqlx/axum/reqwest/对象存储/messaging/AI provider，也不依赖任何业务 crate 或 `apps/*`（fitness 守卫，§7 N）。
- `business-module-contracts` / `business-application-compiler` **零改动**：不新增 seam、不改 digest。需要「编译器实际哈希的字节」时采用**同构 struct 复现 + 漂移守卫测试**（§4.3）；把 seam 暴露进纯 crate 记 §12 SP-2 提案。

### 4.2 工件目录布局

```text
<module-id>-<module-version>/                 # 例 identity-0.1.0-rc.1
├── package/                                  # ← L1 payload：唯一被字节摘要覆盖的部分
│   ├── module.manifest.json                  # BusinessModuleManifest（typed 序列化，canonical）
│   ├── contract/contract-surface.json        # 编译器实际哈希的公开契约面字节（§4.3）
│   ├── contract/packages/<crate>/Cargo.toml  # 独立版本化后的 crate manifest（§4.7）
│   ├── contract/packages/<crate>/src/**      # 该 crate 全部 .rs 源（公开契约的真实载体）
│   ├── migrations/migration-mapping.json     # 迁移映射（§6）
│   └── CHANGELOG.md                          # 本候选的真实变更说明
├── artifact-manifest.json                    # L2：文件清单 + 工件字节完整性摘要 + 排除声明
├── release.json                              # L2：ModuleReleaseIdentity（module-release.identity.v1，8 字段）
├── release-bundle.json                       # L2：ModuleReleaseBundle（identity + crate_versions 审计映射）
└── verification.json                         # L3：验证结果索引（验证之后才写）
```

### 4.3 四类摘要各自覆盖什么（禁止混用）

| 摘要 | 位置 | 覆盖 | 不覆盖 | 出处 |
|---|---|---|---|---|
| **contract digest** | `release.json.contract_digest`；同时等于 `package/contract/contract-surface.json` 的文件字节 SHA-256 | 编译器 canonical 公开契约面：`module_id`、`data_classification`、`published_commands/queries/events`、`resource_kinds`、`semantic_contributions`、`agent_tool_contributions`、`extension_points`（normalize 之后） | `module_version`、`dependencies`、`compatibility`、UI 声明、消费者侧 contribution、任何工件字节 | A2/A3/A4 + `contract_digest()` seam |
| **package digest** | `release.json.package_digest` = `package_fingerprint(compiled package)` | compiled declaration fingerprint：整个规范化声明（含发布元数据与消费者侧声明），与 planner `AddModule.digest` 同值 | 工件字节、持久化、构建输入 | A2（Wave 2 F-5 修正后的语义，本轮不改） |
| **artifact integrity digest** | `artifact-manifest.json.artifact_integrity_digest`（工具新定义，**不属于** `ModuleReleaseIdentity`） | `package/` 下每个文件的规范化字节 SHA-256 + 相对路径（§4.4），即物理工件内容 | L2/L3 三个描述性文件本身（在 `excluded_members` 中显式声明） | A1 + A2「工件字节摘要属 Wave 5」 |
| **migration set digest** | `release.json.migration_revision.set_digest` | 正式 catalog 中该模块相关迁移的 canonical 清单：namespace + 每条 `(backend, file, sha256)`（§6.2） | 迁移执行结果、SQL 语义正确性、独立安装可行性 | A7 + `MigrationRevision` |

`contract-surface.json` 的关键性质：写出的字节**逐字节等于**编译器哈希输入，因此第三方工具（`sha256sum` / `certutil`）对该单文件求 SHA-256 应等于 `contract_digest`。漂移守卫测试断言该等式；等式破坏即视为工具与纯 crate 脱钩的缺陷，修工具，不改纯 crate。

### 4.4 确定性规范化规则（本轮新定义，见 §2.1）

规则前缀写进被摘要内容（`plan-0014.artifact-integrity.v1` / `plan-0014.migration-set.v1`），改规则必然改摘要，因此会被重复构建测试观察到。

1. **成员集合**：仅 `package/` 下普通文件；目录不成成员；空目录不编码；符号链接或越界路径**拒绝**而非跳过。
2. **路径**：相对 `package/` 的 POSIX 形式，分隔符 `/`；禁止绝对路径、盘符、`.`、`..`、反斜杠；大小写原样。
3. **排序**：相对路径 UTF-8 **字节序**升序（与 locale、目录遍历顺序、文件系统无关）；同一路径不得重复。
4. **文本规范化**（求字节摘要之前）：非法 UTF-8 拒绝；删 BOM；`\r\n` 与孤立 `\r` → `\n`；末尾缺 `\n` 则补一个（不折叠已有空行，避免改变语义）。这直接吸收本仓 `core.autocrlf=true` 的跨机换行差异（Wave 3 Round 2b 已记录 CRLF 入库现象）。
5. **逐条编码**：`"<sha256_hex(规范化字节)> <SP> <relative-path> \n"`，按序拼接。
6. **总摘要**：`SHA-256( "plan-0014.artifact-integrity.v1\n" ‖ 全部条目 )`，小写 hex。
7. **禁止项**：工件内任何文件都不得含输出目录绝对路径、机器名、用户名、当前时间/日期、随机 UUID、`target/` 路径或凭据。时间戳与 RUN_ID 只存在于**工件之外**的证据日志中。
8. **JSON 规范化**：typed struct 的 serde_json 字段顺序即规范顺序（与 compiler 同法）；工具自有 map 一律 `BTreeMap`；L1 JSON 不 pretty-print（美化版只出现在证据目录副本）。
9. **migration set 编码**：`"plan-0014.migration-set.v1\n" + "namespace=<ns>\n" + 按 (backend, file_name) 字节序的 `"<backend> <file_name> <sha256_hex>\n"``。

### 4.5 摘要自引用的规避

顺序严格如下：

1. 先物化并冻结 **L1 `package/`**（manifest、contract surface、packed crate、migration mapping、changelog）；
2. 再计算并写 **L2** `artifact-manifest.json`（只描述 L1；工具按规则拒绝把 L2/L3 文件计入摘要，并在 `excluded_members` 列出被排除文件与理由——排除是显式声明，不是事后忽略）；
3. `release.json` / `release-bundle.json` 只钉**声明摘要**与源 revision，不引用 `artifact-manifest.json` 的字节，故不存在「摘要覆盖自身所在文件」的循环；
4. `verification.json` 属 **L3**，验证跑完后写，明确排除在两个摘要之外。

不使用随机 UUID、当前时间或机器名充当稳定 release identity；稳定身份 = `module_id` + `module_version` + `source_revision` + 四类摘要。

### 4.6 版本来源（唯一、稳定、非随机）

| 版本 | 值 | 来源 |
|---|---|---|
| module 独立 release version | `0.1.0-rc.1`，各试点在 `tools/module-release-tool/specs/<module>.json` 显式声明 | 本轮人工声明；pre-release tag 表达「候选，非 GA」 |
| crate version（bundle 映射） | 与 module version 同值，写入打包后的 `Cargo.toml` | §4.7 打包变换；`ModuleReleaseBundle.crate_versions` 记录 |
| workspace/产品 version | `0.1.0`（root `[workspace.package]`）**不变** | `SAAS_MODULE_STANDARD` §8 明令分离；工件不把工作区版本当模块版本 |
| `platform_version`（compile 输入） | `0.1.0` | 取自 workspace 产品版本，作为兼容性判定的平台坐标 |
| 第三方依赖版本 | `=x.y.z` 精确锁定，逐个取自 `Cargo.lock` | 解析确定，不隐式漂移 |
| `source_revision` | `git rev-parse HEAD`（运行时读取，40 hex 小写，经 `SourceRevision` 校验） | 稳定 identity，而非可变分支/tag 名 |

`0.1.0-rc.1` 的一个真实 semver 后果必须记录并测试：`VersionReq::parse("^0.1.0")` **不**匹配 `0.1.0-rc.1`。任何声明依赖候选版本的兼容窗口必须显式带 pre-release；该行为作为断言固化（§8.1 C-5），不靠巧合通过。

### 4.7 打包变换（crate 从属于工作区 → 独立）

只允许以下四类确定性变换，全部记录在 `artifact-manifest.json.packaging_transforms`：

1. `version.workspace = true` → `version = "<module version>"`；
2. `edition/rust-version/license` 的 `.workspace = true` → 具体值（`2021` / `1.94.1` / `MIT`，取自 root `[workspace.package]`）；
3. `dep = { workspace = true }` → `dep = "=<Cargo.lock 精确版本>"`（保留原 feature 列表）；
4. 删除 `[dev-dependencies]` 与 `[lints] workspace = true`：dev-dependencies 不属发布契约面（且其测试源未打入工件，会指向缺失资源），`[lints]` 属仓库内部编码门禁。删除动作与理由写进工件，可审计。

打包成员固定为 `Cargo.toml` + `src/**/*.rs`。**不**打包 `tests/`、`benches/`、`migrations/`、`README`，不生成 `Cargo.lock`（工件是库 crate，非最终可执行体）。

### 4.8 workspace 外消费者

- 源模板 checked-in：`tools/module-release-tool/consumers/{identity,policy}/`，登记进 `[workspace] exclude`，确保不是 workspace 成员。
- 构建时由编排脚本复制到**仓库外**临时目录，并把 Cargo.toml 中占位 `__ARTIFACT_CRATE_DIR__` 替换为工件内 `package/contract/packages/<crate>` 绝对路径。
- 硬性守卫（脚本内断言，失败即整轮失败）：
  1. 消费者依赖图中不存在指向 `F:\Workspace\business-platform\{crates,apps}` 的 path 依赖；
  2. 第三方依赖 `Cargo.lock` 条目均为 `source = "registry+..."`；
  3. 消费者不引用 `identity-authorization-contracts`、`public-api-contracts`、任何 `*-sqlite`/`*-postgres`/`apps/*`；
  4. 记录 `cargo tree -e features,normal` 与 `Cargo.lock` 快照，即「解析后的依赖和 feature」。
- 必须**实际调用**公开契约（不是只 `use`）：
  - identity：`TenantAccessChecker::new(fake query port).check(tenant, user)` → 断言 fail-closed `NoMembership`；seed 后断言 `Active`；
  - policy：`Authorize::new(query, subject, org).check(ctx, permission, None)` → 断言默认 DENY，seed 后 ALLOW。
  两者使用 crate 自身声明的 `testing` feature（记录在解析结果中），不引入任何具体数据库适配器或 `apps/*`。

## 5. 工具命令面

```text
cargo run -p module-release-tool -- generate --module <identity|policy> --out <dir>
cargo run -p module-release-tool -- verify   --artifact <artifact-dir>
cargo run -p module-release-tool -- compare  --left <dir-a> --right <dir-b>
```

- `generate`：读 spec → 构造 `BusinessApplicationPackage` → `compile()` → `contract_digest` + `package_fingerprint` → 冻结 L1 → 写 L2 → `verify_release_identity` 复核 → 写 L3 `verification.json`。
- `verify`：独立进程，不信任 `generate`：重算 contract digest（比对 `contract-surface.json` 文件字节 SHA-256）、从 `module.manifest.json` 反序列化 → `compile()` → 重算 declaration fingerprint、回读正式 catalog 重算迁移集摘要、重算工件字节摘要，最后跑 `verify_release_identity`。
- `compare`：只比较两个目录的 L2 摘要与清单条目集合，逐条打印差异，任一不同即非零退出。
- 任何失败一律非零退出，禁止「打印警告后返回 0」。

## 6. 迁移映射规则

### 6.1 输入（只读、权威）

- `migrations/MANIFEST.sha256` + `migrations/*.sql`（workspace 平面序列，PostgreSQL 权威；`runtime-migration::MIGRATOR` embed 全目录）；
- `crates/identity-sqlite/migrations/{001_identity_foundation.sql,MANIFEST.sha256}` + ledger 表 `identity_migrations`；
- `crates/policy-sqlite/migrations/{001_policy_foundation.sql,MANIFEST.sha256}` + ledger 表 `policy_migrations`；
- `migrations/019_identity_authorization_foundation.sql` 头部归属注释（identity-management / organization / policy 三段）；
- Cargo metadata `migration_catalog`（`identity-sqlite`、`policy-sqlite`、`runtime-migration`）。

### 6.2 映射内容

`package/migrations/migration-mapping.json`：

- `migration_namespace`：取既有权威标识 `data_owner`（`identity-management` / `policy`，来自两个 crate 的 `[package.metadata.architecture]`），不发明新名；
- 每条 entry：`backend`（`sqlite`/`postgres`）、`file`、`revision`（文件名数字前缀）、`sha256`、`digest_form`（`raw`/`lf-normalized`）、`module_owned`、`catalog`、`catalog_manifest`、`ledger_table`（SQLite）、`ownership_note`；
- 每条 sha256 必须**同时**满足：与 `MANIFEST.sha256` 记录一致，且与磁盘字节重算一致（允许 raw 或 CRLF→LF 归一化两种形态，与 `scripts/check-architecture.ps1::Assert-MigrationManifest` 同一容忍规则）；不一致 → `generate` 失败。映射因此不可能与正式 catalog 脱钩。
- `set_digest` = §4.4 第 9 条，覆盖**两个 backend 的全部条目**（含 `module_owned = false` 的共享 PostgreSQL 条目），把共享事实钉住而不是省略。

### 6.3 禁止项

不修改历史迁移文件；不修改 `MANIFEST.sha256`；不为试点新建 SQL；不把 `019` 拆成 per-module 文件（属 schema/权威变更）；不在映射里写「已验证可独立升级」这类未执行结论。

### 6.4 如实保留的 R2 阻塞（写进工件与文档）

- **PostgreSQL**：identity 与 policy 与 organization 共享 `019` 单文件、同 `public` schema、由 `runtime-migration` 全量 embed → 单模块在 PostgreSQL 上独立安装/升级不可能，也无法由本轮证明（G-203 未关闭）。
- **SQLite**：物理归属已按模块隔离（独立目录 + 独立 `MANIFEST.sha256` + 独立 ledger 表），但 `migration_namespace`（`identity-management`/`policy`）尚未落为目录层级；且 `identity-sqlite`/`policy-sqlite` 的 ledger `checksum` 列绑定的是**原始 SQL 字节而非摘要**（`crates/identity-sqlite/src/lib.rs:56`，`crates/policy-sqlite/src/lib.rs` 同形）——本轮如实记录为观察，不改行为。
- 本轮不执行任何 PostgreSQL lane（§13）。

## 7. 精确文件范围

| # | 路径 | 动作 | 说明 |
|---|---|---|---|
| A | `Cargo.toml`（root） | 修改 | members `+= "tools/module-release-tool"`；`[workspace] exclude += "tools/module-release-tool/consumers"` |
| B | `tools/module-release-tool/Cargo.toml` | 新增 | 依赖白名单见 §4.1 |
| C | `tools/module-release-tool/src/lib.rs` | 新增 | 规范化、摘要、错误类型、公共 API 文档 |
| D | `tools/module-release-tool/src/canonical.rs` | 新增 | §4.4 字节规范化 + 两类摘要 + 清单 |
| E | `tools/module-release-tool/src/spec.rs` | 新增 | spec 载入 + `BusinessApplicationPackage` 构造（每字段带 authority 出处） |
| F | `tools/module-release-tool/src/packaging.rs` | 新增 | L1 物化、L2/L3 写入、`verify`、`compare` |
| G | `tools/module-release-tool/src/migrations.rs` | 新增 | §6 catalog 读取与校验 |
| H | `tools/module-release-tool/src/main.rs` | 新增 | CLI：`generate` / `verify` / `compare` |
| I | `tools/module-release-tool/specs/{identity,policy}.json` | 新增 | 试点声明（每条带 `authority` 出处字符串） |
| J | `tools/module-release-tool/specs/{identity,policy}.changelog.md` | 新增 | 真实 changelog |
| K | `tools/module-release-tool/tests/{canonical,packaging,pilot_candidates,compatibility}.rs` | 新增 | §8.1 用例 A/C/D/E 组 |
| L | `tools/module-release-tool/consumers/{identity,policy}/**` | 新增 | §4.8 消费者模板 |
| M | `scripts/plan0014-wave5a.ps1` | 新增 | 编排：两次生成 → 比较 → verify → packed crate 真实构建 → 外部消费者复制/构建（在线 + `--locked --offline`）→ 篡改负例 → 日志与退出码入证据目录 |
| N | `scripts/check-architecture.ps1` | 修改 | 新增 fitness：两个纯 crate 依赖纯度（无 fs/打包/DB/供应商）、`module-release-tool` 不依赖 `apps/*` 与业务 crate、pilot spec 存在且 module id 唯一 |
| O | `docs/reviews/2026-09-29-wave5a-implementation-record.md` | 新增 | 本文件 |
| P | `docs/reviews/2026-09-29-wave5a-gates/**` | 新增 | §10 证据目录（closeout 提交） |
| Q | `docs/plans/current/PLAN-0014-...md`、`docs/architecture/ARCHITECTURE_STATUS.md`、`docs/README.md` | 修改 | 追加 Wave 5A 记录与导航；不删旧证据，不回填独立 PASS |
| R | `docs/reviews/2026-09-24-saas-align-module-packaging.md` | 修改（仅追加带日期标注） | A-10/A-12、G-201/G-202/G-203 处追加「Wave 5A 已产出候选工件与外部消费者证据，判定权在独立审阅」指针；原行状态与原文保留 |

明确**不在**范围：`crates/business-module-contracts/**`、`crates/business-application-compiler/**`、`migrations/**`、`crates/{identity,policy,organization}*/**`、`apps/**`、`docs/adr/**`、`docs/standards/**`、`.github/**`。

## 8. 验证命令与用例映射

### 8.1 工具与 contract/compiler 定向测试

```powershell
cargo test -p module-release-tool --all-features
cargo test -p business-module-contracts -p business-application-compiler --all-features
```

| 组 | 任务要求 | 测试 |
|---|---|---|
| A-1 | 两个不同临时目录重复生成一致 | `packaging::regenerating_in_two_temp_dirs_yields_identical_digests` |
| A-2 | 排序/遍历顺序无关 | `canonical::inventory_order_is_byte_order_not_walk_order` |
| A-3 | 换行差异不影响（autocrlf） | `canonical::crlf_and_lf_source_bytes_yield_same_digest` |
| A-4 | 无路径/时间/机器名泄漏 | `packaging::no_host_path_time_or_hostname_leaks_into_payload` |
| A-5 | 排除规则显式且被测试 | `packaging::excluded_members_are_declared_not_ignored` |
| B-1 | 每个工件可校验且来源可追溯 | `pilot_candidates::artifact_verifies_independently_and_traces_source_revision` |
| C-1 | 当前消费者读当前候选 | `compatibility::current_candidate_compiles_and_plans_add_module` |
| C-2 | 已支持的旧契约 fixture | `compatibility::previous_contract_revision_upgrades_to_candidate`（`dry_plan` → `UpgradeModule`；contract digest 相同、package digest 不同） |
| C-3 | 不兼容版本被拒绝 | `compatibility::incompatible_dependency_requirement_is_rejected`、`compatibility::platform_below_declared_minimum_is_rejected` |
| C-4 | 降级被拒绝 | `compatibility::downgrade_over_installed_higher_version_is_rejected` |
| C-5 | pre-release 兼容窗口语义 | `compatibility::caret_requirement_excludes_prerelease_candidate` |
| D-1 | 改公开契约 → contract digest 变 | `pilot_candidates::published_query_surface_change_moves_contract_digest` |
| D-2 | 改工件内容 → 校验失败 | `pilot_candidates::tampering_a_packed_source_breaks_integrity_verification` |
| D-3 | 缺文件被拒 | `pilot_candidates::missing_package_file_is_rejected` |
| D-4 | 错误模块 ID | `pilot_candidates::wrong_module_id_is_rejected` |
| D-5 | 错误版本 | `pilot_candidates::wrong_module_version_is_rejected` |
| D-6 | 错误/未知依赖 | `compatibility::unknown_or_incompatible_dependency_is_rejected` |
| D-7 | identity 一致性走真门禁 | `pilot_candidates::release_identity_passes_verify_release_identity`（正向 + 每类单字段错配） |
| E-1 | 迁移映射与正式 catalog 一致 | `pilot_candidates::migration_mapping_matches_formal_catalog_checksums` |
| E-2 | catalog 漂移 → 生成失败 | `pilot_candidates::migration_manifest_drift_fails_generation` |
| E-3 | 非持久化模块不伪造迁移 | `packaging::stateless_module_pins_no_migration_revision`（本轮两试点均持久化，该路径由工具单测覆盖） |

### 8.2 真实构建与外部消费者（脚本编排，工件落在仓库外）

```powershell
pwsh -NoProfile -File scripts/plan0014-wave5a.ps1 -OutRoot <temp-root> -EvidenceDir docs/reviews/2026-09-29-wave5a-gates
```

按序执行并逐步记录命令原文、退出码、工具链版本：

1. `generate identity` / `generate policy` → `<temp>/run-a`；
2. 再次生成两者 → `<temp>/run-b`（不同临时目录）；`compare`（A-1）；
3. 对每个工件 `verify`（独立进程复核）；
4. 每个工件**真实构建**：把 packed crate 自身作为 workspace root 执行 `cargo build`（不只是校验 manifest 形状）；
5. 复制消费者模板到 `<temp>/consumers/<module>`，替换 artifact 路径，依次：`cargo build` → `cargo test`（真实调用公开契约）→ `cargo build --locked --offline`（锁定依赖的第二次验证；失败逐字记录原因，**不**改判为成功）→ `cargo tree -e features,normal` + `Cargo.lock` 快照 + path-dependency 守卫；
6. 文件级篡改负例：追加一行到已生成工件的 `src/lib.rs` → `verify` 必须非零；删除一个文件 → 非零；改 `release.json.module_version` → 非零。三段日志入库。

### 8.3 仓库门禁（任务 §七，逐条执行）

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pwsh -NoProfile -File scripts/check-architecture.ps1
git diff --check
```

## 9. 失败条件（任一命中即本轮不成立，不得交付「就绪」）

1. 任一试点工件无法由 `verify` 在独立进程复现摘要；
2. 两个临时目录生成的 L2 摘要或清单条目不一致（且无法用已声明排除规则解释）；
3. 外部消费者出现任何指回 `crates/`、`apps/` 的 path 依赖，或依赖解析依赖工作区 feature/patch/私有路径；
4. 外部消费者未真实调用公开契约（只 `use`、只 build 不 test）；
5. 篡改、缺文件、错误 ID/版本/依赖的任一负例**通过**了校验；
6. 迁移映射 checksum 与正式 `MANIFEST.sha256` 不一致却继续生成；
7. 为满足构建而修改 `crates/identity`、`crates/policy`、两个纯 crate 或任何迁移文件；
8. 任一门禁非零退出，或用旧日志/局部 focused 结果替代全量结果；
9. 出现「打包成功 ⇒ R2」推论，或文档在独立评审栏填 PASS；
10. 为消除噪声删除门禁、弱化断言、扩大 `#[ignore]` 或缩小 `--all-features`。

## 10. 证据目录约定

`docs/reviews/2026-09-29-wave5a-gates/`：

```text
README.md                       命令↔日志↔退出码对照 + 结论边界 + NOT RUN 表
00-toolchain.txt                cargo / rustc / rust-toolchain / OS
10-generate-run-a.log  11-generate-run-b.log  12-compare-run-a-b.log  13-verify-artifacts.log
20-consumer-identity-{build,test,offline-locked,tree,path-guard}.log(.txt)
30-consumer-policy-*  同构
40-crate-build-*.log            packed crate 自身作为 workspace root 的真实构建
50-tamper-*.log                 内容篡改 / 缺文件 / 错版本
60-targeted-*.log               module-release-tool、contracts、compiler
70-{fmt,check,clippy,test,architecture,git-diff-check}.log
artifacts/<module>/{artifact-manifest,release,release-bundle,verification}.json
```

不提交：完整 `target/`、无关构建缓存、任何凭据或数据库连接串。README 中的临时根以 `<TEMP>` 归一化；日志原文保留绝对路径以便核对（不含凭据）。

## 11. 终态定义

本轮可达终态：**LOCAL RELEASE CANDIDATES READY FOR REVIEW / NOT PUBLISHED**。

不可由本轮得出：R2 判定、发布、集成、C1 关闭、G-02 的真实 PostgreSQL 结论。

## 12. 边界划分：直接落实 vs 停在设计提案

**直接落实**（权威已确定、可验证、不改边界）：

- manifest 每字段可追溯：`owned_bounded_contexts`/`data_owner` ← `[package.metadata.architecture]` + BOUNDED_CONTEXT_MAP；`published_commands/queries` ← 真实公开用例（`identity::application`、`policy::application`）与既有 HTTP 路由 `apps/business-api/src/routes/iam_admin.rs`；`data_classification` ← `SECURITY_ARCHITECTURE` §8（身份/个人信息属 Confidential）；`migration_namespace` ← `data_owner`；
- `module-release.identity.v1` 8 字段与 `ModuleReleaseBundle` 的组装（既有契约，A5 明确要求 pilot 做）；
- 工件字节摘要与确定性规则（A1 要求 deterministic digest，细节仓库未定义 → §2.1 认领）；
- 只读迁移映射与 checksum 校验（A7 的 per-namespace digest 钉定部分）；
- changelog（工件必填，内容来自真实历史状态）。

**停在设计提案**（改变已接受边界，本轮只登记不实现）：

| 提案 | 越界原因 |
|---|---|
| SP-1 把工件字节完整性摘要并入 `ModuleReleaseIdentity`（`module-release.identity.v2`） | 修改 Accepted 契约类型与 fitness 成员集合；Wave 2 §16 F-5 明确其为独立关注点 |
| SP-2 纯 compiler 暴露 `contract_surface_bytes()` seam | 修改纯 crate 公开 API；本轮用漂移守卫测试替代 |
| SP-3 PostgreSQL 迁移按模块拆文件/建 schema（G-203 真关闭） | 改 schema 与迁移历史，权威所有者边界变更 |
| SP-4 `migration_namespace` 落为物理目录层级并重命名 SQLite catalog | 改既有 catalog 布局与 `apps/migration` 语义 |
| SP-5 identity/policy 是否发布 `ResourceRef` resource kind / 引入 `published_events` | 公开契约新增（INTER_MODULE_COMM §6、ADR-0022）；本轮保持空并记录理由，不发明 |
| SP-6 platform capability 目录 owner（现声明为空） | 无权威 capability 目录（G-204 registry DEFER），不发明 ID |
| SP-7 changelog/工件格式并入 `SAAS_MODULE_STANDARD` §13（D-07） | Accepted 标准文本修订 |

## 13. 执行结果

由 closeout 提交追加：工件位置/版本/摘要表、外部消费者结果、门禁结果、NOT RUN / BLOCKED 表、剩余 C1 与 R2 阻塞、提交链。证据目录 `docs/reviews/2026-09-29-wave5a-gates/`。

## 14. 顺带发现（不属本轮修复范围，如实登记）

1. **T 级标签错位**：`2026-09-24-saas-align-module-packaging.md` §9 把 Performance 标为 T7、Release artifact build 标为 T5、Out-of-workspace consumer 标为 T6；PLAN-0014 §8 的定义是 **T6 = Performance evidence、T7 = Release compatibility**。按 PLAN 的 T7 六行核对，Wave 2 仅「manifest/digest」与「rollback/forward-only statement」有既有证据，其余 NOT RUN。本轮按 PLAN §8 阶梯记录，并在该文档追加带日期指针；不改写其历史结论。
2. **Wave 2 文档内部张力**：A-10 单元格写「内部 PASS / 外部 NOT RUN」，§10 合计写 PARTIAL；A-10 写「51 个 compiler/contracts 测试」而 §4.2/§9 写 113。均不在本轮修正范围。
3. **SQLite ledger 语义**：`identity-sqlite` / `policy-sqlite` 的 `*_migrations.checksum` 列绑定原始 SQL 字节而非摘要（命名易误读为校验和）。本轮不改（改列语义属迁移变更），登记为 C3 候选。

## 15. 独立审阅栏

| 项 | 结论 |
|---|---|
| 独立 reviewer | `<pending>` |
| Reviewed HEAD | `<pending>` |
| 工件可校验性 / 来源可追溯性 | `<pending>` |
| 外部消费者证据是否成立 | `<pending>` |
| 重复构建一致性是否成立 | `<pending>` |
| G-01 / G-201 / G-202 / G-203 可否关闭 | `<pending>` |
| R2 | **NOT CLAIMED**（实现者不判定） |
