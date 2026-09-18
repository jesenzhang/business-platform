# PLAN-0006: Enterprise AI Workspace Foundation

> Status: Proposed / BLOCKED  
> Revision: 1  
> Date: 2026-09-18  
> Owner: Platform Foundation / Agent Integration  
> Rebased From: Revision 0 (2026-08-06)  
> Planning Base: main at b9eadf8f1b2b46c88f40f2defc88ffcb8bdb0b34  
> Activation Gate: PLAN-0013 Integrated + first Contract Business Vertical Slice Integrated  
> Architecture Decisions: ADR-0018, ADR-0021, ADR-0022, ADR-0024

## 1. Goal

在已有 v0.1 Business Platform、真实 OIDC、Document Revision/Evidence、model-provider、Public API/CLI/MCP、Business Application compiler 基础上，交付最小 Enterprise AI Workspace 与只读企业助手。

本计划不再把 Workspace 当成“下一项平台基础设施”。它必须建立在已经被真实业务验证的 Identity/Authorization 与 Contract Published Application Contract 上。

目标运行链：

~~~text
OIDC Authenticated User
  -> Platform User / Tenant Membership / Policy
  -> Workspace / Turn
  -> AgentRun
  -> Compiled Agent Tool Catalog
  -> per-turn Tool Resolution
  -> task-scoped Capability Grant
  -> typed Published Application Query
  -> Observation / Audit / Evidence Reference
  -> durable SSE response
~~~

## 2. Why Revision 1

Revision 0 编写于 2026-08-06，当时以下能力尚不存在或尚未收口：

- PLAN-0007 REST/CLI/MCP/Business Console；
- PLAN-0008 Document Revision/Lifecycle/Evidence；
- PLAN-0010 Business Module Isolation + Semantic Contract；
- PLAN-0011 Business Application Packaging/Contribution compiler；
- PLAN-0012 real OIDC、model-provider、observability、backup/restore、v0.1 release；
- ADR-0024 authentication externalized / authorization internalized。

同时，项目审阅确认：

- identity / organization 尚未形成真实用户和授权系统；
- Contract 等业务 crate 仍主要是骨架；
- 直接做 Workspace 会让 Capability 建立在不完整业务权限之上；
- Agent Tool 若没有真实业务 Published Contract，只会继续扩张抽象。

因此 Revision 1 调整实施顺序，不推翻 ADR-0018。

## 3. Activation gate

PLAN-0006 不得进入 Active，直到同时满足：

1. PLAN-0013 Identity and Authorization Foundation 已 Integrated；
2. 至少一个真实 Contract Business Vertical Slice 已 Integrated；
3. Contract 至少发布稳定的 read-only Application Query contract；
4. Document/Contract 权限能够由统一 Policy 评估；
5. OIDC → PlatformUser → Policy 的 E2E 已有证据；
6. Business Application compiler 可提供 Agent Contribution 的稳定 identity/schema input；
7. 当前 main CI 与安全门禁绿色。

若上述任一项缺失，继续完善业务切片，不以 Workspace 代码替代缺口。

## 4. Existing capabilities to reuse

### 4.1 Authentication

复用现有 production OIDC/JWT validation。平台不实现 credential issuer，不引入内部 OAuth Authorization Server。

### 4.2 Authorization

复用 PLAN-0013 的 PlatformUser、TenantMembership、Role/Permission/ResourceScope 和 Authorize。

Capability Grant 是当前用户 authority 的更窄、短期委托，不是第二套 RBAC。

### 4.3 Model Provider

复用 ai-worker 已有 vendored model-provider seam 与 timeout/retry/error classification。

本计划只增加 Workspace/Agent 所需的 provider-independent usage attribution，不重新设计 Provider SDK。

### 4.4 Document

复用 Document Revision、Processing Job、Candidate、Evidence 和 typed public query contracts。

### 4.5 Business Application compiler

复用 PLAN-0011 的 stable module/package/contribution identity、deterministic compiler/dry-plan。

Agent Tool definition 应由 Business Module 的 Agent Contribution 编译得到，不建立独立手工 registry 作为长期权威。

## 5. Agent Tool model

吸收 Ever Gauzy 参考中“静态贡献 + 每 turn 解析”的产品经验，但保持本项目确定性和 fail-closed 规则。

~~~text
Compile / Package Time
  stable tool id
  contribution id
  module id/version
  input/output schema
  Published Query/Command target
  risk class
  required permissions
  capability template
  confirmation policy
  contract digest
       |
       v
Compiled Agent Tool Catalog
       |
       v
Turn / Invocation Time
  tenant enabled state
  current principal
  current Policy decision
  task Capability Grant
  resource classification
  provider/model availability
  feature gates
       |
       v
Resolved Tool Set
~~~

重复 stable id、ownership collision、unknown target 必须 deterministic fail closed。不得使用“加载顺序决定赢家”。

## 6. Minimum domain/application model

### Workspace Management

~~~text
Workspace
Conversation
Turn
WorkspaceResourceReference
DurableWorkspaceEvent
~~~

### Agent Integration

~~~text
AgentRun
ToolInvocation
Observation
CapabilityRequestReference
CompiledToolReference
~~~

### Policy collaboration

~~~text
CapabilityGrant
- grant_id
- tenant_id
- principal_id
- agent_run_id
- resource_scope
- allowed_actions
- field_policy/reference
- constraints
- issued_at
- expires_at
- revocation state
- policy version/reference
~~~

Policy 拥有授权有效性；Agent Integration 只保存请求和使用引用。

## 7. First read-only tools

首批工具只允许查询：

~~~text
contract.search
contract.get
document.get
document.processing_status.get
~~~

如果 Contract slice 尚未提供某 Query，则 PLAN-0006 不能私自直连 Contract persistence 补齐。

Knowledge tools 如 knowledge.search / knowledge.ask 只有在 Knowledge Provider/Projection 已经形成独立计划和权限契约后才能加入；不是 PLAN-0006 的前置必选项。

## 8. Target user flow

~~~text
1. User opens Contract detail.
2. UI opens/reuses Workspace.
3. Page context contains only typed ContractRef/DocumentRevisionRef.
4. User asks a read-only business question.
5. Workspace commits Turn + durable event.
6. AgentRun resolves current Compiled Tool Catalog.
7. Policy evaluates current user and resource.
8. A short-lived task Capability is issued.
9. Agent Adapter validates schema + capability.
10. Tool calls Published Application Query.
11. Result returns bounded Agent Read DTO + evidence/resource refs.
12. Observation and Runtime Audit references are recorded.
13. Answer streams through durable SSE.
14. Reconnect resumes from committed event cursor.
~~~

## 9. Public contracts

Minimum Workspace endpoints or equivalent versioned contracts:

~~~text
POST /api/v1/ai/workspaces
GET  /api/v1/ai/workspaces/{workspace_id}
POST /api/v1/ai/workspaces/{workspace_id}/turns
GET  /api/v1/ai/workspaces/{workspace_id}/turns
GET  /api/v1/ai/workspaces/{workspace_id}/events
POST /api/v1/ai/runs/{run_id}/cancel
~~~

Rules:

- tenant/user always from trusted authentication + platform identity;
- mutating Workspace commands require idempotency;
- list queries use keyset cursor;
- SSE uses durable sequence;
- resource refs are typed and version-aware;
- error contract stays stable;
- no raw DB/provider types leak.

## 10. Work packages

| ID | Scope | Required evidence |
|---|---|---|
| WP-00 | activation preflight against PLAN-0013 + Contract slice | documented PASS |
| WP-01 | Workspace/Conversation/Turn domain + persistence | domain/adapter tests |
| WP-02 | AgentRun/ToolInvocation/Observation lifecycle | transition tests |
| WP-03 | Agent Contribution → Compiled Tool Catalog integration | deterministic compiler tests |
| WP-04 | per-turn Tool Resolution + Policy integration | authorization matrix |
| WP-05 | task-scoped Capability issue/use/revoke | security tests |
| WP-06 | typed read-only Contract/Document tools | public contract tests |
| WP-07 | replaceable AgentRuntime port + deterministic Fake Runtime | runtime contract tests |
| WP-08 | API + durable SSE/reconnect/recovery | process E2E |
| WP-09 | Audit/Observation redaction and lineage | security/audit tests |
| WP-10 | model usage attribution/reasonable budgets | provider-independent tests |
| WP-11 | Business Console assistant shell / business cards | frontend contract tests |
| WP-12 | metrics/runbook/threat model/completion audit | CI evidence |

## 11. Capability rules

1. Capability <= current user authority。
2. 每次 Tool invocation 重新验证 tenant、expiry、revocation 和 resource scope。
3. Tool arguments 不能扩展 grant。
4. 用户 membership/role 被撤销后，已有长期 Workspace 不得保留旧权限。
5. Tool output 应按 field/data policy 过滤。
6. Agent runtime 不持有数据库凭证。
7. Agent runtime/provider failure 不能改变业务权威状态。
8. read-only Tool 不得产生隐藏业务副作用。

## 12. Observation and evidence

Observation 记录：

- tool/version；
- target ResourceRef；
- policy/capability reference；
- data classification；
- bounded summary/hash；
- evidence refs；
- outcome；
- trace。

默认不保存：

- 全量合同正文；
- object storage key；
- provider secret；
- system prompt；
- raw model transcript；
- unrestricted audit payload。

## 13. Runtime and recovery

必须证明：

- Turn 已提交但 AgentRun 尚未启动时崩溃可恢复；
- ToolInvocation stale completion 不能覆盖新版本；
- Agent runtime interruption 可恢复或 terminally classified；
- SSE reconnect 不丢已提交事件；
- duplicate Turn/idempotency converges；
- Business Platform 在 Agent components 停止时仍正常运行。

## 14. Non-goals

本计划不实现：

- 通用 Workflow DAG Designer；
- 动态 native/WASM/Node/Python plugin runtime；
- Marketplace；
- Generated App Sandbox；
- 任意 Shell/SQL/filesystem/browser/arbitrary HTTP tool；
- 高风险业务写 Tool；
- 完整 Model Gateway routing/budget platform；
- 自建 Knowledge/RAG engine；
- OAuth/OIDC credential issuer；
- 第二套业务权限系统。

## 15. Controlled write path

未来写 Tool 必须独立里程碑实现：

~~~text
Intent
 -> Prepare
 -> server-side ActionPlan
 -> Preview
 -> Human Confirm
 -> Execute(action_plan_id)
 -> Application Use Case
 -> Audit / Outbox
~~~

Agent 在确认后不得重新生成执行参数。

Contract 主体变更、归档编号重生、签署变更合同等场景属于该后续里程碑，不进入本 read-only slice。

## 16. Security tests

至少覆盖：

- forged tenant/resource context；
- missing/revoked/expired capability；
- role revoked after Workspace creation；
- cross-tenant resource ref；
- prompt injection requesting new tool；
- schema argument scope expansion；
- sensitive field filtering；
- tool catalog collision；
- arbitrary tool absence；
- direct DB dependency absence；
- provider/JWKS outage fail closed where relevant。

## 17. Architecture fitness

必须新增/保持：

1. agent-adapter 不依赖业务 persistence adapters；
2. agent-integration core 不依赖 Axum/SQLx/provider SDK；
3. Tool target 必须是 Published Application Contract；
4. duplicate Tool identity fail closed；
5. Agent Contribution 输入顺序不改变 catalog digest；
6. runtime tenant enablement 不改变 compiled definition identity；
7. Tool resolution 权限只能缩小；
8. no generic Shell/SQL/filesystem/arbitrary HTTP；
9. Workspace tables 不被 Contract/Document domain 直接依赖；
10. Generated App dependencies absent。

## 18. Completion definition

PLAN-0006 只有在以下全部完成后成为 Accepted Candidate：

- activation gate 已满足并记录；
- Workspace/Turn/AgentRun/ToolInvocation/Observation 已持久化；
- Compiled Agent Tool Catalog + per-turn resolution 已实现；
- Capability 与统一 Policy 正确协作；
- contract.search/get 与 document tools 只调用 Published Application Query；
- Fake Runtime E2E 通过；
- crash/recovery/SSE reconnect 通过；
- Audit/Observation redaction 通过；
- real PostgreSQL CI 通过；
- Architecture Fitness / OpenAPI / fmt/check/clippy/test/security gates 通过；
- Business Console 可以完成一个 read-only assistant flow；
- 没有写 Tool、Generated App、OAuth server、通用插件 runtime。

到 Accepted Candidate 后停止，等待独立审阅与显式集成。

## 19. Roadmap relationship

当前顺序：

~~~text
v0.1 release
  -> PLAN-0013 Identity/Authorization
  -> Contract Business Vertical Slice
  -> optional Knowledge/Evidence Projection
  -> PLAN-0006 Revision 1 Workspace/Agent
  -> Controlled Write ActionPlan
  -> Analytics / broader cross-functional modules
~~~

该顺序的目的不是延后 Agent，而是确保 Agent 首次出现时就连接真实用户权限和真实业务能力，而不是连接 TODO 领域或临时权限枚举。
