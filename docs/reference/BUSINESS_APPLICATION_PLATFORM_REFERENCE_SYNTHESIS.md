# Business Application Platform Reference Synthesis

> 文档类型：Reference synthesis / architecture input
> 检查日期：2026-08-12
> 参考项目：Twenty、Odoo、Frappe Framework/ERPNext、Cloudflare OS、WrenAI
> 许可证边界：只吸收源码可观察的架构机制；不复制代码、Schema、UI 资产或运行时依赖

## 1. 证据与方法

本综合分析基于各项目固定提交的源码路径和现有仓库 reference analysis。每项结论区分：

- **FACT**：在固定提交源码/许可证文件中可直接观察；
- **INFERENCE**：由机制推导的架构含义；
- **PROJECT DECISION**：business-platform 的正式适配选择，必须以后续 ADR/Baseline 为准。

| 项目 | 仓库 | 默认分支 | 检查提交 | 检查日期 | 许可证边界 |
|---|---|---|---|---|---|
| Twenty | `twentyhq/twenty` | `main` | `65616332b452361e639c41d7340d54febf95fae5` | 2026-08-12 | 根仓库无单一 SPDX；LICENSE 将主体 AGPLv3 与 MIT SDK/包、Enterprise 标记路径区分 |
| Odoo | `odoo/odoo` | `19.0` | `2f0f8e5e00685129b5bbe954117bc9f80a568e88` | 2026-08-12 | 根目录 LGPLv3；COPYRIGHT 说明存在第三方文件，复用前逐路径核验，Odoo 代码不复制 |
| Frappe | `frappe/frappe` | `develop` | `21b840572497b02bc42e6bf842cd62e1abca4ddb` | 2026-08-12 | GitHub API MIT；仍需核验路径、依赖与 ERPNext 组合许可证 |
| ERPNext | `frappe/erpnext` | `develop` | `ca1b03cd4647b1968f74256070c4d3453614d408` | 2026-08-12 | GitHub API GPL-3.0；不复制代码 |
| Cloudflare OS | `cloudflare/cloudflare-os` | `main` | `213ea6aa0a0e29d91d72832dcc9871432c1e01c5` | 2026-08-12 | Apache-2.0；参考 only，不成为 runtime dependency |
| WrenAI | `Canner/WrenAI` | `main` | `ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902` | 2026-08-12 | GitHub API NOASSERTION；现有分析记录路径许可证映射，直接复用需逐路径核验 |

## 2. 五个项目各自回答什么问题

| 参考 | 高价值机制 | 不能直接搬入的边界 |
|---|---|---|
| Twenty | Application Manifest、stable universal ID、source→build、SemVer、dry-run sync、UI/Role/Agent/Logic contribution、install/upgrade/uninstall | 任意 App 修改 Object/Field/Relation、metadata 取代 DDD、卸载删除业务数据、产品特权 application |
| Odoo | manifest/depends、模块加载和 registry、Python model inheritance、XML view inheritance、ACL/record rules、migration hooks | shared mutable model、全局 registry 隐式耦合、模块规则可被继承覆盖、GPL/商业许可证风险 |
| Frappe/ERPNext | DocType metadata、Custom Field、hooks/override、fixtures、permissions/workflow/events、app install/migrate、自动 REST | schema/runtime metadata 取代领域模型、运行时任意扩展、弱 compile-time ownership、跨 app hooks 隐式耦合 |
| Cloudflare OS | Workspace、Gadget、Blueprint、Gatekeeper、Capability、Observation、Generated App boundary、分享时再授权 | AI Workspace 取代业务权威、任意网络/代码/数据库访问、供应商 runtime 进入 Platform Core |
| WrenAI | semantic model、context、source→compiled artifact、结构化校验、受控 query planning | Python/SQL/MCP/Schema exposure、Analytics 取得业务事实权威、第二语义模型 |

## 3. 最终模型

```text
Business Module
├── Authoritative DDD Domain
├── Application API
├── Published Contracts
├── Extension Metadata
├── UI Contributions
├── Semantic Contributions
├── Agent Contributions
├── Policy Requirements
└── Manifest
```

### 3.1 Platform Core

**FACT/INFERENCE**：Twenty/Frappe/Odoo 证明宿主可以提供稳定 manifest、registry、生命周期和 contribution 接口；Cloudflare OS 证明 capability gateway、workspace 和 observation 可以作为平台能力；WrenAI 证明 source-to-compiled semantic artifact 可被结构化校验。

**PROJECT DECISION**：Platform Core 只拥有通用 capability、身份/租户/Policy、registry/compiler、事件与 durable execution primitives、UI/Agent host 和审计；不得知道 Contract、Finance、Legal、HR、CRM 或 C-specific 名称。添加新业务模块只新增其 package/contract，不改 Platform Core。

### 3.2 Business Module

Business Module 按业务能力、统一语言、不变量和数据所有权划分，不按表、页面、topic 或 plugin 划分。它拥有正式业务事实、状态机、Application Command/Query、Domain Event、事务、版本、幂等、迁移和公开契约。模块之间只能以公开 Application API、Integration Event、ResourceRef、Projection、Snapshot、Published Extension Point 协作。

### 3.3 Metadata

Metadata 适合描述稳定 ID、展示标签、导航、列表/详情区域、简单扩展槽位、权限需求、语义声明、版本和依赖。它不是业务事实数据库，不得定义或绕过 Aggregate 不变量。避免“万能 Object + Field + JSON”：简单展示扩展可 metadata 化，复杂状态、金额、审批、版本、证据和跨对象不变量必须回到 DDD Domain。

### 3.4 Semantic Contract

Semantic Contract 只描述可公开分析含义：Dataset、Projection、Field、Relationship、Measure、Metric、Dimension、Filter Policy 和 Lineage。它与 DDD Domain、Extension Metadata 共享 Module Identity，但不是其替代。唯一语义权威仍是 ADR-0017/ADR-0020；Analytics 只拥有 compiled registry input、projection 和 query execution metadata。

## 4. UI、Agent 与 Semantic contribution

三者必须属于同一个 Module Identity，但不能互相代替：

```text
Contract-like Module Identity
  ├── Domain/Application: formal facts and rules
  ├── UI: navigation/list/detail/action declarations
  ├── Semantic: dataset/metric/dimension/projection declarations
  └── Agent: typed query/approved action capability declarations
```

- **UI**：宿主控制的 typed declarative contribution；业务规则不在 UI；Action 调 Application API。
- **Agent**：声明 capability requirement/tool contract；实际 grant 由 Platform Policy 决定；tool 调 Application API，不调 DB/schema/repository。
- **Semantic**：声明分析含义与公开 projection；不产生正式写入；不暴露任意 SQL/schema。
- **Policy**：模块声明要求，不自授权限；高风险动作仍 Prepare→Preview→Confirm→Execute。

## 5. 跨业务引用、扩展与一致性

### 5.1 A 如何安全引用 B

按目的选择：当前状态用 Published Query；需要立即决策用 Owner Command；事实通知用 versioned Integration Event；持久关系用 ResourceRef；历史解释用 Reference + immutable Snapshot；列表/报表用 Published Projection。绝不使用 private FK、repository 或跨模块 SQL JOIN。

### 5.2 A 如何扩展 B

只有 B 主动发布 `PublishedExtensionPoint`，A 才能提交 `ExtensionContribution`。Extension Point 的 owner、consumer、stable ID、schema/version、classification、authorization、lifecycle、dependency 和 removal semantics 必须可编译验证。B 删除仍被使用的 point 必须 `BlockedRemoval`；不能 silent break。

### 5.3 跨模块事务与 Saga

单 Owner 本地事务强一致；跨 Owner 使用 Outbox→Integration Event→幂等 consumer/Saga→下一步 Owner Command。Saga 拥有业务过程状态；Durable Task 拥有 Job/Step/Lease/Fence/Retry/Recovery。补偿是新的业务动作，人工审批通过 Owner Application Use Case，不能用技术 Job Completed 推导业务完成。

## 6. 安装、升级、禁用和移除

```text
source declarations
 → validate/normalize
 → resolve dependencies/capabilities/extensions
 → compile stable manifest
 → SHA-256 digest
 → deterministic Dry Plan
 → future Prepare/Confirm/Execute
```

Dry Plan 支持 Add/Upgrade/Disable/Enable/Remove Module、Contribution 变更、Extension Point 变更、Dependency/Compatibility Change、BlockedRemoval 和 Conflict。Module removal 只移除能力注册/代码组合；`Uninstalled != Data Purged`。Purge 必须是独立授权、保留、审计、验证和恢复流程。

必须 compile-time/pure compiler validation 的内容：stable ID、duplicate/ownership collision、dependency graph/cycle、SemVer ranges、unknown reference、private reference、extension owner、semantic ownership、canonical ordering/digest。可以 runtime registry 的内容：当前 enabled state、tenant grants、freshness/watermark、lease/attempt、projection instance 和 audit evidence；其状态不能改变编译规则或成为业务事实权威。

## 7. 五项目机制的 Adopt/Adapt/Reject/Defer

| 机制 | 处理 |
|---|---|
| manifest、stable ID、source→compile、SemVer、dry plan、typed contribution | Adopt |
| Custom Object/Field、Relation、Front Component、Role、Logic Function、Gadget/Blueprint、semantic Context | Adapt 为 Published Extension Point、host-controlled UI、Policy requirement、Artifact/Capability、唯一 Semantic Contract |
| private schema mutation、shared mutable object、arbitrary SQL/DB tool、metadata-only domain、uninstall purge、Platform Core 特权业务 | Reject |
| dynamic native/WASM/Node/Python plugin、Marketplace、remote registry、Generated App sandbox、完整 Agent Runtime、Wren runtime | Defer |

## 8. Synthetic multi-business proof

未来 `module-a/module-b/module-extension` 必须验证：独立安装、公开 Query/Event/ResourceRef/Projection、Extension Point 合法贡献通过；private model/repository/FK 失败；重复/乱序事件可重放；Snapshot 保留历史；删除 extension 不改变 A；A 被 B 依赖时 remove blocked；输入注册顺序置换后 compiled manifest/digest/plan 完全一致；Platform Core 对 fixture 业务知识为零。

这只是 PLAN-0011 的验收设计，不重开 PLAN-0009，不继续 C production migration，不激活 PLAN-0006。

## 9. 结论

五个项目共同支持“声明→校验→编译/注册→生命周期”的平台化方向，但没有任何一个可直接成为本项目的 Domain、运行时或边界规范。最终收敛模型是：

```text
DDD Domain != Extension Metadata != Semantic Contract
Platform Core = business-neutral capabilities and contracts
Business Module = authoritative business capability + controlled contributions
Cross-module collaboration = Published Contract/Event/Ref/Projection/Extension Point
Module removal != data purge
```

正式决策进入 `BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md`、ADR-0021（Proposed）和 ADR-0022（Proposed）；外部 reference 只作为可追溯事实输入。
