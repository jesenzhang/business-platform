# 企业业务领域参考项目分析

> 类型：Reference  
> 首次检查日期：2026-08-08  
> Twenty 深度复核：2026-08-12，固定提交 `65616332b452361e639c41d7340d54febf95fae5`  
> 适用范围：合同、财务、法务、项目、客户、HR/绩效、文档和跨部门业务建模  
> 决策入口：[`../adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md`](../adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md)  
> Baseline：[`../architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md`](../architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md)

## 1. 使用说明

本文记录企业业务领域参考项目的横向定位，不构成项目架构规则。正式边界以 ADR 和 Architecture Baseline 为准。

Twenty 已在 2026-08-12 完成独立源码级专项研究，其 Business App/Module Packaging 结论以 [`TWENTY_REFERENCE_ANALYSIS.md`](TWENTY_REFERENCE_ANALYSIS.md) 为准；本文不再承担 Twenty 的完整架构分析。

本项目不因参考项目而直接引入其运行时。实际复用代码、Schema、SDK 或 UI 资产前必须再次核对当前许可证、版本和依赖边界。

## 2. 总览

| 项目 | 定位 | 主要参考领域 | 采用方式 |
|---|---|---|---|
| Odoo | 集成式开源 Business Apps / ERP | CRM、项目、HR、库存、会计、制造、模块组合 | 领域组合和模块边界参考 |
| ERPNext | 完整开源 ERP | 财务、采购、销售、项目、资产、制造、HR | 企业业务领域词典和流程参考 |
| Frappe HRMS | HR + Payroll 垂直应用 | 员工生命周期、请假、考勤、绩效、薪资 | People & Performance 领域参考 |
| Twenty | Business App Platform / CRM Product | Application Manifest、Object/Field、View/Layout、Role/Permission、Logic/Agent、install/upgrade/uninstall | **Tier-1 Business App Packaging 参考**，详见专项分析 |
| Bigcapital | 开源会计和库存软件 | 双重记账、财务报表、交易/库存 | Finance 演进参考 |
| Comp AI CRM | Agentic CRM | Agent Task、Evidence、研究、CRM Activity | Agent 工程参考，业务权威模型不照搬 |
| Plane | 项目管理 | Work Item、Cycle、Module、View、Analytics | Project/Work Management 参考 |
| Chatwoot | 客户支持平台 | Contact、Conversation、Inbox、Team、Channel | 客户交互和协作参考 |
| OpenContracts | Document Intelligence / Legal document platform | Document、Corpus、Version、Annotation、Citation、Extraction、MCP | 合同/法务文档和 Evidence 首要参考 |
| Mayan EDMS | 企业文档管理系统 | Document、File、Version、Page、Parsing、OCR | 文档领域边界参考 |
| Paperless-ngx | 文档归档与搜索 | 文档版本、回收、历史、任务处理 | 轻量文档生命周期和 UX 参考 |
| Documenso | 开源电子签署 | Document、Recipient、Signature、Audit/Trust | 合同签署和可信产物参考 |

## 3. Odoo

上游将自身描述为一组 Web-based open-source business apps，核心 App 包括 CRM、Website、eCommerce、Inventory、Project、Accounting、POS、HR、Marketing 和 Manufacturing；各 App 可单独使用，也可以组合成完整 ERP。

### 对本项目的参考价值

- 业务模块可以独立演进，但共享平台基础能力；
- 合同、项目、财务、HR 不需要提前拆为微服务；
- 一个业务对象往往会被多个模块引用，但拥有者仍应明确；
- UI 导航和业务应用边界可以与技术部署边界分离。

### 不直接照搬

- Odoo 的 ORM、动态模型和模块加载方式；
- 将大量业务语义放入共享可变模型；
- Python 技术栈和部署结构。

## 4. ERPNext

ERPNext 定位为 100% open-source ERP，覆盖 Accounting、Order Management、Manufacturing、Asset Management、Projects 等企业核心领域，并基于 Frappe Framework 提供数据库抽象、认证和 REST API。

### 对本项目的参考价值

ERPNext 更适合作为“成熟企业管理软件到底有哪些业务对象和关系”的领域词典，而不是代码模板。重点研究：

- Customer/Supplier/Party 类主体关系；
- Order/Invoice/Payment/Settlement；
- Project/Task/Timesheet/Expense；
- Asset/Cost/Accounting 维度；
- 业务模块之间的引用和报表需求。

## 5. Frappe HRMS

Frappe HRMS 是独立的人力资源和 Payroll 应用，覆盖员工生命周期、请假、考勤、费用、绩效、薪资和税务等能力。它从 ERPNext 中拆出独立产品的历史也证明了“领域先在模块化平台中成熟，再按真实边界独立”的演进方式。

### 对本项目的参考价值

- Employee 与 Organization/Position 的边界；
- Performance Cycle、Goal/KRA、Appraisal；
- HR 敏感数据分类；
- Payroll 若未来加入时与 Finance 的边界。

## 6. Twenty

Twenty 的参考定位在 2026-08-12 调整：它不再只是“可配置 CRM 与前端体验参考”，而是提升为 **Tier-1 Business App / Module Packaging / Metadata / UI / Lifecycle 架构参考**。

源码级调查确认当前 Twenty Application Manifest 已覆盖：

```text
Application
Objects / Fields / Indexes
Roles / Permission Flags
Views / View Fields
Navigation / Page Layout / Page Layout Tabs / Commands
Logic Functions / Front Components
Skills / Agents
Connection Providers / Assets / Translations
```

并且具有 source declaration → build manifest、stable universal identifier、required server version range、dry-run manifest sync、install/upgrade/uninstall 等完整 App Platform 机制。

### 对本项目的核心参考价值

- Business Module 应有稳定、版本化、与物理代码路径无关的 Package/Contribution identity；
- 业务模块应通过 Manifest 声明 UI、Policy、Agent、Semantic 等贡献，而不是由 Platform Core 硬编码；
- 安装/升级前应先进行 compatibility validation 与 deterministic dry-plan；
- View、Navigation、Detail Layout、Action 等可以成为 typed UI Contribution；
- Module 生命周期、Package checksum、版本兼容和 remove plan 应成为正式治理对象；
- 业务扩展需要显式 Published Extension Point。

### 必须改造或拒绝

Twenty 允许 App 向标准对象或其他 App Object 添加字段/关系，这对 CRM Extension 很灵活，但不满足本项目更严格的“真实业务互不污染、业务可剥离”要求。

本项目禁止：

```text
Module B → ALTER Module A private schema
Module B → private FK / private table JOIN
Module B → 未经 owner 发布直接给 Aggregate 注入字段
```

Twenty 的 Object Metadata 也不能取代本项目的 Rust DDD Domain、业务不变量、事务、Event、Revision/Evidence/Audit。卸载 Module 与 Purge 历史数据继续严格分离。

专项事实、许可证、Adopt/Adapt/Reject/Defer 与 PLAN-0011 实施映射详见：

[`TWENTY_REFERENCE_ANALYSIS.md`](TWENTY_REFERENCE_ANALYSIS.md)

## 7. Bigcapital

Bigcapital 的上游 README 将其定义为面向中小企业的开源 Accounting + Inventory 软件，支持自动化会计流程和财务报表，并提供 Headless Accounting API；README 明示为 AGPL。

### 对本项目的参考价值

当 Finance 从付款/收款管理进一步演进时，可重点参考：

- Double-entry accounting；
- Transaction → Journal/Ledger 的不可变链路；
- Financial Statements；
- Inventory/financial interaction；
- Headless accounting API 的边界。

当前 `business-platform` 不应因此立即扩展为总账系统。

## 8. Comp AI CRM

Comp AI CRM 的 Agent 是独立部署的智能工作单元，使用 Durable Task、Evidence、Sandbox 和 CRM authorized tools。其设计强调 Evidence 而不是让模型自报 confidence。

### 值得吸收

- AgentTask + lease/scheduling；
- Evidence-first AI；
- 弱证据形成 suggestion，强证据才可进入候选事实；
- Agent Activity 对用户可见；
- Sandbox 无数据库凭证、业务访问只能走工具。

### 不直接照搬

- 上游明确是 single-tenant；
- 上游将大量 intelligence 放在 Agent，而本项目继续坚持 Rust Business Platform 是业务权威。

## 9. Plane

Plane 定位为现代开源 Project Management，核心能力包括 Work Items、Cycles、Modules、Views、Pages 和 Analytics。

### 对本项目的参考价值

项目域不应只建模 `Project + Task`，还应根据真实需求研究：

- Work Item；
- Milestone/Cycle；
- Module/Workstream；
- Saved View；
- Roadmap；
- Project Analytics。

## 10. Chatwoot

Chatwoot 是开源客户支持平台，集中处理多渠道 Conversation，并具备 Contact、Inbox、Team、Label、Automation、Report 和 AI support agent。

### 对本项目的参考价值

未来若 Customer 领域包含服务和沟通，可参考：

- Contact 与 Conversation 分离；
- Channel/Inbox；
- Assignment/Team；
- 内部 Note；
- 客户交互历史；
- AI 辅助但人工仍拥有复杂决定。

## 11. OpenContracts

OpenContracts 定位为开源 Document Intelligence 平台：同一文档/Corpus 图谱同时提供 GraphQL/REST、React UI 和 MCP；支持 Annotation、结构化 Extraction、AI Agent、Citation/Relationship Graph。README 明示 MIT License。

其文档版本架构使用 Content Tree + Path Tree，强调：

- Content != Location；
- 历史只追加，不覆盖；
- Soft delete/restore；
- 每次内容变化形成独立版本；
- Hash 用于完整性而不是强制业务去重；
- 解析/结构化产物具有 parser/version 信息。

### 对本项目的参考价值

它是当前合同/法务文档生命周期和 Evidence 的首要参考，尤其适用于：

- Document Revision；
- Source provenance；
- Annotation/Evidence；
- 结构化抽取人工 approve/reject；
- MCP 暴露受控文档查询；
- 历史版本恢复和审计。

## 12. Mayan EDMS

Mayan EDMS 是成熟的企业文档管理系统。其 GitHub 仓库 README 已明确提示 GitHub 只是过时镜像，官方源码位于 GitLab，因此本项目只使用 GitHub 镜像研究模型，不将其视为当前版本事实来源。

其模型清晰地区分：

- Document；
- DocumentFile；
- DocumentVersion；
- DocumentFilePage；
- DocumentFilePageContent；
- OCR/Parsing。

### 对本项目的参考价值

最重要的是证明：

```text
逻辑文档
≠ 二进制文件
≠ 文档版本
≠ 页面
≠ 解析文本
```

这些实体分离后，版本、删除、恢复、解析绑定和缓存清理都更容易拥有明确语义。

## 13. Paperless-ngx

Paperless-ngx 是把物理/电子文档转成可搜索档案的文档管理系统。当前代码中已经存在 root document、version index/label、版本 API、soft-delete 模型和文档历史 UI。

### 对本项目的参考价值

- 比 Mayan 更轻量的版本 UX；
- Root + Version 的简单关系；
- 删除版本后的 current version 解析；
- 回收站和审计历史；
- 后台 Document Task 状态。

## 14. Documenso

Documenso 是开源 DocuSign alternative，README 明示 AGPLv3，并支持自托管。其技术栈包含 PDF viewing/manipulation/signature 和 S3 存储开发环境。

### 对本项目的参考价值

当合同进入正式签署阶段时重点研究：

- Recipient；
- Signing Order；
- Signature Field；
- Envelope/Document Status；
- Final signed artifact；
- 审计和可信证明；
- 签署后的不可变文件版本。

签署不是普通 Document 状态，应由 Contract/Signing 专业业务与 Document Revision 协同。

## 15. 综合吸收结论

本项目不寻找一个“万能 ERP 项目”直接复刻，而采用组合参考：

```text
业务领域广度          → Odoo + ERPNext
Business App Packaging → Twenty
Semantic / Analytics    → WrenAI
财务演进              → Bigcapital + ERPNext/Odoo
HR/绩效               → Frappe HRMS
项目                  → Plane
客户交互              → Chatwoot
Agent 工程            → Comp AI CRM
合同/法务文档         → OpenContracts
企业文档生命周期       → Mayan EDMS + Paperless-ngx
电子签署              → Documenso
```

Twenty 负责回答“业务如何作为独立 App/Module 存在”；WrenAI 负责回答“业务公开的数据语义是什么”；Rust DDD 继续回答“正式业务事实与规则由谁拥有”。

最终边界仍以 `business-platform` 的 DDD、Data Ownership、Security、Audit、Analytics、Business Module Isolation、Document Processing 和 ADR-0019/ADR-0020 为准。
