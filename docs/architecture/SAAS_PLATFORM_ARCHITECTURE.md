# Enterprise SaaS Platform Modular Architecture

> 文档类型：Architecture  
> 状态：Baseline（Accepted by ADR-0025）  
> 日期：2026-09-23
> 架构决策：ADR-0025 Enterprise AI SaaS Platform and Independently Releasable Modules  
> 适用范围：business-platform 的 SaaS Platform Core、Business Module、AI/Agent Platform、外部基础设施集成  
> 当前代码基线：main `9df475d5b834e4f43851deb9613e513f82ef8575`

## 1. 决策摘要

business-platform 是三个相关项目中企业级 SaaS Platform 的主承载项目。平台核心能力优先使用 Rust 实现；已经形成成熟独立产品边界的通用基础设施通过标准协议接入，不为了语言统一而重写。

本架构由 ADR-0025 接受并成为服务端 Baseline。它不要求“一个仓库包含 SaaS 的全部功能”，也不要求每个逻辑模块立即拆成微服务。目标是：

1. 每个功能模块拥有独立、稳定、版本化的公开合同；
2. 每个模块可以独立构建、测试、版本化、发布和替换；
3. 同一模块可以由本仓 Rust crate、独立服务或外部 OSS adapter 承担；
4. 消费者不因承载方式变化而改写业务逻辑；
5. Platform Core 不知道 Contract、Finance、Legal、HR 等具体业务名；
6. Business Module 继续拥有业务事实、领域不变量和正式写入；
7. SaaS 基础能力与 AI/Agent 能力共享 Tenant/Principal/Policy/Audit/Usage 等平台合同。

“独立发布”是 package/contract 级要求，不等于立即独立部署。默认仍采用模块化单体，出现独立扩缩容、安全域、生命周期或跨产品复用需求时再切换为独立服务。

## 2. 当前代码事实

截至基线，仓库已经具备本架构的重要基础，而不是从零开始：

- `shared-kernel::TenantContext` 已将 tenant/user/role/authentication level 作为请求上下文；
- PLAN-0013 已实现 `PlatformUser → TenantMembership → OrganizationMembership → RoleBinding → Permission/ResourceScope → PolicyDecision`；
- `identity`、`organization`、`policy` 均有独立 Domain/Application/Port，并具有 PostgreSQL/SQLite adapter；
- `identity-authorization-contracts` 让两个数据库适配器执行同一行为契约；
- `business-module-contracts::BusinessModuleManifest` 已定义 module ID/version、capability 依赖、published command/query/event、resource kind、migration namespace、module dependency、compatibility；
- `business-application-compiler` 提供 deterministic compile/dry-plan 基础；
- `semantic-contract` 提供模块公开 Dataset/Projection/Metric/Lineage 等语义合同；
- `messaging` 已有 Outbox/Inbox；`audit`、`observability`、`object-storage` 已存在明确 capability seam；
- ADR-0024 已确定“认证外置、授权内置”，平台不自研人员凭证签发。

因此 SaaS 化的正确方向是扩展现有 Business Application Platform，而不是引入第二套 SaaS/Plugin Core。

## 3. 架构原则

### 3.1 Authority first

模块按“谁拥有最终事实”划分，不按页面、数据库表、框架、供应商或部署进程划分。

- IdP 拥有登录凭证、MFA、OIDC session/token 签发；
- Tenancy Core 拥有平台 Tenant、Membership 和业务租户生命周期；
- Organization 拥有部门/组织关系；
- Policy 拥有平台授权定义、Binding 和决策语义；
- Business Module 拥有业务事实；
- Metering/Billing 引擎可以计算用量和账单，但产品套餐/Entitlement 语义由平台拥有；
- Secret Manager 保存 secret value，平台只持有 `SecretRef`；
- Observability 保存 trace/metric，不成为 Audit 或 Domain Event 的权威。

### 3.2 Logical ownership first, deployment second

```text
Bounded Context
  != Rust crate
  != process
  != microservice
  != database
```

默认实现可在一个 Rust workspace/进程内；模块合同必须允许以后移到独立进程而不改变调用者的业务语义。

### 3.3 Rust-first, protocol-first

核心 Domain、Application、Ports、contract compiler 优先 Rust。

外部成熟能力通过稳定协议接入：

- OIDC/OAuth2/SAML/SCIM：Identity Provider；
- HTTP/gRPC：Authorization、Billing、Secrets 等服务；
- S3：Object Storage；
- OTLP：Observability；
- CloudEvents/JSON/Protobuf：跨服务事件。

不因为 business-platform 是 Rust 就重写成熟 IdP、Webhook、Billing、Secret Vault。

### 3.4 Public contract only

跨模块只允许：

- versioned Command；
- versioned Query；
- versioned Integration Event；
- ResourceRef；
- Reference + Snapshot；
- Published Projection；
- Published Extension Point；
- declared Platform Capability。

禁止依赖其他模块 private repository、Row、table、migration、Rust internal type 或隐式单例状态。

## 4. 一级 SaaS 能力边界

### 4.1 Tenancy & Organization

**权威所有者：business-platform。**

负责：

- Tenant 生命周期；
- PlatformUser 与 TenantMembership；
- Organization/Department/Group；
- Membership；
- Resource Ownership；
- Tenant settings 引用；
- tenant-aware provisioning/deprovisioning。

不负责登录凭证、密码、MFA、token 签发。

当前落点：

- `shared-kernel`
- `identity`
- `organization`
- `identity-*`
- `organization-*`

后续需要把当前 `TenantContext` 演进为稳定、可跨进程序列化的 `PrincipalContext` / `TenantContext` contract，而不是让 Handler 自由拼接 claims。

### 4.2 Identity & Authorization

**AuthN 外置；AuthZ 语义内置。**

外部 IdP：

- login/password/passkey/MFA；
- OIDC/OAuth2/SAML；
- session/refresh；
- federation；
- SCIM/JIT provisioning 可选。

平台：

- issuer+subject → PlatformUser；
- Membership；
- Permission catalog；
- Role/RoleBinding；
- ResourceScope；
- authorize/explain；
- tenant isolation；
- business-state final authorization。

当前 `policy` 是平台授权权威。未来 OpenFGA/SpiceDB/Cerbos 只能作为 `AuthorizationEnginePort` 的实现或补充，不得成为业务权限词汇和资源所有权的第二权威。

### 4.3 Commercial & Entitlement

**当前主要缺口。**

拥有：

- Product/Plan；
- Feature/Entitlement；
- Subscription reference；
- Quota/Budget；
- Credit；
- UsageEvent contract；
- tenant/user/service-account cost attribution；
- billing provider binding。

建议边界：

```text
Business Platform
  owns Plan / Feature / Entitlement vocabulary
        |
        +--> UsageEvent --> Metering Adapter
        |
        +--> EntitlementCheck --> Metering/Entitlement engine
        |
        +--> Invoice/Billing reference --> Billing Adapter
```

OpenMeter/Lago 可以承担 meter/balance/invoice engine，但平台不能把“某套餐是否允许 Agent/模型/合同能力”的业务语义交给外部供应商模型。

### 4.4 Security & Credentials

平台只持有：

- `SecretRef`；
- Credential Binding；
- tenant/service identity scope；
- rotation metadata；
- access/audit reference。

Secret value、dynamic credential、PKI/KMS 由 Secret Manager 提供。

现有 `runtime-config::secret` / `secret_url` 是合适起点；后续应形成供应商无关 `SecretResolverPort`。

### 4.5 Execution & Automation

必须区分两类：

**Generic Background Job**

- email；
- webhook preparation；
- index refresh；
- import/export；
- non-authoritative maintenance。

Rust 首选参考 Apalis/PostgreSQL。

**Durable Domain Execution**

- Document Processing Job；
- Agent Run；
- Approval wait/resume；
- tool side effect；
- lease/fencing；
- checkpoint/recovery/reconciliation。

继续由本平台现有 durable execution / runtime governance 语义拥有，不得用通用 queue 抹平业务执行真相。

`jarvis-rs` 和 AgentOS/Trigger.dev 是此类执行语义的参考，不是 business-platform 的替代 Runtime。

### 4.6 Data & Resource Plane

拥有：

- business metadata；
- Document/Artifact/Knowledge/ResourceRef；
- revision/version/lineage；
- ACL metadata；
- blob checksum/object key；
- retention。

物理 bytes 通过 ObjectStoragePort 保存到 S3-compatible backend。

```text
PostgreSQL: authoritative metadata / relation / state
S3/RustFS: binary blob / artifact / large object
Search/Vector: rebuildable index/projection
```

索引、向量库和 Analytics projection 均不是业务事实权威。

### 4.7 Integration & Communication

包括：

- Integration Gateway / ACL；
- Outbox/Inbox；
- Webhook；
- Notification；
- Connector；
- external sync。

平台拥有 `WebhookIntent`、`NotificationIntent`、Integration Event 语义；Svix/Novu 等服务只负责交付。

### 4.8 Observability & Governance

分离三种事实：

```text
Domain/Audit Fact     authoritative, 不可采样丢失
Runtime Event         execution truth
Telemetry             logs/metrics/traces, 可聚合/采样
```

现有 `audit` 和 `observability` 边界保持。OpenTelemetry/Langfuse 不能替代 Audit/Domain Event。

治理还包括：

- retention；
- compliance export；
- data classification；
- integrity finding/repair；
- policy change audit；
- usage/cost attribution。

### 4.9 Operations & Delivery

负责：

- Platform Admin；
- Tenant Admin；
- module registry/lifecycle；
- config；
- feature rollout；
- health/readiness；
- deployment topology；
- backup/restore；
- upgrade/rollback；
- compatibility catalog。

业务模块 lifecycle 继续使用：

```text
Installed → Enabled ↔ Disabled → Uninstalled
Data: Retained / Purged (独立状态)
```

### 4.10 AI & Agent Platform（产品特有扩展）

这不是通用 SaaS 基础设施，但对 business-platform 是一等平台能力：

- AgentDefinition / AgentVersion；
- RuntimeBinding；
- ModelPolicy；
- Tool/MCP Registry；
- KnowledgeBinding；
- Capability Grant；
- Approval Policy；
- Agent Usage；
- Agent Trace/Eval；
- external runtime adapter（hdbot、jarvis-rs、Agno/AgentOS 等）。

Agent 不建立第二套用户/tenant/permission；必须消费同一 Principal/Tenant/Policy/Secret/Usage/Audit contract。

## 5. 可独立发布的目标拓扑

每个 SaaS/Business 模块按同一模型发布：

```text
Module Source
├── domain/application
├── public contracts
├── ports
├── manifest
├── contribution descriptors
├── migrations (owner namespace)
├── adapters
├── contract tests
└── release metadata
        |
        v
Independent Module Package
        |
        +--> Embedded Rust Adapter
        +--> Remote Service Adapter
        +--> External OSS Adapter
```

调用者只依赖 Module Contract/Port。

例如 Authorization：

```text
Application
   |
AuthorizationPort
   |
   +--> InProcessPolicyAdapter (current)
   +--> OpenFgaAdapter (future)
   +--> SpiceDbAdapter (future)
```

更换实现不改变 Contract/Document/Agent 的业务代码。

## 6. 当前代码映射与缺口

| 能力 | 当前代码 | 判断 |
|---|---|---|
| Tenant context | `shared-kernel` | 已有，需升级跨进程 contract |
| Identity | `identity` + PG/SQLite adapters | 已实现 foundation |
| Organization | `organization` + adapters | 已实现 foundation |
| Policy/AuthZ | `policy` + adapters | 已实现 foundation |
| AuthN | `business-api` OIDC relying party | 正确，继续外置 |
| Audit | `audit` + adapters | 已有 |
| Messaging | `messaging` | 已有 Outbox/Inbox |
| Object storage | `object-storage` | 已有 S3 seam |
| Observability | `observability` | 已有 |
| Durable processing | `document-processing*`, runtime governance | 已有强基础 |
| Business module packaging | `business-module-contracts`, compiler | 已有 foundation |
| Semantic contract | `semantic-contract` | 已有 foundation |
| Commercial/Entitlement | 无完整 owner | **优先新增** |
| Secret manager adapter | 仅配置/Secret URL 基础 | 待标准化 |
| Webhook delivery | 无独立能力 | 可外置 |
| Notification delivery | `notification` 骨架 | 需明确 intent/provider seam |
| Feature rollout | 无统一 owner | 后续 Operations capability |
| Agent Registry | `ai-application`/`agent-integration` + PLAN-0006 方向 | 待真实业务切片后推进 |
| Module Registry/install executor | compiler/dry-plan 已有 | 待后续实现 |

## 7. 数据与租户隔离

所有持久资源必须明确一种 tenant strategy：

1. pooled rows：每行 tenant_id + repository fail-closed filter；
2. schema/database silo：tenant-level routing；
3. hybrid：按 tenant tier/policy 选择。

禁止由 Handler 自带 tenant_id 作为唯一隔离。TenantContext 必须来自可信 Principal resolution，并传播到 Repository、Object Storage key、Job、Message、Audit、Usage 和 Agent runtime。

跨租户共享默认不支持；需要时必须建显式 share/grant 模型。

## 8. 发布与替换原则

任何模块如果要被称为“可独立发布”，至少必须满足：

- stable module ID；
- SemVer module version；
- versioned public contracts；
- manifest + package digest；
- compatibility window；
- own migration namespace；
- no private cross-module persistence dependency；
- contract tests；
- tenant/auth/audit behavior contract；
- documented failure model；
- rollback/data retention semantics；
- embedded/remote implementation behind same port；
- consumer 不引用 concrete adapter。

具体规范见 `docs/standards/SAAS_MODULE_STANDARD.md`。

## 9. 实施顺序

不改变当前“先真实 Contract Vertical Slice 验证平台”的方向。

建议顺序：

```text
0. 保持 PLAN-0013 Identity/AuthZ 基线
1. Contract Vertical Slice 消费真实 Policy/Tenant/Document contracts
2. 固化 PrincipalContext + SaaS Module Standard
3. Commercial/Entitlement/Usage foundation
4. SecretResolver + service/workload identity seam
5. Module Registry + independent package release evidence
6. Webhook/Notification provider seams
7. Agent Registry/RuntimeBinding（复用 PLAN-0006）
8. external AuthZ/Metering/Secret adapters 按真实需求接入
9. tenant admin / operator control plane
```

不要因为参考项目已经存在就同时部署所有外部组件；先固定 Port/Contract，再以真实需求激活 adapter。

## 10. 与现有架构的关系

本文件不替代以下文档，而是把其能力组合成 SaaS Platform 视图：

- `SERVER_BACKEND_ARCHITECTURE.md`：总体 DDD/模块化单体；
- `BOUNDED_CONTEXT_MAP.md`：业务上下文；
- `BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md`：Business Module/Packaging；
- `IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md`：IAM；
- `DATA_OWNERSHIP_AND_CONSISTENCY.md`：事务/一致性；
- `WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`：执行；
- `ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`：Agent/Workspace；
- ADR-0020/0021/0022/0024：模块、通信、认证边界。

改变上述既有 Authority 或数据所有权仍需 ADR。本文件不能被解释为已部署 OpenFGA、OpenMeter、Infisical、RustFS、Svix、Novu、Unleash、AgentOS 或其他外部服务。


## Shared Execution / Sandbox Infrastructure Boundary (Proposed 2026-09-24)

Controlled execution is treated as an Infrastructure Plane capability, not as a Business Module or an AI Worker private implementation.

    Business Modules / Workspace / Durable Jobs
                    |
            ExecutionServicePort
                    |
         Shared Execution/Sandbox Service
                    |
              execution nodes
                    |
        process / OS sandbox / container / VM providers

Normative ownership direction:

- Business Platform retains tenant authorization, business state, Durable Job/JobStep/Attempt authority, approval, business audit and formal result commit.
- The Execution/Sandbox Service owns only environment lifecycle, provider admission, resource placement, isolation enforcement, process/PTY mechanics, cleanup and execution evidence.
- The service must be independently deployable and replaceable; no shared database or cross-service transaction with Business Platform is assumed.
- Jarvis may consume the same service semantics through an independent adapter, but Business Platform must not depend on Jarvis Runtime crates or desktop internals.
- Local/fake providers are allowed for development and contract tests; production guarantees must be re-proved by the selected remote backend.
- Backend selection (Docker/Kubernetes/containerd/microVM/VM) is deferred. Unsupported requested guarantees fail closed; no silent isolation downgrade is allowed.
- Optional snapshot/restore/fork are reserved capabilities, not current platform guarantees.

Authority candidate: [ADR-0026](../adr/ADR-0026-execution-sandbox-as-shared-infrastructure-service.md). Detailed design: [EXECUTION_SANDBOX_SERVICE_ARCHITECTURE.md](EXECUTION_SANDBOX_SERVICE_ARCHITECTURE.md). Proposed plan: [PLAN-0015](../plans/current/PLAN-0015-execution-sandbox-service-boundary.md).

This proposal does not activate a new runtime/service and does not block PLAN-0014 or the Contract Business Vertical Slice.
