# 外部参考项目

本目录保存对外部项目、产品和技术方案的事实性研究。参考材料用于形成架构输入，不能直接覆盖本项目的 Baseline、ADR、数据所有权或安全边界。

## 使用规则

1. 记录检查日期、来源、许可证和版本/提交；
2. 区分外部项目事实、项目适配分析和本项目正式决策；
3. 明确哪些能力采用、改造采用、延后或拒绝；
4. 长期架构变化必须进入 `docs/architecture/` 并通过 ADR；
5. 实现顺序和验收标准必须进入 `docs/plans/current/`。

## Tier-1 Architecture References

| 项目 | 分类 | 主要参考价值 | 分析文档 |
|---|---|---|---|
| Twenty | Business App / Module Packaging / Metadata / UI / Lifecycle | Application Manifest、stable identity、source→manifest、版本兼容、UI/Role/Logic/Agent Contribution、install/upgrade/uninstall；不采用任意跨模块 schema 注入 | [`TWENTY_REFERENCE_ANALYSIS.md`](TWENTY_REFERENCE_ANALYSIS.md) |
| Canner/WrenAI | Semantic Contract / GenBI / Context Layer | MDL、语义建模、source-to-compiled manifest、结构化校验与 dry-plan；不采用其运行时和任意 SQL/MCP 边界 | [`WRENAI_REFERENCE_ANALYSIS.md`](WRENAI_REFERENCE_ANALYSIS.md) |
| Cloudflare OS | Enterprise AI Workspace / Agent Application | Workspace、Gatekeeper、Gadget、Blueprint、Capability-based security、Observation/Observer | [`CLOUDFLARE_OS_REFERENCE_ANALYSIS.md`](CLOUDFLARE_OS_REFERENCE_ANALYSIS.md) |
| OpenContracts / Mayan EDMS | Document / Revision / Evidence | Document/File/Version、解析绑定、Evidence、恢复和文档生命周期 | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |

Tier-1 只表示架构研究优先级，不表示本项目运行时依赖、代码复用授权或产品替代关系。

## 其他已登记项目

| 项目 | 分类 | 主要参考价值 | 分析文档 |
|---|---|---|---|
| Odoo / ERPNext / Frappe HRMS / Bigcapital | 企业业务领域 | ERP 领域组合、财务、HR/绩效和企业业务词典 | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |
| Plane / Chatwoot / Comp AI CRM | 专业协作与 Agentic Business App | 项目、客户交互、Evidence-first Agent、Durable Agent Task | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |
| Paperless-ngx / Documenso | 合同与企业文档 | 文档版本、归档、签署和可信产物 | [`BUSINESS_DOMAIN_REFERENCE_PROJECTS.md`](BUSINESS_DOMAIN_REFERENCE_PROJECTS.md) |

## 当前组合参考定位

```text
Twenty
  → Business App / Module Packaging / Metadata / UI / Lifecycle

WrenAI
  → Semantic Contract / Context / Analytics

Cloudflare OS
  → Workspace / Capability / Agent Application

OpenContracts + Mayan
  → Document / Revision / Evidence

business-platform 自身
  → Rust DDD / Data Ownership / Audit / Integrity / Durable Execution
```

任何参考项目都不得绕过本项目的 ADR、Architecture Baseline、Policy/Audit 和 Business Module Isolation。
