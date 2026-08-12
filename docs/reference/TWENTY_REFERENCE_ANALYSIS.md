# Twenty 参考项目深度分析

> 检查日期：2026-08-12  
> 固定项目：`twentyhq/twenty`  
> 研究提交：[`65616332b452361e639c41d7340d54febf95fae5`](https://github.com/twentyhq/twenty/tree/65616332b452361e639c41d7340d54febf95fae5)  
> 默认分支：`main`  
> 分类：Tier-1 Business App / Module Packaging / Metadata / UI Contribution 参考  
> 研究目的：为 `business-platform` 的 Business Module Isolation、Business Application Packaging、UI Contribution、模块生命周期与权限声明提供源码级设计输入；不引入 Twenty 运行时。

## 1. 结论摘要

Twenty 在当前研究提交上已经不只是“可配置 CRM”，而是明显演进为一个可安装、可版本化、可声明数据模型、UI、权限、运行逻辑、Skill/Agent 和安装生命周期的 Business App Platform。它最值得本项目参考的不是 CRM 领域对象，而是以下通用机制：

```text
source declarations
  → build-time extraction / validation
  → Application Manifest
  → compatibility validation
  → dry-run metadata diff / workspace migration
  → install / upgrade / uninstall
  → runtime metadata / UI / permissions / functions / agents
```

Twenty 与 WrenAI 在本项目中的参考职责是互补而不是竞争：

```text
Twenty
  → 一个业务 App/Module 如何声明、安装、升级、贡献 UI/权限/逻辑

WrenAI
  → 一个业务 Module 如何声明可治理的数据语义、关系、指标、维度和 Context

business-platform
  → Rust DDD 权威业务模型 + Business Module Contract + Extension Metadata
    + Semantic Contract + Platform Policy/Audit/Agent
```

本项目不应复制 Twenty 的“任意 App 可给其他 App/标准 Object 增加字段、关系并形成物理耦合”能力，也不应让 Object Metadata 取代 DDD 业务权威。正确吸收方式是：**采用其 Application Manifest、稳定 ID、版本兼容、声明式 UI、权限需求和生命周期设计；把跨模块扩展改造成显式 Published Extension Point；保留 ADR-0020 的严格隔离和 `Uninstalled != Data Purged` 规则。**

## 2. 项目与许可证事实

| 项目 | 固定事实 |
|---|---|
| 仓库 | [`twentyhq/twenty`](https://github.com/twentyhq/twenty) |
| 研究提交 | `65616332b452361e639c41d7340d54febf95fae5` |
| 主体许可证 | 仓库 `LICENSE` 明确说明项目大部分为 AGPLv3 |
| Enterprise 文件 | 顶部标记 `/* @license Enterprise */` 的文件受独立商业许可证约束 |
| MIT 包 | `twenty-sdk`、`twenty-client-sdk`、`create-twenty-app`、`twenty-shared`、`twenty-ui` 和 `packages/twenty-apps` 当前按 LICENSE 说明属于 MIT 范围 |
| Application Exception | LICENSE 对通过官方 Application Interfaces 开发的独立 Application 提供 AGPLv3 Section 7 额外许可；不改变 Twenty 本体修改仍受 AGPLv3 的事实 |
| 本项目复用方式 | 只研究架构、契约和行为；不复制 Twenty Server/Front 源码，不引入 Twenty Runtime；未来如直接复用 SDK/代码，必须按具体路径和版本重新核验许可证 |

许可证证据入口：[`LICENSE`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/LICENSE)。

## 3. Twenty 当前 Application Platform 事实

### 3.1 Application Manifest 已经覆盖完整 App 贡献面

当前 `Manifest` 包含：

```text
application
objects
fields
indexes
logicFunctions
frontComponents
permissionFlags
roles
skills
agents
connectionProviders
publicAssets
views
viewFields
navigationMenuItems
pageLayouts
pageLayoutTabs
commandMenuItems
translations
```

证据：[`packages/twenty-shared/src/application/manifestType.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-shared/src/application/manifestType.ts)。

这意味着 Twenty 的 App 已经可以作为一个组合交付单元同时贡献：

- 数据对象和字段；
- 查询/展示 View；
- 导航、Record Page、Tab 和命令；
- Role/Permission；
- Server-side Logic Function；
- Skill/Agent；
- Front Component；
- Connection Provider；
- 多语言资源。

### 3.2 Object/Field 采用稳定 Universal Identifier

`defineObject()` 要求 Object 必须存在 `universalIdentifier`，并对名称、Label、字段和 Label Identifier 引用做构建时校验。Object Manifest 还声明 `isSearchable`、`isUICreatable`、`isUIEditable`、`openRecordIn` 和字段集合。

证据：

- [`define-object.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-sdk/src/sdk/define/objects/define-object.ts)
- [`objectManifestType.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-shared/src/application/objectManifestType.ts)

这里的关键设计价值不是 UUID 本身，而是：**配置实体身份不依赖文件路径、显示名称、数据库列名或 UI Route。**

### 3.3 Source Definition → Build Manifest

Twenty SDK 的 build 流程会扫描 App 中的 TS/TSX，识别 `defineApplication`、`defineObject`、`defineField`、`defineRole`、`defineSkill`、`defineAgent`、`defineLogicFunction`、`defineFrontComponent`、`defineView`、`definePageLayout` 等声明，执行结构化提取与校验，再汇总生成 Manifest。

证据：[`packages/twenty-sdk/src/cli/utilities/build/manifest/manifest-build.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-sdk/src/cli/utilities/build/manifest/manifest-build.ts)。

对本项目最重要的启示是：

```text
业务模块源码声明是权威输入
compiled manifest 是派生产物
运行时消费 manifest，而不是扫描业务 crate 内部实现
```

这一点与 ADR-0020/WrenAI 的 `source → validate → compile → derived manifest` 纪律一致。

### 3.4 Server Version / Workspace Version / App Version Compatibility

Twenty Application Manifest 允许声明 `requiredServerVersionRange`。安装/部署时会验证：

- 当前 Instance 已完成版本；
- Workspace 已完成 Upgrade 版本；
- App 要求的 SemVer Range；
- Incoming App Version 与 Current Version；
- 同版本重复部署和 downgrade。

证据：[`application-version-validation.service.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-server/src/engine/core-modules/application/application-package/application-version-validation.service.ts)。

这比简单 `minimum_platform_version / maximum_platform_version` 更适合未来 Module Compatibility，但不要求重新打开 PLAN-0010；应在后续 Packaging Plan 中独立演进。

### 3.5 Manifest Sync、Dry Run 与 Workspace Migration

Twenty Server 的 `ApplicationSyncService` 支持：

```text
manifest
  → resolve installed/virtual application
  → sync metadata from manifest
  → build/validate workspace migration
  → dry-run or apply
  → sync translations
```

并提供 `preInstallSynchronizeFromManifest()` 与 `uninstallApplication()`。

证据：[`application-sync.service.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-server/src/engine/core-modules/application/application-manifest/application-sync.service.ts)。

这是本项目未来 `Module Registry + Plan-before-Apply` 的高价值参考：**先计算变化和风险，再执行实际注册/迁移。**

### 3.6 安装、升级、卸载是真正的生命周期

`ApplicationManifest` 有 pre-install、post-install 和 uninstall logic function 描述，并带 package/lock checksum 与兼容范围。Server 卸载时会从当前 Application 拥有的 metadata 构造 “to empty” 的删除 migration，执行可卸载检查、uninstall hook、metadata/data removal 和 runtime resource cleanup。

Twenty 证明了“一个业务能力包可以有明确安装/升级/卸载生命周期”，但其卸载路径可能删除应用拥有的数据，因此不能直接复制到企业业务事实系统。

本项目继续坚持 ADR-0020：

```text
Installation: Installed / Enabled / Disabled / Uninstalled
Data:         Retained / Purged

Uninstalled != Data Purged
```

### 3.7 UI Contribution 是 Twenty 的核心参考价值

Twenty App Manifest 可以贡献：

- View / View Field；
- Navigation Menu Item；
- Page Layout / Page Layout Tab；
- Command Menu Item；
- Front Component；
- Settings Front Component；
- Translation / Asset。

这说明平台 Shell 不需要硬编码每个业务页面。一个业务 App 可以声明“我要出现在什么导航、提供哪些列表视图、详情页有哪些区域、有哪些 Action”。

对本项目的适配目标应是声明式、宿主控制的 UI Contribution：

```text
Business Module
  → UiContribution
      → Navigation
      → ListView
      → DetailSection
      → DetailTab
      → Action
      → Command
  → Platform UI Registry
  → React Shell
```

第一阶段不实现 Twenty 式动态任意 Front Component Runtime；先实现静态/编译时 typed contribution。

### 3.8 Role / Permission / Agent Tool Packaging

Twenty Manifest 同时包含 Role、Permission Flag、Skill、Agent 和 Logic Function，并在具体 App 中使用独立 Role 限定 App Function 对 Workspace 数据的访问。

对本项目的正确吸收方式不是让 Module 自己授予权限，而是：

```text
Module declares requirement
  → Platform Policy resolves grant
  → Runtime issues task/user scoped authority
```

这与 ADR-0018 的 Capability Grant 和 ADR-0020 “UI/Agent contribution 是声明，不是授权”一致。

## 4. Twenty 不是完全业务中立的平台

Twenty 仍存在固定的 `Twenty Standard Application` 和 `Workspace Custom Application`。前端有 `isTwentyStandardApplication()`，Standard Application 有固定 universal identifier；Server 会为 Workspace 创建 Standard/Custom Application，并设置 `canBeUninstalled = false`。

证据：

- [`isTwentyStandardApplication.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-front/src/modules/applications/utils/isTwentyStandardApplication.ts)
- [`TwentyStandardApplicationUniversalIdentifier.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-shared/src/application/constants/TwentyStandardApplicationUniversalIdentifier.ts)
- [`application.service.ts`](https://github.com/twentyhq/twenty/blob/65616332b452361e639c41d7340d54febf95fae5/packages/twenty-server/src/engine/core-modules/application/application.service.ts)

因此 Twenty 当前更准确的定位是：

```text
CRM Product
+
不断通用化的 Business App Platform
```

本项目目标应更严格：Platform Core 不得知道 Contract/Finance/CRM/HR 是谁。

## 5. 必须拒绝的 Twenty 机制

### 5.1 任意跨 App 修改 Object

Twenty 的 App 模型允许 Extension App 给标准对象、Workspace Custom Object 或其他 App Object 增加字段；Relation 还能形成双向关系和数据库级删除语义。这对 CRM Extension 很灵活，但不符合本项目“真实业务互不污染、业务可剥离”的强隔离要求。

本项目禁止：

```text
Finance Module
  → ALTER Contract private schema
  → direct private FK
  → silently inject field into Contract authoritative aggregate
```

只能通过明确发布的 Extension Point、Public Projection、ResourceRef、Command/Query/Event 或 Reference + Snapshot 协作。

### 5.2 Object Metadata 不得替代 DDD

Twenty 的 Object/Field/Record 模型非常适合低代码和可配置 CRM，但不能取代本项目已有的：

- Aggregate/Entity/Value Object；
- 业务不变量；
- 状态转换；
- Command/Query；
- 事务边界；
- 领域事件；
- Revision/Evidence/Audit；
- Outbox/Lease/Fence/Recovery。

正式业务事实仍必须由对应 Bounded Context 的 Rust Domain/Application 拥有。

### 5.3 Uninstall 不得默认删除业务历史

企业业务模块下线与历史数据保留必须分开治理。Module uninstall 只能解除代码/能力/注册关系；业务数据 Purge 必须有单独权限、保留规则、审计、证据和验证。

## 6. 推荐的三层 Business Module 模型

结合 Twenty 与 WrenAI，正式推荐：

```text
Business Module
  ├── Authoritative Domain Model
  │     business facts / invariants / state / commands / events
  │
  ├── Extension Metadata                # Twenty-inspired
  │     simple custom fields / simple extension objects
  │     views / layouts / navigation / UI actions
  │
  └── Semantic Contract                 # Wren-inspired
        dataset / projection / field semantics / relationship
        metric / measure / dimension / filter policy / lineage
```

三层职责：

| 层 | 权威职责 | 不允许 |
|---|---|---|
| Authoritative Domain | 正式业务事实、不变量、生命周期、事务、事件 | 被通用 metadata 动态替代 |
| Extension Metadata | 简单扩展字段/对象、View、Layout、Navigation、UI Action | 修改其他模块私有模型、拥有核心不变量 |
| Semantic Contract | 分析语义、指标、维度、关系、Filter Policy、Lineage | 成为业务事实存储、暴露 SQL/Schema |

## 7. Twenty / WrenAI / Cloudflare OS 在本项目中的分工

| 参考项目 | 本项目主要吸收方向 | 不吸收 |
|---|---|---|
| Twenty | Business App Manifest、stable ID、版本兼容、UI/Role/Logic/Agent Contribution、install/upgrade lifecycle | Object Metadata 取代 DDD、任意跨模块 schema 注入、uninstall 自动清数据 |
| WrenAI | Semantic Contract、source→compile→manifest、Context/Knowledge、dry-plan、结构化语义校验 | 任意 SQL、原始 Schema/MCP、Python/LanceDB runtime |
| Cloudflare OS | Workspace、Capability Gatekeeper、Agent Application、Observation/Artifact/Generated App 边界 | 直接作为平台运行时依赖 |
| OpenContracts/Mayan | Document/File/Revision/Evidence/解析血缘 | 取代 Contract/Legal 正式领域模型 |

## 8. Adopt / Adapt / Reject / Defer

### Adopt

- Business/Application Manifest；
- stable universal identity；
- source declarations → validate/build manifest；
- manifest diff/dry-run before apply；
- SemVer compatibility gate；
- typed UI contribution；
- Role/Permission requirement declaration；
- install/upgrade lifecycle；
- Agent/Skill/Logic contribution descriptors；
- package/manifest checksum and version evidence。

### Adapt

- Custom Object → 仅用于 simple extension object，不替代 DDD Aggregate；
- Custom Field → Published Extension Point / sidecar extension storage；
- Relation → Published semantic/resource relationship，不直接跨模块 private FK；
- Front Component → 先做 Platform-hosted typed UI contribution，后续才评估沙箱组件；
- Logic Function → Platform-controlled function/tool execution，不给数据库凭证；
- Uninstall Hook → best-effort cleanup，不自动 Purge retained business facts；
- Application Role → Platform Policy/Capability Requirement，不由 Module 自授予权限。

### Reject

- 业务模块任意修改其他模块 private object/schema；
- 跨业务直接物理 FK；
- Object Metadata 作为所有正式业务的唯一模型；
- Module uninstall 自动删除正式业务历史；
- App/Agent 获取任意数据库访问；
- Platform Core 内置 Contract/CRM/Finance 特权应用；
- 通过动态字段绕过 Domain Invariant。

### Defer

- 动态运行时 Front Component 插件；
- Marketplace/远端 App Registry；
- 热装载/热卸载 Rust/Native Plugin；
- 任意第三方 Logic Function Runtime；
- 多租户 App Marketplace Billing；
- Twenty Server/ORM/Metadata Engine 直接复用。

## 9. 对 ADR-0020 的影响

本次调研不要求重开或改写 ADR-0020。相反，它验证了 ADR-0020 的方向：

- `BusinessModuleManifest` 作为平台无关声明入口是合理的；
- `ui_contributions`、`agent_tool_contributions`、`compatibility` 需要后续独立 Plan 具体化；
- `Installed/Enabled/Disabled/Uninstalled` 与 `Retained/Purged` 分离比 Twenty 更适合企业事实系统；
- 跨模块 private reference fail-closed 必须继续保持，比 Twenty 的 Extension Model 更严格；
- Semantic Contract 继续由 WrenAI/ADR-0017 负责，不能把 Twenty Object Metadata 变成第二套语义层。

后续扩展应通过独立计划，而不是修改 PLAN-0010 Accepted Candidate。

## 10. 对 PLAN-0009 / C Legacy 的影响

C 项目未来必须成为第一个验证这一架构的真实案例：

```text
C Legacy System
  → integrations/legacy-c-contract-management
  → ACL / translator
  → Contract Business Module
      ├── Authoritative Domain
      ├── UI Contribution
      └── Semantic Contribution
  → Platform Capabilities
```

C-specific table、path、state、JSON、import mode 只能存在于 Integration/Rehearsal 边界。Contract Module 不依赖 C Schema；Platform Core 不依赖 Contract Module。

因此 PLAN-0009 在真正激活 120-contract materialization 前，应确认最小 Business Application Packaging/Contribution Foundation 已集成，至少能证明：

- Contract 作为一个独立 Module 被注册；
- Module Identity/Compatibility 明确；
- UI/Semantic Contribution 有独立声明入口；
- 删除 C integration 不影响 Contract Module；
- 删除 Contract Module 的 composition 不影响 Platform Core。

## 11. 实施入口

本研究对应的落地计划：

[`PLAN-0011-business-application-packaging-and-contribution-foundation.md`](../plans/current/PLAN-0011-business-application-packaging-and-contribution-foundation.md)

PLAN-0011 只建立 Business App Packaging/Contribution 的通用基础，不实现动态插件、Marketplace、任意 Script Runtime，也不启动 C production migration。

## 12. 最终定位

Twenty 从综合业务项目参考提升为 Tier-1 架构参考：

```text
Tier-1 Architecture References

Twenty
  → Business App / Module Packaging / Metadata / UI / Lifecycle

WrenAI
  → Semantic Contract / Context / Analytics

Cloudflare OS
  → Workspace / Capability / Agent Application

OpenContracts + Mayan
  → Document / Revision / Evidence
```

本项目的目标不是复制 Twenty，而是独立实现一个更严格的企业 Business Module Platform：**DDD 事实权威、Manifest 声明、Extension Point 可控、Semantic Contract 可编译、Policy/Audit 强制、业务可剥离且数据保留独立治理。**
