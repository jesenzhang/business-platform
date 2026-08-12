# PLAN-0011：Business Application Packaging & Contribution Foundation

文档 ID：PLAN-0011  
版本：0.2  
状态：Proposed / NOT ACTIVE  
日期：2026-08-12  
Owner：Platform Foundation / Business Module Runtime / Frontend Platform  
架构前提：ADR-0020 `Accepted`；ADR-0021 `Proposed`，必须先被接受才可激活本计划  
前置集成：PLAN-0010 必须先 `Integrated`；本计划不得在 PLAN-0010 仍为 Accepted Candidate 时激活实现  
主要参考：Twenty `65616332b452361e639c41d7340d54febf95fae5`；WrenAI `ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902`

## 1. 背景

PLAN-0010 已建立 Business Module Isolation 与 Semantic Contract 的最小纯 Rust foundation：模块身份、版本、平台能力、公开命令/查询/事件、资源类型、语义贡献、UI/Agent contribution descriptor、依赖、兼容性，以及确定性 semantic compiler。

但当前 `BusinessModuleManifest` 仍只是“模块边界声明”，还没有形成可供真实业务模块使用的 Business Application Packaging/Contribution 体系。特别缺少：

- 稳定、强类型的 contribution identity；
- 声明式 UI contribution 细分协议；
- Published Extension Point；
- Policy/Capability requirement；
- Platform/module SemVer compatibility range；
- package source → validate → compiled manifest；
- install/upgrade 前的 dry-plan / diff；
- module package checksum/evidence；
- 可验证的模块添加/删除/升级 fixture。

Twenty 当前 Application Platform 已经证明这些机制在真实 Business App 中的价值，但其允许 App 任意扩展其他 Object、形成物理关系以及 uninstall 删除应用数据的语义不符合本项目的严格业务隔离。PLAN-0011 只吸收可安全迁移的机制，不复制 Twenty Runtime。

Published Extension Point、typed Business Application Packaging 和 remove-plan 属于长期架构边界，由 ADR-0021 提案决定；**不得先实现再补 ADR**。

## 2. 目标

建立一个**平台中立、声明式、确定性、可审计、不可绕过业务所有权**的 Business Application Packaging & Contribution Foundation，使未来每个真实业务模块可以：

1. 以稳定 module/package identity 和版本声明自身；
2. 声明公开业务契约和平台能力需求；
3. 声明 UI、Agent、Policy、Extension Point 与 Semantic Contribution；
4. 在安装/升级前生成 deterministic dry-plan；
5. 通过兼容性、所有权和冲突检查 fail closed；
6. 在不修改 Platform Core 的情况下加入或移除业务模块；
7. 为 Contract Module/C Legacy rehearsal 提供第一个真实验证入口。

最终目标关系：

```text
Business Module source declarations
  → validate
  → normalize stable identifiers
  → resolve dependencies/capabilities/extension points
  → compile package manifest
  → deterministic digest
  → dry-plan against current registry snapshot
  → future registry/apply runtime
```

本计划完成后仍不代表已有动态插件系统或生产 Module Registry。

## 3. 非目标

本计划明确不实现：

- Twenty Server、Twenty ORM、Twenty SDK Runtime 依赖；
- WrenAI Runtime；
- 动态 Native/Rust Plugin loading；
- WASM/Node/Python 任意业务脚本运行时；
- Marketplace、App Store、远程 package registry；
- 热安装/热卸载生产模块；
- 在线 schema migration engine；
- arbitrary custom object platform；
- 通用 low-code builder；
- 任意第三方 Front Component 动态执行；
- 任意 SQL / Schema / DB Credential 暴露；
- PLAN-0006 Enterprise AI Workspace；
- PLAN-0009 C production migration；
- Contract/Finance/Legal/HR 的正式业务实现；
- 大规模移动现有 `crates/*` 到 `modules/*`。

## 4. 权威边界

### 4.1 DDD 事实权威保持不变

业务模块的正式事实、状态机、不变量、Command、Event、事务与数据所有权继续由对应 Domain/Application 拥有。

Business Application Manifest 不是第二业务数据库，也不能用动态 metadata 绕过 Domain Invariant。

### 4.2 Semantic Contract 保持唯一

PLAN-0011 不新增第二套 Metric/Dataset/Relationship 语义。Analytics/Semantic 继续以 ADR-0017 + ADR-0020 为权威。

### 4.3 Platform Core 不知道具体业务

Platform Core 不得增加：

```text
Contract
Finance
Legal
HR
CRM
C Project
```

等具体业务分支或枚举。

所有模块识别、UI 展示和能力发现必须通过稳定 manifest/registry descriptor。

## 5. 目标契约模型

在复用现有 `business-module-contracts` 的前提下，补齐以下通用概念。最终物理 crate 划分以 Concept Inventory 为准，禁止为了命名重复创建平行 crate。

### 5.1 Stable Contribution Identity

建立强类型稳定 ID：

```text
ContributionId
ExtensionPointId
UiContributionId
PolicyRequirementId
AgentCapabilityId
PackageDigest
```

要求：

- 与文件路径、Rust module path、React route、数据库表名无关；
- 全局使用 `<module-id>.<local-id>` namespace；
- rename label 不改变 identity；
- duplicate/collision fail closed。

### 5.2 UI Contribution Contract

第一阶段只建立声明式类型，不执行第三方组件代码：

```text
NavigationContribution
ListViewContribution
DetailSectionContribution
DetailTabContribution
ActionContribution
CommandContribution
```

每个 contribution 至少声明：

```text
stable id
owner module
resource kind / public query target
label/translation key
ordering/group
required capability/policy refs
visibility condition contract
version
```

禁止 UI contribution 携带：

- SQL；
- private table/column；
- DB URL/credential；
- 任意 JS/Rust executable blob；
- 直接修改其他模块 private state 的 callback。

### 5.3 Published Extension Point

建立显式：

```text
PublishedExtensionPoint
ExtensionContribution
```

用于解决“业务可扩展但互不污染”。

只有 Owner Module 主动发布 Extension Point，其他模块才能贡献扩展。

例如仅作概念说明：

```text
contract.metadata-extension
contract.detail-extra-section
contract.public-reference-slot
```

禁止：

```text
Finance → alter Contract private table
Finance → inject field into Contract aggregate without owner contract
Finance → FK to Contract private storage model
```

Extension Contribution 必须声明 owner、consumer、schema/version、classification 和 removal semantics。

第一阶段不创建通用 EAV/JSON extension 数据库表；只建立 contract/compiler/fixture。

### 5.4 Policy / Capability Requirement

模块只能声明需要的权限，不得自授予：

```text
PolicyRequirementDescriptor
CapabilityRequirementDescriptor
```

未来由 Platform Policy/ADR-0018 决定实际 grant。

Manifest 中声明 `requires contract.read` 不等于调用者自动获得该能力。

### 5.5 Compatibility

将当前 minimum/maximum window 演进为明确的 SemVer compatible range，或在保持 backward-compatible serialization 的前提下增加新的 range representation。

需要校验：

```text
manifest schema version
module package version
platform version range
module dependency range
contribution contract version
```

禁止 downgrade/不兼容 package 被静默接受。

本计划只实现纯校验和 dry-plan，不建立在线 updater。

## 6. Package Source → Compiled Manifest

参考 Twenty/WrenAI 的共同模式，但使用本项目自己的协议：

```text
Typed Module Source
  + UI Contributions
  + Extension Points
  + Policy/Agent Requirements
  + Semantic Contribution descriptors
        ↓
local validation
        ↓
namespace normalization
        ↓
dependency / capability / ownership resolution
        ↓
conflict detection
        ↓
stable sort
        ↓
Compiled Business Application Manifest
        ↓
canonical JSON + SHA-256
```

Compiled Manifest 必须：

- deterministic；
- rebuildable；
- schema-versioned；
- non-authoritative；
- 不包含业务事实；
- 不包含 private persistence mapping；
- 不包含 secret；
- 不允许手工变成第二 source of truth。

## 7. Dry Plan / Package Diff

新增纯计算的：

```text
BusinessApplicationPlan
PackageChange
```

至少支持：

```text
AddModule
UpgradeModule
RemoveModule
AddContribution
UpdateContribution
RemoveContribution
AddExtensionPoint
RemoveExtensionPoint
DependencyChange
CompatibilityChange
Conflict
BlockedRemoval
```

输入：

```text
Current compiled registry snapshot
Incoming compiled package set
```

输出只描述计划，不执行数据库/API/文件系统修改。

必须能够回答：

- 哪些模块新增/升级/移除；
- 哪些 public contract/semantic/UI/extension contribution 改变；
- 是否存在 consumer 仍引用即将删除的 public endpoint；
- 是否存在版本不兼容；
- 是否存在 ownership collision；
- 是否存在 extension point 被删除但 consumer 未先迁移。

任何 unresolved conflict 必须 fail closed。

## 8. Lifecycle 与数据保留

继续沿用 ADR-0020：

```text
Installation:
Installed → Enabled ↔ Disabled → Uninstalled

Data:
Retained / Purged
```

PLAN-0011 只验证状态模型和 dry-plan，不实现生产 uninstall executor。

`RemoveModule` dry-plan 必须明确显示：

```text
code/registration removal
!=
data purge
```

如果 retained data 存在，计划必须保留为事实状态，不得自动产生 Purge。

## 9. 实施阶段

### Stage A — Concept Inventory 与兼容扩展

- 读取 ADR-0017/0018/0020/0021、PLAN-0010、现有 `business-module-contracts` / `semantic-contract`；
- 确认 Contribution/Compatibility/Resource/Capability 现有概念；
- 不重复建立平行类型；
- 定义新增 ID 与 descriptor 的最小边界；
- 补 crate dependency fitness。

完成条件：无术语重复、无具体业务类型进入 generic contract。

### Stage B — UI / Policy / Agent Typed Contribution

实现纯 Rust typed descriptors 与 validation：

- Navigation/ListView/DetailSection/DetailTab/Action/Command；
- Policy Requirement；
- Agent Capability Requirement；
- stable namespaced identity；
- duplicate/unknown resource/capability ref fail closed。

完成条件：两个 fixture module 能声明不同 UI contribution，删除一个不影响另一个。

### Stage C — Published Extension Point

实现：

- owner-published extension point；
- consumer extension contribution；
- version/classification/removal contract；
- unknown/private/unowned endpoint reject；
- cross-module mutation prohibition tests。

完成条件：consumer 不能向未发布 endpoint 贡献任何 extension。

### Stage D — Compatibility + Package Compiler

- SemVer package/platform/module dependency validation；
- deterministic compiled Business Application Manifest；
- canonical JSON + SHA-256；
- registration order independence；
- package checksum evidence。

完成条件：相同 logical input 不同排序产生完全相同 bytes/digest。

### Stage E — Dry Plan / Diff

实现纯函数式 registry snapshot diff：

- add/upgrade/remove；
- contribution change；
- dependency/compatibility change；
- blocked removal；
- structured conflict diagnostics。

完成条件：不访问 DB/FS/Network 也能生成 deterministic plan。

### Stage F — Realistic Fixtures 与 Architecture Fitness

构造至少三个通用 fixture：

```text
module-a
module-b
module-extension
```

fixture 不能命名为 Contract/C/Finance 以避免 generic tests 固化业务知识。

验证：

- module-b 只能使用 module-a published contract/extension point；
- 删除 module-extension 不改变 module-a manifest；
- 删除 module-a 时若 module-b 仍依赖则 dry-plan blocked；
- Platform Core 不引用 fixture modules；
- compiler 不依赖 Axum/SQLx/Reqwest/Object Storage/Messaging/AI Provider/Twenty/WrenAI。

## 10. Architecture Fitness Functions

至少新增/加强：

1. Platform Core → Business Module dependency = forbidden；
2. generic packaging crate → concrete business crate = forbidden；
3. generic packaging crate → DB/Web/Provider runtime = forbidden；
4. cross-module private extension = reject；
5. stable ID duplicate = reject；
6. owner collision = reject；
7. unknown extension point = reject；
8. dependency/version incompatibility = reject；
9. compiled output determinism = required；
10. module removal with live dependency = blocked；
11. uninstall plan must never imply automatic data purge；
12. no Twenty/Wren runtime dependency。

## 11. 与 PLAN-0009 的关系

PLAN-0009 保持 `Proposed / NOT ACTIVE`。

PLAN-0011 集成后，PLAN-0009 的 120-contract rehearsal 才允许进入“Contract Business Module 真实接入验证”阶段。

验证链：

```text
C Legacy System
  → C-specific read-only ACL / rehearsal
  → Contract Business Module
      → Module Manifest
      → UI Contribution
      → Semantic Contribution
  → Platform Capabilities
```

PLAN-0009 不得因为 PLAN-0011 直接获得 production migration 权限。

## 12. 与 PLAN-0006 的关系

PLAN-0006 继续 `Proposed / NOT ACTIVE`。

PLAN-0011 的 Agent Capability Contribution 只定义声明协议，不实现 Workspace/Agent Runtime。未来 PLAN-0006 可以消费已发布的 Module Agent Capability，而不能反向决定业务模块边界。

## 13. 测试与验证

最低验证：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pwsh ./scripts/check-architecture.ps1
pwsh ./scripts/check-openapi.ps1
git diff --check
```

新增 focused tests 必须覆盖：

- namespaced stable ID；
- duplicate contribution；
- unknown dependency；
- version incompatibility；
- cross-module private extension；
- extension point removal；
- blocked module removal；
- deterministic compiled bytes/digest；
- deterministic dry-plan；
- `Uninstalled != Purged`；
- module fixture removal isolation。

如果本计划不修改 persistence/runtime storage，不强制本地 PostgreSQL/MinIO；远程 workspace CI 仍按仓库既有 E2E 执行并记录结果。

## 14. Activation Gate

只有同时满足以下条件才可从 `Proposed / NOT ACTIVE` 进入实现：

- ADR-0021 已正式 `Accepted`；
- PLAN-0010 已 `Integrated` 并成为 main 权威基础；
- 新实现分支从执行时真实 `origin/main` 创建；
- Base..HEAD 不包含 PLAN-0009 runtime 或 C-specific runtime；
- 不引入 Twenty/WrenAI runtime dependency；
- 本计划 scope 仍限制在纯 contract/compiler/dry-plan/fitness foundation。

## 15. Accepted Candidate Gate

只有同时满足以下条件才能进入 `Accepted Candidate`：

- ADR-0021 已 Accepted；
- PLAN-0010 已 Integrated；
- Base..HEAD scope 仅为 PLAN-0011；
- generic contracts 不包含具体业务概念；
- 没有 Twenty/WrenAI runtime dependency；
- 没有动态插件/runtime script；
- UI contribution 是声明式并由 Platform Host 控制；
- Extension 只能进入 Published Extension Point；
- dry-plan 对冲突/删除/版本不兼容 fail closed；
- compiled package 与 dry-plan deterministic；
- module removal fixture PASS；
- full workspace gates PASS；
- exact Candidate HEAD remote CI PASS。

## 16. 禁止事项

本计划 Proposed 阶段和实现阶段均禁止：

- merge/修改 PLAN-0010 Candidate；
- 在 ADR-0021 Accepted 前实现 Published Extension Point runtime contract；
- 激活 PLAN-0009 production migration；
- 修改 C 项目；
- 创建 Contract 特例进入 Platform Core；
- 引入 Twenty/WrenAI runtime；
- 动态 native plugin；
- 任意 JS/Python function runtime；
- Marketplace；
- arbitrary SQL；
- generic EAV 平台替代 DDD；
- uninstall 自动 purge business data。

## 17. 后续路线

若 PLAN-0011 Integrated，下一步按价值顺序：

1. 用 Contract 作为第一个真实 Business Module fixture/迁移目标；
2. 重新激活 PLAN-0009 120-contract isolated rehearsal；
3. 建立 C-specific `integrations/legacy-c-contract-management` ACL；
4. Contract Module 发布首批 UI/Semantic contribution；
5. 验证 C Integration 可移除、Contract Module 可独立、Platform Core 零 Contract 知识；
6. 再决定是否进入生产迁移 Wave 1；
7. Analytics Query Runtime 与 PLAN-0006 Workspace 按独立 Plan 推进。

## 18. 完成定义

PLAN-0011 的成功标准不是“做出 App Store”，而是证明：

```text
一个真实业务可以作为稳定、版本化、可声明的 Module Package 存在
+
业务只能通过 Published Contract / Extension Point 与其他业务协作
+
UI / Agent / Semantic 都是受控 contribution，而不是平台硬编码
+
模块安装/升级/删除可以先 dry-plan
+
删除模块代码不等于删除业务事实
+
Platform Core 仍然完全不知道具体业务是谁
```
