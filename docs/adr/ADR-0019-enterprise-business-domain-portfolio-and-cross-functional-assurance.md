# ADR-0019：Enterprise Business Domain Portfolio and Cross-Functional Assurance

> 状态：Accepted
> 日期：2026-08-08
> 决策所有者：Business Architecture / Platform Foundation
> 关联文档：[`../architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md`](../architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md)、[`../architecture/BOUNDED_CONTEXT_MAP.md`](../architecture/BOUNDED_CONTEXT_MAP.md)、[`../architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`](../architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md)、[`../architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`](../architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md)
> 替代：无
> 被替代：无

## 1. 背景

现有业务架构已经定义 Customer、Contract、Project、Approval、Finance、Document、Document Intelligence、Audit 和 Analytics 等上下文，但目标业务范围已经明确扩展到：

- 合同管理；
- 财务管理与财务核对；
- 法务管理；
- 跨部门共享数据和专业业务数据协作；
- 审计、复核、对账、核对、合并/汇总、差异处理和整改；
- 正式报告、报表、审计/核对结论和可追溯工作底稿；
- 人事与绩效考核；
- 文档、证据、AI 解析和 Agent 辅助。

如果继续只在现有 Contract/Finance/Analytics 上追加字段和跨表查询，会产生三个长期问题：

1. 外部主体、客户、合同相对方、供应商、律师、银行等身份数据重复，无法形成稳定共享引用；
2. 业务审计/核对与 Runtime Audit 日志、Analytics 报表语义混淆；
3. 跨部门流程容易退化为共享数据库事务、万能 Workflow 或任意跨表 JOIN，破坏数据所有权。

因此需要在开始大规模业务实现前确定目标业务领域版图和跨上下文协作模式。

## 2. 决策驱动因素

- 每份可变业务事实必须只有一个权威所有者；
- 跨部门协作需要共享数据，但不能共享写模型；
- 合同、法务、财务和审计结论必须能追溯到明确版本和证据；
- 报告和审计结果可能成为正式业务记录，而不仅是可重建 Analytics 产物；
- 人事与绩效需要复用组织、项目和业务指标，但不能让 Organization 或 Analytics 拥有最终绩效决定；
- 目标架构必须支持模块化单体渐进实现，而不要求提前拆微服务；
- Agent 和 AI 继续保持候选、辅助和受控调用者角色，不拥有业务事实。

## 3. 候选方案

### 方案 A：继续扩展现有上下文

在 Customer、Contract、Finance、Analytics 中逐步增加法务、人事、审计和核对能力。

优点：短期文件少、实现直接。

缺点：Customer 会承担外部主体主数据，Finance/Analytics 会逐渐吞并跨部门专业结论，长期边界不清。

### 方案 B：建立统一共享业务表和通用 Workflow

把部门共享字段放在统一业务基础表，由一个通用工作流驱动合同、财务、法务、HR 和审计。

优点：初期查询方便。

缺点：形成共享可变模型、跨上下文耦合和万能状态机，难以审计、迁移、授权和独立演进。

### 方案 C：领域组合 + 稳定共享引用 + 专门跨部门 Assurance 上下文

保持领域所有权，增加必要的领域边界，跨上下文使用 ID、版本化查询、事件、Read Model 和不可变 Snapshot；把专业审计、复核、核对和结论建模为 Business Assurance & Reconciliation，而不是 Runtime Audit 或 Analytics。

## 4. 决策

采用方案 C。

### 4.1 新增目标业务边界

在既有 Bounded Context Map 基础上增加以下目标上下文：

1. **Party & Counterparty Management**：外部自然人/法人/机构的稳定身份和联系主数据；
2. **Legal Management**：法务事项、法律审查、法律意见、争议/案件、法律风险和期限；
3. **People & Performance**：员工业务档案、任职/劳动关系引用、绩效周期、目标、评价、校准和最终绩效结果；
4. **Business Assurance & Reconciliation**：跨部门业务审计、复核、核对、对账、差异、工作底稿、整改建议、签核和正式结论。

新增边界是业务和数据所有权边界，不自动要求新增 crate、数据库 Schema、进程或微服务。

### 4.2 Party 与 Customer/Organization 的关系

- Organization 继续只拥有内部组织单元、岗位、成员关系和汇报关系；
- Party & Counterparty 拥有外部主体的稳定法律身份、标识和联系信息；
- Customer Management 拥有“某 Party 作为客户”的关系生命周期、分类、销售/服务状态；
- 未来 Supplier/Vendor 等角色可以引用同一个 PartyId，而不是复制一份主体信息；
- 合同相对方、法务当事方和财务往来方引用 PartyId，并在需要历史真实性时保存版本化 Snapshot。

### 4.3 Organization 与 People & Performance 的关系

- Organization 拥有组织结构、Position、Membership 和 ReportingRelation；
- People & Performance 拥有 EmployeeProfile、Employment/Assignment 引用、PerformanceCycle、Goal/KPI、Review、Calibration、PerformanceResult；
- 绩效可消费项目、合同、财务或 Analytics 的版本化指标 Snapshot，但最终评分、校准和决定由 People & Performance 所有。

### 4.4 三类“审计/报告”必须严格区分

1. **Runtime Audit**：谁在何时以何身份执行了什么系统/业务操作，是不可抵赖日志；
2. **Analytics Report**：基于可重建投影和版本化指标生成的分析、Dashboard、报表和受控导出；
3. **Business Assurance Result**：业务审计、复核、核对、对账或专项检查形成的 Finding、Exception、Conclusion、Sign-off 和正式报告，是专业业务事实。

三者可以互相引用，但不得互相代替。

### 4.5 跨部门协作模式

跨部门业务默认使用：

```text
Owner Context
  → Versioned API / Domain Event
  → Read Model / Immutable Snapshot
  → Professional Case / Reconciliation Run
  → Finding / Proposal / Decision
  → Owner Context Command
  → Approval / Audit
```

禁止通过共享可变表、直接写其他上下文私有表或跨上下文大事务实现“方便的混合业务”。

### 4.6 Reference + Snapshot

跨上下文长期记录同时保存：

- 稳定资源引用，例如 `PartyId`、`ContractId`、`ProjectId`、`EmployeeId`、`DocumentRevisionId`；
- 当时使用的 `resource_version` 或等价版本；
- 对结论有法律/财务/审计意义的最小不可变 Snapshot；
- 来源、时间、操作者和 lineage。

这样既能读取当前主数据，也能证明历史结论依据的当时事实。

### 4.7 文档与证据

正式合同、法务意见、工作底稿、对账附件、审计证据、绩效附件和正式报告必须通过 Document Management 引用明确 Document Revision。AI/OCR/Parser 产物和 Evidence 必须绑定明确 Revision/Processing Run，不得只绑定“当前 Document”。

### 4.8 合并/汇总的所有权规则

“合并”按业务语义分类：

- 为展示、统计或分析进行的数据汇总：Analytics 派生投影；
- 财务正式合并、结算、对账结果：Finance 或 Business Assurance 所有；
- 重复主体合并：Party & Counterparty 的受控主数据操作；
- 合同/法务业务记录不得通过简单物理合并丢失历史，使用关联、替代、版本或业务迁移操作。

## 5. 边界与非目标

本 ADR 不决定：

- 是否建设完整总账、税务、固定资产或薪资系统；
- Legal Management 的具体诉讼/案件模板；
- 每个部门的最终字段清单和 UI；
- 立即新增对应 Rust crate 或数据库迁移；
- 建设通用 BPM/低代码平台；
- 让 Analytics、Agent 或 AI 成为业务写入权威。

这些内容进入具体业务 Plan 时再按本 ADR 细化。

## 6. 后果

### 正面

- 合同、财务、法务、HR 和跨部门审计可以共享稳定数据而不共享写模型；
- 正式业务结论与技术审计、分析报表边界清晰；
- 共享 Party 主数据避免客户/供应商/相对方重复建模；
- 绩效可以复用业务指标但保留 HR 决策权；
- 所有专业结论可追溯到资源版本、文档版本、证据和签核。

### 负面与成本

- 需要更多明确的 Application API、Snapshot、Read Projection 和 Context Mapping；
- 需要为跨上下文 Case/Process Manager 设计幂等、超时和补偿；
- 现有 Customer/Contract/Finance 的部分模型未来可能需要迁移到更精确的数据所有者。

### 风险

- 过早把每个边界映射为独立 crate/服务会增加开发成本；
- 把 Party 设计成万能业务实体会重复产生“共享模型”问题；
- Business Assurance 若吞并 Finance/Legal 的专业规则，也会成为新的万能上下文。

## 7. 实施

1. 新增 `ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md` 作为业务领域组合 Baseline；
2. 后续修改总体架构、Bounded Context Map 和数据所有权矩阵时必须保持本 ADR 语义；
3. 业务实现计划必须先声明目标 Context、Owner、共享引用、Snapshot、正式输出和审批边界；
4. Document Lifecycle/Revision、Party Master、Legal/Finance、Assurance、People & Performance 分阶段实现；
5. 当前 PLAN-0006 不因本 ADR 自动扩大范围或改变状态。

## 8. 验证证据

本决策基于：

- 当前 `business-platform` 的 DDD、Data Ownership、Durable Document Processing、Runtime Audit、Analytics Baseline；
- Odoo、ERPNext/Frappe HRMS、Twenty、Bigcapital 等业务领域实现的模块边界参考；
- OpenContracts、Mayan EDMS、Paperless-ngx、Documenso 等文档/合同生命周期参考；
- Plane、Chatwoot、Comp AI CRM 等项目/客户协作和 AI 辅助业务参考。

外部项目只作为 Reference，不改变本项目的 Rust、数据所有权、安全和 Agent 边界。

## 9. 后续复审条件

出现以下事实时重新评估：

- 平台决定建设完整会计总账或 Payroll；
- Party Master 无法覆盖实际主体和权限边界；
- Business Assurance 的工作量或安全边界要求独立部署；
- 多法人、多账套或跨租户合并成为核心产品能力；
- 法务案件/诉讼管理发展为独立产品；
- 绩效模型需要独立的组织/岗位历史快照体系。
