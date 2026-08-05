# 数据治理、分析与可视化架构

> 状态：Baseline  
> 日期：2026-08-05  
> 依据：ADR-0013

## 1. 目的

本文定义业务平台原生的数据审计、分析语义、查询投影、基础可视化、报表和 Agent 分析架构。

目标不是建设一个独立于业务平台的通用 BI 产品，而是使所有业务 Bounded Context 以一致、受控、可审计和可演进的方式获得基础数据能力。

## 2. 架构目标

平台必须支持：

- 所有正式业务操作可审计；
- 业务状态变化可通过领域事件表达和追踪；
- 分析读模型可重建、可恢复和可验证；
- 指标、维度、度量、时间口径和 Dataset 统一管理；
- UI、Open API、报表和 Agent 使用同一分析服务；
- 基础 Dashboard 由声明式配置生成；
- 复杂领域分析可以扩展，但不能绕过统一权限、审计和指标语义；
- 当前规模优先复用 PostgreSQL，满足客观条件后再拆分分析执行器和 OLAP 存储。

## 3. 非目标

本架构不要求：

- 第一阶段实现企业级自助 BI 产品；
- 允许用户或 Agent 执行任意 SQL；
- 将 Analytics 建模为拥有业务事实的新 Bounded Context；
- 使用事件溯源重写全部业务聚合；
- 立即引入 ClickHouse、数据湖、Spark 或复杂流处理平台；
- 自动推断领域语义、指标口径或敏感数据分类；
- 使用分析投影承载正式业务写入。

## 4. 核心原则

### 4.1 业务事实仍由领域上下文拥有

合同、客户、审批、付款、文档等事实的所有权不因分析需求发生转移。

Analytics 只拥有：

- 指标和 Dataset 定义；
- Dashboard 和报表定义；
- 可重建分析投影；
- 查询计划和结果元数据；
- 投影偏移、质量检查和报表运行状态。

### 4.2 审计、日志、领域事件和分析事实分离

四类数据分别建模、分别治理，通过公共关联标识建立链路。

### 4.3 一个指标只定义一次

页面、导出、Open API、定时报表和 Agent 不得各自实现相同指标。

### 4.4 Agent 使用受控语义能力

Agent 生成结构化分析请求并调用平台工具，不生成或执行任意数据库查询。

### 4.5 读模型可丢弃、可重建

分析投影不是业务权威。投影损坏、版本升级或存储迁移时，可以删除并从权威来源重新构建。

## 5. 逻辑架构

```text
┌──────────────────── 业务领域层 ────────────────────┐
│ Aggregate / Application Service / Domain Event    │
└──────────────────────────┬─────────────────────────┘
                           │ transaction + outbox
                           ▼
┌────────────── 数据治理与审计层 ────────────────────┐
│ Audit / Classification / Masking / Lineage        │
└──────────────────────────┬─────────────────────────┘
                           │ event stream / rebuild source
                           ▼
┌──────────────── 投影与语义层 ──────────────────────┐
│ Projection / Dataset / Metric / Dimension         │
└──────────────────────────┬─────────────────────────┘
                           ▼
┌──────────────── 受控查询层 ────────────────────────┐
│ AuthZ / Query Plan / Budget / Cache / Metadata    │
└──────────────────────────┬─────────────────────────┘
                           ▼
┌──────────────── 消费层 ────────────────────────────┐
│ UI / Dashboard / Report / Export / Open API       │
│ Agent Analytics Tools / Domain Analytics Skills   │
└────────────────────────────────────────────────────┘
```

## 6. 模块边界

建议模块：

```text
crates/
├── audit/
├── analytics-core/
├── analytics-semantic/
├── analytics-projection/
├── analytics-query/
├── visualization/
├── reporting/
└── agent-analytics/
```

### 6.1 `audit`

负责：

- 审计事件模型；
- 审计写入端口；
- Actor、Resource、Action、Result 和 ChangeSet；
- 敏感字段记录策略；
- 审计查询的受控接口。

不得负责业务规则或修改业务聚合。

### 6.2 `analytics-core`

定义稳定核心类型：

- `MetricId`；
- `DatasetId`；
- `DimensionId`；
- `MeasureId`；
- `MetricVersion`；
- `TimeRange`；
- `FilterExpression`；
- `QueryBudget`；
- `AnalysisResultMetadata`。

不得依赖 PostgreSQL、HTTP、具体图表库或 Agent Runtime。

### 6.3 `analytics-semantic`

负责：

- Metric、Measure、Dimension 和 Dataset 注册；
- 指标版本与兼容规则；
- 维度和时间口径校验；
- 权限策略引用；
- 数据血缘元数据；
- 业务声明契约。

### 6.4 `analytics-projection`

负责：

- 消费领域/集成事件；
- 构建分析投影；
- 偏移、版本、重建和恢复；
- 重复、乱序和缺口处理；
- 数据质量检查。

### 6.5 `analytics-query`

负责：

- 解析结构化分析请求；
- 校验身份、租户、权限、指标和 Dataset；
- 生成受控查询计划；
- 执行扫描量、超时、并发和结果行数限制；
- 返回结构化结果及元数据；
- 查询审计、缓存和结果摘要。

### 6.6 `visualization`

负责：

- Dashboard Schema；
- Widget Schema；
- 筛选、联动、下钻和布局定义；
- 前端组件协议；
- Dashboard 版本和发布状态。

### 6.7 `reporting`

负责：

- 报表定义；
- 报表运行；
- 导出、订阅和定时生成；
- 大结果集异步处理；
- 结果对象存储和过期策略。

### 6.8 `agent-analytics`

负责：

- 将受控分析能力暴露为 Agent Tool；
- 将自然语言分析请求约束为结构化 Schema；
- 传播最终用户身份和租户；
- 限制工具、预算和结果；
- 记录 Agent Run、Tool Call 和查询关联。

不得提供 SQL、数据库 Schema 浏览或未脱敏导出能力。

## 7. 业务模块声明契约

领域模块需要提供平台无法自动推导的语义。

建议声明模型：

```yaml
entity: contract
owner: contract-context
fields:
  amount:
    type: money
    classification: confidential
    measure: true
  department_id:
    dimension: true
  status:
    dimension: true
  signed_at:
    time_dimension: true

auditable_actions:
  - contract.create
  - contract.update
  - contract.submit
  - contract.approve

metrics:
  - contract.total_amount
  - contract.created_count
  - contract.approval_duration
```

声明必须经过代码或启动时校验。未知指标、重复标识、无权限策略、无时间口径或引用不存在字段时必须失败关闭。

## 8. 审计架构

### 8.1 审计事件

```text
AuditEvent
├── audit_event_id
├── schema_version
├── tenant_id
├── actor
├── delegated_actor
├── source_channel
├── action
├── resource
├── occurred_at
├── committed_at
├── result
├── change_set
├── reason
├── request_id
├── trace_id
├── causation_id
├── correlation_id
├── job_id
└── agent_run_id
```

### 8.2 原子性

优先顺序：

1. 业务状态、审计事件和 Outbox 同一数据库事务；
2. 无法同事务时，使用明确的 Saga/补偿和不可丢失中间状态；
3. 不允许“先提交业务，再尽力写审计”的 fire-and-forget 模式。

### 8.3 变更内容

- 低敏感字段可以记录 Before/After；
- 高敏感字段仅记录字段名、变更类型、摘要或哈希；
- 文件正文、Prompt、Token、Secret、签名 URL 和原始凭证不得写入审计；
- 大型差异存储为受保护对象并记录引用和 checksum。

## 9. 领域事件与分析投影

### 9.1 事件来源

分析投影优先消费稳定的领域/集成事件。

没有可用事件时，可以通过受控快照或批处理初始化，但必须明确：

- 数据来源；
- 一致性窗口；
- 重建步骤；
- 水位线；
- 与后续增量事件的衔接。

### 9.2 投影状态

每个投影至少保存：

```text
projection_name
projection_version
partition_key
last_event_id / last_offset
updated_at
rebuild_generation
status
last_error_class
```

### 9.3 重建

重建过程必须：

- 创建新 generation；
- 不覆盖当前可用投影；
- 校验计数、checksum 或不变量；
- 原子切换到新 generation；
- 保留回退窗口；
- 记录完整审计和运行状态。

## 10. 指标语义模型

### 10.1 Metric

Metric 是具有业务名称、口径、版本、权限和聚合规则的正式定义。

### 10.2 Measure

Measure 表示可聚合数值或持续时间，必须声明允许的聚合方法，例如 `sum`、`count`、`avg`、`min`、`max`、`distinct_count`。

### 10.3 Dimension

Dimension 用于分组、过滤和下钻。每个维度声明：

- 数据类型；
- 来源字段或投影；
- 权限和脱敏；
- 层级关系；
- 可用过滤操作；
- 是否允许 Agent 使用。

### 10.4 Time Dimension

时间指标必须明确使用：

- 业务发生时间；
- 创建时间；
- 提交时间；
- 完成时间；
- 会计期间；
- 租户时区。

不得由页面或 Agent 临时选择含义不明的 `created_at`。

### 10.5 Dataset

Dataset 是受控、版本化的分析数据边界，不等于数据库表。Dataset 公开业务语义字段和允许操作，隐藏底层 Schema、连接和实现细节。

## 11. 查询架构

### 11.1 请求模型

```json
{
  "metric": "contract.total_amount",
  "metric_version": 1,
  "time_range": {
    "from": "2026-01-01",
    "to": "2026-12-31"
  },
  "dimensions": ["department", "month"],
  "filters": [],
  "order": [],
  "limit": 100
}
```

### 11.2 查询执行顺序

```text
认证
→ 解析租户与委托身份
→ 指标/Dataset 注册校验
→ 权限与数据范围决策
→ 过滤和脱敏注入
→ 查询预算评估
→ 生成数据库适配器查询
→ 执行
→ 结果后处理
→ 查询审计和元数据
```

### 11.3 资源限制

必须支持：

- 最大时间范围；
- 最大维度数；
- 最大结果行数；
- 最大扫描量或估算成本；
- 查询超时；
- 用户、租户和全局并发；
- 导出转异步任务阈值；
- 缓存命中和失效规则。

超出限制时返回结构化拒绝，不自动退化为无限制查询。

## 12. Dashboard 与报表

### 12.1 Dashboard Schema

```yaml
dashboard: contract_overview
version: 1
permission: contract.analytics.read
filters:
  - department
  - contract_type
  - signed_date
widgets:
  - id: total_amount
    type: metric
    metric: contract.total_amount
  - id: monthly_count
    type: line_chart
    metric: contract.created_count
    dimension: month
  - id: by_department
    type: bar_chart
    metric: contract.total_amount
    dimension: department
```

### 12.2 发布模型

Dashboard 定义建议具有：

```text
Draft → Validated → Published → Deprecated
```

发布前验证：

- 引用指标和 Dataset 存在；
- 权限不弱于底层数据；
- 默认时间范围和结果限制安全；
- 所有 Widget 可执行；
- 无敏感字段泄漏。

### 12.3 报表

小结果同步返回；大结果创建 Durable Report Run：

```text
Pending → Running → Materializing → Stored → Completed
                         ↘ Failed / Cancelled
```

报表文件存储于 MinIO/S3，PostgreSQL 保存元数据、checksum、权限、生成参数和过期时间。

## 13. Agent 分析集成

### 13.1 工具接口

Agent Tool 输入必须是版本化结构化 Schema，不接受 SQL 字符串。

### 13.2 身份传播

每次调用传播：

```text
service_identity
+
end_user_identity
+
tenant_id
+
authentication_level
+
agent_run_id
```

### 13.3 结果解释

Agent 可以：

- 比较指标；
- 寻找异常候选；
- 生成归因假设；
- 建议下钻；
- 生成报告草稿。

Agent 不得将推理结果写回正式业务数据，除非经过正常的 Prepare → Preview → Confirm → Execute 业务写入流程。

### 13.4 领域分析 Skill

领域 Skill 可以包含确定性算法或模型调用，但必须：

- 通过 Analytics Query Service 获取数据；
- 声明输入、输出和适用范围；
- 记录算法/模型版本；
- 返回证据和置信度；
- 受权限、预算、审计和超时约束。

## 14. 持久化策略

### 14.1 PostgreSQL

初期保存：

```text
metric_definitions
metric_versions
dataset_definitions
dimension_definitions
measure_definitions
analytics_projections
projection_offsets
metric_snapshots
dashboard_definitions
report_definitions
report_runs
data_quality_results
```

具体表名由实现 PLAN 决定，本文定义语义而非冻结物理 Schema。

### 14.2 SQLite

SQLite 仅用于本地单进程和开发适配器：

- 不承诺多 Worker 投影；
- 不承诺独立 `analytics-worker`；
- 不用于生产大规模分析；
- 必须使用与 PostgreSQL 相同的核心契约测试；
- 不支持的能力必须明确拒绝，不得静默降低一致性。

### 14.3 独立 OLAP

引入独立 OLAP 前必须有 ADR 或当前 ADR 的替代决策，明确：

- 数据复制和一致性窗口；
- 权限和租户隔离；
- 重建与回滚；
- 运维成本；
- RPO/RTO；
- PostgreSQL 和 OLAP 的职责边界。

## 15. 部署架构

初期：

```text
business-api
business-worker  ← projection / small report execution
ai-worker
agent-adapter
agent-runtime
PostgreSQL
MinIO/S3
NATS/Kafka
```

条件满足后：

```text
analytics-worker × 1..N
optional OLAP adapter
```

`analytics-worker` 只消费事件、维护派生数据和执行报表，不直接修改业务上下文私有表。

## 16. 故障、恢复与一致性

### 16.1 投影失败

- 暂时性错误按分类重试；
- 无法解析的事件进入失败状态和人工处理队列；
- 不跳过未知事件并继续推进偏移；
- 恢复后从最后确认偏移继续；
- 重复事件必须幂等。

### 16.2 指标定义变更

- 产生新 Metric Version；
- 旧版本在兼容期继续可查询；
- 需要重建时生成新 Projection Version；
- 不原地篡改历史报表的指标口径。

### 16.3 查询结果未知

只读查询超时可以安全重试，但大规模报表必须使用 Durable Report Run 和幂等键，避免重复生成与重复占用资源。

## 17. 安全与隐私

- 默认拒绝；
- Dataset 和 Metric 不是权限绕过层；
- 权限决策在查询执行前完成；
- 缓存键包含租户、主体、权限范围、指标版本和过滤摘要；
- 缓存不得跨权限范围复用；
- 日志、Trace、审计和 Agent Context 不记录未脱敏结果正文；
- 导出文件使用短期授权、对象级权限和明确过期；
- Prompt Injection 不得改变指标、Dataset、权限或 Tool 白名单；
- 任意 SQL、通用数据库浏览和未脱敏导出保持禁止。

## 18. 可观测性

技术指标：

- 投影积压和延迟；
- 投影失败、重试和重建状态；
- 查询 P50/P95/P99；
- 查询拒绝、超时和预算超限；
- 报表队列、运行时长和失败率；
- 缓存命中率；
- PostgreSQL 分析负载对在线事务的影响。

业务/数据质量指标：

- 指标结果完整性；
- 事件和投影计数差异；
- 数据新鲜度；
- 空值、重复和违反约束的记录数；
- Dashboard 与指标使用率；
- Agent 分析成功率、下钻轮次和权限拒绝率。

## 19. API 与事件契约

公开分析 API 必须版本化，并至少返回：

```text
metric / dataset
metric_version
query_id
as_of
freshness
applied_filters
result_schema
truncated
next_cursor
```

分析事件包含：

```text
event_id
schema_version
tenant_id
projection_name
projection_version
correlation_id
causation_id
occurred_at
```

不得将底层数据库 SQL、表名或连接细节作为公开协议。

## 20. 测试策略

至少覆盖：

- Metric 和 Dataset 注册校验；
- 权限、租户、行列过滤和脱敏；
- 同一指标在 UI/API/Agent 请求下结果一致；
- 投影重复、乱序、失败、重启和重建；
- 指标版本升级和历史结果稳定；
- 查询超时、预算、并发和分页；
- 报表幂等、取消、恢复和文件权限；
- Agent 无法调用 SQL、越权 Dataset 或未脱敏导出；
- PostgreSQL 与 SQLite 支持矩阵；
- 在线事务与分析负载隔离。

## 21. 架构适配门禁

后续实现应建立可自动检查的规则：

- Domain/Application 不依赖数据库、图表或 Agent 实现；
- Agent Tool 不接受 SQL；
- 分析查询必须经过 `analytics-query`；
- 投影代码不得写入其他业务上下文私有表；
- Dashboard 只能引用已注册 Metric/Dataset；
- Metric 必须具有版本和权限；
- 导出必须经过 Reporting Application Service；
- 新独立分析部署单元必须同步 Deployment Architecture 和 ADR。

## 22. 分阶段实施

### 阶段 A：文档与契约

- 接受 ADR-0013；
- 建立本 Baseline；
- 定义核心术语、所有权、边界和验收。

### 阶段 B：统一审计基座

- AuditEvent 核心模型；
- 同事务写入/Outbox；
- 敏感字段策略；
- 审计查询与测试。

### 阶段 C：分析核心与投影

- Metric、Dataset、Dimension、Measure；
- 投影偏移、版本和重建；
- PostgreSQL 读模型；
- 数据质量检查。

### 阶段 D：查询与可视化

- Analytics Query Service；
- 查询预算和权限；
- Dashboard Schema；
- 报表和导出。

### 阶段 E：Agent 分析

- 只读分析 Tool；
- 领域分析 Skill；
- 解释、归因和报告草稿；
- Agent 安全和评估。

### 阶段 F：规模化演进

仅在质量属性触发时拆分 `analytics-worker` 或引入独立 OLAP 适配器。

## 23. 完成定义

该架构能力的 MVP 至少满足：

1. 正式业务写操作具有不可丢失审计；
2. 至少一个领域事件可构建可重建分析投影；
3. 至少一个版本化 Metric 和 Dataset 可查询；
4. UI 和 Agent 通过同一 Analytics Query Service 获得一致结果；
5. 多租户、权限、脱敏、预算和审计测试通过；
6. 基础 Dashboard 可由声明配置生成；
7. 投影可以在崩溃后恢复并从源数据重建；
8. Agent 不具备任意 SQL、数据库 Schema 浏览或未脱敏导出能力。
