# ADR-0017：平台原生分析与可视化

> 文档 ID：ADR-0017
> 版本：1.0
> 状态：Accepted
> 日期：2026-08-06
> 所有者：Analytics and Visualization 能力（待实施）
> 关联：ADR-0008、ADR-0013、ADR-0014、ADR-0015、ADR-0016

## 1. 背景

平台需要为 UI、开放 API、报表和可选 Agent 提供一致的统计、下钻和导出能力。已有
业务上下文分别拥有权威业务事实，PLAN-0005 已集成 Runtime Audit、完整性 Finding、
受控修复、Repair Ledger 以及 Lease/Fence Recovery。分析能力必须建立在这些已接受边界
之上，而不能通过共享表、临时 SQL 或第二套业务规则重新获得事实。

## 2. 决策

采用平台原生、可重建的分析与可视化能力：

```text
业务领域声明业务语义
→ 权威业务数据、领域/集成事件和现有 Runtime Audit
→ 可重建分析投影
→ 版本化指标语义层
→ 受控 Analytics Query Service
→ UI / Open API / Report / Agent
```

Analytics and Visualization 是独立的平台能力边界，但初期仍在模块化单体内实现。
它只拥有派生数据和定义，不成为任何业务事实的写入入口。

## 3. 所有权与边界

业务 Bounded Context 继续拥有权威业务数据、业务规则、生命周期和正式写入。Analytics
只拥有：

- 可重建的分析投影；
- 指标定义及其版本；
- Dataset、Dashboard 和 Report 定义；
- 查询执行元数据、可重建指标快照和报表产物。

Analytics 不拥有业务状态，不替代 Audit、Integrity 或 Repair。它不重新定义
`AuditEvent`、Finding、Repair Ledger、哈希链或修复审批语义；这些语义分别由
ADR-0013、ADR-0014、ADR-0015 和 ADR-0016 及其 Baseline 负责。

## 4. 统一入口

UI、Open API、报表和 Agent 必须复用同一受控指标语义和查询执行层。Agent 是分析能力的
受控客户端，不拥有指标口径、权限决策或正式指标值。Agent 不得获得任意 SQL、任意表
查询、数据库 Schema 浏览、未脱敏导出或分析服务外的数据库凭证。

所有查询在最终用户身份、租户、行列级策略、脱敏策略、预算和审计上下文中执行。
高风险导出或跨租户操作不因来自 Agent 而降低审批和确认要求。

## 5. 初始技术选择与演进

初期采用 PostgreSQL 专用投影表、物化视图和指标快照，复用 ADR-0008 的 Query Model
和 Read Projection 约束。投影是最终一致的派生读模型，可通过事件重放或权威数据重建。

只有在有可测量证据显示 PostgreSQL 投影无法满足查询延迟、扫描量、并发、保留窗口、
重建恢复时间或资源隔离目标时，才评估独立 `analytics-worker`、ClickHouse 或其他
OLAP。独立分析存储仍是可重建派生数据，不改变业务数据所有权；拆分部署或引入新基础
设施必须另行通过 ADR、计划、迁移和回滚评审。

## 6. 一致性与审计

分析数据允许有界陈旧，必须记录投影 offset、版本、血缘、质量结果、延迟和恢复状态。
权威业务写入仍遵循：正式业务状态、AuditEvent 和相关 Outbox 由数据所有者在同一本地
事务中提交；AuditEvent 写入失败则业务事务失败并回滚；Outbox 只负责后续发布，不替代
权威审计记录。审计载荷使用 `change_summary`、`changed_field_names`、
`resource_version`、策略允许时的 `redacted_before_after` 和稳定失败码，不强制保存
完整敏感 Before/After。

## 7. 后果

正面结果是业务规则只有一份，所有入口拥有一致的口径、权限和查询审计，投影可重建且能
逐步演进到 OLAP。代价是分析结果不是读己之写的强一致事实，必须运营 offset、缺口、
重放、质量和导出成本；新的实现还需要独立的契约、性能和安全门禁。

## 8. 实施约束

本 ADR 只建立架构决策，不在本变更中新增 Rust、SQL migration、API、Worker、ClickHouse
或生产配置。后续必须以独立 PLAN 交付投影基座、指标语义层、Analytics Query Service、
声明式 Dashboard/Report 和受控 Agent 分析技能，并在每个阶段提供架构 Fitness Function、
契约测试、质量属性证据、迁移和回滚方案。
