# Cloudflare OS 对标与 Enterprise AI Workspace 现状审查

> Review Date: 2026-08-06  
> Reviewed Base: `f5870e58ee5b371e80ce125bbac0a8e16288b208`  
> Scope: architecture documents, `agent-integration`, `agent-adapter`, current plans and Cloudflare OS reference  
> Verdict: **FOUNDATION READY / PRODUCT LAYER NOT STARTED**

## 1. 审查结论

当前项目已经形成可靠的企业业务内核和第一个可恢复的 AI 文档处理垂直切片，但尚不能称为企业 AI Workspace，也尚不能提供 Cloudflare OS 类的内置智能助手产品。

当前状态可以概括为：

```text
业务权威、事务、任务恢复、审计治理：已建立可信基础
Agent 接口边界：架构已定义
Agent Runtime / Workspace / Skill / Context / Artifact：基本未实现
Generated App / Sandbox / Collaboration：未开始
```

Cloudflare OS 不构成对现有业务平台路线的否定。它证明了“企业 AI Workspace”应当成为业务平台之上的独立产品层，也暴露了当前架构中 Agent 入口描述过薄的问题。

## 2. 已验证的当前能力

### 2.1 业务与数据基础

- PostgreSQL 是生产权威状态；
- SQLite 仅作为本地适配；
- Document Management 与 Document Intelligence 的所有权已分离；
- 业务数据、AI 候选和执行状态具有独立语义；
- 对象存储、版本、幂等、Outbox 和审计边界已形成。

### 2.2 Durable Processing

PLAN-0004 已落地固定 Document Processing Pipeline，并验证：

- Processing Job/Step；
- claim、lease、heartbeat 和 fencing；
- PostgreSQL 多 Worker；
- SQLite 单进程；
- 进程崩溃后 reclaim/resume；
- AI 任务和人工复核边界；
- 真实 PostgreSQL/MinIO E2E。

### 2.3 Runtime Governance

PLAN-0005 已落地：

- Unified Runtime Audit；
- Integrity Scan/Finding；
- Controlled Repair；
- Repair Run/Step/Ledger；
- approval、lease、fence、retry 和 verification；
- Hash-chain tamper evidence；
- 真实 CI 与架构门禁。

这些能力可以被未来 Agent Workspace 复用，避免为 Agent 再建一套审计、恢复和修复机制。

## 3. Agent 当前实现事实

### 3.1 `crates/agent-integration`

当前只有模块说明和 `TODO: 阶段二实现`，尚无：

- Workspace；
- Session/Thread/Turn；
- Skill Registry；
- Tool Registry；
- Context Registry；
- Capability Grant；
- ActionPlan 实现；
- Observation；
- Artifact。

### 3.2 `apps/agent-adapter`

当前仅加载配置、初始化 tracing 并退出，注释标记为后续阶段实现。没有 HTTP/MCP 服务、身份委托、工具调用、授权、审计或业务接口。

### 3.3 产品界面

当前没有业务网站内置 Assistant Panel，也没有独立 Workspace UI、流式消息协议、业务卡片和确认界面。

因此，现有 Agent 部分属于正确的架构占位，而不是可运行能力。

## 4. 与 Cloudflare OS 的差距矩阵

| 能力 | 当前项目 | Cloudflare OS 对标 | 差距 |
|---|---|---|---|
| 企业业务权威状态 | 已形成基础 | 非核心 | 我方优势 |
| Durable Business Processing | 已形成固定切片 | 非公开重点 | 我方优势 |
| Runtime Audit/Repair | 已实现基础 | Gatekeeper/Workspace audit | 我方更适合关键业务 |
| Agent Chat | 未实现 | 已产品化 | 大 |
| Workspace | 未实现 | 核心能力 | 大 |
| Skill/Context Registry | 未实现 | 已使用 | 大 |
| Capability-based resource introduction | 未实现 | Gatekeeper 核心 | 大 |
| Observation/derived access | 未实现 | Observer/Observation | 大 |
| Artifact | 未实现 | Gadget/office-like artifact | 大 |
| Blueprint | 未实现 | 核心分享方式 | 大 |
| Generated App Sandbox | 未实现 | Dynamic Worker + iframe | 很大 |
| Collaboration | 未实现 | 实时共享 | 很大 |
| Model routing/cost governance | Provider 基础 | AI Gateway | 中到大 |

## 5. 架构问题

### 5.1 Agent 被建模为入口，但缺少产品层

原架构中 `agent-runtime` 和 `agent-adapter` 主要描述协议和工具调用，没有定义 Workspace、Skill、Context、Artifact、Collaboration 和 Model Governance。

修正：新增 `ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`，明确 Workspace 是独立产品能力层，但不拥有业务权威状态。

### 5.2 授权模型缺少任务级 Capability

现有 RBAC/ABAC 和 Delegated Principal 可以判断用户是否有权访问业务资源，但不能完整表达：

```text
哪个 Agent
在什么任务中
可以访问哪些具体资源
允许哪些操作和字段
何时失效
```

修正：在 Policy/Agent Integration 之间增加可撤销、短期、资源范围内的 Capability Grant。

### 5.3 审计缺少 Observation 血缘

现有 Runtime Audit 能记录动作和结果，但尚未记录 Agent 读取了哪些资源，以及由此生成的 Artifact 应继承什么访问要求。

修正：新增 Observation、ArtifactSource 和 DerivedAccessRequirement；敏感内容本身仍不得无界写入 Audit。

### 5.4 Artifact 和业务事实边界尚未定义

未来生成报告、Dashboard 或应用时，如果没有明确所有权，容易形成第二份业务事实。

修正：Artifact 只拥有产物内容、版本、布局、来源引用和分享策略；正式业务状态始终由目标 Bounded Context 拥有。

### 5.5 生成代码运行边界未定义

任意代码不能进入 `business-api`、Worker 或 Agent Adapter 进程。

修正：Generated App 延后到独立 Sandbox Runtime，默认无网络、数据库、宿主文件系统和长期凭证，只能通过 Capability Binding 调用受控 API。

## 6. 建议决策

1. 接受 Enterprise AI Workspace 为正式产品层；
2. 保持 Rust Business Platform 为唯一业务权威；
3. Cloudflare OS 作为最高优先级参考项目，不作为直接运行依赖；
4. PLAN-0006 只实现 Workspace/Skill/Context/Capability/Tool/Observation 的最小基础；
5. 第一个垂直切片只读，不实现高风险写入和 Generated App；
6. ActionPlan 写入闭环在后续计划中建立；
7. Artifact/Blueprint 和 Sandbox 分阶段实施。

## 7. 实施优先级

```text
PLAN-0006  Workspace Foundation + Read-only Business Assistant
PLAN-0007  Controlled Agent Actions + ActionPlan UI
PLAN-0008  Artifact / Blueprint / Sharing
PLAN-0009  Model Gateway and Evaluation Governance
PLAN-0010  Generated App Sandbox, subject to separate ADR
```

上述编号仅用于路线排序；PLAN-0007 以后不得在 PLAN-0006 完成前自动启动。

## 8. 阻断条件

PLAN-0006 进入实现前必须满足：

- ADR-0017 被接受；
- Workspace、Capability、Observation 和 Artifact 的所有权明确；
- 不将 Cloudflare OS Runtime 作为隐式依赖；
- 第一阶段工具白名单和只读范围明确；
- 安全威胁模型覆盖身份委托、跨租户、Prompt Injection 和数据泄漏；
- 计划包含真实 PostgreSQL、API、恢复、授权和审计证据。

## 9. 最终判定

```text
继续现有业务平台路线：YES
直接采用 Cloudflare OS：NO
吸收其产品与安全模型：YES
立即实现任意 Gadget：NO
下一阶段建设 Enterprise AI Workspace Foundation：YES
```
