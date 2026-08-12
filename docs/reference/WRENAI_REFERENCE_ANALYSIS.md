# Canner/WrenAI 参考项目分析

> 检查日期：2026-08-11
> 固定项目：`Canner/WrenAI` 证据提交：[`ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902`](https://github.com/Canner/WrenAI/tree/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902)
> 默认分支：`main`
> 研究目的：为 Business Module Isolation 与 Semantic Contract 提供可追溯的外部设计输入，不引入 WrenAI 运行时。

## 1. 结论摘要

Canner/WrenAI 的高价值部分是 MDL（Modeling Definition Language）作为“业务语义与物理数据之间的声明式契约”，以及 `source → validate → compile → derived target` 的构建纪律。它能回答“指标、维度、关系和可见字段是什么意思”，但不能回答“平台核心与具体业务模块如何隔离、谁拥有正式事实、跨模块如何协作”。

本项目因此采用两个互补层：

```text
Business Module Isolation
  解决：平台核心 ↔ 业务模块、业务模块 A ↔ 业务模块 B 的所有权/依赖/生命周期隔离

Semantic Contract
  解决：业务含义 ↔ 物理存储/投影的语义声明、版本、关系、指标与血缘
```

二者都不能成为第二业务内核。业务模块仍拥有正式业务事实；Analytics 只拥有语义注册、编译产物、查询计划与可重建分析投影。此结论与 [`ADR-0017`](../adr/ADR-0017-platform-native-analytics-and-visualization.md) 一致，不新增第二套 Metric/Dataset 术语。

## 2. 事实记录

### 2.1 项目、提交和许可证

| 项目 | 固定事实 |
|---|---|
| Git 仓库 | [`Canner/WrenAI`](https://github.com/Canner/WrenAI) |
| 研究提交 | `ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902`，2026-08-11 检查时 `main` 最新提交 |
| 仓库 API 许可证元数据 | GitHub API 返回 `Other` / `NOASSERTION`；不能仅凭仓库级字段决定复用范围 |
| 路径许可证 | 仓库许可证映射声明 `core/**`、`sdk/**`、`skills/**`、`examples/**` 和根目录为 Apache-2.0；`docs/**` 为 CC BY-4.0；其他路径以各自 package manifest 和许可证文件为准 |
| 本项目复用方式 | 本轮不复制 WrenAI 源码、依赖、模型、Connector、MCP Server 或许可证文本；只记录设计事实与适配边界 |

证据入口：[`LICENSE`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/LICENSE)、[`README.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/README.md)。如果未来直接复用实现，必须逐路径核验许可证、依赖许可证和 NOTICE 要求，并单独通过许可证扫描。

### 2.2 关键事实

- MDL 是 Wren 的语义中心：源 YAML 描述 Models、Columns、Relationships、Views、Cubes、Measures、Dimensions 和 Time Dimensions，再编译为派生的 `target/mdl.json`。参见 [`what_is_mdl.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/concepts/what_is_mdl.md) 与 [`mdl.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/reference/mdl.md)。
- Wren 的 Context 分为 Structural、Semantic、Business、Operational、Behavioral 五层；业务规则和术语进入 `knowledge/`，向量/检索 memory 是可重建派生物。参见 [`what_is_context.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/concepts/what_is_context.md) 与 [`memory_system.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/concepts/memory_system.md)。
- Wren 的规划层采用 parse/validate/plan/dry-plan，再由 Connector 执行；`wren-core` 是 Rust 语义引擎，Python 层负责编排和适配。参见 [`architecture.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/reference/architecture.md) 与 [`correctness.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/concepts/correctness.md)。
- Wren MCP/CLI 同时暴露 `dry_plan`、`dry_run`、`query_cube`、schema/knowledge 读取和 `run_sql` 等能力；HTTP MCP 文档的默认安全假设是本地使用，不能直接作为本平台的 Agent 安全模型。参见 [`mcp.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/reference/mcp.md)。
- Wren 的 OSS 与商业功能边界包括 UI、RLS/CLS、SSO 等产品化能力；本项目不能把商业文档中的安全能力当作已获得的开源实现。参见 [`oss_vs_commercial.md`](https://github.com/Canner/WrenAI/blob/ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902/docs/core/concepts/oss_vs_commercial.md)。

## 3. 采用矩阵

| WrenAI 设计 | 决策 | Business Platform 适配 |
|---|---|---|
| MDL 作为语义契约 | Adopt / Adapt | 采用为 Semantic Contract；字段改为业务语义 ID、ResourceRef、公开 Projection 和版本化 Lineage，不暴露物理表/Schema/SQL |
| 源定义与派生 compiled target 分离 | Adopt | `BusinessModuleManifest` 与 `SemanticContribution` 是声明源；`CompiledSemanticManifest` 是可重建、带摘要的派生注册产物 |
| Models/Columns/Relationships/Views/Cubes | Adapt | 映射为 Dataset/Projection/Field/Relationship/Metric/Measure/Dimension/Time Dimension；不直接复制 Wren 的 schema v5 或 JSON 字段名 |
| `validate_project` 与结构化诊断 | Adopt | 纯 Rust compiler 返回稳定错误分类：重复、版本、未声明依赖、关系端点、所有权冲突、循环依赖和非法平台依赖 |
| `wren-core` 的 Rust 规划内核 | Defer / Evaluate later | 先独立实现平台语义注册与编译；若未来复用，必须通过 ACL/Adapter、许可证和性能证据，不把 Wren 类型引入核心领域 |
| Context 五层与 knowledge | Adapt | Platform Context/Workspace 继续遵循 ADR-0018；业务模块拥有业务术语和规则，Knowledge 不能覆盖正式业务事实或 ADR-0017 指标版本 |
| LanceDB/vector memory | Defer | 只可作为可重建检索加速器；当前不引入 Python、LanceDB、Arrow、Sentence Transformers 或向量数据库 |
| `dry-plan`/plan-before-execution | Adopt / Adapt | Analytics Query Service 未来采用 Typed Semantic Query → Query Plan；Agent 只看 Read DTO、口径、版本和新鲜度，不看生成 SQL |
| Connector 执行数据库查询 | Adapt | 由平台 Query Service/Projection Adapter 执行，拥有租户、策略、预算和审计；模块不能直接执行任意查询 |
| MCP `run_sql`、`dry_run` 与原始 schema 工具 | Reject | 与本项目 Agent 禁止任意 SQL/Schema/数据库凭证的安全边界冲突 |
| ClickHouse/OLAP 与通用查询引擎 | Reject for this round / Defer for evidence | ADR-0017 规定只有在 PostgreSQL 投影无法满足可测量质量属性后评估；本轮不实现 |
| Wren Project/Plugin/热加载/多服务拆分 | Reject for this round | 本项目优先模块化单体；manifest 是编译契约，不是运行时插件系统或微服务部署协议 |

## 4. 统一语言映射

| WrenAI 术语 | 本项目正式术语 | 说明 |
|---|---|---|
| MDL | Semantic Contract | 业务模块声明可被分析消费的语义边界；不是业务事实存储 |
| Model | Dataset / Resource Projection | 只能引用发布的资源或投影；不能把物理表名当作公开模型 |
| Column | Field | 字段含义、分类、权限与血缘；物理列只是 Adapter 内部映射 |
| Relationship | Relationship | 两个语义对象之间的业务关系；跨模块通过公开对象、ResourceRef、Public Projection 或 Reference + Snapshot |
| Measure | Measure | 可聚合数值及聚合、空值规则和数据来源 |
| Metric | Metric + Metric Version | 指标口径、所有者、版本、依赖和生效窗口沿用 ADR-0017 |
| Dimension | Dimension | 可切片业务维度及权限分类 |
| Time Dimension | Time Dimension | 业务时间、记录时间、时区和粒度 |
| View/Cube | Projection / Metric Composition | 只作为分析声明和可重建投影定义，不成为业务权威 |
| Project Context | Module Context + Platform Context | 业务模块负责语义和业务知识；平台负责注册、授权、查询计划和执行预算 |
| target/mdl.json | Compiled Semantic Manifest | 编译、排序、校验、摘要后生成；不可手工编辑，也不反向拥有业务事实 |
| Memory | Rebuildable Retrieval Projection | 未来可用于检索，不得成为指标或权限的第二权威 |

## 5. 对本项目架构的具体输入

### 5.1 两个互补问题

仅引入 MDL 会留下平台核心与业务模块的依赖/生命周期边界；仅引入 Business Module Isolation 又无法稳定表达指标、字段、关系、过滤和血缘。因此正式设计为：

```text
Module Manifest
  module_id / owned_contexts / capabilities / contracts / resource kinds
  data classification / migration namespace / semantic contributions / dependencies
          │
          ▼
Semantic Contract
  Dataset / Projection / Field / Relationship / Measure / Metric
  Dimension / Time Dimension / Filter Policy / Lineage
          │
          ▼
Deterministic Compiler
  validate → normalize namespace → resolve public refs → sort → canonical JSON → SHA-256
          │
          ▼
Analytics Registry / Query Service（未来实现）
```

Module Manifest 负责“谁能贡献什么、依赖什么”；Semantic Contract 负责“贡献的业务含义是什么”。二者通过 module ID、version、semantic contribution descriptor 和 owner 关联，但不共享业务数据库表。

### 5.2 C 项目边界

PLAN-0009 仅完成 C legacy contract/document 的只读 rehearsal，不能把 C 的实体名、数据库模型或状态码提升为 Platform Core/Contract Management 的内部语言。未来受控迁移必须保持：

```text
C Project (external read-only source)
  → integrations/legacy-c-contract-management (ACL / translator)
  → Contract Management business module
  → platform public capabilities / published semantic objects
```

本轮只建立 manifest/compiler 与架构门禁，不新增 `integrations/` 代码、不读写 C 数据、不新增迁移、不启动 PLAN-0009 migration phase。

## 6. 明确拒绝的依赖与能力

本轮验收必须保持下列结果：

- `Cargo.toml` 不包含 WrenAI、Python、LanceDB、DataFusion、SQLGlot、ClickHouse 或通用 Text-to-SQL 运行时依赖；
- 新 crate 只使用 serde/serde_json/sha2/thiserror 等纯库依赖，不依赖 Axum、SQLx、Reqwest、对象存储、Messaging、AI Provider 或具体业务 crate；
- Agent、MCP、Public API 和日志不返回 SQL、Schema、数据库 URL、凭证、物理表名、存储 key 或内部路径；
- 没有 Wren-specific `WrenModel`、`WrenMetric`、`MDL` 第二套运行时术语；正式术语继续由 ADR-0017、ADR-0018 和本仓库架构文档提供；
- 没有热卸载、任意插件执行、通用 DAG、工作流设计器、OLAP 迁移或微服务拆分。

## 7. 结论

WrenAI 被正式登记为参考项目，采用的是“可声明、可校验、可编译、可重建的语义契约”这一设计思想；不采用其 Python/Connector/MCP/任意 SQL/数据库 schema 暴露和商业安全边界。Business Module Isolation 与 Semantic Contract 的实现边界、错误模型和确定性编译由 [`BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`](../architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md)、[`ADR-0020`](../adr/ADR-0020-business-module-isolation-and-semantic-contract.md) 和 [`PLAN-0010`](../plans/archive/2026/PLAN-0010-business-module-isolation-and-semantic-contract-foundation.md) 固化。
