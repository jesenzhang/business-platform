# ADR-0021：Business Application Packaging 与 Published Extension Points

> 状态：Proposed  
> 日期：2026-08-12  
> 决策范围：Business Module Packaging、typed contribution、跨模块扩展、版本兼容和 install/upgrade/remove planning  
> 参考：Twenty `65616332b452361e639c41d7340d54febf95fae5`；ADR-0020；WrenAI Semantic Contract

## 1. 背景

ADR-0020 已接受 Business Module Isolation 与 Semantic Contract，但当前只定义了模块边界、公开契约、资源、语义/UI/Agent contribution descriptor、依赖和生命周期的最小基础。

源码级复核 Twenty 后，确认一个真实 Business App Platform 还需要解决：

- 一个模块如何以稳定、版本化 package 形式存在；
- UI/Policy/Agent 等 contribution 如何形成 typed contract；
- 安装/升级前如何做兼容性检查和 dry-plan；
- 业务 A 如何允许业务 B 扩展，而不让 B 修改 A 的 private schema/domain；
- 模块移除与业务数据保留如何分开。

Twenty 的 Application Manifest、stable universal identifier、required server version range、manifest dry-run sync、UI/Role/Logic/Agent contribution 和 install/upgrade/uninstall lifecycle 具有高参考价值；但 Twenty 允许 App 给标准对象或其他 App Object 直接增加字段/关系，这不满足本项目更严格的业务隔离。

如果不新增明确的 Extension Point 规则，未来容易出现两种错误：

1. 为了扩展性允许 `Module B → Module A private schema`；
2. 为了隔离性禁止一切扩展，导致 Platform Core 或 Owner Module 重新硬编码所有业务组合。

因此需要定义受控中间层：**Published Extension Point**。

## 2. 决策提案

### 2.1 Business Module 以稳定 Package Manifest 对外

业务模块继续由 DDD Domain/Application 拥有正式事实，但对 Platform Registry 暴露稳定、版本化的 package declaration。

Package 必须基于 ADR-0020 的 `BusinessModuleManifest`，并允许声明：

```text
module identity / version
platform compatibility
published commands / queries / events
resource kinds
semantic contributions
ui contributions
agent capability requirements
policy/capability requirements
published extension points
extension contributions
dependencies
package digest
```

Package/Compiled Manifest 是声明和派生产物，不是业务事实存储。

### 2.2 Stable Contribution Identity

所有长期可引用 contribution 必须拥有稳定、namespaced identity：

```text
<module-id>.<local-id>
```

显示名称、文件路径、Rust module path、React route、数据库表/列变化不得隐式改变 identity。

### 2.3 Published Extension Point

跨模块扩展只能发生在 Owner Module 主动发布的 Extension Point 上。

```text
Owner Module A
  → PublishedExtensionPoint
          ▲
          │ versioned/public contract
Consumer Module B
  → ExtensionContribution
```

Extension Point 必须声明：

- stable ID；
- owner module；
- contract/schema version；
- allowed contribution kind；
- data classification；
- compatibility/removal semantics；
- required public resource/query/capability refs。

Consumer 不得修改 Owner 的 private Domain/Persistence。

#### Extension Point contract

每个 Published Extension Point 是 Owner 的公开架构资产，而不是一个可任意写入的 metadata 表。其最小声明为：

| 字段 | 规则 |
|---|---|
| `owner_module_id` | 只能是发布该 point 的模块；consumer 不能伪造 owner |
| `extension_point_id` | 稳定、namespaced、不可由 label/path/table 推导 |
| `contract_version` | 有兼容窗口；破坏性变更必须新版本 |
| `allowed_contribution_kind` | 明确是 resource metadata、detail UI、action、public reference 或 public projection 等哪一类 |
| `classification` | 由 owner 声明，跨边界只能保持或收紧 |
| `authorization` | 声明需要的 capability/policy；不代表自动授予权限 |
| `lifecycle` | 随 owner module/package 安装、升级、禁用、卸载的状态和迁移规则 |
| `consumer_module_id` | 贡献者必须显式声明；依赖图和反向引用可计算 |
| `dependency` | point、contract、resource kind 的依赖必须可解析、无环、满足版本范围 |
| `removal_semantics` | 有活动 consumer 时 `BlockedRemoval`；不能 silent break |

Owner 删除 Extension Point 的 dry-plan 必须检查所有 consumer contribution、module dependency 和兼容窗口。只有消费者先移除/迁移贡献且当前 registry snapshot 满足反向引用为空，才可执行 point removal。Point removal 不自动删除 owner 或 consumer 的正式业务事实。

### 2.4 禁止跨模块 private mutation

明确禁止：

```text
Module B → ALTER Module A private table
Module B → private FK / private table JOIN
Module B → inject field into Module A Aggregate without published contract
Module B → bypass Module A Application API
```

允许的跨模块机制仍为：

```text
Published Command / Query / Event
ResourceRef
Public Projection
Reference + Snapshot
Published Semantic Object
Published Extension Point
```

### 2.5 Typed UI Contribution

第一阶段只接受宿主控制的声明式 UI contribution：

```text
Navigation
List View
Detail Section
Detail Tab
Action
Command
```

UI contribution 不得携带数据库访问、SQL、secret、任意 native/plugin executable 或直接写其他模块 private state 的 callback。

未来如需动态 Front Component，必须单独 ADR 讨论 sandbox、版本、权限、CSP/DOM isolation 和供应链安全。

### 2.6 Policy/Agent 只声明 requirement

业务模块只能声明 Policy/Capability Requirement 和 Agent Capability Contribution；实际授权由 Platform Policy/ADR-0018 决定。

Manifest 声明能力不等于自动获得能力。

### 2.7 Compatibility 与 Dry Plan

Package/Module 兼容性使用明确版本范围并 fail closed。

安装、升级、移除之前必须能生成 deterministic dry-plan：

```text
Current Registry Snapshot
  + Incoming Package Set
  → validate
  → diff
  → BusinessApplicationPlan
```

至少识别：

- Add/Upgrade/Remove Module；
- Contribution Add/Update/Remove；
- Extension Point Add/Remove；
- Dependency/Compatibility change；
- blocked removal；
- ownership/version/conflict diagnostics。

Plan 阶段不得产生业务数据或 persistence side effect。

### 2.8 Module Removal 与 Data Purge 分离

继续采用 ADR-0020：

```text
Uninstalled != Data Purged
```

移除 package/registration 不自动删除正式业务历史。Purge 必须通过单独授权、保留策略、审计、验证和恢复规则。

## 3. 与 Twenty 的关系

### Adopt

- Application/Module Manifest；
- stable identity；
- source → validate/build manifest；
- SemVer compatibility；
- dry-run before apply；
- typed UI/Role/Logic/Agent contribution 思路；
- install/upgrade/remove lifecycle；
- package checksum/evidence。

### Adapt

- Custom Field/Object → Published Extension Point / sidecar extension；
- Role → Policy/Capability Requirement；
- Front Component → 第一阶段仅 typed declarative contribution；
- uninstall → registration removal 与 retained data 分离。

### Reject

- 任意跨 App 修改 private Object/schema；
- 跨业务 private FK；
- Object Metadata 取代 DDD；
- uninstall 自动 purge business facts；
- Platform Core 内置具体业务特权 Application。

## 4. 与 WrenAI / Semantic Contract 的关系

本 ADR 不新增第二套 Semantic Model。

```text
Twenty-inspired layer
  → Business App Packaging / UI / Extension / Lifecycle

Wren-inspired layer
  → Semantic Contract / Analytics / Context
```

二者共享 module identity/version，但语义权威继续由 ADR-0017/ADR-0020 管理。

## 5. 对 C Legacy / PLAN-0009 的影响

如果本 ADR 被接受并由 PLAN-0011 实现，未来经单独批准的 C Integration/Contract Module 计划可以把 C 项目作为真实验证案例：

```text
C Legacy
  → C-specific Integration ACL
  → Contract Business Module
      → Package Manifest
      → UI Contribution
      → Semantic Contribution
  → Platform
```

C-specific schema/path/state 不能进入 Contract Domain 或 Platform Core。

## 6. 非目标

本 ADR 不授权：

- 动态 Rust/Native/WASM/Node/Python plugin runtime；
- Marketplace/App Store；
- 通用 low-code/EAV 平台；
- online schema migration engine；
- arbitrary SQL；
- Twenty/Wren runtime dependency；
- C production migration；
- PLAN-0006 Workspace Runtime。

## 7. 影响

### 正面影响

- 业务模块可以贡献 UI/Extension/Agent/Policy 信息而不污染 Platform Core；
- 跨模块扩展从“隐式 schema coupling”转为 owner-published contract；
- package/version/digest/dry-plan 提供可审计的安装升级证据；
- Contract/C legacy 可以成为真实模块隔离验证，而不是把 legacy 规则塞入平台。

### 成本与限制

- 需要维护更多稳定 descriptor/version；
- Extension Point 设计需要 Owner 明确哪些位置可扩展；
- 第一阶段不提供任意动态 UI/代码插件，扩展能力会比 Twenty 更受限；
- Registry/apply runtime、持久化和生产 lifecycle executor 仍需未来 Plan。

## 8. 验收与实施

若本 ADR 被接受，由 [`PLAN-0011-business-application-packaging-and-contribution-foundation.md`](../plans/current/PLAN-0011-business-application-packaging-and-contribution-foundation.md) 首次实现。

PLAN-0011 必须优先实现纯 contract/compiler/dry-plan/fixture 和 Architecture Fitness，不得把本 ADR 扩展为动态插件、Marketplace 或数据库迁移平台。

在 ADR-0021 未被接受前，PLAN-0011 保持 `Proposed / NOT ACTIVE`。

## 9. 关联文档

- [`ADR-0020`](ADR-0020-business-module-isolation-and-semantic-contract.md)
- [`BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`](../architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md)
- [`TWENTY_REFERENCE_ANALYSIS.md`](../reference/TWENTY_REFERENCE_ANALYSIS.md)
- [`WRENAI_REFERENCE_ANALYSIS.md`](../reference/WRENAI_REFERENCE_ANALYSIS.md)
- [`PLAN-0011`](../plans/current/PLAN-0011-business-application-packaging-and-contribution-foundation.md)
- [`PLAN-0009`](../plans/archive/2026/PLAN-0009-c-legacy-contract-and-document-migration-rehearsal.md)：`Completed / Rehearsal Closed / Archived`；本 ADR 不重开、不扩展该 rehearsal，也不授予 production migration 权限
