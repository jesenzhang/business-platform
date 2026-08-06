# Cloudflare OS 参考项目分析

> 文档类型：Reference Analysis  
> 状态：Current  
> 检查日期：2026-08-06  
> 检查对象：`https://blog.cloudflare.com/cloudflare-os/`、`https://github.com/cloudflare/cloudflare-os`  
> 仓库检查提交：`aedcda8b3066ff666f57ae28ecef7341d6c2dee7`  
> 许可证：Apache-2.0  
> 结论用途：Enterprise AI Workspace 架构输入，不作为直接依赖决策

## 1. 结论

Cloudflare OS 与本项目属于同一上层方向，但不是同一种系统主体。

```text
Cloudflare OS
= 企业 AI Workspace
+ 通用 Agent
+ 沙箱化个人应用
+ 外部系统 Capability Gateway

Business Platform
= 企业权威业务平台
+ 领域规则和事务
+ 持久化业务流程
+ 审计、完整性与受控修复
+ 可选智能入口
```

Cloudflare OS 已经把本项目原总体架构中较薄的“Agent 智能入口”扩展成完整产品层，覆盖 Workspace、Agent、企业 Context、Skill、Gadget、Blueprint、共享协作和 Capability-based security。它在用户体验、Agent 原生安全和动态应用生成方面具有很高参考价值。

它不提供合同、客户、审批、财务等权威业务模型，也不替代 PostgreSQL 业务状态、Application Service、事务、版本、Outbox、Durable Processing、Runtime Audit、Integrity Finding 或 Controlled Repair。因此不应采用“以 Cloudflare OS 替换 Business Platform”的路线。

正式决策是：不直接依赖或部署 Cloudflare OS；吸收其能力模型，在现有 Rust 业务内核上建设自主可控的 Enterprise AI Workspace。

## 2. Cloudflare OS 的产品定位

Cloudflare OS 自称 AI productivity environment，不是传统操作系统。其核心目标是让企业员工在公司上下文和安全边界中使用 Agent 完成工作，并让 Agent 生成、运行和共享小型应用。

主要产品能力包括：

1. 通用 Agent Chat，预加载公司 Context 和 Skills；
2. Gadget：由 Agent 生成、每个用户或团队独立运行的小型应用；
3. Blueprint：可复用的应用代码模板；
4. Gatekeeper：面向外部服务的受控 Capability Gateway；
5. Workspace：承载对话、资源、应用、共享和协作；
6. 实时协作与共享权限；
7. 多模型接入和费用治理；
8. 基于 Workers、Durable Objects、Dynamic Workers、Facets 和 Cap’n Web 的运行时。

开源仓库为 TypeScript Monorepo。README 明确说明当前开源的是基于内部经验重写的 v2，仍处于 early access；独立服务器上的完整 `workerd` 自托管文档和工具尚未成熟。

## 3. 核心架构抽象

### 3.1 Workspace

Workspace 是用户与 Agent、资源、Gadget 和协作者交互的容器。它不仅保存聊天记录，还保存可执行应用、绑定资源和共享关系。

参考价值：本项目当前缺少统一的 AI 工作空间模型，业务网站内置助手、独立 AI 门户和未来桌面入口需要共享同一套 Workspace、Session、Artifact 和权限语义。

### 3.2 Gadget

Gadget 是 Agent 生成的小型全栈应用。每个 Gadget 拥有独立的服务端运行实例、前端和持久化状态。服务端默认无任意网络访问，前端运行在受限 iframe 中。

参考价值：企业员工不只需要聊天，还需要持续存在、可交互、可共享的报表、Dashboard、表单和小工具。

限制：Gadget 状态不能被视为合同、审批、客户、付款等权威业务状态。本项目若实现 Generated App，其持久化仅拥有 UI、临时分析、草稿、布局和用户偏好；正式业务写入必须调用 Business Platform Application Use Case。

### 3.3 Blueprint

Blueprint 是 Gadget 的代码模板。分享 Blueprint 等于分享应用定义，而不是分享原始业务数据、凭证或现有运行实例。

参考价值：本项目可以先实现受控 Artifact Blueprint，例如合同到期分析、项目周报、审批积压 Dashboard、文档复核表单，再逐步演进到沙箱化 Generated App。

### 3.4 Gatekeeper

Gatekeeper 是 Cloudflare OS 最重要的安全抽象。它包装外部服务 API、处理 OAuth、限制资源和操作、记录访问，并对有副作用的操作实施人工确认。

它与普通 MCP Server 的关键差异是：

- Agent 默认没有环境级广泛权限；
- 用户将具体资源引入当前 Agent 或 Gadget；
- 权限是任务和资源范围内的 Capability，不是长期环境权限；
- 凭证不暴露给 Agent；
- Gadget 和 Agent 只能通过受控 Binding 访问外部资源。

本项目对应实现应命名为 Capability Gateway，而不是提供通用 HTTP、SQL、Shell 或数据库工具。

### 3.5 Observation 与 Observer

Cloudflare OS 不只记录“执行了什么”，还追踪 Gadget 通过 Gatekeeper 读取过什么。共享 Gadget 时，系统要求查看者使用自己的连接账户重新证明有权访问 Gadget 历史上读取过的资源；后续读取若会向无权限协作者泄漏数据，则被阻止。

参考价值：这是比普通 Tool Audit 更强的数据血缘与派生产物授权模型。本项目已有 Runtime Audit，可以在其上增加：

```text
Agent Observation
→ Artifact Source
→ Derived Access Requirement
→ Share-time Reauthorization
```

### 3.6 Agent-friendly API

Cloudflare OS 要求 Gadget 的客户端与服务端通过 Cap’n Web RPC 通信，因此应用天然拥有结构化 API，Agent 可以直接调用。

参考价值：本项目的业务能力已经要求 UI、OpenAPI、Worker 和 Agent 复用 Application Use Case。后续需要补充稳定的 Tool Schema、Skill Registry 和 Agent Read DTO，而不是让 Agent理解数据库模型或前端页面协议。

## 4. 与本项目的相同点

| 方向 | Cloudflare OS | Business Platform |
|---|---|---|
| 企业内部 AI | AI productivity environment | 企业 AI 业务平台与智能助手 |
| Agent 连接业务系统 | Gatekeeper | Agent Adapter / Integration Gateway |
| 默认拒绝 | Capability introduction | RBAC + ABAC + fail-closed |
| 人工确认 | Gatekeeper approval | ActionPlan / Confirmation |
| 企业 Context / Skill | 内建 Context 和 Skills | 原架构有 Skill 概念，尚未落地 Registry |
| 审计 | Gatekeeper/Workspace actions | Runtime Audit、Repair Ledger |
| 多模型 | AI Gateway / provider selection | AI Provider 抽象，尚缺统一 Model Gateway |
| 私有化意图 | Workers 或未来 workerd | Rust/PostgreSQL/MinIO 自主部署 |

## 5. 与本项目的关键差异

### 5.1 系统主体不同

Cloudflare OS 的主体是 AI Workspace；本项目主体是权威业务平台。我们的原则仍保持：

```text
业务平台可以没有 Agent；Agent 不能没有业务平台。
```

### 5.2 数据所有权不同

Cloudflare OS 的 Workspace/Gadget 拥有会话、应用和协作状态；本项目业务上下文拥有合同、客户、审批、项目、财务和文档正式状态。

### 5.3 可靠性重点不同

本项目已经实现的固定文档处理 Pipeline、Lease、Fence、Crash Recovery、Runtime Audit、Integrity Finding、Controlled Repair 和 Repair Ledger，面向关键业务状态和可恢复执行。Cloudflare OS 的公开重点是 Agent 生产力、沙箱和协作，而非复杂业务事务和修复治理。

### 5.4 部署依赖不同

Cloudflare OS 深度使用 Workers Runtime 原语。直接采用会引入新的运行时、运维和供应商依赖。本项目应保留 Rust 服务、PostgreSQL、MinIO/S3 和现有 Worker 体系作为权威基础。

## 6. 当前项目现状审查

截至主干 `f5870e58ee5b371e80ce125bbac0a8e16288b208`：

### 已具备

- Rust Workspace 和领域/能力 crate 边界；
- PostgreSQL 生产权威与 SQLite 本地适配；
- 文档元数据、对象存储和固定文档处理切片；
- Processing Job、Step、Lease、Fence、Heartbeat 和 Crash Recovery；
- AI 任务与候选结果基础；
- Runtime Audit、Integrity Finding、Controlled Repair 和 Repair Ledger；
- 真实 PostgreSQL/MinIO E2E 与 Architecture Fitness CI；
- Agent 不得绕过 Application Service 的正式架构规则。

### 尚未具备

- `agent-integration` 仍为 TODO 骨架；
- `agent-adapter` 仅完成配置和 tracing 启动；
- 没有 Workspace、Conversation、Thread、Turn 的产品模型；
- 没有 Skill Registry、Context Registry 和 Tool Registry；
- 没有任务级 Capability Grant；
- 没有 Agent Observation 和派生产物访问继承；
- 没有 Artifact/Blueprint 平台；
- 没有业务网站内置 Assistant UI；
- 没有统一 Model Gateway、预算、配额和成本归因；
- 没有 Generated App Sandbox。

判定：当前底层业务可靠性基础明显强于一般 Agent Demo，但 AI Workspace 产品层几乎尚未开始。

## 7. 采用矩阵

### 7.1 直接吸收概念

- Workspace；
- Skill Registry；
- Context Registry；
- Capability-based resource introduction；
- Observation lineage；
- Artifact 与 Blueprint；
- Agent-friendly typed API；
- 用户与 Agent 分离但可追责的委托身份。

### 7.2 改造后采用

- Gatekeeper → Rust Capability Gateway；
- Gadget → 受控 Artifact，后续 Generated App；
- Blueprint → Artifact/App Blueprint Registry；
- Cloudflare approval → 与服务端 ActionPlan 对接；
- AI Gateway → 自有 Model Gateway；
- Durable Object state → PostgreSQL、对象存储和 Durable Task Execution。

### 7.3 不采用

- Cloudflare OS 作为业务权威系统；
- Gadget SQLite 保存正式业务状态；
- Agent 直接获得数据库、SQL、Shell、任意 HTTP；
- Cloudflare Runtime 作为核心业务平台必须依赖；
- 生成代码与 `business-api` 同进程运行；
- 仅凭前端页面上下文决定租户和授权；
- 用通用 Agent 审批替代业务 ActionPlan、版本和事务检查。

### 7.4 延后

- 任意全栈应用生成；
- 多人实时编辑；
- 通用 MCP Portal；
- 任意第三方连接器市场；
- microVM 或多运行时沙箱；
- 自动模拟全部副作用并延迟批量审批。

## 8. 目标实现映射

| 目标能力 | 目标模块/部署单元 | 权威状态 |
|---|---|---|
| Workspace/Session | `ai-workspace` | PostgreSQL |
| Skill Registry | `agent-integration` | PostgreSQL + 版本化定义 |
| Context Registry | `agent-integration` / `knowledge-context` | PostgreSQL + ArtifactRef |
| Capability Grant | `policy` / `agent-integration` | PostgreSQL，短期可撤销 |
| Tool Gateway | `agent-adapter` | 无业务数据所有权 |
| Observation | `audit` / `agent-integration` | Runtime Audit 扩展 |
| Artifact | `artifact` + object-storage | PostgreSQL 元数据 + MinIO 内容 |
| Blueprint | `artifact` / `app-blueprint` | 版本化定义 |
| Model Gateway | `ai-application` | 路由、预算、Usage Ledger |
| Generated App | 独立 `sandbox-runtime` | 非权威应用状态 |
| 正式业务写入 | 目标业务 Application Service | 目标 Bounded Context |

## 9. 实施优先级

```text
P0  Enterprise AI Workspace 基础
    Workspace / Session / Skill / Context / Tool Registry

P0  Agent-native 安全
    Delegated Principal / Capability Grant / Tool Policy / Audit

P1  业务网站内置助手
    当前页面资源引用 / Assistant API / SSE / 业务卡片

P1  受控写入
    ActionPlan / Confirmation / Execution / Version Conflict

P2  Artifact 与 Blueprint
    Report / Dashboard / Form / Spreadsheet / Sharing

P3  Generated App Sandbox
    无网络默认 / Capability Binding / App Version / Isolation
```

PLAN-0006 只覆盖 P0 的最小基础和一个只读业务垂直切片，不实现 Gadget 等价能力。

## 10. 风险

1. 将 AI Workspace 误建成新的业务系统，造成双重业务事实；
2. 将 Skill 写成重复业务规则；
3. 只做 RBAC，不实现任务级资源 Capability；
4. 把完整敏感业务数据无条件写入 Prompt、日志或 Observation；
5. 在产品需求驱动下过早实现任意代码沙箱；
6. 同时在 Business Platform 和 jarvis-rs 重复实现 Agent Runtime；
7. 过早绑定 workerd、WASI 或特定模型 Gateway；
8. Artifact 分享没有继承来源数据的访问约束。

## 11. 后续维护

Cloudflare OS 处于快速开发阶段。本参考分析应在以下事件后重新审查：

- 其正式自托管方案发布；
- Gatekeeper/Observer 安全模型发生重大变化；
- Generated App 沙箱接口稳定；
- 本项目启动 Generated App 阶段；
- 本项目决定采用或排除 `workerd`。
