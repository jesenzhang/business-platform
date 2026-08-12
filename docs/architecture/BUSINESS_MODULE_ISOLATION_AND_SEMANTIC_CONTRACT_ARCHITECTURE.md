# Business Module Isolation 与 Semantic Contract 架构

> 文档类型：Baseline
> 状态：Accepted by ADR-0020
> 生效日期：2026-08-11
> 适用范围：平台核心、业务模块、分析语义注册和未来遗留 ACL

## 1. 目标与非目标

本文定义两个互补的架构层：

1. **Business Module Isolation**：隔离平台核心与具体业务模块，并隔离业务模块 A 与业务模块 B 的所有权、依赖、契约和生命周期；
2. **Semantic Contract**：表达业务模块愿意向 Analytics/查询消费者公开的 Dataset、Projection、Field、Relationship、Measure、Metric、Dimension、Time Dimension、Filter Policy 和 Lineage。

本文不建立新的 Bounded Context，不改变已有业务数据所有者，不实现 WrenAI 运行时、Python、Text-to-SQL、任意 SQL、ClickHouse、OLAP、通用 Workflow、插件热加载或微服务拆分。Analytics 继续遵循 [`ADR-0017`](../adr/ADR-0017-platform-native-analytics-and-visualization.md)，Workspace/Agent 继续遵循 [`ADR-0018`](../adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md)。Business Application Packaging、Published Extension Point 和跨模块通信分别由 Proposed [`ADR-0021`](../adr/ADR-0021-business-application-packaging-and-published-extension-points.md) 与 [`ADR-0022`](../adr/ADR-0022-inter-module-communication-and-business-collaboration.md) 继续审阅；它们不得被本 Baseline 解释为已实现 runtime。

## 2. 两层模型

```text
┌────────────────────────────────────────────────────────────────┐
│ Business Module Isolation                                      │
│ ownership · module manifest · capabilities · lifecycle         │
│ module dependencies · published commands/queries/events        │
└───────────────────────────────┬────────────────────────────────┘
                                │ declares
                                ▼
┌────────────────────────────────────────────────────────────────┐
│ Semantic Contract                                               │
│ business meaning · version · public semantic refs · lineage     │
│ Metric/Measure/Dimension · Filter Policy · classification       │
└───────────────────────────────┬────────────────────────────────┘
                                │ compile/register
                                ▼
┌────────────────────────────────────────────────────────────────┐
│ Analytics platform capability (future runtime)                  │
│ validation · registry · policy · query planning · budgets       │
│ projections are rebuildable and never business authority        │
└────────────────────────────────────────────────────────────────┘
```

Business Module Isolation answers “who owns and exposes this capability?” Semantic Contract answers “what does the exposed capability mean?” A manifest field must not be used as a second business data store, and a semantic definition must not transfer ownership of the underlying business fact to Analytics.

## 3. 统一语言与概念清单

本架构复用 ADR-0017 的 `Dataset`、`Metric`、`Measure`、`Dimension`、`Time Dimension`、`Metric Version`、`Filter Policy`、`Lineage`、`Query Plan`、`ContextDefinition` 和 `QueryExample`。不引入 `WrenMetric`、`WrenModel`、`MDLMetric` 等平行术语。

| 概念 | 所属层 | 语义 |
|---|---|---|
| Business Module | 隔离层 | 以业务能力、统一语言、不变量和数据所有权组织的一组可安装业务能力；不是数据库表集合 |
| Business Module Manifest | 隔离层 | 模块边界、平台能力需求、公开契约、资源类型、分类、迁移命名空间、语义贡献和依赖的声明 |
| Module Lifecycle | 隔离层 | Installed、Enabled、Disabled、Uninstalled；与 Data Retained、Data Purged 两个数据保留状态分开 |
| Semantic Contribution | 语义层 | 一个模块为 Analytics/受控查询发布的版本化语义定义集合 |
| Semantic Object | 语义层 | Dataset、Projection、Field、Relationship、Measure、Metric、Dimension、Time Dimension、Filter Policy 或 Lineage |
| Semantic Reference | 语义层 | 指向已发布语义对象、ResourceRef、Public Projection 或 Reference Snapshot 的稳定引用；不指向私有表/JOIN |
| Compiled Semantic Manifest | 平台能力层 | 通过校验、命名空间归一化、确定性排序和摘要生成的可重建注册输入 |
| Platform Capability | 平台层 | Identity、Policy、Audit、Object Storage、Messaging、Analytics Query Service 等通用能力；不包含 Contract/Finance 业务含义 |
| Resource Kind | 资源边界 | 业务模块公开的业务资源类型名和版本；资源实例仍由其拥有上下文管理 |

以下词语在本架构中保持分离：

- **事实**是业务模块/Bounded Context 拥有的正式状态；**语义贡献**是对事实的可治理解释；
- **业务状态**由业务上下文拥有；**执行状态**由 Durable Task Execution 拥有；**模块生命周期**只表示能力安装/启用和数据保留意图；
- **源 manifest**是声明输入；**compiled manifest**是可删除、可重建的派生注册物；
- **模块依赖**是能力/契约依赖；**语义关系**是业务对象关系；二者不能混为数据库外键或进程启动顺序。

## 4. 模块边界、所有权与目录约定

### 4.1 目标拓扑

```text
platform/
  core/                         # 仅通用平台能力和稳定 contracts
  analytics/                    # Semantic registry/compiler/Query Service
modules/
  <module-id>/                  # 未来业务模块的边界目录；本轮不迁移现有 crate
    module.manifest.json        # 未来可选的声明格式；纯 contract 类型与格式无关
    semantic/                   # 未来 Semantic Contribution 源
integrations/
  legacy-c-contract-management/ # C 项目唯一允许的 C-specific ACL 边界
```

仓库当前还没有 `modules/` 或 `integrations/` 目录。现有 `crates/customer`、`crates/contract`、`crates/project`、`crates/finance`、`crates/document*` 等是已有模块化单体的过渡实现；本轮只登记其目标归属，不进行 mass move、数据库迁移或业务重构。

### 4.2 目标归属 Gap Analysis

| 现有区域 | 目标分类 | 本轮处理 |
|---|---|---|
| `crates/shared-kernel`、`identity`、`policy`、`audit*`、`notification`、`object-storage`、`messaging`、`observability`、`runtime-*`、`public-api-contracts`、`business-api-client` | Platform Capability / stable contract | 保留现状；不得依赖具体业务模块 |
| `crates/customer`、`contract`、`project`、`finance`、`approval` | Business Module / Bounded Context | 保留 transitional 状态；未来分别增加 manifest/semantic contribution |
| `crates/document` | Document Management Business Module | 继续拥有 Document Metadata、Revision 和 storage reference 语义 |
| `crates/document-processing*` | Document Intelligence Business Module + durable execution adapter | 继续拥有 ProcessingJob/Step/AI Task/Candidate/Review 执行状态，不由 Analytics 或模块 manifest 接管 |
| `apps/plan-0009-rehearsal`、`crates/legacy-migration-rehearsal` | Migration Rehearsal / test-only | C-specific 字符串只在 rehearsal 和报告例外内出现；不能变成 Platform Core 依赖 |
| `docs/` 与未来 `integrations/legacy-c-contract-management` | C ACL / documentation exception | 允许描述 C 边界；不代表已完成生产迁移 |
| 当前不存在的 `modules/`、`integrations/`、Analytics runtime | Planned seam | 本轮只实现纯 Rust contract/compiler 和门禁，不创建运行时目录或服务 |

### 4.3 所有权矩阵

| 数据/状态 | 权威所有者 | Analytics/Platform 允许拥有 |
|---|---|---|
| 业务聚合、业务事件、业务状态 | 对应 Business Module/Bounded Context | 只读引用、发布事件、可重建投影 |
| Module Manifest 与 module lifecycle | Module Registry/Platform Control（未来） | 安装/启用元数据、校验结果、审计 |
| Semantic Contribution 源 | 贡献模块的代码/声明 | 版本化注册记录和编译摘要 |
| Compiled Semantic Manifest | Analytics/Platform Registry | 可重建产物、索引、兼容缓存；不成为正式业务事实 |
| Query Plan、预算、策略判定 | Analytics Query Service | 执行记录、摘要审计；不写回业务聚合 |
| C legacy 数据 | 外部 C 项目，直至受控切流 | ACL 的只读快照/迁移证据；不得直接写 Platform Core |

## 5. Module Manifest 契约

Manifest 是平台无关的声明契约，最小字段为：

```text
module_id
module_version
manifest_schema_version
owned_bounded_contexts
required_platform_capabilities
optional_platform_capabilities
published_commands
published_queries
published_events
resource_kinds
data_classification
migration_namespace
semantic_contributions
ui_contributions
agent_tool_contributions
dependencies
compatibility
```

约束：

1. `module_id` 是稳定、全局唯一、大小写规范化的值；semantic ID 使用 `<module-id>.<semantic-id>` 命名空间；
2. 公开命令/查询/事件是版本化协议声明，不把数据库 Row、供应商 DTO、SQL 或内部路径放进 manifest；
3. `required_*` 缺失或版本不兼容时编译拒绝；`optional_*` 缺失时模块必须保持可安全禁用；
4. `migration_namespace` 只为未来 owner migration/catalog 提供隔离名，不代表本轮创建 migration；
5. `dependencies` 只能依赖已发布模块契约，平台能力必须写入 required/optional capability，不得伪装成另一个模块依赖；
6. `ui_contributions` 和 `agent_tool_contributions` 只描述已授权的声明入口，不授予执行权限，不启用任意插件/热加载；
7. manifest 未声明的资源、语义对象、工具和协议默认不可见。

模块生命周期必须显式区分：

```text
Installation: Installed → Enabled ↔ Disabled → Uninstalled
Data:         Retained / Purged
```

`Uninstalled` 不自动等于 `Data Purged`；数据清除是独立、授权、可审计且具有回滚/保留策略的操作。`Disabled` 不撤销历史事实，也不删除可重建投影。

## 6. Semantic Contract 与编译边界

### 6.1 语义对象

Semantic Contract 的最小对象集合为：

- `DatasetDefinition`：公开的业务来源集合、字段、分类、租户范围和血缘；
- `ProjectionDefinition`：面向查询的可重建公开投影；
- `FieldDefinition`：业务字段含义、语义类型、分类和 lineage；
- `RelationshipDefinition`：两个已发布语义对象间的业务关系；
- `MeasureDefinition`：可聚合数值、聚合方式和空值规则；
- `MetricDefinition`：对外指标、依赖 Measure/Dimension、Metric Version 和公式语义；
- `DimensionDefinition` / `TimeDimensionDefinition`：切片维度、时间基准、时区和粒度；
- `FilterPolicyDefinition`：主体、租户、行列策略、脱敏和导出限制的引用；
- `LineageDefinition`：来源上下文、资源/事件/投影版本、变换、freshness 和 checksum 线索。

定义可以使用 `SemanticReference`，但引用目标必须是本模块或其他模块已发布的语义对象。跨模块关系只能使用：

```text
Published Semantic Object
ResourceRef
Public Projection
Reference + Snapshot
```

禁止 private table foreign key、裸表名、任意 JOIN、数据库 URL、凭证和供应商对象进入 Semantic Contract。

### 6.2 编译流水线

```text
load typed manifests/contributions
  → validate local descriptors and ownership
  → validate platform capability/module versions
  → reject illegal dependency and module cycles
  → namespace semantic IDs
  → resolve public semantic references and relationship endpoints
  → reject duplicate IDs and metric ownership conflicts
  → stable sort modules/objects/references
  → canonical JSON + SHA-256
  → Compiled Semantic Manifest (rebuildable)
```

编译器是纯函数式边界，不能访问数据库、对象存储、网络、Provider 或 Agent。未来 Registry 可以持久化 compiled artifact，但必须允许从源声明重建并进行摘要核对。

### 6.3 稳定拒绝条件

- module ID 或 semantic ID 无效；
- module/semantic ID、Metric ownership 或公开 descriptor 重复；
- 必需平台能力/模块依赖缺失或版本不兼容；
- 依赖循环；
- semantic contribution 与 manifest 声明不一致；
- Relationship/Measure/Dimension/Lineage 引用未知端点；
- 跨模块引用标记为 private；
- 模块用平台依赖伪装成 business module dependency；
- 同一 metric key 由多个模块声明 owner；
- 规范化前后产生不同摘要或不可确定排序。

## 7. 运行时调用边界（未来）

Semantic Contract 不允许 Agent 直接使用。目标链路仍为：

```text
User
  → Agent
  → Typed Semantic Query Request
  → Analytics Query Service
  → Semantic Resolver / Policy
  → Query Plan
  → Projection Adapter / controlled execution
```

返回值必须是带 Metric Version、口径、租户、授权结果、时间范围、新鲜度和截断状态的 Read DTO。Agent、MCP、UI 和公开 API 不接收 SQL、物理 Schema、数据库凭证、内部路径或可绕过 Filter Policy 的表达式。

## 8. C legacy ACL 边界

C 项目的正确边界是：

```text
external C Project
  → read-only snapshot / provider adapter
  → integrations/legacy-c-contract-management ACL
  → Contract Management Application API
  → published semantic contribution (if approved)
```

ACL 负责字段、状态、错误和身份转换；Contract Management 负责内部业务不变量；Analytics 只能消费 Contract Management 发布的语义对象。C 的数据库表、状态码、路径和 ORM 类型不能进入 Contract/Finance/Platform Core。PLAN-0009 rehearsal 仅提供迁移证据，不是生产 ACL。

## 9. 质量属性与门禁

| 目标 | 本基础层证据 |
|---|---|
| 可维护性 | 两个小 crate，纯类型/编译 seam；不引入框架和供应商依赖 |
| 确定性 | 输入顺序变化不改变 canonical JSON 与 SHA-256 |
| 安全 | 默认拒绝未知依赖、私有跨模块引用、任意 SQL/Schema/凭证字段 |
| 多租户 | manifest/semantic definitions 声明分类和 owner；真正租户策略仍由业务/Analytics Query Service 执行 |
| 可恢复 | compiled artifact 可从声明重建；本轮不持久化、不迁移 |
| 可替换性 | WrenAI 仅作为参考；compiler API 不依赖 Wren/Python/数据库/Provider |
| 性能 | 本轮只约束编译输入和 O(n) registry 检查方向；运行时 P95/SLO 留给 Analytics 实施计划 |

本轮通过 `architecture-check`、PowerShell source/dependency scans、crate tests、workspace fmt/check/clippy/test 和文档门禁证明边界；无法执行的外部 PostgreSQL/MinIO/Provider/CI 证据保持 `NOT RUN`，不得虚报。

## 10. 后续实现条件

进入真正 Business Module/Analytics runtime 计划前，必须另行明确：租户与授权模型、Manifest Registry 持久化所有者、安装/启用 API、语义注册一致性、版本兼容窗口、Projection 重建、Query Plan/预算、审计、事件、迁移和回滚。该计划不能通过新增 WrenAI 运行时依赖来绕过本架构。

## 11. Architecture Foundation Convergence boundary

本专题的长期不变量收敛为：

```text
DDD Domain != Extension Metadata != Semantic Contract
Platform Core != any concrete business module
Uninstalled != Data Purged
```

Business Module 的完整边界、Contribution 统一入口、模块生命周期和 synthetic validation 见 [`BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md`](BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md)。Module A/B 的 Query、Command、Integration Event、ResourceRef、Reference + Snapshot、Published Projection 和 Process Manager/Saga 见 [`INTER_MODULE_COMMUNICATION_STANDARD.md`](../standards/INTER_MODULE_COMMUNICATION_STANDARD.md)。

在 ADR-0021/0022 被审阅和后续 PLAN-0011 通过 activation gate 前，以上规则是设计约束和验收要求，不是已存在的安装/注册/卸载执行器。Platform Core 的架构门禁必须对 fixture-business knowledge、private persistence access、private extension/reference、semantic ownership collision、dependency cycle、compatibility failure 和 non-deterministic output fail closed。
