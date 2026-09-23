# ADR-0025：Enterprise AI SaaS Platform 与独立可发布模块

> 状态：Accepted  
> 日期：2026-09-23  
> 决策所有者：Platform Architecture  
> 关联文档：SAAS_PLATFORM_ARCHITECTURE、SAAS_MODULE_STANDARD、BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE、PLAN-0014  
> 替代：无  
> 被替代：无

## 1. 背景

business-platform 已形成 production OIDC、Tenant/User Principal、Identity/Organization/Policy、Document/Processing、Audit/Governance、Object Storage、Messaging、Business Module Manifest/compiler、Public API/CLI/MCP 和 Agent/Workspace 架构边界。

SaaS 所需的认证、细粒度授权计算、计量计费、Secret、Webhook、通知、Feature Flag、Telemetry、对象存储等问题已有成熟独立 OSS/服务。全部内建会重复造轮子；直接把外部产品模型当 Domain，又会失去 Tenant、业务权限、Resource Ownership、Entitlement 和业务事实的权威边界。

现有 Rust crate 已有较好代码隔离，但多数仍共用 workspace version，也没有完整独立 manifest/release artifact，因此“crate 已模块化”不能等价为“模块已可独立发布”。

## 2. 决策驱动因素

- business-platform 已是相关项目中最完整的企业业务、数据与权限承载层；
- 核心 Domain 优先 Rust，外部基础设施不应因语言不同被重写；
- SaaS Platform 必须有明确 Tenant/Principal/AuthZ/Entitlement/Audit Authority；
- 模块应能被本平台内嵌、独立服务化或被其他产品复用；
- 独立发布必须可验证，不能只依赖目录/crate 结构；
- 模块化不能被误解为立即微服务化；
- Jarvis/hdbot 等 Runtime 应消费平台合同，而不是复制 Tenant/RBAC/Billing；
- 成熟参考项目应成为质量校准输入，而不是框架依赖。

## 3. 决策

### 3.1 business-platform 是 Enterprise AI SaaS Platform 主承载

```text
SaaS Platform Core
  Tenancy / Identity Mapping / Policy
  Commercial & Entitlement
  Security/Credential Binding
  Audit/Governance/Operations
        +
Business Application Platform
  Contract / Document / Customer / Project / Approval / Finance / ...
        +
AI & Agent Platform
  Agent Registry / Tool-MCP / Knowledge Binding / Runtime Binding / Capability
```

三层共享同一 Tenant/Principal/Policy/Audit/Usage contract，不创建第二套用户、租户或权限体系。

### 3.2 核心 Rust-first，基础设施 protocol-first

Domain/Application/Ports/contract compiler 优先 Rust。外部成熟组件可承担 credential issuance、授权计算、metering/billing、secret/PKI、object bytes、webhook/notification、feature rollout、telemetry 和 AI trace/eval。

所有外部组件必须通过稳定 Port/HTTP/gRPC/Event/S3/OTLP 等协议接入。Provider 的语言、SDK、DTO、数据库模型不得成为本平台 Domain/public contract。

### 3.3 平台必须保留的 Authority

无论采用什么外部组件，business-platform 必须拥有：

- Tenant 与 Membership 语义；
- Organization/业务资源关系的 canonical ID；
- 业务 Permission vocabulary；
- Resource Ownership；
- Product/Feature/Entitlement 语义；
- UsageEvent/AuditEvent 语义；
- Business Domain facts；
- Agent Platform contract 与 Delegation/Capability 上限。

### 3.4 模块独立发布采用 R0-R4

```text
R0 Internal
R1 Isolated
R2 Independently Versioned Package
R3 Deployment Independent
R4 Cross-product Reusable
```

所有正式 Platform/Business/Agent Module 的目标最低等级为 R2。R2 必须有 stable identity、独立 release version、manifest、public contract、digest、compatibility、migration namespace（如持久化）、contract tests、release metadata 和 reproducible artifact。

workspace/product version 不等于 module canonical release version。

### 3.5 独立发布不等于独立部署

默认仍为模块化单体。R2 package 可以嵌入 business-api/worker。只有出现可测量的独立扩缩容、安全域、故障域、发布周期、特殊运行时或跨产品复用需求时，才进入 R3 standalone service；拆服务继续需要独立 ADR。

### 3.6 同一 Port 支持三种承载

```text
Application -> CapabilityPort
               ├─ Embedded Rust Adapter
               ├─ Remote Service Adapter
               └─ External Provider Adapter
```

调用方不能因承载方式不同改变业务规则。

### 3.7 R2 前强制 Reference Conformance Review

每个模块进入 R2 前必须：

- 至少两个同领域成熟 reference；
- 至少一个真实生产型 OSS/平台；
- 固定 reviewed revision/release；
- 检查核心源码、测试、failure/security/recovery semantics；
- 形成 capability matrix；
- gap 分为 C0/C1/C2/C3/REJECT；
- C0=0、C1=0；
- independent reviewer 对固定 SHA PASS。

对齐允许 KEEP/DEFER/REJECT，不要求复制参考项目。

## 4. 不采用的方案

- 不新建独立 SaaS 总控仓库复制 business-platform 的 Tenant/Policy/Document/Audit；
- 不用单一 SaaS Framework 全量替换现有 Rust DDD/Authority；
- 不把每个 capability 立即微服务化；
- 不为了语言统一重写成熟基础设施；
- 不把外部 AuthZ/Billing/IdP schema 提升为业务权威。

## 5. 后果

正面：SaaS、Business、Agent 三层 Authority 统一；Rust 模块可渐进 R1→R2→R3→R4；外部 OSS 可替换且不污染 Domain；Jarvis/hdbot 可消费稳定跨语言 contract。

成本：需要独立 module release identity/artifact、compatibility、Reference Conformance 和独立 review；Remote adapter 只在真实复用需求出现时实现。

## 6. 实施

1. 将 SAAS_PLATFORM_ARCHITECTURE 晋升 Baseline；
2. 将 SAAS_MODULE_STANDARD 晋升 Baseline；
3. 对齐 Backend Manifest、Server Architecture、Bounded Context Map、Business Application Platform、Code、Deployment、Fitness；
4. PLAN-0014 执行成熟模块 reference alignment；
5. Contract Business Vertical Slice 继续作为下一主要业务代码计划；
6. Wave 5 至少两个模块完成 R2 pilot 后，再决定是否扩大独立发布转换。

本 ADR 不授权本轮新增 OpenFGA/OpenMeter/Infisical/Svix/Novu/Unleash 等 runtime dependency，也不授权微服务拆分。

## 7. 验证

文档对齐必须证明 Authority、R0-R4/R2、independent release vs deployment、provider boundary 与 PLAN-0014 gate 一致，且无 Rust/migration/OpenAPI/runtime 变更。

## 8. 后续复审条件

- business-platform 不再承担 SaaS Product/Control Plane；
- Tenant/Entitlement/Authorization Authority 改由外部系统完全拥有；
- R2 package model 无法支持实际跨产品复用；
- 多数模块需要独立部署，模块化单体不再满足运行指标；
- 监管要求改变 identity/credential/data isolation 责任边界。
