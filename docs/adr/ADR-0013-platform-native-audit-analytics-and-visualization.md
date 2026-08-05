# ADR-0013：平台原生审计、分析与可视化

- 状态：Accepted
- 日期：2026-08-05
- 决策者：项目架构所有者
- 适用范围：所有业务 Bounded Context、查询模型、报表、Dashboard、Agent 分析能力和后续数据平台演进

## 1. 背景

企业通用业务平台中的合同、客户、项目、审批、财务、文档和后续领域都需要回答同一类问题：

- 谁在什么时间对哪个业务对象执行了什么操作；
- 数据为什么发生变化，修改前后分别是什么；
- 业务状态如何随时间演进；
- 哪些指标可以按组织、租户、时间和业务维度分析；
- 普通 UI、开放 API、报表和 Agent 如何使用同一份权威数据与指标口径；
- 业务团队如何以最低重复成本获得审计、统计、Dashboard、导出和领域分析能力。

如果每个业务模块分别实现审计表、统计 SQL、图表接口和 Agent Prompt，将产生重复建设、指标漂移、权限绕过、查询不可控和结果不可复现。

如果把数据分析完全交给 Agent，Agent 将被迫直接理解数据库 Schema、生成任意 SQL、临时定义指标并处理租户权限。这会使非确定性模型成为分析权威，与“Rust 业务平台掌握业务事实、权限和最终执行”的既有架构原则冲突。

## 2. 决策

平台采用以下总体定位：

```text
业务领域声明业务语义
→ 平台原生提供审计、投影、指标、查询和基础可视化
→ UI、Open API、报表和 Agent 使用同一受控分析能力
```

审计、数据治理、指标语义、分析查询、基础 Dashboard、报表、导出和下钻属于平台级共享能力。

Agent 位于分析平台之上，负责自然语言理解、分析计划生成、受控工具编排、结果解释和建议生成；Agent 不拥有指标口径、数据权限、聚合计算或正式分析结果。

## 3. 权威数据所有权

本决策不创建拥有业务事实的新业务 Bounded Context。

- 各业务 Bounded Context 继续拥有各自可变权威业务数据和业务规则；
- Audit 保存不可变的操作审计事实，但不得替代业务聚合状态；
- Analytics 保存可重建的投影、指标快照和报表产物，不得成为正式写入入口；
- Agent 保存会话、分析计划和解释结果，不得维护正式业务事实或权威指标值；
- Dashboard 定义和指标定义属于平台配置，必须版本化、授权并可审计。

任何从业务事实派生的分析读模型都必须可从权威数据、领域事件或受控快照重新构建。

## 4. 数据类型分离

平台必须分别建模以下四类数据：

| 类型 | 目的 | 权威性 |
|---|---|---|
| 审计事件 | 证明谁执行了什么操作及结果 | 对操作事实权威，追加写 |
| 技术日志与 Trace | 排查系统运行、依赖和性能问题 | 非业务事实 |
| 领域/集成事件 | 表达已经发生的业务事实 | 由业务上下文发布 |
| 分析事实与投影 | 支持聚合、趋势、下钻和报表 | 可重建、非写入权威 |

四类数据可以通过 `tenant_id`、`actor_id`、`aggregate_id`、`event_id`、`request_id` 和 `trace_id` 关联，但不得使用一张通用日志表相互替代。

## 5. 审计模型

正式业务操作至少记录：

```text
actor / delegated_actor / service_identity
occurred_at / committed_at
tenant / organization / source_channel
action / resource_type / resource_id
reason / action_plan / request_context
before / after / changed_fields
success / rejected / failed
request_id / trace_id / job_id / agent_run_id
```

审计记录必须满足：

- 与业务事务原子写入，或由同事务 Outbox 保证不可丢失；
- 默认追加写，不允许业务调用者覆盖历史；
- 敏感字段按策略脱敏、哈希、摘要或不记录正文；
- 审计失败不得被悄然忽略；
- 审计 Schema、事件版本和保留策略显式管理。

## 6. 分析数据流

```text
权威事务数据
→ 同事务领域事件 / Outbox
→ Projection Worker
→ 分析读模型 / 物化视图 / 指标快照
→ Analytics Query Service
→ UI / Open API / Report / Agent
```

分析投影必须具备：

- 幂等消费；
- 处理偏移和投影版本；
- 重放与重建；
- 失败恢复；
- 数据血缘；
- 数据质量检查；
- 至少一次交付下的重复、乱序和缺口处理。

## 7. 指标语义层

指标不得散落在页面 SQL、报表脚本、数据库视图名称或 Agent Prompt 中。

平台统一管理：

- Metric：业务指标；
- Measure：可聚合度量；
- Dimension：分析维度；
- Time Dimension：时间口径；
- Dataset：受控分析数据集；
- Filter Policy：权限、租户和数据范围规则；
- Metric Version：指标版本；
- Lineage：来源与转换链路。

Analytics Query Service 将结构化指标请求编译为受控查询计划，并强制执行：

- 身份、租户和资源权限；
- 行列级过滤与脱敏；
- 时间范围；
- 扫描量、超时、并发和结果行数限制；
- 指标版本和查询结果元数据；
- 可复现的查询摘要和审计记录。

## 8. 声明式可视化

业务模块通过声明组合平台提供的基础组件：

- 指标卡；
- 趋势图；
- 柱状图、饼图和漏斗；
- 明细表；
- 筛选、排序、分页、联动和下钻；
- 导出、定时报表和订阅；
- 权限、脱敏和水印。

业务模块可以扩展专用算法、查询或前端组件，但不得绕过统一身份、权限、审计、指标语义和资源限制。

## 9. 业务模块声明契约

每个业务模块至少声明：

1. 实体和字段语义；
2. 领域事件；
3. 可审计操作和风险级别；
4. 敏感字段及脱敏策略；
5. 可分析维度、度量和时间维度；
6. 指标业务口径及版本；
7. 数据权限范围；
8. 数据保留策略；
9. 基础 Dashboard 配置；
10. 必要的领域分析 Skill。

平台可以基于声明生成或注册数据字典、审计 Schema、分析 Dataset、查询 API、Agent Tool Schema 和基础 Dashboard，但不得自动猜测领域不变量和指标口径。

## 10. Agent 分析边界

允许提供受控工具：

```text
analytics.list_metrics
analytics.query_metric
analytics.query_dataset
analytics.compare_segments
analytics.drill_down
analytics.detect_anomaly
analytics.explain_result
report.prepare
```

允许业务模块提供领域技能：

```text
contract.analyze_expiration_risk
approval.find_process_bottleneck
finance.analyze_payment_delay
customer.analyze_churn_risk
```

禁止向 Agent 暴露：

```text
execute_sql
query_any_table
read_database_schema
export_unmasked_data
```

Agent 输出属于分析解释、建议、候选异常或报告草稿。正式指标值、权限决策、审计事实和业务状态仍以 Rust 业务平台为准。

## 11. 部署与存储演进

初期：

- PostgreSQL 保存权威业务数据、审计事件、指标定义、Dashboard 定义和中小规模分析投影；
- `business-worker` 消费领域事件并维护投影；
- 物化视图、专用投影表和指标快照承载分析查询；
- 不因预期中的未来规模提前引入独立 OLAP 系统。

满足以下条件时可以拆分 `analytics-worker` 或引入 ClickHouse/现有企业数据平台：

- 分析任务需要独立资源和故障隔离；
- PostgreSQL 扫描和聚合影响在线事务 SLO；
- 数据规模、保留周期或并发超出既定质量属性；
- 存在明确的实时分析、列式存储或大规模报表需求。

拆分后仍保持业务 Bounded Context 的数据所有权，分析存储只保存可重建派生数据。

## 12. 安全要求

- 默认拒绝；
- 所有查询绑定最终用户和服务身份；
- 所有数据集、指标、下钻和导出执行租户与权限过滤；
- 敏感数据按列、记录和结果用途脱敏；
- 导出和报表属于可审计操作；
- Agent 不持有绕过分析服务的数据库凭证；
- 外部 AI 不接收未经授权或未脱敏的数据；
- 查询计划和错误不得泄漏内部 Schema、连接信息或敏感字段。

## 13. 质量属性与验收

本决策要求后续实现定义并验证：

- 审计不可丢失和失败关闭语义；
- 投影延迟、恢复时间和可重建性；
- 指标结果在 UI、API、报表和 Agent 间一致；
- 多租户和权限隔离；
- 查询预算、超时和并发限制；
- 在线事务与分析负载隔离；
- 指标和 Dashboard 版本兼容；
- 数据血缘与质量检查可追踪。

## 14. 被拒绝的方案

### 14.1 每个业务模块独立建设报表和图表

拒绝原因：重复建设、指标口径漂移、权限与审计不一致。

### 14.2 Agent 直接连接数据库并生成 SQL

拒绝原因：无法稳定执行权限、租户隔离、指标版本、查询资源限制和结果复现。

### 14.3 立即引入独立大数据或 OLAP 平台

拒绝原因：当前缺乏规模证据，会增加部署、运维、同步和一致性成本。先使用 PostgreSQL 投影和物化视图，按客观阈值演进。

### 14.4 将分析读模型作为业务写入入口

拒绝原因：破坏单一数据所有者和业务不变量，导致事实来源不清。

## 15. 影响与后果

正面影响：

- 业务模块减少通用审计、统计和 Dashboard 重复代码；
- UI、报表和 Agent 使用相同指标与权限；
- 分析结果可审计、可复现、可重建；
- Agent 可专注于意图理解、编排和解释；
- 后续可以平滑演进到独立分析执行器或 OLAP 存储。

成本与约束：

- 需要设计统一指标和 Dataset Schema；
- 需要维护投影版本、重建和数据质量；
- 业务模块仍必须显式提供领域语义，不能实现真正的“零设计”；
- 需要防止平台分析模块成为新的跨领域共享数据库或万能上下文。

## 16. 迁移与回滚

- 本 ADR 首先建立文档和契约，不立即改变生产数据路径；
- 后续通过独立 PLAN 分阶段实现审计基座、投影、语义层、查询服务和 Dashboard；
- 每个阶段保留现有业务查询入口，直到新接口通过一致性和性能验收；
- 分析投影可删除并从源数据或事件重建；
- 如独立分析存储不满足目标，可回退到 PostgreSQL 投影，不影响业务写模型。

## 17. 关联文档

- `企业AI业务平台与智能助手总体架构方案_v2.md`
- `docs/architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`
- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`
- `docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`
- `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`
- `docs/architecture/SECURITY_ARCHITECTURE.md`
- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
