# PLAN-0014 Wave 2 — Business Module Packaging Reference Conformance Review

> Review ID: REVIEW-SaaS-module-packaging-2026-09-24
> Status: READY FOR REVIEW / NOT INTEGRATED（实现者自检完成；独立 reviewer verdict 未返回，本文不自行填写 PASS）
> Module: Business Module Packaging foundation — `business-module-contracts` + `business-application-compiler`（P0-B）
> Module revision: 见 §2 基线与候选（代码 candidate 为候选链首提交）
> Reviewer: 实现者自检（本 agent）；独立 reviewer 待指派（§14）
> Review date: 2026-09-24
> Related plan: PLAN-0014（Wave 2 / P0-B）
> 执行环境: 全部命令与写入均在本地 Windows（Git Bash，`F:\Workspace\business-platform`）；未连接任何远程服务器；无 push、无合并、main 未修改。

## 1. Review question and method

Review question：`business-module-contracts` / `business-application-compiler` 的打包与发布
契约是否达到"可定义 canonical module release identity、R2 packaging acceptance 可执行"的程度；
与 ABP、Frappe/ERPNext、Odoo、Twenty 的模块打包语义相比，差距在哪里、哪些必须补、哪些拒绝。

Method（与 Wave 0/1 相同的三层重建，禁止互相推断）：

1. **fields/types defined** —— 源码中定义了哪些声明、值类型与校验；
2. **tests executed** —— 实际运行过哪些测试与门禁（命令、环境、结果逐项记录，§9）；
3. **independent reproducible release artifacts** —— 是否存在可独立复现的发布工件与真实消费者。
   第三层不能由前两层推断得出。

Boundary：

- 本轮只审 P0-B packaging foundation；不审 Identity/Organization/Policy 运行时（Wave 0/1 已审，
  其 G-09 C2 原样保留，§8）。
- Wave 5（R2 pilot release：identity、policy、contracts/compiler bundle）未启动；本轮凡需要
  真实发布工具、workspace 外独立消费者工程或真实 PostgreSQL 的验收项一律记 `NOT RUN`，不得写 PASS。
- 参考项目功能数量不计为缺陷；只对照"同一能力上参考的做法与我们的契约语义差异"。
- 不新增动态安装器、marketplace、微服务编排或任何部署单元。

## 2. 基线与候选（Base and candidate）

| 项 | 值 |
|---|---|
| 分支 | `feat/PLAN-0014-wave2-business-module-packaging-conformance`（本地，未推送） |
| Base（Wave 1 closeout 顶端） | `5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04` |
| `origin/main` | `505d2039f5ba46f0d3023e3eaa31fb7891fd6580` |
| merge-base(HEAD, origin/main) | `505d203`（Wave 1 链尚未合入 main，因此 Wave 2 分支直接从本地 Wave 1 链顶创建） |
| Wave 1 提交链等价性 | `git range-diff` 确认本地 5 提交 `dfd74bf,33b66b2,564fd1d,5a47204,5d3d2eb` 与远端同名 PR 链 `e532ac2…881f3aa` patch 等价（SHA 不同）；两条链不得同时合入 |
| 本地 `main` | 落后于 `origin/main`，**不作为基线** |
| 工作区起点 | clean tree（起点复核于 2026-09-24） |

Candidate 改动（代码链，`git diff --stat` 于提交前）：

| 文件 | 类型 | 内容 |
|---|---|---|
| `crates/business-module-contracts/src/release.rs` | 新增 | canonical module release identity + multi-crate bundle 值契约（9 个单元测试） |
| `crates/business-module-contracts/src/manifest.rs` | 修改 | 将 `validate_sha256_hex`/`validate_version_token`/`is_valid_namespace_characters` 收敛为 `pub(crate)` 共享校验，`PackageDigest::new` 改走共享校验 |
| `crates/business-module-contracts/src/lib.rs` | 修改 | 导出 release 模块公开类型 |
| `crates/business-application-compiler/src/lib.rs` | 修改 | 新增 `pub fn contract_digest(&BusinessApplicationPackage)` canonical 契约面摘要 seam |
| `crates/business-application-compiler/tests/release_surface_digest.rs` | 新增 | 契约摘要与 release identity 装配的 5 项行为测试 |
| `scripts/check-architecture.ps1` | 修改 | release identity 成员与 compiler digest seam 的 fitness 检查 |

代码候选提交：`4ad7096fbfa7582a608252145709f8c56f514191`（`feat(packaging): pin
canonical module release identity and contract digest`）；本文档与 PLAN-0014 /
ARCHITECTURE_STATUS 同步构成文档提交，即候选链末端（SHA 见分支 `git log`）。

## 3. Fixed references（review date 2026-09-24）

全部 revision 于 2026-09-24 经 GitHub API/浅克隆固定并读取源码与测试，不允许只引用 README；
README/文档声称与源码证据在下表与 §6 中分开标注。

| Reference | Reviewed revision | License | 源码/测试取证路径（节选） | Why relevant |
|---|---|---|---|---|
| ABP Framework | `10.6.1` = `9ca1d3ee62c8c557af34b50fc49c146405d752d4`（API 验证 tag 指向；更新的 10.7.0-rc 未采用） | LGPL-3.0-only（`common.props` `PackageLicenseExpression`） | `framework/src/Volo.Abp.Core/Volo/Abp/Modularity/`（`AbpModule.cs`、`ModuleLoader.cs`、`AbpModuleHelper.cs`、`AbpModuleDescriptor.cs`）、`System/Collections/Generic/AbpListExtensions.cs`、`framework/test/Volo.Abp.Core.Tests/…/ModuleLoader_Tests.cs`、`modules/feature-management/` | .NET 生态模块化单体基线：编译期模块、依赖排序、per-tenant feature |
| Frappe Framework | `v16.35.0` = `012667b9c4e7f66d5e1ff5858d2e922331d4300a` | MIT | `frappe/installer.py`、`frappe/modules/patch_handler.py`、`frappe/migrate.py`、`frappe/core/doctype/module_def/`、`frappe/tests/test_installer.py`、`test_patches.py` | Python 多 app 站点模型：per-app patches、卸载即删数据、顺序敏感 |
| ERPNext | `v16.36.0` = `b30aa5334bcea94dba74f5b866af13c43a861948`（独立仓库版本线） | MIT | `erpnext/modules.txt`、`erpnext/patches.txt`、`erpnext/setup/install.py`、hooks | 真实业务模块在 Frappe 打包模型下的落地形态 |
| Odoo | 分支 `19.0` tip = `2e2acfd6d2725d1a4a95b1a605634e1ac34163f`（仓库无版本 tag，按发布分支 tip 固定；此点如实记录） | LGPL-3.0（manifest 默认） | `odoo/modules/module.py`（`_DEFAULT_MANIFEST`、`_module_data_uninstall`）、`odoo/modules/graph.py`、`odoo/modules/loading.py`、`odoo/addons/base/__manifest__.py`、`odoo/tests/common/test_module_graph.py`、`test_install.py`、`test_uninstall.py` | 事实上的模块化 ERP 生命周期标准：per-module migrations、install/upgrade/uninstall 语义、环处理 |
| Twenty | `v2.9.0` = 注解 tag `da2eac83` peel 到 commit `b53f1832d80f87be2cfd88990b855c0e5f3cc926` | AGPL-3.0 + `@license Enterprise` 文件商业双许可 | `packages/twenty-sdk/src/cli/operations/{build,publish}.ts`、`packages/twenty-shared/src/application/manifestType.ts`、`packages/twenty-server/src/engine/core-modules/application/`（`application-install.service.ts`、`application-manifest/application-sync.service.ts`、`application-package/application-version-validation.service.ts`）、converters 测试目录 | ADR-0021 的既有参考；npm tarball 独立分发 + manifest 驱动同步的对照系 |

### Reference reading conclusions（源码证实要点，供 §6 引用）

- **ABP**：模块是编译期概念，`AbpModuleDescriptor` 无版本字段；版本只存在于 NuGet 包层
  （根 `common.props` 全仓统一 `10.6.1`）。依赖 `[DependsOn(typeof(X))]` 无版本语义；
  环检测在拓扑排序 `SortByDependenciesVisit` 抛 `"Cyclic dependency found!"`（`framework/test`
  未覆盖该异常路径）；开源版无运行时 install/uninstall/purge（仅 feature flag 与开发期 CLI）；
  迁移为宿主单一迁移程序集 + 时间戳顺序 + host→tenant 两段脚本，历史表可按 DbContext 更名。
- **Frappe/ERPNext**：app 版本动态取自 `__version__`；app→frappe 兼容**无任何声明机制**
  （NOT FOUND）。模块归属双写（文件系统 `modules.txt` + 站点 `Module Def.app_name`）。
  卸载唯一入口 `remove_app()`：默认自动备份 + 确认 + `--dry-run` + `required_apps` 依赖守卫，
  但对每张 DocType `DROP TABLE`——卸载即删业务数据。迁移为 per-app `patches.txt`
  （`[pre_model_sync]/[post_model_sync]`），逐 patch commit、失败中止整个迁移、
  Patch Log 记账、`--skip-failing`/`never_skip_patches` 显式策略；`installed_apps` 顺序 load-bearing。
  fixtures 是随 git 分发的种子数据，**不是** release artifact（文档原文亦未如此声称）。
- **Odoo**：per-module `__manifest__.py`；per-module `migrations/<version>/pre|post|end-*.py`，
  由版本比较驱动、无迁移历史表；install/upgrade 按模块逐个 commit（非单事务），失败留下
  `base.partially_updated_database` 自愈标记，下次启动自动续跑；依赖成环的模块被**跳过**
  （"in a dependency loop, skipped"）而非拒绝；uninstall 物理删除独占数据（`DROP … CASCADE`）
  并级联下游，但保留 `ir_module_module` 行 `state=uninstalled`，无 purge/软卸载选项。
- **Twenty**：manifest 由 SDK CLI（TS Compiler API 提取 `define*`）在 app 工具链生成到
  `.twenty/output/manifest.json`，`npm pack` tarball 为正式分发单元（`APP_REGISTRY_URL` 默认
  registry.npmjs.org）；安装做 `engines.twenty` semver 检查并拒绝降级
  （`CANNOT_DOWNGRADE_APPLICATION`）；首装失败自动 uninstall 补偿、升级失败保留旧版本；
  **无** enable/disable 概念（无 status 列；`canBeUninstalled` 仅保护内置应用）；卸载执行
  workspace migration 后对 app 的 custom object `dropTable(cascade)`，数据丢失有 issue 证据
  （#20589）。**修正 ADR-0021 的既有假设**：`TwentyCoreModule`、`serverVersionRange`、
  `ApplicationActivation` 在 v2.9.0 与 ADR 早期参考 commit `65616332` 均 NOT FOUND。

## 4. Current implementation facts（三层重建）

### 4.1 Fields/types defined（已定义）

- `BusinessModuleManifest`（`business-module-contracts/src/manifest.rs`）：`module_id`、
  `module_version`（SemVer）、`manifest_schema_version`（`business-module.manifest.v1`）、
  `owned_bounded_contexts`、required/optional platform capabilities、published commands/queries/events、
  `resource_kinds`、`data_classification`、`migration_namespace`、semantic/UI/agent contributions、
  `dependencies`、`compatibility`——为唯一模块声明，本轮未新增第二 manifest。
- 生命周期值类型：`ModuleInstallationState`（Installed/Enabled/Disabled/Uninstalled）与
  `ModuleDataState`（Retained/Purged）分离（ADR-0020）；`PackageDigest`（64 位小写 hex）。
- `business-application-compiler`：`BusinessApplicationPackage`（manifest + contributions +
  extension points/contributions）、`compile()`（确定性、fail-closed 诊断）、
  `CompiledBusinessApplicationManifest`（digest 校验）、`dry_plan()`（AddModule/RemoveModule/
  BlockedRemoval，逐包 digest）、`BusinessApplicationPlan`。
- **本轮新增（定义补齐）**：`ModuleReleaseIdentity`（schema `module-release.identity.v1`，
  字段 `module_id`/`module_version`/`manifest_schema_version`/`source_revision`/`contract_digest`/
  `package_digest`/`migration_revision`(optional)/`build_toolchain`，`deny_unknown_fields`）、
  `SourceRevision`（40/64 小写 hex git object，拒绝分支/标签名）、`ContractDigest`、
  `MigrationRevision`（复用 manifest 的 namespace 语法 + `MigrationSetDigest`）、`BuildToolchain`、
  `ModuleReleaseBundle`（identity + 非空去重的 `crate_name→crate_version` 映射，落实
  SAAS_MODULE_STANDARD §8 多 crate 审计映射）；compiler 新增 `contract_digest()` seam。
- 缺口（定义层，本轮之前）：`git grep` 于 Base 证实 `source_revision` 仅作为
  document-processing 领域词存在；`contract_digest`/`migration_revision`/`build_toolchain`
  在 Base 无任何定义——canonical release identity 当时不存在（Wave 1 review G-01 C1）。

### 4.2 Tests executed（本轮执行，细节见 §9）

- `business-module-contracts`：22 unit + 22 integration（digest 稳定性、tamper、fail-closed）
  ——本轮新增 release.rs 9 unit。
- `business-application-compiler`：18 compiler + 26 planning + 6 synthetic + **本轮新增 5**
  （`release_surface_digest.rs`）；既有语义覆盖确定性、环/未知依赖/版本不兼容拒绝、
  BlockedRemoval、uninstall≠purge、Downgrade 拒绝。

### 4.3 Independent reproducible release artifacts（不存在）

- workspace 内 `modules/` 目录不存在；没有任何真实业务模块 manifest（synthetic fixture 除外）。
- 无发布工具产生 tarball/包工件；无外部 release 元数据仓库；`apps/*`、其他 crate 对这两个
  crate 的依赖数为 **0**（`cargo metadata` 证实）——即"独立工件 + 独立消费者"这一层为零证据。
- 结论：不能由 4.1/4.2 推断 R2；这正是 G-201/G-202 的记录理由。

### 4.4 Owner（release metadata / install-enable 状态 / 持久事实）

| 事实 | 当前 owner | 状态 |
|---|---|---|
| 模块声明 | `BusinessModuleManifest`（业务模块作者提供，contract crate 校验） | 已定义、已验证 |
| Release identity（source/contract/package/migration/toolchain 钉定） | 本轮定义值契约；组装者是未来 release tool（读 compiled package + build evidence） | 契约已实现，组装/发布执行未实现（Wave 5） |
| Manifest Registry / install 状态持久化 | 无。ADR-0020 §2.3 将其明确为未来 Manifest Registry；ADR-0025 说明三种发布形态在同一 Port 之后 | 记录为设计缺口 G-204，不从表结构向上发明实现 |
| 迁移文件序列 | workspace `migrations/001..019.sql` + `MANIFEST.sha256`（单一平面序列，非模块命名空间物理隔离）；`runtime-migration` embed 全目录 | G-203 |

## 5. Canonical module release identity（本轮定义并落地）

一个 manifest 是声明；release identity 钉定该声明的一次可审计构建。八字段全部来自
`SAAS_MODULE_STANDARD` §13，落为纯 Rust 值契约（`business-module-contracts/src/release.rs`），
不建包、不注册、不安装、不替代 manifest：

| 字段 | 类型 | 规则 |
|---|---|---|
| `schema_version` | `String` | 必须等于 `module-release.identity.v1`，`deny_unknown_fields` |
| `module_id` | `BusinessModuleId` | 复用 manifest 同一新类型（禁止自由字符串） |
| `module_version` | `BusinessModuleVersion` | SemVer，≠ workspace crate 版本 |
| `manifest_schema_version` | `ManifestSchemaVersion` | 与 manifest 声明一致 |
| `source_revision` | `SourceRevision` | 40/64 位小写 hex git commit；分支/标签名不接受 |
| `contract_digest` | `ContractDigest` | SHA-256( canonical JSON(公开契约面) )，由 compiler `contract_digest()` 产生 |
| `package_digest` | `PackageDigest` | 复用既有类型；即 compiled package digest |
| `migration_revision` | `Option<MigrationRevision>` | `migration_namespace`（manifest 同一语法）+ `set_digest`（迁移文件集 sha256，对齐既有 `MANIFEST.sha256` 机制）；无状态模块为 `None` |
| `build_toolchain` | `BuildToolchain` | `toolchain_id`（小写 slug ≤64）+ `version`（manifest 版本 token 语法） |

`ModuleReleaseBundle { identity, crate_versions }` 提供多 crate 模块的
module_version→crate_version 可审计映射（非空、crate 名去重，重复报 `DuplicateCrate`）。

`contract_digest` 规则（compiler seam）：对 normalized 包的 canonical 表面
`{module_id, data_classification, published_commands/queries/events, resource_kinds,
semantic_contributions, extension_points}` 做 SHA-256；**与输入顺序无关、与发布元数据
（module_version、compatibility）无关，但发布工件 digest 仍覆盖后者**——即"契约没变"与
"发布变了"可分别证明。不纳入持久化与构建输入。

Lifecycle/发布规则（既有契约 + 本轮钉定，均有测试）：

1. **独立契约工件**：contract_digest 可脱离构建独立复算（permutation 不变性测试）；
2. **迁移命名空间**：`migration_revision.migration_namespace` 必须过 manifest 同一语法校验；
   物理文件隔离现状为 workspace-flat，关闭条件记 G-203；
3. **兼容窗口**：`CompatibilityDescriptor` 参与 compile/plan；contract_digest 不变 ⇒ 兼容面不变；
4. **forward-only**：Downgrade 在 compile/dry_plan 拒绝（既有测试）；
5. **rollback**：不提供 schema 自动回滚（与 Frappe DDL 不可回滚、Odoo forward-only+自愈一致），
   回滚 = Retained 数据上恢复先前 release identity；
6. **uninstall ≠ purge**：RemoveModule 计划保留 `data_retained` 标记；Purge 是独立、显式、
   未实现的操作（fitness 禁 token `PurgeBusinessData`/`DeleteData`/`DropBusinessFacts`）——
   **强于** Odoo/Frappe/Twenty 的 uninstall-即-drop；
7. **live consumer 移除阻断**：`BlockedRemoval` + `validate_extension_point_removal`（既有测试）。

纯度边界保持：两个 crate 仍无 SQLx/Axum/vendor SDK/运行时注册表；
`check-architecture.ps1` 新增成员检查（release identity 12 个成员名 + `pub fn contract_digest`）
与既有依赖白名单/禁 token 扫描共同强制。

## 6. Capability matrix

| Dimension | business-platform | Reference evidence | Gap | Decision |
|---|---|---|---|---|
| Authority | 打包事实只有两个纯 contract/compiler crate 定义；compiler 纯函数无写入；安装态持久化 owner 缺失（ADR-0020 未来 Manifest Registry） | ABP 模块=编译期 DI 图；Frappe=站点 DB `installed_apps`+Module Def；Odoo=`ir_module_module` 行；Twenty=core.application 行 | G-204（C2，设计缺口如实记录） | KEEP（纯函数边界）+DEFER（registry） |
| Module identity/version | `BusinessModuleId`+SemVer+schema 版本；本轮 release identity 8 字段钉定；bundle 映射 crate 版本 | ABP 模块无版本（NuGet 层统一 10.6.1）；Frappe `__version__` 动态；Odoo `19.0.x.y` 系列前缀；Twenty engines.twenty semver | 参考普遍缺"契约面摘要"维度 | ADAPT（取 Twenty 兼容声明思路；弃 ABP 无版本模型） |
| Manifest | 单一 `BusinessModuleManifest`，唯一声明；compiler 校验+canonical digest | Odoo `__manifest__.py`；Frappe `modules.txt`+Module Def 双写；Twenty manifest.json 由 SDK CLI 生成 | 无第二 manifest 问题（我们已强制单一） | KEEP |
| Dependencies/capability | 声明式依赖+平台能力证据 fail-closed；环→`DependencyCycle` 拒绝；未知依赖拒绝 | ABP 环=拓扑排序异常（无测试）；Odoo 环=**跳过**；Frappe `required_apps` 仅装/卸守卫；app→框架兼容无声明机制 | 我们的拒绝语义更严 | ADOPT（更严侧） |
| Public contract | ContractDescriptor 本地 ID+owner 限定；published commands/queries/events/resource kinds 全部进 canonical digest | Twenty universalIdentifier UUID 全量收敛（会覆盖用户定制，#21550）；ABP 无公开契约概念 | 参考无"契约摘要可独立复算"概念 | KEEP+ADAPT |
| Tenant/principal | manifest 平台中性；Wave 1 提供 Tenant/Principal/Policy foundation；**per-tenant 安装/启用状态的持久事实无 owner** | Frappe host→tenant 两段迁移+per-tenant Feature；ABp Feature per-tenant 覆盖链；Twenty per-workspace application 行 | G-206（C2） | DEFER（Wave 5 pilot 与 registry 一并定 owner） |
| Enable/disable/uninstall | 4 态安装状态 + 独立数据态；BlockedRemoval；无运行时执行器 | ABP 开源无装卸；Twenty 无 enable/disable，`canBeUninstalled` 只护内置应用；Frappe/Odoo 卸载即删数据 | 执行器缺（G-204） | KEEP（状态模型强于全部参考） |
| Data retention | Retained/Purged 显式分离；Purge 未实现且被 fitness 禁 token 守卫 | Odoo `_module_data_uninstall` DROP CASCADE、Frappe `remove_app` DROP TABLE、Twenty dropTable(cascade)（#20589 数据丢失、Frappe #24108 残留 custom field） | 参考共同反模式 | REJECT（参考侧）→KEEP（我们的 uninstall≠purge） |
| Migration ownership | manifest 有 `migration_namespace`；**物理文件仍是 workspace 平面 `001..019` + `MANIFEST.sha256`**；本轮 release identity 可钉 `migration_revision` | Odoo per-module `migrations/<version>/`（版本比较驱动）；Frappe per-app `patches.txt`+Patch Log；ABP 宿主单迁移程序集；Twenty 无 per-app 迁移 | G-203（C1）：namespace 未落到文件/执行隔离 | ADAPT（保留 workspace runner，R2 pilot 落 per-module 命名空间+per-set digest） |
| Upgrade/rollback | forward-only（Downgrade 拒绝）；dry-plan；回滚=Retained 数据上恢复旧 release | ABP host→tenant 脚本序；Odoo per-module commit+部分升级自愈标记；Frappe 无整体回滚（DDL 隐式提交）；Twenty 拒绝降级 | 我们无运行时"半升级恢复"（因为尚无运行时执行——如实记录，不算已验证能力） | KEEP+DEFER（执行期语义归 Wave 5/registry） |
| Digest | 三层：package digest、compiled manifest digest、**新增 contract_digest（canonical 契约面，与顺序/发布元数据无关）** | 四个参考均无契约面摘要概念（Frappe 有 patch 名记账、Twenty 有 packageJsonChecksum） | 参考侧缺位 | KEEP（优势维度） |
| Independent artifact | 无——npm tarball/NuGet/sdist 类工件的生成与校验不存在 | Twenty tarball@registry 为正式分发单元（checksum+安全解包）；ABP NuGet 统一版本；Odoo addons=git 树、无工件 | G-202（C1，R2 BLOCKED） | ADOPT（方向：pilot 产可校验工件；Wave 5） |
| Consumer build | workspace 内 51+ 编译期测试以纯 contract/compiler 依赖构建；**workspace 外独立消费者工程 = 0** | Twenty apps 为独立 npm 包（仓库外安装）；ABP NuGet 消费 | 4.3：第三层零证据 | G-202 同关（Wave 5） |
| Failure/recovery | compile/plan fail-closed 诊断；无运行时状态故无半安装态；registry 执行期恢复语义未定义 | Odoo 自愈续跑；Twenty 首装失败自动卸载/升级失败保留旧版；Frappe #42090 Patch Log 预写致失败不可修复 | G-204 关联 | KEEP+DEFER |
| Security/ops | 纯度禁 token 扫描（无 secret/credential/vendor 泄漏面）；digest 不纳入秘密；license 无传染（LGPL/MIT/AGPL 参考均为只读取证） | Twenty AGPL+Enterprise 双许可（引用其思路无传染：未复制代码） | 无 | KEEP |

## 7. Decision register

| # | 决定 | 依据 |
|---|---|---|
| D-01 | ADOPT：环与未知依赖**拒绝**（非 Odoo 跳过）、Downgrade 拒绝（Twenty 证实拒绝降级是生产可用语义） | 现有实现 + Twenty `CANNOT_DOWNGRADE_APPLICATION` |
| D-02 | ADAPT：模块级迁移保留 workspace runner，R2 pilot 内落 `migration_namespace` 文件隔离 + per-set `MigrationSetDigest`（复用 `MANIFEST.sha256` 思路，取 Odoo/Frappe per-app ownership） | G-203 |
| D-03 | ADAPT：独立发布工件形态对齐 ADR-0025（embedded/remote/OSS 三形态同一 Port），pilot 工件可校验（digest），参考 Twenty tarball+checksum，但不引入 registry/marketplace | Wave 5 范围 |
| D-04 | KEEP：uninstall≠purge、Retained/Purged 分离、BlockedRemoval、纯函数 compiler——全部参考的"卸载即 drop 数据"被拒收为反模式（有 issue 证据），我们的状态机更严 | §6 Data retention |
| D-05 | KEEP：release identity 为独立派生值契约（非第二 manifest）；manifest 仍是唯一声明 | SAAS_MODULE_STANDARD §4/§13 |
| D-06 | DEFER：Manifest Registry/install executor/per-tenant 安装态/AI/semantic registry runtime（ADR-0020/0021 已记录为未来 owner） | G-204/G-206 |
| D-07 | DEFER：changelog 工件格式、Odoo 式失败自愈标记（属执行器语义，Wave 5 随 registry 设计） | — |
| D-08 | REJECT：ABP 无版本模块模型；Frappe `installed_apps` 顺序 load-bearing 兼容（隐式契约）；Twenty manifest 全量收敛覆盖用户定制；"app→框架兼容无声明"（Frappe） | §3/§6 |

## 8. Gap registry

C0（阻断 correctness/tenant-isolation）：**无**——本轮未发现新 C0；Wave 1 无 C0 结论未被推翻。

### C1（R2 必需）

- **G-201（Wave 1 G-01 重编号承接）：canonical module release identity 缺失** ——
  本轮**部分关闭**：值契约（8 字段+bundle）、canonical `contract_digest` seam、fitness 守卫、
  9+5 项测试已落地（`release.rs`、`release_surface_digest.rs`、`check-architecture.ps1`）。
  残留关闭条件：真实模块 manifest 的 release identity 由 release tool 实际组装并钉入 pilot
  工件（Wave 5 关闭）。证据：§4.1 Base `git grep`；测试 §9 T1/T2。
- **G-202：无独立发布工件与 workspace 外消费者证据**（R2 BLOCKER）。
  影响：`identity`/`policy`/`contracts-compiler` 均不可宣布 R2。owner 边界：Wave 5 pilot
  （pilot #3 = contracts/compiler bundle，可先于业务模块证明工件+消费者闭环）。
  关闭条件：pilot 工件带 release identity 且外部工程仅依赖发布产物编译/测试通过。
  必需测试：§10 条目 A-09/A-10（本轮 NOT RUN）。
- **G-203：migration 所有权仍是 workspace 平面序列**。`migration_namespace` 是声明字段，
  `migrations/001..019.sql`+单一 `MANIFEST.sha256` 未按模块隔离；`runtime-migration` embed 全目录。
  影响：多模块共存时无法证明单模块迁移可独立演进/校验。owner 边界：pilot 模块的迁移落
  `<namespace>` 子目录 + per-namespace `MANIFEST.sha256`，`MigrationRevision.set_digest` 钉定；
  平面序列不重写（历史保持）。必需测试：per-namespace sha 校验 + release identity 钉定测试（Wave 5）。

### C2（应做）

- **G-204：Manifest Registry / install executor / 运行时安装态持久化未实现**。ADR-0020 §2.3、
  SAAS_PLATFORM_ARCHITECTURE §6 已记录为未来 owner；本轮如实记录，不发明表结构。
- **G-206：per-tenant 模块启用/禁用状态无持久事实与契约**（ABP per-tenant feature、
  Twenty per-workspace 安装行的对照缺口）；随 registry 设计关闭。
- **G-09（继承 Wave 1，原样保留）**：OrganizationMembership roster 移除不能单独证明另一条
  Policy RoleBinding 已撤销；维持 C2，本轮未改动任何 IAM 语义。

### C3（推迟）

- 通用 registry/marketplace/多目录源、升级检查 cron（Twenty marketplace catalog）；
- ABP Studio `.abppkg` 式 IDE/工具链元数据；Frappe fixtures 式种子数据通道；
- Odoo demo-data 安装通道；changelog 工件格式规范化（随 D-07）。

### REJECTED reference patterns

uninstall 即物理删数据（Odoo `_module_data_uninstall`/Frappe `remove_app` DROP TABLE/Twenty
dropTable cascade，issue 证据 Twenty #20589、Frappe #24108/#37799）；依赖环跳过（Odoo）；
无版本模块依赖（ABP）；installed_apps/apps.txt 顺序 load-bearing（Frappe #42113/#43172）；
manifest 全量收敛覆盖用户定制（Twenty #21550）；无任何 app→框架兼容声明（Frappe）。

## 9. Validation matrix（T0–T7）

统一环境：**本地 Windows（Git Bash），`F:\Workspace\business-platform`，无远程主机**；
候选链：代码提交（下表 Candidate-code）+ 文档提交（本文件所在提交）；Base `5d3d2eb`。

| Test | Level | Command | Result | Summary / evidence |
|---|---|---|---|---|
| Format | T0 | `cargo fmt --all -- --check` | PASS | 无 diff（门禁链 `FMT_EXIT=0`） |
| Check | T0 | `cargo check --workspace --all-targets --all-features` | PASS | `CHECK_EXIT=0` |
| Clippy | T1 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | `CLIPPY_EXIT=0` |
| Focused unit/integration | T1/T2 | `cargo test -p business-module-contracts -p business-application-compiler --all-features` | PASS | 79 tests 全绿（contracts 22u+22i，compiler 18+26+6+**5 新增**；release.rs 9 新 unit 在 contracts 计数内） |
| Workspace tests | T2 | `cargo test --workspace --all-features` | PASS | `TEST_EXIT=0`；日志无 FAILED/panic/error；per-suite 明细未保留在日志（tail-only），以退出码为准 |
| Architecture fitness | T2 | `pwsh scripts/check-architecture.ps1` | PASS | `Cargo metadata architecture fitness: PASS / OpenAPI contract: PASS / Architecture fitness: PASS`；含新 release identity 成员与 contract_digest seam 检查（本轮最后一次运行） |
| Git hygiene | T0 | `git diff --check` | PASS | clean |
| PostgreSQL lanes | T4 | — | **NOT RUN** | 本机无 PostgreSQL；本轮未改任何持久化路径。Wave 5 fixture：pilot 模块迁移子目录上跑 `runtime-migration` 契约测试 + `--include-ignored`（PLAN-0015 无关；Wave 3 同规则） |
| Release artifact build | T5 | — | **NOT RUN** | release tool 不存在（G-202）；Wave 5 fixture：pilot bundle `cargo package`/tarball + `ModuleReleaseIdentity` 钉定 + `MigrationSetDigest` 复算 |
| Independent out-of-workspace consumer | T6 | — | **NOT RUN** | Wave 5 fixture：新建外部 cargo 工程仅依赖发布产物（无 path 依赖），编译并运行 contract 测试 |
| Performance | T7 | — | NOT RUN | 本轮无性能声明需求（compile/plan 为开发期/发布期路径） |

历史证据 vs 本轮证据分离声明：Wave 0/1 的 SQLite/`iam_*` focused 证据只属于 Wave 0/1
review（见其 Validation 表），不构成本轮或 R2 证据；本轮为 contract/compiler/fitness/docs
变更，workspace 门禁全量运行如上；SQLite 与 mock 不替代任何 PostgreSQL 结论。

## 10. R2 packaging acceptance（可执行入口）

"可执行"定义：每条验收有确定的 fixture/测试、命令与判定；本轮可运行项已运行（PASS），
需要发布工具/外部工程/PG 的项给出精确 fixture 并记 NOT RUN。

| # | Acceptance entry | Fixture / test | Command | Result |
|---|---|---|---|---|
| A-01 | 同输入置换 → 同 contract digest | `release_surface_digest.rs::contract_digest_ignores_input_order_of_the_same_contract_surface` + `contract_digest_is_stable_across_repeated_invocations` | `cargo test -p business-application-compiler --test release_surface_digest` | PASS |
| A-02 | 发布元数据变（module_version/compatibility）→ contract digest 不变、package digest 变 | 同文件 `contract_digest_ignores_release_metadata_but_package_digest_covers_it` | 同上 | PASS |
| A-03 | 契约面变（命令/查询/事件/扩展点/分类/semantic 版本）→ contract digest 变 | 同文件 `contract_digest_changes_when_a_published_contract_or_extension_version_changes` | 同上 | PASS |
| A-04 | 重复 ID | 既有 `business-application-compiler` compiler tests（duplicate module id）+ contracts integration | `cargo test -p business-application-compiler` | PASS |
| A-05 | 未知依赖/未知 endpoint | 既有 compiler tests（unknown dependency、extension point endpoint） | 同上 | PASS |
| A-06 | 版本不兼容（含 Downgrade 拒绝、compat 窗口） | 既有 compiler/planning tests | `cargo test -p business-application-compiler --all-features` | PASS |
| A-07 | 依赖环 | 既有 compiler tests（`DependencyCycle`） | 同 A-06 | PASS |
| A-08 | 非法 platform dependency / 跨模块 private ref / manifest 非法字段 | 既有 contracts integration tests（`tests/` 22 项：contribution、private ref、schema 版本、unknown field 等）+ 新 `release.rs` 9 unit（`release_contracts_reject_unknown_fields_and_bad_digest_bytes` 等） | `cargo test -p business-module-contracts --all-features` | PASS |
| A-09 | manifest 与 release contract 一致性（identity 钉定 manifest 的 id/version/schema/namespace 语法；bundle crate 去重） | `release.rs` unit（`migration_revision_pins_manifest_namespace_grammar`、`bundle_requires_non_empty_unique_crate_mapping`）+ `release_surface_digest.rs::release_identity_and_bundle_assemble_from_compiled_evidence` | `cargo test --all-features`（两 crate） | PASS |
| A-10 | 独立消费者无 concrete adapter 编译/测试 | workspace 内证据：两 crate 依赖白名单（fitness）+ 全部 51 个 compiler/contracts 测试仅用纯 contract 类型（本轮 PASS）；workspace 外独立工程 | Wave 5：外部 cargo 工程仅依赖发布产物（fixture 见 §9 T6） | 内部 PASS / 外部 **NOT RUN** |
| A-11 | live consumer 移除阻断、uninstall≠purge | 既有 planning tests（BlockedRemoval、RemoveModule data_retained） | 同 A-06 | PASS |
| A-12 | 真实模块 release 工件 + PG 迁移 lane | — | Wave 5 pilot #1/#2/#3（fixture 见 §9 T4/T5） | **NOT RUN** |

## 11. R2 readiness（P0-B foundation crates）

| Requirement | Verdict | Evidence |
|---|---|---|
| Stable module identity | MET（契约层） | `BusinessModuleId` + `SourceRevision`；测试 T1 |
| Independent version | MET（契约层） | `BusinessModuleVersion` ≠ workspace 0.1.0；A-02 |
| Manifest | MET（单一声明，无第二 manifest） | §4.1/§5 D-05 |
| Public contracts | MET | published commands/queries/events/resource kinds 全入 canonical surface |
| Package/contract digest | MET | package digest（既有）+ `contract_digest`（新增）A-01..A-03 |
| Compatibility | MET（契约层） | `CompatibilityDescriptor` + A-06 |
| Migration namespace | **PARTIAL** | 声明字段+`MigrationRevision` 已有；物理隔离/执行隔离未落（G-203） |
| Contract suite | MET（workspace 内） | 79 tests；外部消费者缺口=A-10 后半 |
| Failure model | MET（compile/plan 层）/执行层 DEFER | fail-closed + BlockedRemoval；registry 属 G-204 |
| Security/tenant | MET（边界层） | 纯度禁 token/依赖白名单 fitness；per-tenant 安装态= G-206（DEFER，非本层） |
| Reference review | 本文件；**独立 verdict 未返回** | §14 |
| Release artifact | **MISSING** | G-202（R2 BLOCKER，Wave 5 关闭） |

结论：P0-B 两个 crate 维持 **R1 foundation**；不宣布任何模块 R2。Identity/Organization/
Policy 维持 R1 / R2 BLOCKED（Wave 1 结论不变：独立发布证据 + 真实 PostgreSQL 证据不足，
G-09 C2 保留）。

## 12. Wave 2 exit determination

| Exit condition | Verdict | Basis |
|---|---|---|
| canonical module release identity defined | **MET** | §5 落地为 `module-release.identity.v1` + bundle，测试与 fitness 守卫（G-201 契约层关闭） |
| R2 packaging acceptance executable | **MET（定义+本轮可运行部分全部 PASS）**；工件/外部消费者/PG 条目如实 NOT RUN，属 Wave 5 | §10 A-01..A-12 |
| no forced microservice split | **MET** | 零新增部署单元；`apps/*`、infra、数据库拓扑零改动；改动仅 2 个纯 crate + 1 个 fitness 脚本 + 文档 |

## 13. Reviewer questions（供独立审阅）

1. `contract_digest` 的 canonical surface 选择（含 extension points、semantic contributions，
   排除 UI/agent contributions）是否遗漏了应属"公开契约面"的内容？
2. release identity 作为独立派生值契约（而非 manifest 新字段）是否真正满足"无第二 manifest"？
3. `contract_digest` 与发布元数据解耦（A-02）是否会造成"同契约不同版本"被误判可互换？
4. G-203 记 C1 而非 C2 是否恰当（声明字段已有、隔离未落）？
5. §3 对 ADR-0021 的 Twenty 事实修正（`TwentyCoreModule`/`serverVersionRange`/
   `ApplicationActivation` NOT FOUND）是否需要另开 ADR 修订？
6. G-206（per-tenant 安装态）归 C2/DEFER 而非 C1 是否成立（R2 pilot 是否必须先有 per-tenant 状态）？
7. 其余同模板 §8 九问（authority 未变、无 provider 类型泄漏、fail-closed、分类诚实、
   测试证明失败语义、版本/迁移声明诚实、无投机抽象、可替换性、R 级诚实）。

## 14. Independent reviewer record

- Candidate 链：代码提交 `4ad7096fbfa7582a608252145709f8c56f514191` + 本文档/
  PLAN-0014/ARCHITECTURE_STATUS 同步提交（候选链末端 = 分支顶端，SHA 见 `git log`；
  Base `5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04`）。
- 状态：**READY FOR REVIEW / NOT INTEGRATED**。未 push、未合并 main、未启动 Wave 3、
  未宣布任何模块 R2。
- 独立 verdict（由实现者之外的 reviewer 只读填写）：

```
Reviewer:            <pending>
Reviewed base:       5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04
Reviewed candidate:  <pending>
Verdict:             <pending: PASS / PASS WITH C2-C3 / FAIL>
Notes:               <pending>
```

## 15. Conclusion

Wave 2 交付：(1) canonical module release identity 从"字段缺失"（Base 证据）落地为
`module-release.identity.v1` 值契约 + `ModuleReleaseBundle` + compiler canonical
`contract_digest` seam，并以 fitness 与 14 项新测试锁定；(2) R2 packaging acceptance 12 条
全部具有可执行入口，本轮可运行的 9 条全 PASS，需要发布工具/外部工程/PostgreSQL 的条目以
精确 fixture 记 NOT RUN；(3) 四个固定 revision 的外部参考对照产生 8 项决定与 7 项缺口
（C0=0；C1×3——其中 G-201 契约层关闭、G-202/G-203 保持 R2 阻断；C2×3 含继承的 G-09；
C3 与 REJECT 各成清单）。Business Module Packaging foundation 与 Identity/Organization/
Policy 一律维持 R1；R2 宣布被 G-202（独立发布工件与外部消费者证据）显式阻断，归 Wave 5。
本轮未 push、未合并、未启动 Wave 3；等待实现者之外的独立 reviewer 回填 §14。
