# ADR-0020：Business Module Isolation 与 Semantic Contract

> 状态：Accepted
> 日期：2026-08-11
> 决策范围：平台核心、业务模块边界、分析语义契约和 C legacy ACL 预留

## 1. 背景

ADR-0017 已接受平台原生 Analytics/Visualization 和统一的 Metric/Dataset 语义，但当前仓库没有声明业务模块可公开哪些语义、如何与平台能力隔离、如何避免业务模块彼此直接依赖。Canner/WrenAI 的 MDL 证明了声明式语义契约和可重建编译产物的价值，但 WrenAI 本身不是本平台的业务边界、权限模型或运行时依赖。

如果只复制 MDL，物理 schema、SQL 和具体业务模块可能重新进入平台核心；如果只定义模块清单，Metric/Measure/Dimension/Lineage 的含义仍会散落在 UI、报表和 Agent 中。因此需要一个长期架构决策把两者组合为两个互补 seam。

## 2. 决策

### 2.1 两个互补层

1. **Business Module Isolation** 是所有权、依赖、公开契约和生命周期层。模块按业务能力、统一语言、不变量和数据所有权划分，不按表、页面、Prompt、Topic 或供应商 SDK 划分。
2. **Semantic Contract** 是业务含义和分析发布层。业务模块声明 Dataset、Projection、Field、Relationship、Measure、Metric、Dimension、Time Dimension、Filter Policy 和 Lineage；Analytics 负责校验、编译、注册、授权、查询计划和预算。
3. 两层共享稳定 module ID/version 和 contribution descriptor，但不共享业务数据库事务，不建立 Analytics 对模块私有表的外键或裸 JOIN。

### 2.2 Module Manifest

每个未来业务模块必须有平台无关的 manifest，至少包含：

```text
module_id / module_version / manifest_schema_version
owned_bounded_contexts
required_platform_capabilities / optional_platform_capabilities
published_commands / published_queries / published_events
resource_kinds / data_classification / migration_namespace
semantic_contributions / ui_contributions / agent_tool_contributions
dependencies / compatibility
```

Platform capability 必须走 capability 字段；业务模块依赖必须指向已发布模块契约。`ui_contributions` 和 `agent_tool_contributions` 是声明，不是权限授予或插件执行协议。

### 2.3 所有权与生命周期

- 业务模块/Bounded Context 继续拥有正式业务事实、业务状态、业务事件和不变量；
- Analytics 只拥有语义注册、compiled manifest、可重建投影、Query Plan 和执行摘要；
- Manifest Registry（未来）拥有安装/启用元数据和校验记录，不拥有业务事实；
- 模块生命周期 `Installed / Enabled / Disabled / Uninstalled` 与数据状态 `Data Retained / Data Purged` 分离；卸载不自动删除数据，清除必须是显式授权、审计和可验证的操作。

### 2.4 Semantic Contract 编译

纯 Rust compiler 负责：

```text
validate → normalize <module-id>.<semantic-id>
→ resolve published references → reject conflicts/cycles
→ stable sort → canonical JSON → SHA-256
```

必须拒绝重复模块/语义 ID、Metric ownership 冲突、版本不兼容、未知依赖/关系端点、循环依赖、非法平台依赖、未声明 contribution 和跨模块 private 引用。编译输入和结果不得携带 SQL、物理 schema、数据库 URL、凭证或内部对象路径。

### 2.5 C legacy 边界

PLAN-0009 的 C 项目是外部只读 rehearsal，不改变本决策。未来 C 只能通过：

```text
C Project
  → integrations/legacy-c-contract-management ACL
  → Contract Management Application API
  → published semantic contribution
```

C-specific 名称、表、状态码和 SDK 只能在 ACL、rehearsal、迁移报告和文档例外中出现，不能进入 Platform Core、Contract module 或通用 semantic compiler。

## 3. 不采用的方案

- 不引入 WrenAI/Python/LanceDB/DataFusion/SQLGlot/ClickHouse/Text-to-SQL/通用 OLAP 运行时；
- 不直接复制 Wren MDL schema v5，不把 `target/mdl.json` 当成本平台协议；
- 不暴露 Wren MCP 的 `run_sql`、原始 schema、数据库凭证或商业 RLS/CLS/SSO 假设；
- 不把 Module Manifest 变成任意插件、热卸载、通用工作流或微服务拆分协议；
- 不新增第二套 `WrenMetric`/`WrenModel` 术语，ADR-0017 的语义层继续是唯一平台语义权威。

## 4. 影响

### 正面影响

- 业务含义与平台实现形成清晰 seam，业务模块可独立声明公开分析能力；
- 依赖、租户分类、迁移命名空间和生命周期有可自动检查的入口；
- Semantic Contract 可确定性编译、摘要核对和重建，适合未来 Registry/Projection；
- C legacy 的 ACL 预留不会污染平台核心，也不自动激活 PLAN-0009 迁移。

### 接受的成本和限制

- 本轮需要维护 manifest/semantic contract 类型和跨模块引用规则；
- 运行时 Registry、安装 API、Query Service、投影、权限和审计仍未实现；
- 现有业务 crate 暂时是 transitional，目录移动和模块化迁移需要后续独立 Plan；
- WrenAI 的 SQL 规划能力不能立即复用，未来复用必须有独立许可证、性能和安全证据。

## 5. 验收与回滚

本 ADR 的首个实现由 `PLAN-0010-business-module-isolation-and-semantic-contract-foundation.md` 验收：两个纯 Rust crate、编译冲突测试、确定性摘要测试、依赖/业务名称/任意 SQL Fitness Functions、文档登记和完整 workspace gates。它不新增数据库、迁移、API、Worker、部署单元或外部依赖。

回滚只需移除新 crate、manifest registry/compiler 代码和本 ADR 的实现引用；没有持久化数据、事件、迁移或外部系统副作用。若未来事实证明模块 manifest 或语义层需要拆分/替代，必须通过新的 ADR 处理，不得静默改变所有权。

## 6. 关联文档

- [`BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`](../architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md)
- [`ADR-0017`](ADR-0017-platform-native-analytics-and-visualization.md)
- [`ADR-0018`](ADR-0018-enterprise-ai-workspace-and-capability-security.md)
- [`ADR-0019`](ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md)
- [`WRENAI_REFERENCE_ANALYSIS.md`](../reference/WRENAI_REFERENCE_ANALYSIS.md)
- [`PLAN-0010`](../plans/archive/2026/PLAN-0010-business-module-isolation-and-semantic-contract-foundation.md)
