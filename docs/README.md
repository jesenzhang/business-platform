# 项目文档中心

本目录是项目文档的统一入口。文档位置、状态和权威关系由 [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) 管理。

## 1. 权威顺序

```text
总体产品与系统架构
→ 服务端架构清单
→ 服务端总体与专题架构
→ 代码架构和标准
→ 当前计划
→ 当前实现
```

发生冲突时，当前代码不能自动覆盖 Baseline；应修正实现或通过 ADR 修改架构。

## 2. 总体与服务端架构

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| 总体架构 | [`../企业AI业务平台与智能助手总体架构方案_v2.md`](../企业AI业务平台与智能助手总体架构方案_v2.md) | Baseline（内部 v2.2） | 产品、系统主体、Agent 与总体部署边界 |
| 完整服务端架构清单 | [`architecture/BACKEND_ARCHITECTURE_MANIFEST.md`](architecture/BACKEND_ARCHITECTURE_MANIFEST.md) | Baseline | 定义完整架构文档集、权威关系和任务准入 |
| 服务端总体架构 | [`architecture/SERVER_BACKEND_ARCHITECTURE.md`](architecture/SERVER_BACKEND_ARCHITECTURE.md) | Baseline | 战略 DDD、模块化单体、分层、数据所有权和质量属性 |
| Bounded Context Map | [`architecture/BOUNDED_CONTEXT_MAP.md`](architecture/BOUNDED_CONTEXT_MAP.md) | Baseline | 业务能力、上下文职责与协作关系 |
| 企业业务领域与跨部门协作 | [`architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md`](architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md) | Baseline | 合同/财务/法务/HR/绩效、Party 主数据、跨部门审计核对、正式报告和共享数据边界 |
| 数据与一致性 | [`architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`](architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md) | Baseline | 数据所有权、事务、事件、幂等和补偿 |
| 长时任务 | [`architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`](architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md) | Baseline | 业务状态、流程协调和可靠执行边界 |
| Enterprise AI Workspace | [`architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`](architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md) | Baseline | Workspace、Skill、Context、Capability、Observation、Artifact 和 Generated App 边界 |
| 质量属性 | [`architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`](architecture/QUALITY_ATTRIBUTE_SCENARIOS.md) | Baseline | 性能、可用性、安全、恢复和容量验收 |
| 安全架构 | [`architecture/SECURITY_ARCHITECTURE.md`](architecture/SECURITY_ARCHITECTURE.md) | Baseline | 身份、租户、授权、文件、AI 和 Agent 安全 |
| Identity & Authorization | [`architecture/IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md`](architecture/IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md) | Baseline | 外部 OIDC 后的平台用户、Tenant Membership、Organization、Role/Permission/RoleBinding、Resource Scope 与 Policy Decision |
| 部署架构 | [`architecture/DEPLOYMENT_ARCHITECTURE.md`](architecture/DEPLOYMENT_ARCHITECTURE.md) | Baseline | 进程、网络、环境、发布和扩缩容 |
| 可观测性 | [`architecture/OBSERVABILITY_ARCHITECTURE.md`](architecture/OBSERVABILITY_ARCHITECTURE.md) | Baseline | 日志、指标、追踪、审计和告警 |
| Runtime Audit | [`architecture/RUNTIME_AUDIT_ARCHITECTURE.md`](architecture/RUNTIME_AUDIT_ARCHITECTURE.md) | Baseline profile | 统一审计模型、原子写入和查询 |
| Integrity and Repair | [`architecture/DATA_INTEGRITY_AND_REPAIR_ARCHITECTURE.md`](architecture/DATA_INTEGRITY_AND_REPAIR_ARCHITECTURE.md) | Baseline profile | Finding、受控修复和恢复边界 |
| Audit Retention | [`architecture/AUDIT_RETENTION_AND_TAMPER_EVIDENCE.md`](architecture/AUDIT_RETENTION_AND_TAMPER_EVIDENCE.md) | Baseline profile | 保留、归档和 Hash Chain 证据 |
| 数据治理、分析与可视化 | [`architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`](architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md) | Baseline | 可重建分析投影、指标语义、受控查询、Dashboard 与报表边界 |
| Business Module Isolation 与 Semantic Contract | [`architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`](architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md) | Baseline | 平台核心/业务模块隔离、模块 manifest、语义贡献、确定性编译和 legacy ACL 边界 |
| Business Application Platform | [`architecture/BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md`](architecture/BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md) | Baseline | Platform Core/Business Module 边界、贡献、跨模块协作、生命周期和 synthetic validation |
| Enterprise SaaS Platform | [`architecture/SAAS_PLATFORM_ARCHITECTURE.md`](architecture/SAAS_PLATFORM_ARCHITECTURE.md) | Proposed | 将现有模块平台提升为 Tenant/AuthZ/Commercial/Security/Execution/Data/Integration/Governance/Ops/Agent 的 SaaS 组合架构；Rust-first、provider-neutral |
| 遗留迁移 | [`architecture/LEGACY_MIGRATION_ARCHITECTURE.md`](architecture/LEGACY_MIGRATION_ARCHITECTURE.md) | Baseline | 从现有系统渐进迁移与退出策略 |
| 代码架构 | [`architecture/CODE_ARCHITECTURE.md`](architecture/CODE_ARCHITECTURE.md) | Baseline | crate、层次、依赖和运行边界 |
| 架构状态 | [`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md) | Living | 当前实现符合程度和计划门禁 |
| Durable Document Processing | [`architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`](architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md) | Baseline | PLAN-0004 的 Job、固定 Pipeline、Worker、Review 和恢复边界 |

以上文档共同构成完整服务端架构，不能只选择其中一份解释系统设计。

## 3. 标准与基础设施

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| API 与事件 | [`standards/API_AND_EVENT_CONTRACT_STANDARD.md`](standards/API_AND_EVENT_CONTRACT_STANDARD.md) | Baseline | 命令、查询、事件、版本、幂等和兼容 |
| 架构门禁 | [`standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`](standards/ARCHITECTURE_FITNESS_FUNCTIONS.md) | Baseline | CI 依赖检查、契约测试和发布证据 |
| Rust 编码 | [`standards/RUST_CODING_STANDARD.md`](standards/RUST_CODING_STANDARD.md) | Baseline | Rust 代码、错误、异步、测试和安全规则 |
| 查询与数据库适配 | [`standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`](standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md) | Baseline | Query Object、Read DTO、分页、SQL/ORM 与多数据库规则 |
| 跨模块通信 | [`standards/INTER_MODULE_COMMUNICATION_STANDARD.md`](standards/INTER_MODULE_COMMUNICATION_STANDARD.md) | Proposed input to ADR-0022 | Query、Command、Event、ResourceRef、Snapshot、Projection 和 Saga 边界 |
| SaaS Module 独立发布 | [`standards/SAAS_MODULE_STANDARD.md`](standards/SAAS_MODULE_STANDARD.md) | Proposed | 每个 Platform/Business/Agent 模块的独立 identity/version/manifest/contracts/migration/adapters/release 与 embedded/remote/provider 可替换规则 |
| 基础设施验证 | [`../企业AI业务平台基础设施开发验证与预生产方案_v1.md`](../企业AI业务平台基础设施开发验证与预生产方案_v1.md) | Baseline | 本地、测试、CI、预生产与恢复 |
| 文档治理 | [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) | Baseline | 文档目录、生命周期、变更和归档 |

## 4. 已接受架构决策

- [`adr/ADR-0001-s3-sdk-selection.md`](adr/ADR-0001-s3-sdk-selection.md)：对象存储采用 `aws-sdk-s3`。
- [`adr/ADR-0002-outbox-claim-retry.md`](adr/ADR-0002-outbox-claim-retry.md)：Outbox claim/lease/retry 设计。
- [`adr/ADR-0003-domain-driven-layered-backend.md`](adr/ADR-0003-domain-driven-layered-backend.md)：服务端采用战略 DDD、模块化单体、领域/应用/适配器分层、数据所有权、质量属性与自动架构门禁。
- [`adr/ADR-0004-rust-msrv-toolchain.md`](adr/ADR-0004-rust-msrv-toolchain.md)：Rust 1.94.1 为当前锁定依赖的最低验证工具链。
- [`adr/ADR-0008-cqrs-query-model-and-read-projections.md`](adr/ADR-0008-cqrs-query-model-and-read-projections.md)：命令/查询分离与可重建 Read Projection。
- [`adr/ADR-0009-multi-database-persistence-adapters.md`](adr/ADR-0009-multi-database-persistence-adapters.md)：PostgreSQL 生产权威与 SQLite 本地适配策略。
- [`adr/ADR-0010-durable-processing-job-and-fixed-pipeline.md`](adr/ADR-0010-durable-processing-job-and-fixed-pipeline.md)：持久化 Job 与固定处理 Pipeline。
- [`adr/ADR-0011-worker-leases-fencing-and-crash-recovery.md`](adr/ADR-0011-worker-leases-fencing-and-crash-recovery.md)：Worker Lease、Fencing 与崩溃恢复。
- [`adr/ADR-0012-document-candidate-and-human-review.md`](adr/ADR-0012-document-candidate-and-human-review.md)：候选结果与人工复核边界。
- [`adr/ADR-0013-unified-runtime-audit-model.md`](adr/ADR-0013-unified-runtime-audit-model.md)：统一 Runtime Audit 模型。
- [`adr/ADR-0014-data-integrity-finding-lifecycle.md`](adr/ADR-0014-data-integrity-finding-lifecycle.md)：完整性 Finding 生命周期。
- [`adr/ADR-0015-controlled-repair-and-approval.md`](adr/ADR-0015-controlled-repair-and-approval.md)：受控修复与审批。
- [`adr/ADR-0016-repair-ledger-and-verification.md`](adr/ADR-0016-repair-ledger-and-verification.md)：Repair Ledger 与验证。
- [`adr/ADR-0017-platform-native-analytics-and-visualization.md`](adr/ADR-0017-platform-native-analytics-and-visualization.md)：平台原生分析与可视化，建立在 ADR-0008 与 ADR-0013～0016 之上。
- [`adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md`](adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md)：接受 Enterprise AI Workspace、任务级 Capability、Observation 血缘与 Artifact 非权威边界；Cloudflare OS 仅作为参考项目。
- [`adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md`](adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md)：接受 Party/Counterparty、Legal、People & Performance、Business Assurance & Reconciliation 的目标领域边界和跨部门 Reference + Snapshot 协作模式。
- [`adr/ADR-0020-business-module-isolation-and-semantic-contract.md`](adr/ADR-0020-business-module-isolation-and-semantic-contract.md)：接受 Business Module Isolation、Semantic Contract、确定性 compiler 和 C legacy ACL 预留。

## 5. 新近接受架构决策

- [`adr/ADR-0021-business-application-packaging-and-published-extension-points.md`](adr/ADR-0021-business-application-packaging-and-published-extension-points.md)：Accepted，Business Application Packaging、Contribution 和 Published Extension Point。
- [`adr/ADR-0022-inter-module-communication-and-business-collaboration.md`](adr/ADR-0022-inter-module-communication-and-business-collaboration.md)：Accepted，跨模块通信、一致性和 Process Manager/Saga。
- [`adr/ADR-0023-ai-worker-model-provider-dependency.md`](adr/ADR-0023-ai-worker-model-provider-dependency.md)：Accepted，ai-worker model-provider 可替换性与依赖边界。
- [`adr/ADR-0024-authentication-externalized-authorization-internal.md`](adr/ADR-0024-authentication-externalized-authorization-internal.md)：Accepted，人员认证/凭证外置到 OIDC IdP，平台内部拥有租户、业务授权和审计。

## 6. 文档目录

```text
docs/
├── README.md
├── governance/       治理、流程和文档制度
├── architecture/     业务、应用、数据、技术和部署架构
├── standards/        编码、测试、安全和契约规范
├── adr/              长期架构决策
├── plans/            当前执行计划与归档
├── reviews/          审查和验收记录
├── runbooks/         部署、恢复、值守和故障手册
├── reference/        外部参考与调研结论
└── templates/        标准模板
```

现有两份中文长文档暂时保留在仓库根目录，属于已登记迁移例外。后续新增长文档不得继续放在根目录。

## 7. 文档状态

- `Draft`：讨论中；
- `Proposed`：等待接受；
- `Baseline`：当前权威规则，代码必须遵循；
- `Accepted`：ADR 已接受；
- `Living`：持续反映当前状态，不替代 Baseline；
- `Superseded`：已被替代；
- `Archived`：不再参与当前决策。

## 8. 新任务使用方式

开始任何服务端任务前：

1. 阅读 `BACKEND_ARCHITECTURE_MANIFEST.md`；
2. 确认 Bounded Context 和数据所有者；
3. 明确业务不变量、用例、API/Event 和一致性；
4. 检查安全和质量属性；
5. 在计划中增加架构符合性章节；
6. 确定需要运行的 Fitness Functions；
7. 改变长期边界时新增 ADR；
8. 代码、测试和文档在同一变更中更新。

涉及合同、财务、法务、HR/绩效、Party 主数据、跨部门审计/核对/合并、正式业务报告或共享专业数据时，同时阅读：

```text
architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md
adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md
reference/BUSINESS_DOMAIN_REFERENCE_PROJECTS.md
```

涉及 AI Workspace、Agent、Skill、Context、Tool、Artifact 或 Generated App 时，同时阅读：

```text
architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md
adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md
reference/CLOUDFLARE_OS_REFERENCE_ANALYSIS.md
reference/EVER_GAUZY_REFERENCE_ANALYSIS.md
```

禁止从数据库表、Handler、消息 Topic、Prompt、Skill 文件或 SDK 直接推导业务边界。

## 9. 当前实施

截至 2026-09-19：

- v0.1 已发布：PLAN-0012 Integrated / Archived，annotated tag v0.1 → 2383651；真实 PostgreSQL + MinIO + vLLM + Prometheus/Grafana 与真实 Auth0 production-mode 验收已记录在完成审计。
- 发布后供应链修复已进入 main：rustls 0.23.45 清除 RUSTSEC-2026-0285；MinIO/mc 镜像从已下线的 Docker Hub 仓库迁移到 quay.io。
- 身份与授权已生产化：[PLAN-0013 Identity and Authorization Foundation](plans/archive/2026/PLAN-0013-identity-and-authorization-foundation.md) Integrated / Archived（PR #17）——PlatformUser/TenantMembership/Organization/Role/Permission/RoleBinding/ResourceScope 与统一 default-DENY Policy Decision 已实现，Governance API 已迁移到统一 Authorize + 有界兼容桥（7 个 governance 键，flag 默认开启）。
- 当前下一候选是 Contract Business Vertical Slice（待创建计划）：Contract List/Detail → Document Revision → AI Extract/Evidence → Review → Apply Candidate → Contract Version Update。
- [PLAN-0006 Enterprise AI Workspace](plans/current/PLAN-0006-enterprise-ai-workspace-foundation.md) 仍为 Revision 1 / Proposed / BLOCKED：PLAN-0013 已集成，还需等待首个 Contract 真实垂直切片集成。
- Business Application Platform 的 package/contribution/compiler/dry-plan foundation 已实现，但 Module Registry、安装执行器、Marketplace、动态插件 Runtime 尚未实现。
- Analytics/Visualization 只有 Baseline；Knowledge/RAG/通用 Workflow/Generated App Runtime 尚未形成生产实现。
- 外部参考增加 Ever Gauzy，重点吸收 tenant-aware runtime binding、Provider definition/binding、Compiled Tool Catalog + per-turn resolution；其内部 OAuth issuer 与 AGPL runtime/code 不采用。

当前优先顺序：

~~~text
PLAN-0013 Identity / Authorization (Integrated 2026-09-19)
  -> Contract Business Vertical Slice
  -> Knowledge/Evidence Projection（按业务需要）
  -> PLAN-0006 Revision 1 Workspace / Agent
  -> Controlled Write ActionPlan
  -> Analytics / Approval / Finance / Legal
~~~

近期不优先：通用 Workflow Designer、动态 WASM/Node/Python 插件、Marketplace、Generated App Sandbox、自建 OAuth Server、自建 RAG 引擎。

## 10. 合并后的后续任务规则

所有新计划和实施指令必须明确引用：

```text
docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md
docs/architecture/BOUNDED_CONTEXT_MAP.md
docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md
docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md
docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md
```

涉及合同、财务、法务、HR/绩效、跨部门核对/审计、Party 主数据或正式专业报告时，必须同时引用：

```text
docs/architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md
docs/adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md
```

涉及用户、Tenant Membership、组织、角色、Permission、RoleBinding、Resource Scope 或业务授权时，必须同时引用：

~~~text
docs/architecture/IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md
docs/architecture/SECURITY_ARCHITECTURE.md
docs/adr/ADR-0024-authentication-externalized-authorization-internal.md
~~~

涉及 AI Workspace、Agent、Capability、Observation、Artifact 或生成应用时，必须同时引用：

```text
docs/architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md
docs/adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md
```

涉及 Enterprise SaaS Platform、Tenant/Entitlement/独立模块发布或外部 SaaS infrastructure 组合时，同时阅读：

```text
architecture/SAAS_PLATFORM_ARCHITECTURE.md
standards/SAAS_MODULE_STANDARD.md
reference/SAAS_PLATFORM_REFERENCE_MATRIX.md
```

涉及 Business Module、Semantic Contract、Metric/Dimension/Lineage 注册、模块依赖或
legacy ACL 时，必须同时引用：

```text
docs/architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md
docs/architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md
docs/adr/ADR-0020-business-module-isolation-and-semantic-contract.md
```

涉及安全、长时任务、部署、可观测性或迁移时，同时引用对应专题 Baseline。

功能测试通过但架构门禁失败，任务不得声明完成。

## 11. 后续运行文档

架构设计已经形成，后续按实施阶段补充具体 Runbook：

- 本地开发和故障处理；
- 预生产部署；
- 正式生产部署；
- 数据库和对象存储恢复；
- 安全事件响应；
- Provider 故障和任务恢复；
- Agent Runtime、Workspace Turn 和 SSE 恢复；
- Capability 撤销与安全事件响应；
- 遗留系统切流与回滚。

Runbook 是架构的操作实现，不得修改 Baseline 语义。

## 12. 模板

- [`templates/ADR_TEMPLATE.md`](templates/ADR_TEMPLATE.md)
- [`templates/REFERENCE_CONFORMANCE_REVIEW_TEMPLATE.md`](templates/REFERENCE_CONFORMANCE_REVIEW_TEMPLATE.md)：成熟模块 Reference Conformance Review 与 R2 admission 固定模板。
