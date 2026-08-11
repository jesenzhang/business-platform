# 企业业务领域与跨部门协作架构

> 文档 ID：ARCH-BUSINESS-001
> 版本：1.0
> 状态：Baseline
> 生效日期：2026-08-08
> 所有者/责任模块：Business Architecture / Platform Foundation
> 关联 ADR：ADR-0019
> 关联 Baseline：BOUNDED_CONTEXT_MAP、DATA_OWNERSHIP_AND_CONSISTENCY、DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE、DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE

## 1. 目的

本文定义企业 AI 业务平台面向合同、财务、法务、项目、客户、人事、绩效以及跨部门审计/核对/合并业务时的目标领域版图、共享数据边界、跨上下文协作方式和正式输出模型。

本文解决的不是“菜单怎么分”，而是以下长期问题：

- 哪个上下文拥有哪类权威业务事实；
- 哪些数据可以共享，如何共享；
- 合同、法务、财务、人事之间如何协作而不共享写模型；
- 业务审计/核对如何区别于 Runtime Audit；
- 报告、报表、正式结论和分析结果分别由谁拥有；
- 文档、版本、解析结果和证据如何绑定到专业业务；
- Agent 如何访问跨部门数据而不拥有业务规则。

本文不自动要求新增 crate、数据库 Schema、进程或微服务。初期仍采用模块化单体，并按业务优先级逐步实现。

## 2. 核心原则

### 2.1 唯一权威所有者

每份可变业务事实必须有且只有一个 Bounded Context 负责语义、写入规则、状态机、版本和迁移。

共享数据不等于共享写模型。跨部门使用其他上下文数据时只能通过：

- 稳定 ID 引用；
- Application Query/API；
- 版本化事件；
- 可重建 Read Projection；
- 不可变 Snapshot。

### 2.2 Reference + Snapshot

长期专业结论不能只指向“当前值”。对合同、法务、财务、审计、绩效等有历史证明要求的数据，应保存：

```text
SourceRef
  context
  aggregate_id
  resource_version
  observed_at

Snapshot
  minimal business fields
  source version
  checksum / lineage when required
```

Reference 用于定位当前资源，Snapshot 用于证明当时的事实。

### 2.3 专业结论不属于 Analytics

Analytics 可以汇总、计算、展示和导出，但不能拥有：

- 法律意见；
- 财务结算决定；
- 审计结论；
- 绩效最终评分；
- 合同正式状态。

这些正式结论由对应专业业务上下文拥有。

### 2.4 Runtime Audit 不等于业务审计

Runtime Audit 记录系统中的操作事实；Business Assurance 记录业务检查、核对、差异、Finding、整改、签核和结论。两者必须可以关联，但不能使用同一模型代替。

### 2.5 文档身份、内容版本、处理结果和业务事实分离

平台统一遵循：

```text
Document
  → DocumentRevision
      → Blob/Object
      → ProcessingRun
          → ProcessingArtifact
              → Evidence
                  → Candidate / Professional Finding
```

合同、法务、审计、财务和绩效只能引用明确 Document Revision；解析和 AI 结果也必须绑定明确 Revision/Processing Run。

### 2.6 跨部门长期流程由 Case + Process Manager 协调

Workflow Runtime 负责可靠执行，不拥有专业业务语义。每类长期业务必须有明确 Case/Instance/Run 作为业务状态所有者，再由 Process Manager 协调跨上下文事件和命令。

## 3. 目标领域版图

```text
┌──────────────────────── 基础调用上下文 ────────────────────────┐
│ Identity / Organization / Policy                              │
└───────────────────────────────────────────────────────────────┘

┌──────────────────────── 共享业务主数据 ────────────────────────┐
│ Party & Counterparty                                          │
│ Customer Relationship                                        │
│ Document Management                                          │
└───────────────────────────────────────────────────────────────┘

┌──────────────────────── 专业业务领域 ──────────────────────────┐
│ Contract              Project                                │
│ Finance               Legal                                  │
│ People & Performance  Approval                               │
│ Business Assurance & Reconciliation                          │
└───────────────────────────────────────────────────────────────┘

┌──────────────────────── 智能与派生能力 ────────────────────────┐
│ Document Intelligence                                         │
│ Analytics / Visualization                                     │
│ Enterprise AI Workspace / Agent Adapter                       │
└───────────────────────────────────────────────────────────────┘

所有正式写入 → Owner Context
所有操作轨迹 → Runtime Audit
所有长时执行 → Durable Task Execution
所有跨部门分析 → Read Projection / Analytics Query
所有正式专业结论 → 对应 Professional Context
```

## 4. Party & Counterparty Management

### 4.1 定位

Party 是平台中“外部业务主体”的稳定身份，不等同于 Customer。

可表示：

- 法人企业；
- 其他组织；
- 自然人；
- 合同相对方；
- 供应商；
- 律师事务所或外部律师；
- 银行、保险、监管或合作机构。

### 4.2 拥有

- Party；
- PartyType；
- LegalIdentifier，例如统一社会信用代码/税号等；
- LegalName / DisplayName；
- ContactPoint；
- RegisteredAddress / ContactAddress；
- PartyMergeRecord；
- PartyStatus。

### 4.3 不拥有

- Customer 生命周期；
- 合同状态；
- 付款账户和账务状态；
- 法律事项；
- 内部员工任职关系。

### 4.4 Customer 的定位

Customer Management 保留，但变为 Party 的业务角色和关系层：

```text
Party
  └── CustomerProfile
        ├── lifecycle
        ├── classification
        ├── service/sales status
        └── relationship metadata
```

未来 Supplier/Vendor 也可形成独立角色模型而复用 PartyId。

### 4.5 主体合并

重复主体合并是 Party Context 的受控业务操作，不是数据库物理去重：

```text
Prepare Merge
→ 展示冲突和引用
→ 确认主记录
→ 迁移可迁移引用
→ 建立 alias / merged_into
→ Audit
→ 保留历史
```

高风险字段和跨上下文引用不能静默覆盖。

## 5. Contract Management

Contract 继续拥有合同正式业务事实：

- Contract；
- ContractVersion；
- ContractParty 引用 PartyId；
- ContractTerm；
- Amount/Period 等正式字段；
- ContractLifecycleState；
- Amendment / Supplement / Termination；
- 履约关键状态。

合同文件通过 DocumentLink/ContractDocumentRelation 引用 Document 与明确 Revision。已签署版本不可覆盖。

法务审查、财务付款、项目履约可以引用 ContractId/Version，但不得直接修改 Contract 私有表。

## 6. Finance Management

### 6.1 当前目标范围

Finance 初期定位为 **Financial Operations & Control**，不是完整总账系统。

目标能力包括：

- PaymentPlan；
- Receivable / Payable 业务记录；
- PaymentRecord / CollectionRecord；
- Invoice / Expense 业务引用，按实际需求逐步引入；
- Settlement；
- Reconciliation；
- Budget/Allocation 的轻量控制需求；
- Currency、金额和汇率业务语义；
- Contract/Project/Party 财务关联；
- FinanceStatus；
- 财务差异和调整请求。

### 6.2 正式账务演进

若未来建设完整会计核心，必须单独 ADR 评估：

- Chart of Accounts；
- Journal / Ledger；
- Fiscal Period；
- Closing；
- Tax；
- Fixed Assets；
- Multi-entity consolidation；
- 会计不可变性和合规要求。

参考 Bigcapital、ERPNext 和 Odoo 的账务模型，但不能在当前阶段把整套 ERP 会计模型直接搬入平台。

## 7. Legal Management

### 7.1 职责

Legal Management 负责合同之外的法务专业事实和意见：

- LegalMatter；
- LegalReview；
- LegalOpinion；
- LegalRisk；
- DisputeCase；
- Claim / Issue；
- LegalDeadline；
- LegalParticipant；
- LegalEvidenceLink；
- ExternalCounsel / Party reference；
- 法律结论和处置建议。

### 7.2 与 Contract 的关系

典型流程：

```text
Contract Draft/Version
→ LegalReview Case
→ Snapshot contract version + evidence
→ Review / Risk / Opinion
→ Approval or requested changes
→ Contract Context receives command/result
```

Legal 可以提出修改建议和风险结论，但正式合同字段仍由 Contract Application Service 修改。

### 7.3 与 Document 的关系

法律意见书、律师函、诉讼材料、证据、法规引用和正式审查报告均引用明确 Document Revision。

## 8. People & Performance

### 8.1 Organization 与 HR 分离

Organization 只负责：

- OrganizationUnit；
- Position；
- Membership；
- ReportingRelation；
- 有效期和组织层级。

People & Performance 负责：

- EmployeeProfile；
- Employment / Assignment reference；
- PerformanceCycle；
- Goal / KPI；
- PerformanceReview；
- ReviewerAssignment；
- Calibration；
- PerformanceResult；
- PerformanceEvidence；
- ImprovementPlan，若业务需要。

### 8.2 绩效指标来源

绩效可以引用：

- 项目交付；
- 合同执行；
- 财务指标；
- 工作量或质量指标；
- 部门指标；
- 人工评价。

数值指标通过 Analytics 的已发布 Metric Version 或 Owner Context 的 Snapshot 获取，People & Performance 保存本次考核实际采用的版本和 Snapshot。

最终绩效评分、校准和结果由 People & Performance 所有，不由 Analytics 自动决定。

### 8.3 Payroll

薪资/Payroll 不属于当前默认范围。若未来引入，应参考 Frappe HRMS/Odoo HR，并单独明确薪酬敏感数据、安全和 Finance 对接边界。

## 9. Business Assurance & Reconciliation

这是平台处理跨部门“审计、复核、核对、对账、检查、差异分析和整改”的专业业务上下文。

### 9.1 拥有

建议统一语言：

- AssuranceCase；
- AssuranceScope；
- EvidenceSnapshot；
- Workpaper；
- CheckRule / ReconciliationRule；
- ReconciliationRun；
- ConsolidationRun，只有正式业务合并场景需要；
- Exception；
- Finding；
- AdjustmentProposal；
- RemediationAction；
- SignOff；
- AssuranceConclusion；
- FormalReportRef。

### 9.2 不拥有

- Runtime AuditEvent；
- Contract/Finance/Legal/HR 的正式业务字段；
- 通用任务租约和调度；
- Analytics 指标定义；
- 文件二进制。

### 9.3 典型核对流程

```text
Create AssuranceCase
→ 定义 Scope / Period / Resources
→ 冻结 SourceRef + Snapshot
→ 执行规则和人工检查
→ 生成 Difference / Exception
→ 形成 Finding
→ 提出 Adjustment / Remediation
→ Owner Context 执行修正
→ Verify
→ Sign-off
→ Formal Conclusion / Report
```

Source Owner 修正后，Assurance 必须保存新的验证证据，而不是回写旧 Snapshot。

### 9.4 Runtime Governance 与 Business Assurance 的区别

现有 Integrity Finding / Controlled Repair 主要用于运行时数据完整性和系统治理。Business Assurance 面向业务人员的专业核对和业务结论。

可以复用相似的 Prepare → Review/Approve → Execute → Verify 思想，但数据模型和业务语言必须独立。

## 10. Project 与跨部门业务

Project 继续拥有：

- Project；
- ProjectMember；
- Milestone；
- Deliverable；
- ProjectStatus。

Project 是 Contract、Finance、Legal、Performance 和 Assurance 的重要共享维度，但只通过 ProjectId、版本化查询和 Snapshot 共享。

项目看板可以聚合：

- 合同状态；
- 回款/付款状态；
- 法务风险；
- 里程碑；
- Assurance Finding；
- 相关 Document；

但这些聚合视图是 Read Model，不改变各 Owner Context 的所有权。

## 11. Approval 的定位

Approval 负责统一审批定义和决定过程，不吞并专业规则。

例如：

```text
Legal Opinion requires approval
Finance Adjustment requires approval
Performance calibration requires approval
Contract amendment requires approval
Assurance conclusion requires sign-off/approval
```

业务上下文保存自己的 pending/waiting 状态；Approval 返回 Decision；业务上下文再次验证版本后执行自身状态转换。

## 12. 合并、核对和汇总的分类

“合并”必须先回答它属于哪一种语义。

### 12.1 分析汇总

例如按部门汇总合同金额、付款、绩效、风险。

所有者：Analytics。

产物：可重建 Projection/Dataset/Report。

### 12.2 财务正式合并/结算

例如多个项目或主体形成正式结算结果。

所有者：Finance；必要时由 Assurance 复核。

### 12.3 跨部门正式审计结论

例如合同金额、付款记录、发票、项目完成量和法务状态共同核对后形成正式检查结论。

所有者：Business Assurance & Reconciliation。

### 12.4 主数据去重合并

例如重复客户/相对方主体。

所有者：Party & Counterparty。

### 12.5 历史业务记录

合同版本、法律意见、付款记录、Finding 等不得使用物理 merge 丢失历史。采用关联、替代、版本、新业务动作或受控迁移。

## 13. 正式报告、报表和产物

平台必须区分四类输出：

| 类型 | 所有者 | 是否权威业务事实 | 典型内容 |
|---|---|---:|---|
| Runtime Audit 查询/导出 | Audit | 审计事件本身权威 | 操作轨迹、主体、资源版本 |
| Analytics Report | Analytics | 否，可重建 | 指标、Dashboard、分析报表 |
| Professional Formal Report | 业务上下文 | 是，结论部分权威 | 审计报告、法律意见、财务结算、绩效结果 |
| File/Artifact | Document/Object Storage | 仅是载体 | PDF/XLSX/JSON/附件/快照 |

正式业务报告至少保存：

- report_id；
- report_type；
- owner_context；
- case/run/version；
- reporting_period；
- source snapshot refs；
- metric/query versions when used；
- finding/conclusion refs；
- generated_at / generated_by；
- approval/sign-off；
- document_revision_id 或 artifact checksum；
- superseded_by，若被新版替代。

## 14. 文档、证据与解析绑定

所有专业业务统一采用以下绑定：

```text
Business Resource
  → DocumentLink(role)
      → Document
          → DocumentRevision
              → ProcessingRun
                  → ProcessingArtifact
                      → Evidence(span/page/bbox/source)
```

禁止：

- 业务记录直接保存可覆盖的 `current.pdf`；
- 解析结果只绑定 DocumentId；
- 新版本文件复用旧版 OCR/抽取结果而没有 lineage；
- 删除业务关系时直接物理删除仍被证据引用的文件；
- AI 建议缺少 source revision 和 evidence。

Document Revision 和删除/恢复生命周期的详细实现应单独进入 Document Lifecycle Plan，并参考 OpenContracts、Mayan EDMS 和 Paperless-ngx。

## 15. 跨部门查询和数据共享

### 15.1 推荐方式

```text
Owner Query API
Event Projection
API Composition
Analytics Dataset
Case Snapshot
```

### 15.2 禁止方式

```text
共享可变 BusinessBase 表
任意跨 Context JOIN 后直接写回
Agent SQL
报表 SQL 作为业务规则
跨上下文 Repository 直接引用私有 Row
```

### 15.3 360° 视图

允许构建只读综合视图，例如：

- Contract 360；
- Party 360；
- Project 360；
- Legal Matter 360；
- Assurance Case 360；
- Employee Performance 360。

360 视图是 Query Composition/Read Projection，不成为新的业务事实所有者。

## 16. 权限和数据分类

跨部门平台必须支持资源、字段和用途级权限。

至少区分：

- General Internal；
- Financial Sensitive；
- Legal Privileged；
- Personal / HR Sensitive；
- Audit Restricted；
- Confidential Document；
- Secret/Credential，禁止进入业务 DTO、日志和报告。

授权必须考虑：

- tenant；
- department/organization scope；
- business role；
- resource relation；
- case assignment；
- field classification；
- action risk；
- purpose/use context；
- temporal validity。

Legal Privilege、HR Sensitive、Finance Sensitive 数据不能因为进入 Analytics 或 Agent Context 而降低权限。

## 17. Agent 与 AI 边界

Agent 可以：

- 搜索和解释授权范围内的数据；
- 创建 Case 草稿；
- 调用版本化分析工具；
- 运行文档解析和结构化抽取；
- 提议核对规则或调整方案；
- 生成报告草稿。

Agent 不可以：

- 直接修改多个上下文数据；
- 自动定义正式指标口径；
- 把 LLM confidence 当作业务证据；
- 绕过 Legal/Finance/HR 字段权限；
- 将报告草稿直接变成正式结论；
- 使用 SQL 或数据库 Schema 作为通用工具。

专业写操作仍遵循 Prepare → Preview → Confirm/Approve → Execute → Audit。

## 18. 可见业务产品结构

前端不需要严格等同 Bounded Context，但建议形成稳定的业务工作区：

```text
首页 / My Work
客户与主体
合同
项目
财务
法务
审计与核对
人事与绩效
文档中心
报表与分析
AI Workspace
系统管理
```

### 18.1 My Work

聚合本人：

- 待审批；
- 待复核；
- 待处理 Finding；
- 合同/项目到期事项；
- 法务期限；
- 财务异常；
- 绩效任务；
- AI 需要人工确认的候选结果。

这些是跨上下文 Read Model，不创建新的“任务真相”。

### 18.2 专业业务页面

每个主业务对象应优先采用“事实 + 文档 + 关联 + 时间线 + AI Activity”的组合：

```text
Overview
Business Facts
Documents / Revisions
Related Resources
Approvals
Findings / Risks
Activity / Audit
AI Suggestions
Reports
```

## 19. 参考项目吸收策略

### ERP/业务组合

- Odoo：模块化业务 App 与跨模块集成；
- ERPNext：财务、销售、采购、项目、HR 等领域模型完整度；
- Frappe HRMS：员工生命周期、绩效、薪资/HR 边界；
- Twenty：对象、字段、View、Workflow、Agent 可扩展业务平台；
- Bigcapital：会计、库存、双重记账和财务报表。

### 文档/合同

- OpenContracts：Document Intelligence、版本、Annotation、Citation/Evidence、MCP；
- Mayan EDMS：Document/File/Version/Page/Parsing 分离；
- Paperless-ngx：文档归档、版本、回收站和历史；
- Documenso：签署流程和可信文档产物。

### 专业协作

- Plane：Project/Work Item/Cycle/Module/View；
- Chatwoot：客户交互、Conversation、Inbox、Team；
- Comp AI CRM：Durable Agent Task、Evidence-first AI、Agent Activity。

参考项目只作为领域语言和实现经验输入。本平台不复制其租户、数据库、Agent 权限或技术栈。

## 20. 分阶段实现建议

本文不改变当前 PLAN-0006 状态。业务领域建议按独立 Plan 渐进实施。

### Wave A：共享业务基础和文档生命周期

- Party & Counterparty Baseline；
- Customer → Party role mapping；
- DocumentRevision 一等实体；
- Trash/Restore/Purge；
- ProcessingRun/Artifact/Evidence 明确绑定 Revision。

### Wave B：合同 + 财务 + 法务首个跨部门垂直切片

建议选择一个真实流程，例如：

```text
合同创建/上传
→ 文档解析
→ 法务审查
→ 审批
→ 合同生效
→ 付款计划
→ 财务核对
→ Finding/差异
→ 修正/确认
→ 形成正式报告
```

用一个完整流程验证 Context、权限、Snapshot、Document、Approval、Audit 和 Report。

### Wave C：Business Assurance & Reconciliation

- Case/Scope；
- Evidence Snapshot；
- Reconciliation Rule/Run；
- Exception/Finding；
- Adjustment Proposal；
- Verification；
- Sign-off / Formal Report。

### Wave D：People & Performance

- EmployeeProfile/Employment reference；
- Performance Cycle；
- Goal/KPI；
- Review/Calibration；
- Analytics Metric Snapshot；
- Performance Result。

### Wave E：Analytics 和 Agent 深化

- 跨上下文 Dataset；
- Metric Semantic Layer；
- Dashboard/Report；
- 专业领域 Skill；
- Agent 只读分析和受控 ActionPlan。

## 21. 计划准入模板

以后任何专业业务 Plan 至少回答：

1. Owner Context 是谁？
2. Aggregate 和关键不变量是什么？
3. 引用了哪些其他 Context 的 ID？
4. 哪些字段需要历史 Snapshot？
5. 哪些文档必须绑定 Revision？
6. AI/解析结果如何追溯 Evidence？
7. 哪些步骤强一致，哪些最终一致？
8. 是否需要 Approval/Sign-off？
9. 正式输出属于业务事实还是 Analytics Artifact？
10. 删除、恢复、替代和保留语义是什么？
11. 权限和字段分类是什么？
12. Agent 能读什么、能提议什么、绝对不能做什么？

没有回答这些问题的跨部门业务不得直接进入数据库/API 实现。

## 22. 验收清单

- [ ] Party、Customer、Organization、Employee 的身份边界没有混用；
- [ ] Contract、Finance、Legal、Performance 的正式事实都有唯一 Owner；
- [ ] Business Assurance 与 Runtime Audit/Analytics 已明确区分；
- [ ] 跨上下文没有共享写表；
- [ ] 历史专业结论具备 Reference + Snapshot；
- [ ] 文件和 Evidence 绑定明确 Document Revision；
- [ ] 合并操作按业务语义选择 Owner；
- [ ] 正式报告和普通分析报表没有混用；
- [ ] 高敏 Finance/Legal/HR 数据在 Analytics/Agent 中权限不降级；
- [ ] 长时流程具备业务 Case 和可靠执行状态的双层模型；
- [ ] 新上下文没有被自动映射成微服务；
- [ ] 实施计划有真实跨部门垂直切片验收。

## 23. Business Module 与语义发布

Contract、Finance、Customer、Project、Approval、Document Management 和 Document
Intelligence 等仍是各自业务能力/Bounded Context 的候选 Business Module；模块 manifest
不能改变它们的正式事实所有权。模块可以向 Analytics 发布 Dataset、Projection、Field、
Relationship、Measure、Metric、Dimension、Time Dimension、Filter Policy 和 Lineage，但
这些是受版本和权限治理的语义贡献，不是第二份业务状态。

跨模块分析必须使用已发布语义对象、ResourceRef、Public Projection 或
Reference + Snapshot。不得用 Analytics registry、Wren-style model、私有表 FK 或任意
JOIN 代替 Contract/Finance/Legal/People 的 Application API、事件和业务协作边界。

现有业务 crate 在正式模块化迁移前保持 transitional；目录移动、manifest registry、
安装生命周期和真实跨部门语义切片必须由独立 Plan/ADR 进入，不在本轮自动激活。
