# Business Application Platform 架构

> 文档类型：Baseline
> 状态：Accepted baseline; PLAN-0011 Integrated
> 日期：2026-08-19
> 适用范围：Platform Core、Business Module、Application Packaging、Contribution、跨模块协作与生命周期

## 1. 目标

本架构定义一个通用 Enterprise Business Application Platform 的最小稳定模型：新增业务模块不要求修改 Platform Core；模块可通过公开契约和贡献声明加入平台；模块之间可以协作、扩展和安全引用；模块代码移除与业务数据清除分离。

本轮只收敛架构、标准、ADR、Plan 和 Fitness Functions，不实现 Module Registry、安装/卸载 executor、动态插件、WASM/Node/Python runtime、Marketplace、数据库迁移或真实业务模块。

## 2. Platform Core 与 Business Module

### 2.1 Platform Core 负责什么

Platform Core 只提供与具体业务无关的 capability 和稳定协议：

- identity、tenant、policy、authorization、audit；
- application contract registration/validation；
- package identity、SemVer、dependency/capability resolution 和 deterministic dry-plan；
- event/outbox/inbox delivery primitives；
- ResourceRef validation；
- rebuildable projection/analytics registry seam；
- UI host/registry；
- Agent capability grant/tool gateway seam；
- durable task execution primitives；
- storage、messaging、observability 和 lifecycle orchestration ports。

Platform Core 不知道 Contract、Finance、Legal、HR、CRM、C Project 或其他具体业务名称，不拥有其业务事实、Domain Invariant、private persistence、业务状态或语义口径。

### 2.2 Business Module 负责什么

每个 Business Module 按业务能力、统一语言、不变量和数据所有权组织，最小完整边界为：

```text
Business Module
├── Authoritative DDD Domain
├── Application API / Ports
├── Published Contracts
├── Extension Metadata
├── UI Contributions
├── Semantic Contributions
├── Agent Contributions
├── Policy Requirements
└── Manifest
```

Module 拥有其 Bounded Context 的正式事实、状态、事务、版本、幂等规则、领域事件、迁移和公开 Application Commands/Queries。Manifest、compiled package、UI metadata、Agent descriptor 和 Semantic Contract 都不能取代 Domain。

## 3. 三层边界：DDD、Extension Metadata、Semantic Contract

```text
DDD Domain
  formal facts · invariants · state transitions · transaction · domain events

Extension Metadata
  typed extension slots · UI placement · labels · simple configuration

Semantic Contract
  datasets · projections · fields · measures · metrics · dimensions · lineage
```

| 层 | 权威问题 | 明确禁止 |
|---|---|---|
| DDD Domain | 当前业务事实是什么、能否转换、谁拥有写入 | 被万能 Object/Field/JSON 或 metadata 替代 |
| Extension Metadata | Owner 发布的哪些位置可以被安全贡献 | 任意注入 private aggregate/schema、隐藏业务不变量 |
| Semantic Contract | 公开数据的业务含义如何查询/分析 | 成为事实存储、第二套 Metric 权威、暴露 SQL/schema |

三层共享稳定 Module Identity/Version，但职责不互相替代。Semantic Contract 继续只有 ADR-0017/ADR-0020 的一套权威；compiled manifest 可重建，Analytics 不拥有业务事实。

## 4. Contribution 统一入口

UI、Agent、Semantic、Policy 和 Extension contribution 必须属于同一个 `module_id`，各自使用独立 schema、生命周期和授权，不得互相代替。

### UI Contribution

第一阶段仅允许宿主控制的声明式类型：Navigation、ListView、DetailSection、DetailTab、Action、Command。每个 contribution 带 classification，并且只引用公开 Resource Kind、Query 或 owner 已发布并进入 capability catalog 的 Capability 和 translation key；不携带 SQL、private table/column、credential、任意 executable blob 或直接业务写 callback。UI 只表达展示和用户意图，业务规则仍由 Application API 执行。

### Agent Contribution

模块声明 typed query tool、approved action tool、Context/Skill 或 capability requirement。每个 Agent contribution 带 classification，target 只允许 Query、Command 或 approved published Capability。Agent Tool 只能调用公开 Application API/Query/Command，授权由 Platform Policy/Capability Grant 决定；模块声明不等于授予权限。Agent 不得访问数据库、schema、private repository 或任意 HTTP/SQL/Shell。

### Semantic Contribution

模块声明 Dataset、Projection、Field、Relationship、Measure、Metric、Dimension、Filter Policy 和 Lineage。它只能发布业务含义和受控查询边界；投影可重建、带 owner/version/freshness/classification，不能用来写回 Domain。

### Published Extension Point

Owner Module A 主动发布版本化 Extension Point；Consumer Module B 通过 ExtensionContribution 提供内容。Extension Point 必须有 owner、stable ID、contract/schema version、kind、classification、authorization、lifecycle、dependency 和 removal semantics。B 不得 alter A private schema、inject A aggregate field、建立 private FK 或替换 A business rule。

如果 Owner 删除仍有 consumer 的 Extension Point，dry-plan 必须产生 `BlockedRemoval`，不能 silent break。只有 consumer 先删除/迁移贡献并完成兼容窗口，Owner 才可移除 point。

## 5. Module 间通信

唯一正式机制为：Synchronous Application Query、Synchronous Command、Integration Event、ResourceRef、Reference + Snapshot、Published Read Projection，以及跨模块业务过程使用的 Process Manager/Saga。细节由 [`INTER_MODULE_COMMUNICATION_STANDARD.md`](../standards/INTER_MODULE_COMMUNICATION_STANDARD.md) 和 Proposed ADR-0022 定义。

```text
Module A
  ├─ query/command → Module B Application/Public Contract
  ├─ subscribe ← Module B versioned Integration Event
  ├─ hold → Module B ResourceRef
  ├─ explain → ResourceRef + immutable Snapshot
  ├─ read → Module B Published Projection
  └─ extend → Module B Published Extension Point
```

跨模块没有共享写事务。Owner 本地事务提交业务状态、Audit、Outbox；消费者以 Inbox/幂等处理；Saga 记录业务协调状态并发送下一步 Owner Command；Durable Task 只持有可靠执行状态。

## 6. Business Application Packaging

Package 是声明和派生产物，不是业务数据源：

```text
Business Module Source
  → validate
  → normalize stable identity
  → resolve dependency/capability
  → resolve extension points and contributions
  → compile
  → canonical representation + SHA-256
  → deterministic dry-plan
```

Required platform capability 必须 against compiler input 的显式 capability
evidence/catalog fail-closed resolution；compiled manifest 保存解析证据以便
canonical rebuild，但不产生授权或第二个 capability owner。

### 6.1 稳定身份与兼容

- Module ID、Contribution ID、Extension Point ID、Resource Kind ID 全部 stable/namespaced；
- label、路径、Rust module path、UI route、数据库名称变化不能改变身份；
- package version、manifest schema version、platform version range、module dependency range、contract version 显式声明；
- duplicate ID、ownership collision、unknown dependency/endpoint、cycle、invalid platform dependency、private ref、version incompatibility 均 fail closed；
- compiled manifest 输出稳定排序、可重建并带 digest。

### 6.2 Dry Plan

纯计算的 plan 至少支持：

```text
AddModule / UpgradeModule / DisableModule / EnableModule / RemoveModule
AddContribution / UpdateContribution / RemoveContribution
AddExtensionPoint / RemoveExtensionPoint
DependencyChange / CompatibilityChange
BlockedRemoval / Conflict
```

Plan 不能产生数据库、文件、网络或业务状态副作用。可规划的未知/不兼容依赖、ownership collision、cycle 和未知 extension target 必须生成结构化 `Conflict`；`Uninstalled != Data Purged`：移除注册/代码不自动清除业务事实；Purge 是独立授权、保留规则、审计、验证和恢复流程。

## 7. Lifecycle

```text
Installed → Enabled ↔ Disabled → Uninstalled

Business Data: Retained ────────────────────────────────┐
                                                        └─ Purged only by separate controlled operation
```

启用/禁用影响能力发现与入口，不改变历史事实。卸载前必须解析反向依赖、Extension Point consumers、公开契约引用、projection rebuildability、数据保留和 compatibility。存在 live dependency 或未迁移 consumer 时为 `Blocked`。

模块安装、升级、禁用、启用和移除的未来执行必须采用 Prepare → Preview → Confirm → Execute（高风险操作），绑定主体、租户、package digest、当前 registry version、资源版本和过期时间。

## 8. Synthetic validation design

后续实现计划使用通用 fixture，不能把 Contract/C/Finance 名称硬编码到 generic compiler：

```text
module-a          owner of resource-a and query-a
module-b          independent resource-b, depends on published query-a/event-a
module-extension  consumes A's published extension point
```

验收要求：

| 场景 | 期望 |
|---|---|
| A 独立安装、B 独立安装 | PASS |
| A+B 公开 query/event/ref/projection | PASS |
| extension → A published Extension Point | PASS |
| extension → A private model/repository/FK | FAIL |
| integration event duplicate/ordering/replay | PASS |
| ResourceRef tenant/auth/lifecycle validation | PASS |
| Reference + Snapshot 历史解释 | PASS |
| projection freshness/version/rebuild | PASS |
| remove extension | A unchanged / PASS |
| remove A while B depends on A | `BlockedRemoval` / PASS |
| registration order permutation | identical compiled bytes/digest/plan |
| Platform Core | zero fixture-business knowledge |

这一阶段只设计验收要求；除非需要很小的 compile fixture 证明 contract 规则，不开始 PLAN-0011 runtime，也不重开 PLAN-0009 或继续 C migration。

## 9. 质量属性与 Fitness Functions

- **性能/容量**：同步调用有界 timeout/结果，projection 与事件消费有 freshness/backlog 指标；不能以“高性能”代替 P95/P99 和容量阈值。
- **可用性/恢复**：模块故障隔离；Outbox/Inbox/Saga/Durable Task 可恢复；stale worker fail closed。
- **安全/多租户**：所有公开协作带 tenant/principal/classification，默认拒绝，禁止 private persistence access。
- **可维护性/替换性**：Core 只依赖 contract ports；in-process adapter 必须可替换为 remote adapter。
- **可观测性**：package digest、contract version、correlation/causation、latency、denial、lag、retry、compensation 和 blocked removal 可审计。
- **兼容性**：schema/semver/event replay/unknown optional fields/upgrade/downgrade 有测试。

架构门禁至少自动检查：Platform Core 无具体业务名或依赖；generic crate 无 DB/Web/Provider/业务 crate；private extension/ref/FK/SQL fail；deterministic output required；live dependency removal blocked；uninstall 不等于 purge；semantic authority 不重复。

## 10. 与现有 Baseline 的关系

- ADR-0003/数据架构继续拥有 DDD、所有权、本地事务、Outbox、幂等和补偿原则；
- ADR-0008 继续拥有 Query Model 和 rebuildable Projection 原则；
- ADR-0017 继续拥有唯一 Semantic Contract/Analytics 语义权威；
- ADR-0018 继续拥有 Agent Capability/Workspace 安全；
- ADR-0020 继续拥有 Business Module Isolation 与纯 Rust compiler 基础；
- ADR-0021 负责 Business Application Packaging/Published Extension Point（Accepted）；
- ADR-0022 负责 Inter-Module Communication/Saga（Accepted）；
- 不引入第二 Durable Task Runtime，不修改 PLAN-0009 已完成归档状态，不激活 PLAN-0006。
