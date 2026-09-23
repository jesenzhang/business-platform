# SaaS Platform Reference Matrix

> 文档类型：Reference Research  
> 状态：Living Reference  
> 日期：2026-09-23  
> 用途：为 Enterprise SaaS Platform 各能力边界提供可采用/可适配/仅参考的开源实现  
> 决策规则：Reference 不覆盖本仓 Baseline/ADR；外部项目必须通过 Port/Contract 接入

## 1. 结论

本项目不寻找单一“SaaS 全家桶”。采用“模块边界稳定、实现可替换”的组合策略：

```text
business-platform owns:
Tenant / Organization / Membership
Business Permission Vocabulary
Resource Ownership
Business Domain
Entitlement semantics
Agent Platform contracts
Audit/Usage event semantics

external components may own:
credential issuance
authorization computation backend
metering/billing engine
secret value/PKI
webhook delivery
notification delivery
feature rollout engine
telemetry backend
object bytes
```

Rust 优先用于本平台 Domain/Application/Ports/Compiler；外部组件不以语言作为采用门槛。

## 2. 评估标签

- **ADOPT**：适合直接以独立服务/协议集成；
- **ADAPT**：吸收模型并通过本平台 Port 接入，不采用其完整业务权威；
- **REFERENCE**：主要学习设计和失败模式；
- **DEFER**：当前无真实需求，不进入运行依赖；
- **REJECT-AS-CORE**：不可成为 Platform Core 权威。

## 3. Modular SaaS / Tenancy

### ABP Framework — ADAPT / REFERENCE

参考：

- https://abp.io/framework
- https://abp.io/docs/latest/framework/architecture/multi-tenancy
- https://abp.io/docs/latest/framework/fundamentals/authorization

价值：

- 模块化单体与可复用 application module；
- multi-tenancy 为一等能力；
- pooled / database-per-tenant / hybrid 数据策略；
- permission、background jobs、DDD、auto API；
- 证明“独立模块”不等于“立即微服务”。

吸收：

- modular package 的独立版本和依赖思想；
- host/tenant 分离；
- tenant-aware permission；
- 同模块可 embedded 或 service 化。

不采用：

- .NET runtime 作为本项目基础；
- ABP SaaS 商业模块成为业务 Tenant 权威。

### Frappe / ERPNext — REFERENCE

参考：

- https://frappeframework.com/
- 仓库已有 `FRAPPE_ERPNEXT_REFERENCE_ANALYSIS.md`

价值：

- metadata-driven application module；
- role/permission；
- background job；
- multi-site tenant；
- module lifecycle/migration/UI contribution。

吸收其成熟产品经验，不采用 metadata 替代 DDD Domain。

### Odoo / Twenty / Plane — REFERENCE

仓库已有专题材料。继续用于：

- module manifest/lifecycle；
- workspace/membership；
- business resource relation；
- extension/contribution；
- migration/compatibility。

## 4. Identity / AuthN

### ZITADEL — ADOPT candidate

参考：

- https://zitadel.com/docs
- https://zitadel.com/docs/guides/manage/console/organizations-overview

适合承担：

- OIDC/OAuth2；
- SAML；
- MFA/Passkey；
- federation；
- IdP session/token；
- B2B identity organization。

边界：

ZITADEL Organization 可映射外部身份组织，但不得直接替代 business-platform Tenant/Organization 权威。

### Logto — ADOPT candidate

参考：

- https://docs.logto.io/organizations
- https://docs.logto.io/authorization/role-based-access-control
- https://docs.logto.io/quick-starts/m2m

价值：

- organization-aware B2B identity；
- user/M2M roles；
- organization token/scopes；
- 适合 SaaS 和 service account 场景。

边界同上。当前 ADR-0024 允许替换 IdP，不绑定具体产品。

## 5. Authorization

### OpenFGA — PRIMARY REFERENCE / ADAPT candidate

参考：

- https://openfga.dev/docs/concepts
- https://openfga.dev/docs/modeling
- https://openfga.dev/docs/modeling/agents/rag-authorization

价值：

- ReBAC；
- user/group/org/project/document 关系；
- Check/Lookup；
- RAG document/folder authorization 模型。

推荐用途：

- Document/Knowledge/Project/Contract Resource relationship；
- explicit share；
- group inheritance；
- Agent/Tool/KB resource authorization。

平台继续拥有 permission vocabulary 和 resource ownership，OpenFGA 仅作为 `AuthorizationEnginePort` 实现。

### SpiceDB — REFERENCE / alternative

参考：

- https://authzed.com/docs/
- https://authzed.com/docs/spicedb/concepts/relationships
- https://authzed.com/docs/spicedb/concepts/caveats

价值：

- Zanzibar/ReBAC；
- consistency token；
- expiring relationship；
- caveat/contextual check；
- 大规模 permission graph。

适合未来高规模/复杂关系权限，不与 OpenFGA 同时默认部署。

### Cerbos — REFERENCE / contextual policy adapter

参考：

- https://docs.cerbos.dev/cerbos/latest/
- https://www.cerbos.dev/features-benefits-and-use-cases/abac

价值：

- principal/resource/action/context policy；
- RBAC/ABAC/PBAC；
- stateless PDP；
- Rust/Python 等 PEP integrations。

适合需要复杂上下文策略时补充；不先与 ReBAC 引擎双重建设。

## 6. Commercial / Usage / Entitlement

### OpenMeter — PRIMARY ADOPT candidate

参考：

- https://openmeter.io/docs/billing/entitlements/overview

价值：

- UsageEvent aggregation；
- metered/static/boolean entitlement；
- credit/balance；
- AI token/API request 等高成本资源限额。

推荐：

`business-platform` 定义 Feature/Plan/UsageEvent/Entitlement contract，OpenMeter 作为 Metering/Entitlement adapter。

### Lago — ALTERNATIVE / REFERENCE

参考：

- https://www.getlago.com/
- https://docs.getlago.com/

价值：

- usage/subscription/invoice/payment orchestration；
- credit/wallet。

采用前必须单独检查当前 OSS 许可证和 self-host 功能边界。

## 7. Secrets / Workload Identity

### Infisical — PRIMARY ADOPT candidate

参考：

- https://infisical.com/platform/secrets-management

价值：

- human/machine identity；
- scoped RBAC；
- temporary access；
- automated rotation；
- dynamic secrets；
- audit；
- SDK/CLI/self-host；
- AI agent secret proxy 思路。

平台只保存 `SecretRef` / binding，不保存 provider secret value。

### OpenBao — REFERENCE / infrastructure alternative

参考：

- https://openbao.org/
- https://openbao.org/docs/2.5.x/secrets/pki/

价值：

- secret engine；
- dynamic credential；
- PKI/X.509；
- short-lived certificate；
- infrastructure-first Vault-like boundary。

如果需要 PKI/KMS/dynamic DB credentials，可优先于自研。

## 8. Background / Durable Execution

### Apalis — PRIMARY Rust generic-job reference

参考：

- https://github.com/apalis-dev/apalis

用途：

- generic background jobs；
- PostgreSQL/Redis queue；
- retry/schedule/worker。

限制：

不能替代本项目 Document Processing/Runtime Governance 的 durable domain semantics。

### Trigger.dev — REFERENCE

参考：

- https://trigger.dev/

价值：

- long-running durable task；
- retries/queues；
- HITL wait；
- schedule/concurrency；
- streaming/observability；
- crash/redeploy survival。

只吸收 lifecycle/UX/evidence；不引入 TypeScript runtime 作为业务执行权威。

### jarvis-rs / AgentOS — REFERENCE for Agent Runtime

用于：

- durable Agent Run；
- approval；
- checkpoint/recovery；
- runtime adapter；
- agent/team/workflow platform。

business-platform 通过 RuntimeBinding/Agent Platform contract 使用，不复制第二套用户/tenant/authorization。

## 9. Object / Artifact Storage

### RustFS — ADOPT candidate behind S3 port

参考：

- https://docs.rustfs.com/en/reference/s3-compatibility
- https://docs.rustfs.com/en/developer/sdk

价值：

- Rust 实现；
- S3 protocol；
- Rust/Python 等 SDK；
- common object operations/presigned/multipart。

边界：

保持当前 `object-storage` S3 port；RustFS 只是 deployment provider。兼容矩阵表明其目标是经过测试的 S3 子集，因此不能把“S3 compatible”解释成所有 AWS S3 edge behavior 完全等价。

## 10. Webhook / Notification

### Svix — ADOPT candidate

参考：

- https://www.svix.com/
- https://docs.svix.com/

承担：

- webhook endpoint；
- signing；
- retry；
- delivery tracking。

平台拥有 `WebhookIntent`，Svix 不拥有业务事件。

### Novu — ADOPT/REFERENCE candidate

参考：

- https://novu.co/
- https://docs.novu.co/

承担：

- email/SMS/push/chat/in-app delivery；
- preference/workflow/template/digest。

平台拥有 `NotificationIntent` 和业务触发条件。

## 11. Observability / AI Eval

### OpenTelemetry — ADOPT

参考：

- https://opentelemetry.io/docs/collector/architecture/

价值：

- vendor-neutral receive/process/export pipeline；
- traces/metrics/logs；
- Collector 作为统一出口。

当前 `observability` 应继续输出 OTel-compatible telemetry。

### Langfuse — ADOPT/REFERENCE for AI

参考：

- https://langfuse.com/docs/observability/overview
- https://langfuse.com/docs/evaluation/overview

适合：

- LLM/Agent trace；
- prompt/model/tool/retrieval latency/cost；
- online/offline eval；
- datasets/experiments。

不替代 runtime event/audit truth。

## 12. Feature Rollout

### Unleash — ADOPT candidate

参考：

- https://docs.getunleash.io/concepts/feature-flags

承担：

- feature flag；
- environment；
- activation strategy；
- rollout。

Platform Ops 保留 feature vocabulary 和 tenant rollout policy；Unleash 是执行引擎。

## 13. API Key / Public API Control

### Unkey — REFERENCE

参考：

- https://www.unkey.com/

价值：

- API key；
- rate limit；
- usage；
- public API identity。

是否采用取决于未来公开 API/Developer Platform 需求；当前不应与 OIDC/M2M 重复建设。

## 14. 推荐组合

短期不建议一次部署全部组件。

### Foundation

```text
business-platform Rust
├── identity / organization / policy (existing)
├── audit / messaging / object-storage / observability (existing)
├── business-module-contracts + compiler (existing)
└── new commercial/entitlement contracts
```

### External services, demand-driven

```text
AuthN          ZITADEL or Logto
AuthZ backend  current Policy first; OpenFGA when resource graph demands it
Metering       OpenMeter
Secrets        Infisical or OpenBao
Object store   S3 / RustFS
Webhook        Svix
Notification   Novu
Feature flag   Unleash
Telemetry      OTel stack
AI trace/eval  Langfuse
```

不要同时启用同类多个 engine。必须先有 business-platform Port 和 contract，再选择 provider。

## 15. 参考项目进入实现的门禁

外部项目只有满足以下条件才进入 runtime dependency：

1. 有真实业务消费者；
2. 本平台 Port/Contract 已定义；
3. license 已审查；
4. pinned version/revision 已记录；
5. deployment/data export/backup/upgrade 已评估；
6. failure/fallback 语义明确；
7. tenant/security contract tests 存在；
8. 替换 provider 不改变 Domain；
9. Architecture Fitness 不允许 provider type 穿透 Core；
10. 有可回滚集成计划。

