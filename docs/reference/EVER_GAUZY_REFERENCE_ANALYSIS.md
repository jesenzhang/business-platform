# Ever Gauzy Reference Analysis

> 文档类型：External reference / architecture input  
> 检查日期：2026-09-12  
> 项目：`ever-co/ever-gauzy`  
> 默认分支：`develop`  
> 固定检查提交：`33e3c09f303a08baef376bb9fde0aba1a057e77a`  
> 许可证：AGPL-3.0  
> 使用边界：仅吸收可观察架构机制、产品边界和失败模式；不复制代码、Schema、UI 资产或运行时实现

## 1. 结论摘要

Ever Gauzy 是当前已登记参考中，与 business-platform **完整产品形态最接近**的项目之一。它不是单一 ERP，也不是单一 Agent Framework，而是已经把以下能力组合在同一 Business Management Platform 中：

```text
ERP / CRM / HRM / ATS / PM / Finance / Time Tracking
                    +
Headless Business APIs
                    +
Plugin Registry / Tenant Configuration
                    +
AI Chat / Multi-provider AI
                    +
MCP Server / OAuth
```

对 business-platform 的核心影响不是推翻现有 Baseline，而是补强四个设计问题：

1. **AI Provider 的静态定义与租户级绑定如何分离**；
2. **Agent Tool/Contribution 如何在编译期登记、在每个 turn 按租户/用户/Capability 动态解析**；
3. **Embedded Chat、MCP、CLI 如何共享同一 Business Tool Catalog，而不是复制业务工具实现**；
4. **MCP/OAuth 接入如何服从 ADR-0024 的“认证外置、授权内置”边界**。

现有 `BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md`、`ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md` 和 ADR-0024 的总体方向仍成立；本分析不形成新的正式架构决策。

## 2. 证据与方法

本文将结论分为：

- **FACT**：可在固定提交源码或许可证中直接观察；
- **INFERENCE**：从实现机制推导出的架构含义；
- **PROJECT IMPACT**：对 business-platform 的适配建议，只有进入 Baseline/ADR 后才成为正式决策。

主要证据路径：

- `README.md`
- `LICENSE`
- `packages/core/src/lib/bootstrap/index.ts`
- `packages/plugins/ai-chat/src/lib/ai-chat.plugin.ts`
- `packages/plugins/ai-chat/src/lib/ai-chat.controller.ts`
- `packages/plugins/ai-chat/src/lib/provider-registry.ts`
- `packages/plugins/ai-chat/src/lib/tools/tool-registry.ts`
- `packages/plugins/ai-chat/src/lib/credentials/ai-provider-credential.entity.ts`
- `packages/plugins/registry/src/lib/domain/entities/plugin-tenant.entity.ts`
- `packages/mcp-server/README.md`
- `packages/auth/src/lib/mcp/index.ts`

## 3. 产品定位与业务覆盖

### 3.1 FACT

仓库 README 将 Ever Gauzy 定义为 Open Business Management Platform，并公开覆盖：

- ERP；
- CRM；
- HRM；
- ATS；
- Project / Task Management；
- Time / Activity / Productivity Tracking；
- Sales；
- Accounting / Invoicing / Billing / Payments / Income / Expense；
- Inventory / Equipment / Supply Chain；
- Organization / Department / Team；
- Roles / Permissions；
- Reports / Insights / Analytics；
- Headless APIs。

部署说明还列出 PostgreSQL、OpenSearch、MinIO、Redis、Jitsu、Cube 和 Zipkin 等基础设施，其中 Cube 被明确描述为 Reports / Dashboards / Analytics 使用的 Semantic Layer。

### 3.2 INFERENCE

Gauzy 最有价值的产品参考不是某一张表或某一个 ERP 页面，而是它证明了：

```text
Organization / People / Customer / Project / Finance / Approval / Analytics
```

可以在一个长期演进的平台中共享身份、租户、权限、文件、搜索、报表和扩展基础设施，同时仍提供 Headless API 给不同客户端。

### 3.3 PROJECT IMPACT

business-platform 不应复制 Gauzy 的领域对象或共享 ORM 模型；但应继续把它作为 P0 Business Platform Reference，用于检查：

- 业务模块组合是否缺少真实企业场景；
- 公共能力是否被错误放入某个具体业务模块；
- 页面/客户端是否绕过 Application API；
- 多租户配置是否有统一承载位置。

## 4. Plugin 与租户级启用模型

### 4.1 FACT：运行时插件组合

`packages/core/src/lib/bootstrap/index.ts` 接受 `ApplicationPluginConfig`，Gauzy 的插件通过宿主 bootstrap 组合。AI Chat、AI Provider、Documents、Registry 等能力均以 package/plugin 形式存在。

`packages/plugins/registry/src/lib/domain/entities/plugin-tenant.entity.ts` 明确维护 tenant/organization 级 Plugin 安装与配置状态，包含：

- `pluginId`；
- `enabled`；
- `scope`（USER / ORGANIZATION / TENANT）；
- `autoInstall`；
- `requiresApproval`；
- `isMandatory`；
- usage quotas；
- tenant-specific configuration / preferences；
- approval/compliance metadata。

### 4.2 INFERENCE

它将“插件是什么”和“某租户是否启用/如何配置”分离，这是正确的问题拆分；但实现大量依赖运行时 entity、JSON configuration 与 load-time registry，不能直接替代 business-platform 已接受的：

```text
source declaration
→ pure validation / compile
→ stable manifest + digest
→ deterministic dry-plan
→ controlled lifecycle
```

### 4.3 PROJECT IMPACT：Adapt

保留 business-platform 的 package compiler 和 deterministic lifecycle，不改为 Gauzy 风格的纯 runtime registry。

可以吸收的模型是：

```text
Compiled Module / Plugin Definition        Tenant Runtime Binding
----------------------------------         ----------------------
stable id                                 enabled / disabled
version                                   tenant / organization scope
capabilities                              grants / policy profile
contributions                             runtime config references
contract digest                           quotas / operational state
```

其中 tenant runtime state 不得修改 compiled ownership、contract digest 或业务数据所有权。

## 5. AI Chat 与 Provider 插件体系

### 5.1 FACT：AI Chat 是独立插件

`packages/plugins/ai-chat/src/lib/ai-chat.plugin.ts` 将 AI Chat 定义为后台引擎，提供：

- streaming chat endpoint；
- 当前 tenant 的 provider/model configuration；
- per-tenant BYOK credentials；
- AI Provider 由独立 `@gauzy/plugin-ai-provider-*` 插件贡献。

### 5.2 FACT：Provider Registry

`provider-registry.ts` 使用进程级静态 `Map<string, IAiChatProviderDefinition>`：

- Provider plugin 在 bootstrap 时 `register`；
- teardown 时 `unregister`；
- Chat engine 在请求时读取 registry；
- 相同 provider id 会被替换并记录 warning。

### 5.3 FACT：租户级 BYOK

`ai-provider-credential.entity.ts` 将 AI Provider 的租户绑定持久化，观察到：

- 一行对应 tenant/provider；
- `apiKey` 加密存储并从序列化响应排除；
- 支持 custom `baseUrl`；
- `enabled`；
- tenant 默认 chat provider；
- provider 默认 model；
- voice provider/default speech model。

### 5.4 INFERENCE

Gauzy 实际形成了两层模型：

```text
Provider Definition
    ↓ runtime registry
Tenant Provider Credential / Preference
    ↓
Chat request-time selection
```

这是 business-platform `Model Gateway` Baseline 中 Model Registry、routing policy、quota/budget、usage ledger 的一个成熟产品化补充。

### 5.5 PROJECT IMPACT：Provider 三分模型

后续实现 Model Gateway 时，应明确区分：

```text
ProviderDefinition
- provider_id
- supported capabilities
- model discovery contract
- health adapter type
- compatibility/version

TenantProviderBinding
- tenant_id
- provider_id
- enabled
- custom endpoint policy
- default model / routing preference
- quota / budget policy references

SecretRef
- credential reference only
- secret material stays in approved secret store / encrypted credential adapter
```

Gauzy 的字段组合值得参考，但不应把供应商 SDK、明文 secret 或 tenant mutable config 放入 Platform Core。

## 6. Agent Tool Registry：最值得吸收的机制

### 6.1 FACT：per-turn context snapshot

`tools/tool-registry.ts` 为每个 chat turn 构造请求用户上下文，包含：

- tenantId；
- organizationId；
- userId；
- employeeId；
- caller Authorization header；
- language；
- UI data-part writer。

源码明确要求工具以请求用户身份执行，不使用 elevated service credential。

### 6.2 FACT：工具贡献与审批

每个 tool factory 返回：

```text
tools: name → Tool
requireApproval?: tool names
```

源码约束期望贡献工具默认 READ-ONLY；如果贡献 mutating tool，必须列入 `requireApproval`。

### 6.3 FACT：运行时解析和故障隔离

`resolveAll` 每个 turn 解析全部 contribution：

- 每个 factory 独立 try/catch；
- 一个坏 contribution 不拖垮其他 contribution；
- tool name collision 时 earlier registration wins，重复项被丢弃并 warning；
- 没有 contribution 时得到空集合。

### 6.4 FACT：Caller 权限继续生效

`ai-chat.controller.ts` 使用 `TenantPermissionGuard`、`PermissionGuard` 和 `AI_CHAT_ACCESS`；其注释明确 Agent API tools 使用 caller 自己的 JWT，因此 Agent 能做的事情受调用用户权限约束。

### 6.5 INFERENCE

Gauzy 已经把 Agent Tool 分成：

```text
static contribution registration
        +
per-turn requesting-user context
        +
runtime availability/permission gating
        +
approval marking
```

这个分解非常适合 business-platform，但 Gauzy 的 load-order collision 语义不适合我们的 deterministic compiler。

### 6.6 PROJECT IMPACT：编译期 Catalog + 运行时 Resolution

建议把现有 `Agent Contribution` Baseline 进一步具体化为两阶段：

```text
Compile / Package Time
  stable contribution id
  stable tool id
  input/output schema
  target Application Query/Command
  risk class
  capability requirements
  confirmation policy
  contract/version digest
        ↓
Compiled Agent Tool Catalog
        ↓
Turn / Invocation Time
  tenant enabled state
  delegated principal
  user authorization
  task Capability Grant
  resource classification
  provider/model availability
  feature/policy gates
        ↓
Resolved Tool Set
```

冲突必须由 compiler 或 catalog loader **fail closed / deterministic conflict**，不能用插件加载顺序决定赢家。

## 7. MCP：多客户端业务能力入口

### 7.1 FACT

`packages/mcp-server/README.md` 将 `@gauzy/mcp-server` 定义为 Gauzy API 的共享 MCP implementation，供：

- standalone MCP app；
- Electron MCP server app；
- 外部 AI assistant。

README 描述其工具通过 `ApiClient` 与 Gauzy API 交互，并声明 authentication/token/session/scope 管理。README 同时列出 22 类业务工具并称共有 323 个工具，但该计数旁有 TODO，注明计数最后更新于 2025-08-05，因此不能把“323”视为当前精确源码统计。

### 7.2 INFERENCE

正确的可迁移思想是：

```text
Business Capability
      ↓
Published Application API / Query / Command
      ↓
Shared typed tool contract
   ┌──────┬──────┬──────────┐
   │Chat  │ MCP  │ CLI/API  │
   └──────┴──────┴──────────┘
```

而不是为 Chat、MCP、CLI 各维护一套业务逻辑。

### 7.3 PROJECT IMPACT：Adopt concept, not implementation

business-platform 已有 REST / CLI / MCP 契约方向。后续应增加一个明确门禁：

> 同一业务 capability 的 Embedded Assistant Tool、MCP Tool 和 CLI Command 必须引用同一 Published Application Contract；适配器只处理 transport/schema/context，不重新实现业务规则。

## 8. MCP OAuth 与 ADR-0024 的明确分歧

### 8.1 FACT

`packages/auth/src/lib/mcp/index.ts` 明确说明 Gauzy 自己实现了完整 MCP OAuth 2.0 Authorization Server，包括：

- authorization code + PKCE；
- client credentials；
- refresh token；
- JWT access token；
- user authentication / consent。

### 8.2 PROJECT IMPACT：Reject

这一部分与本项目 Accepted ADR-0024 直接冲突。

business-platform 已决定：

```text
Authentication / credential issuance → external OIDC IdP
Authorization / tenant / policy       → business-platform
```

因此未来 MCP remote access 应当把 business-platform 作为受保护的 Resource Server / Relying Party，消费企业 IdP 或私有化交付中独立 IdP 的标准 token；不得因为 MCP 需要 OAuth 就在平台内部重新建立 credential issuer。

可吸收的是 OAuth resource metadata、scope/capability 映射、PKCE client compatibility 和 token validation 的产品经验，不吸收 Authorization Server ownership。

## 9. Documents / Attachment / Event 解耦

### 9.1 FACT

`ai-chat.controller.ts` 的 attachment 说明：AI Chat 保存 attachment 后发布 `AiChatAttachmentSavedEvent`；安装了 Docs Plugin 时由其订阅并转换为 Document，未安装时没有 subscriber。

### 9.2 INFERENCE

这是“可选能力通过事件扩展宿主、宿主不直接依赖扩展”的实际例子。

### 9.3 PROJECT IMPACT：Adapt

适合映射为本项目：

```text
Assistant / Workspace
  emits published Integration Event
        ↓
Document capability (optional consumer)
  creates/links owned resource through its Application Use Case
```

必须继续遵守本项目的 owner、Outbox/Inbox、versioned event、ResourceRef 与 idempotency 标准，不复制 Gauzy 的 entity coupling。

## 10. 多租户数据模型

### 10.1 FACT

Gauzy 广泛使用 `TenantOrganizationBaseEntity`；AI Provider Credential、Document、Plugin Tenant 等实体都带 tenant/organization 作用域及组合索引。

### 10.2 INFERENCE

Tenant + Organization 被作为横切数据隔离维度贯彻到业务和插件状态中，这对企业产品是必要的。

### 10.3 PROJECT IMPACT

business-platform 已有更严格的 tenant/principal/policy 边界，因此不需要改数据所有权模型。参考价值主要是检查：

- Provider/Plugin/Assistant 等“平台配置数据”是否也完整租户化；
- unique key 是否包含正确 tenant scope；
- runtime registry 是否错误共享 tenant-specific state。

## 11. Analytics / Semantic Layer

### 11.1 FACT

固定提交 README 的部署拓扑明确包含 Cube，并将其描述为 Reports / Dashboards / Analytics 使用的 Semantic Layer。

### 11.2 证据限制

本轮没有在固定提交中取得足够源码证据证明 Cube 的 semantic model 是否是 Gauzy 全平台唯一指标权威、如何与业务权限/tenant filter 编译，也没有证据证明其所有 Dashboard 都必须经过 Cube。

因此只能将“部署使用 Cube semantic layer”记为 FACT，不能扩展为更强结论。

### 11.3 PROJECT IMPACT

不修改 ADR-0017/ADR-0020：business-platform 继续保持唯一 Semantic Contract，Cube/Wren/其他 BI engine 只能作为可替换 execution adapter 或 downstream consumer，不成为第二指标权威。

## 12. 架构映射

| Ever Gauzy | business-platform | 处理 |
|---|---|---|
| Business Management modules | Business Module / Bounded Context | 参考产品覆盖，不复制模型 |
| Headless APIs | Published Application API | Adopt principle |
| Plugin bootstrap | Module/Plugin composition root | Adapt |
| PluginTenant | Tenant runtime binding / grant/config | Adapt |
| static Provider Registry | Model Gateway ProviderDefinition registry | Adapt，改为 deterministic catalog |
| per-tenant BYOK | TenantProviderBinding + SecretRef | Adopt model split，按本项目 Secret Policy 实现 |
| AI Chat | Enterprise AI Workspace / Assistant UI | Adopt product pattern |
| per-turn Tool Context | Delegated Principal + Capability Grant | Adopt concept |
| `requireApproval` | risk class + confirmation policy / ActionPlan | Adapt；R2/R3 继续使用 Prepare→Preview→Confirm→Execute |
| Tool Registry | compiled Agent Tool Catalog + runtime Resolution | Adapt |
| MCP shared package | Rust Agent Adapter / MCP surface | Adopt concept |
| internal MCP OAuth AS | external IdP + platform Resource Server | Reject |
| chat attachment event → docs | versioned Integration Event + owner Application Use Case | Adapt |
| Cube semantic layer | Semantic execution adapter | Defer/adapter only |

## 13. Adopt / Adapt / Reject / Defer

### Adopt

- Headless Business Platform + multiple product shells；
- Provider definition 与 tenant provider binding 分离；
- Embedded Chat 与外部 Agent 共用业务能力；
- per-turn requesting-user context；
- tool contribution failure isolation；
- mutating tool 必须显式声明 approval/risk；
- optional feature 通过 published event 解耦。

### Adapt

- process-wide Provider Registry → deterministic Provider Catalog；
- runtime Tool Registry → compile-time Agent Tool Catalog + runtime gated resolution；
- PluginTenant → package identity 不变的 tenant runtime binding；
- encrypted BYOK column → `SecretRef`/approved secret adapter 优先，持久化密文只是实现选项；
- tool name collision earlier-wins → stable ID collision fail closed；
- Chat attachment event → Outbox/Inbox/versioned integration event。

### Reject

- 在 business-platform 内实现 OAuth/OIDC credential issuer；
- Agent/MCP 直接访问数据库或 private repository；
- 以 load order 决定 Provider/Tool ownership；
- mutable JSON plugin config 改变编译期 ownership/contract；
- 将 Gauzy AGPL 代码、Schema 或 UI 复制进本仓库。

### Defer

- 完整 Plugin Marketplace；
- 动态第三方 native/Node runtime；
- Gauzy Desktop/Timer 产品形态；
- Cube 作为实际 Analytics execution engine；
- 通用 HR/ATS/Inventory 业务模块，除非业务路线明确需要。

## 14. 对当前 Baseline 的调整建议

### 14.1 不需要重写的部分

以下现有设计被 Gauzy 的实践进一步验证，应保持：

- Platform Core 与 Business Module 分离；
- Agent 是客户端/入口，不是业务权威；
- Tool 调公开 Application API，不调 private persistence；
- Agent 权限不得超过 caller；
- 多租户/权限必须在 Tool execution 前成立；
- Model Provider 可插拔；
- Assistant/Workspace 可选，不影响 Business Platform 独立运行；
- Analytics/Semantic 与正式业务写状态分离。

### 14.2 值得后续 ADR/Plan 明确的三个接口

#### A. Provider Catalog / Tenant Binding

需要在 Model Gateway 实现前明确 ProviderDefinition、TenantProviderBinding、SecretRef、Model Catalogue、routing/health/budget 的 ownership 和生命周期。

#### B. Agent Tool Catalog / Runtime Resolution

需要在 Agent Contribution runtime 实现前明确：compile-time tool identity/schema/risk/capability 与 per-turn tenant/principal/grant/policy resolution 的分层，以及 deterministic collision 规则。

#### C. Multi-surface Business Tool Adapter

需要在 MCP/Assistant/CLI 扩展前明确一个 Business Capability 不得产生多套业务实现；不同 surface 只做 protocol adapter。

这三个接口可以形成独立小 ADR 或同一 Agent/Model Gateway 实现计划中的 acceptance contract；本 reference 不提前作最终类型/API 设计。

## 15. 许可证边界

根 `LICENSE` 为 GNU AGPL v3。

本项目默认处理：

- 允许研究公开源码的架构思想、产品机制和失败模式；
- 不复制 Gauzy 源码、Schema、UI 资源或明显表达性实现；
- 不把 Gauzy package 直接引入 production runtime；
- 若未来任何组件级复用需求出现，必须单独进行许可证和部署方式审查。

## 16. 最终判断

Ever Gauzy 应提升为 business-platform 的 **P0 Business Platform Reference**。

它最有价值的不是告诉我们“应该做 ERP”，而是展示一个真实大型业务平台如何逐步获得：

```text
Business Suite
+ Headless APIs
+ Tenant-aware Plugin Runtime
+ Multi-provider AI
+ Embedded Agent
+ MCP external access
```

business-platform 的差异化仍应保持：

```text
authoritative DDD business ownership
+ deterministic package/compiler contracts
+ durable execution/recovery
+ document revision/evidence/lineage
+ strict ActionPlan / capability security
+ externalized authentication
```

因此正确策略是 **参考 Gauzy 的产品化与运行时经验，保留 business-platform 更严格的权威边界、确定性和可审计性**。