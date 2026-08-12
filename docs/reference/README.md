# 外部参考项目

本目录保存对外部项目、产品和技术方案的事实性研究。参考材料用于形成架构输入，不能直接覆盖本项目的 Baseline、ADR、数据所有权或安全边界。

## 使用规则

1. 记录检查日期、来源、许可证和版本/提交；
2. 区分外部项目事实、项目适配分析和本项目正式决策；
3. 明确哪些能力采用、改造采用、延后或拒绝；
4. 长期架构变化必须进入 `docs/architecture/` 并通过 ADR；
5. 实现顺序和验收标准必须进入 `docs/plans/current/`。

## 已登记项目

| 项目 | 分类 | 主要参考价值 | 分析文档 |
|---|---|---|---|
| Cloudflare OS | 企业 AI Workspace / Agent 应用平台 | Workspace、Gatekeeper、Gadget、Blueprint、Capability-based security、Observation/Observer | [`CLOUDFLARE_OS_REFERENCE_ANALYSIS.md`](CLOUDFLARE_OS_REFERENCE_ANALYSIS.md) |
| Canner/WrenAI | Semantic Contract / GenBI 参考 | MDL、语义建模、source-to-compiled manifest、结构化校验与 dry-plan 思想；不采用其运行时和任意 SQL/MCP 边界 | [`WRENAI_REFERENCE_ANALYSIS.md`](WRENAI_REFERENCE_ANALYSIS.md) |
| Twenty | Business App / Module Packaging / UI / Lifecycle | Manifest、stable identity、兼容性、dry-plan、contribution 和 lifecycle | [`TWENTY_REFERENCE_ANALYSIS.md`](TWENTY_REFERENCE_ANALYSIS.md) |
| Odoo | Modular Business Application | manifest/depends、registry、model/view extension、security、migration、lifecycle | [`ODOO_REFERENCE_ANALYSIS.md`](ODOO_REFERENCE_ANALYSIS.md) |
| Frappe Framework + ERPNext | Metadata-driven Business Application | DocType、Custom Field、hooks、permissions、workflow、events、fixtures、migration、UI contribution | [`FRAPPE_ERPNEXT_REFERENCE_ANALYSIS.md`](FRAPPE_ERPNEXT_REFERENCE_ANALYSIS.md) |
| Odoo / ERPNext / Frappe HRMS / Bigcapital | 企业业务领域 | ERP 领域组合、财务、HR/绩效、可扩展业务对象 | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |
| Five-project synthesis | Business Application Platform architecture | Platform Core、Business Module、contribution、communication、lifecycle 的综合取舍 | [`BUSINESS_APPLICATION_PLATFORM_REFERENCE_SYNTHESIS.md`](BUSINESS_APPLICATION_PLATFORM_REFERENCE_SYNTHESIS.md) |
| Plane / Chatwoot / Comp AI CRM | 专业协作与 Agentic Business App | 项目、客户交互、Evidence-first Agent、Durable Agent Task | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |
| OpenContracts / Mayan EDMS / Paperless-ngx / Documenso | 合同与企业文档 | Document/File/Version、解析绑定、Evidence、恢复、签署 | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |
