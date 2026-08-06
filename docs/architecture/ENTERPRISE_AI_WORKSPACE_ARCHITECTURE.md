# Enterprise AI Workspace 与 Agent Capability 架构

> 文档 ID：ARCH-AIWS-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-08-06  
> 适用范围：业务网站内置助手、企业 AI 门户、Agent Runtime、Skill、Context、Artifact、Capability、Model Gateway 和未来 Generated App

## 1. 目的

本文扩展现有企业 AI 业务平台总体架构，定义业务平台之上的 Enterprise AI Workspace 产品层和 Agent-native 安全边界。

本文解决以下问题：

- 业务网站如何内置智能助手；
- Agent Session、Skill、Context 和 Tool 由谁拥有；
- Agent 如何在不获得环境级广泛权限的前提下访问业务数据；
- Agent 读取数据后生成的 Artifact 如何继承访问要求；
- 写操作如何继续使用业务 Application Service 和 ActionPlan；
- 报告、Dashboard、Blueprint 和未来 Generated App 如何与权威业务状态分离；
- Business Platform、jarvis-rs 或其他 Agent Runtime 如何分工；
- 如何吸收 Cloudflare OS 的参考价值而不绑定其运行时。

本文由 `ADR-0017-enterprise-ai-workspace-and-capability-security.md` 接受。

## 2. 不变原则

### 2.1 业务平台仍是系统主体

```text
关闭 Enterprise AI Workspace 和 Agent Runtime
    ↓
Web、OpenAPI、Worker、文档处理、审批和正式业务仍正常运行
```

Workspace 不得成为业务逻辑唯一入口。

### 2.2 一个业务规则只实现一次

```text
Web / OpenAPI / Worker / Agent Tool
              ↓
       Application Use Case
```

Skill、Prompt、Tool Adapter、Artifact 或 Generated App 不得复制目标业务上下文的正式规则。

### 2.3 Agent 权限不超过用户，也不等于用户全部权限

Agent 使用委托身份，但每个任务只获得完成该任务所需的 Capability。

```text
User Authority
    ⊇ Delegated Agent Authority
        ⊇ Task Capability
```

### 2.4 AI 和生成代码均不可信

模型输出、工具参数、文档内容、检索结果、生成代码和外部系统响应都必须经过 Schema、Policy、业务规则和资源版本校验。

### 2.5 Artifact 不是业务事实

报告、Dashboard、Spreadsheet、Form、Presentation、Gadget-like App 只能拥有产物状态，不拥有合同、客户、审批、财务或文档正式状态。

## 3. 总体架构

```text
┌────────────────────── 访问渠道 ──────────────────────────┐
│ Web Business UI │ Mobile │ AI Portal │ Desktop Assistant │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────── Enterprise AI Workspace ─────────────────┐
│ Workspace / Conversation / Thread / Turn                 │
│ Skill Registry / Context Registry / Tool Registry        │
│ Assistant UI / Artifact / Blueprint / Collaboration      │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────── Agent Runtime & Capability Plane ────────┐
│ Agent Loop / Model Router / Tool Planner                 │
│ Delegated Principal / Capability Grant / Tool Policy     │
│ Observation / Approval / Execution Receipt              │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────── Rust Agent Adapter ──────────────────────┐
│ Typed Business Tools / MCP or REST / Schema Validation   │
│ Identity Propagation / Audit Intent / Rate and Risk Gate │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────── Rust Business Platform ──────────────────┐
│ Identity / Policy / Contract / Document / Approval       │
│ Application Service / Transaction / Version / Outbox     │
│ Runtime Audit / Integrity / Controlled Repair            │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌──────────────── Infrastructure ──────────────────────────┐
│ PostgreSQL / MinIO-S3 / Broker / Model Providers / OTel  │
└──────────────────────────────────────────────────────────┘

未来可选：

┌──────────────── Generated App Sandbox ───────────────────┐
│ App Instance / Isolate or WASI / Sandboxed iframe        │
│ Default no-network / no-database / Capability Bindings   │
└──────────────────────────────────────────────────────────┘
```

## 4. 能力边界与数据所有权

Enterprise AI Workspace 是平台能力层，不新增合同、审批等业务 Bounded Context。它内部按能力和数据所有权划分以下边界。

### 4.1 Workspace Management

拥有：

- Workspace；
- WorkspaceMember；
- Conversation；
- Thread；
- Turn；
- WorkspaceResourceReference；
- UI preference。

不拥有：

- 业务资源正文；
- 业务授权决定；
- 模型供应商会话作为权威状态；
- 正式业务操作结果。

### 4.2 Agent Integration

拥有：

- AgentDefinition；
- SkillDefinition/Version；
- ContextDefinition/Version；
- ToolDefinition/Version；
- AgentRun；
- ToolInvocation；
- CapabilityRequest；
- Observation reference。

不拥有：

- 业务数据；
- 业务状态机；
- 目标业务上下文的 ActionPlan 内容决定；
- 供应商 SDK 类型。

### 4.3 Policy 与 Identity

Identity and Access 继续拥有 Principal、Delegation Grant 和认证会话。Policy 继续拥有通用授权机制。

新增的 Capability Grant 是 Policy 与 Agent Integration 的协作模型：

- Policy 拥有授权判定、撤销和有效性；
- Agent Integration 拥有任务对 Capability 的请求和使用引用；
- 业务上下文拥有具体操作在业务状态下是否允许；
- Tool Adapter 只执行校验后的 Grant，不自行扩权。

### 4.4 Artifact Management

建议新增 Artifact 平台能力，拥有：

- Artifact identity；
- Artifact type；
- Artifact version；
- content reference；
- source references；
- derived access requirements；
- share policy；
- rendering metadata；
- Blueprint definition/version。

不拥有：

- 来源业务资源的正式状态；
- 业务操作事务；
- 外部系统凭证；
- 未经确认的业务写入。

### 4.5 Model Gateway

属于 AI Application 平台能力，拥有：

- Model Registry；
- routing policy；
- provider health；
- quota/budget；
- usage ledger；
- cost attribution；
- fallback policy；
- evaluation references。

不拥有 Prompt 中的业务真相，也不决定正式业务操作。

### 4.6 Generated App Runtime

未来独立部署单元，只拥有：

- AppDefinition/Version；
- AppInstance；
- sandbox-local state；
- Capability Binding references；
- runtime logs and health；
- Blueprint reference。

任何正式业务状态必须通过 Business Platform API 读取和写入。

## 5. 核心模型

### 5.1 Workspace

```text
Workspace
├── workspace_id
├── tenant_id
├── owner_principal_id
├── members
├── conversations
├── attached_resource_refs
├── enabled_skill_versions
├── policy_profile
└── lifecycle_state
```

Workspace 必须租户隔离。资源附件只保存最小引用和显示摘要，不默认复制完整业务内容。

### 5.2 Skill Definition

```text
SkillDefinition
├── skill_id / version
├── name / description
├── input_schema / output_schema
├── required_tools
├── required_capability_template
├── risk_class
├── confirmation_policy
├── context_requirements
├── examples / evaluation_set
├── publisher / status
└── compatibility
```

Skill 是 Agent 行为资产，不是业务规则资产。业务规则改变时，Skill 不应绕过或替代 Application Service。

### 5.3 Context Definition

Context 至少区分：

- organization；
- department；
- role/job；
- business module；
- project；
- workspace；
- user private；
- current page/resource。

每个 Context 必须有：

- 来源和版本；
- 权限；
- 有效期；
- 数据分类；
- Token 预算；
- 引用策略；
- 更新/废弃状态。

当前页面 Context 只提供资源引用，例如 `contract:C10086@version17`。租户和权限必须由服务端受信身份重新解析。

### 5.4 Capability Grant

```text
CapabilityGrant
├── grant_id
├── tenant_id
├── principal_id
├── delegated_agent_id
├── workspace_id / agent_run_id
├── resource_scope
├── allowed_actions
├── field_policy
├── constraints
├── issued_at / expires_at
├── revocation_state
├── policy_version
└── audit_reference
```

关键不变量：

1. Grant 不得超过原用户当前权限；
2. Grant 默认短期和可撤销；
3. 资源范围必须显式；
4. 写操作必须绑定风险策略；
5. 工具参数不能扩展 Grant 范围；
6. Grant 过期、撤销、主体变化或策略变化时 fail-closed；
7. 关键业务执行仍重新检查业务状态和资源版本。

### 5.5 Observation

Observation 记录 Agent 或 Generated App 读取了什么资源和哪种分类，而不是无条件保存完整敏感内容。

```text
Observation
├── observation_id
├── tenant_id
├── agent_run_id / app_instance_id
├── tool_invocation_id
├── resource_reference
├── policy_snapshot_reference
├── data_classification
├── bounded_summary_or_hash
├── observed_at
└── artifact_links
```

### 5.6 Derived Access Requirement

Artifact 的访问要求来自自身分享策略与来源数据约束的合取：

```text
Artifact Access
= Artifact Share Policy
∩ Source Resource Access Requirements
∩ Current Viewer Authority
```

分享时必须重新授权。若无法证明查看者有权访问来源数据，默认拒绝或生成经过明确脱敏的新 Artifact 版本。

### 5.7 Tool Invocation

```text
ToolInvocation
├── invocation_id
├── tool_version
├── agent_run_id
├── capability_grant_id
├── normalized_input_hash
├── target_resource_refs
├── risk_class
├── outcome
├── execution_receipt
└── audit_reference
```

Tool 只公开业务级方法，不提供通用 SQL、Shell、文件系统或任意 HTTP。

## 6. 业务工具契约

### 6.1 查询工具

允许在授权通过后直接执行，例如：

- `document.get`；
- `document.get_processing_status`；
- `document.get_candidate_summary`；
- `audit.get_resource_history`；
- 后续 `contract.search/get`；
- 后续 `approval.list_pending/get`。

查询结果使用 Agent Read DTO：

- 字段最小化；
- 稳定版本化；
- 不泄漏数据库 Row；
- 包含资源引用和版本；
- 对敏感字段应用 field policy；
- 大内容使用 ArtifactRef。

### 6.2 写工具

写工具分成 Prepare 和 Execute：

```text
Agent Intent
→ Prepare Use Case
→ ActionPlan
→ Preview
→ Human Confirmation
→ Execute(action_plan_id)
→ Transaction / Audit / Outbox
```

Agent 不得在确认后重新生成执行参数。ActionPlan 继续绑定用户、租户、资源、版本、权限、计划哈希、过期时间和 nonce。

### 6.3 长时任务

Agent 只创建业务命令或查询 Job 状态。任务租约、重试、恢复和取消由 Durable Task Execution 或拥有该固定 Pipeline 的执行能力处理。

Workspace 中的“任务进度”是 Projection，不是另一份执行状态。

## 7. Agent Runtime 边界

本项目不要求 Business Platform 自己实现所有 Agent Loop。可以使用 jarvis-rs 或其他可替换 Runtime。

```text
AI Workspace
→ Agent Runtime
→ Agent Adapter
→ Business Application API
```

Agent Runtime 可以拥有：

- 推理循环；
- 模型消息；
- 上下文选择；
- Tool planning；
- Run checkpoint；
- 运行时重试。

Agent Runtime 不拥有：

- 业务数据库凭证；
- 正式业务规则；
- 业务事务；
- 长期全租户权限；
- 业务审计的唯一记录。

当 jarvis-rs 被采用时，必须通过稳定协议与 Business Platform 连接，不能形成源码级循环依赖。

## 8. Assistant UI

业务网站内置助手建议采用侧边栏或独立 Workspace 页面，共用后端 Workspace API。

UI 应支持：

- 流式 Turn；
- Tool 状态；
- 引用和业务资源卡片；
- Job 进度；
- ActionPlan Preview；
- Confirm/Reject；
- Artifact 预览；
- 跳转到业务页面；
- 权限拒绝和版本冲突的确定性提示。

前端传入的页面信息仅为不可信导航上下文。可信资源和租户必须由后端重新加载。

## 9. Artifact 与 Blueprint

### 9.1 第一阶段 Artifact 类型

- Report；
- Dashboard Definition；
- Table/Spreadsheet Dataset；
- Form Definition；
- Checklist；
- Presentation Definition；
- Export Package。

### 9.2 Blueprint

Blueprint 是版本化产物或应用定义，不携带：

- 业务数据正文；
- Secret；
- Capability Grant；
- 用户会话；
- 现有 Artifact 的私有来源。

从 Blueprint 创建实例时重新绑定当前用户、租户、资源和 Capability。

## 10. Generated App Sandbox

Generated App 不属于 PLAN-0006。未来实现必须满足：

- 独立运行边界；
- 默认无出站网络；
- 默认无宿主文件系统；
- 无业务数据库凭证；
- 无长期 Secret；
- CPU、内存、时间、并发和存储配额；
- 前端 sandbox/CSP；
- 服务端只通过 Capability Bindings；
- 代码和依赖扫描；
- 版本、回滚和删除；
- 运行审计；
- 租户隔离和逃逸测试。

具体采用 workerd、WASI、容器、isolate 或 microVM 必须通过独立 ADR 和基准验证。

## 11. 安全模型

### 11.1 信任链

```text
Authenticated User
→ Delegated Principal
→ Agent Run
→ Capability Grant
→ Typed Tool
→ Application Use Case
→ Domain Rule / Transaction
```

任何环节无法证明身份、租户、Grant、资源版本或策略时，必须拒绝。

### 11.2 Prompt Injection

- System/Skill/Policy 属于控制平面；
- 文档、检索内容、工具结果属于数据平面；
- 数据平面不能修改 Tool 白名单或 Capability；
- 模型提出的新资源访问必须成为 Capability Request，由服务端和用户决定；
- 不允许从文档文本中解析隐藏指令并扩权。

### 11.3 数据最小化

- Tool 返回最小字段；
- Prompt 使用受控摘要和引用；
- 敏感内容不默认进入日志、trace 或 Observation；
- Artifact 分享前重新检查来源访问要求；
- 外部模型 Provider 的数据处理策略必须匹配数据分类。

## 12. 一致性与恢复

- Workspace、Skill、Context、Capability、Observation 和 Artifact 元数据使用 PostgreSQL；
- 大型内容和版本产物使用 MinIO/S3；
- Agent Turn 可通过 Outbox/SSE 发布；
- Agent Run 中断可由 Agent Runtime checkpoint 恢复；
- Business Job 恢复仍由 Durable Processing 负责；
- ActionPlan 执行必须幂等；
- Observation 与 Tool Invocation 至少与执行结果形成可关联证据；
- 跨 Workspace、Artifact、Audit 的更新不引入分布式事务，使用本地事务、Outbox、幂等和 reconciliation。

## 13. 部署演进

### PLAN-0006 初期

```text
business-api
business-worker / ai-worker
agent-adapter
agent-runtime（可替换，可与测试 harness 分离）
workspace UI（业务网站模块）
PostgreSQL / MinIO / OTel
```

### 后续

可能新增：

- `workspace-api`，仅在独立扩缩容或安全边界出现后；
- `model-gateway`；
- `artifact-worker`；
- `sandbox-runtime`。

在客观部署需求出现前，Workspace application/domain 可以作为模块化单体能力存在。

## 14. 可观测性

至少记录：

- workspace/agent run/turn 数量和延迟；
- tool 选择、成功、拒绝和超时；
- Capability 请求、授予、撤销和拒绝；
- ActionPlan 创建、确认、过期和冲突；
- Model routing、Token、费用和 fallback；
- Observation 数量和分类；
- Artifact 创建、分享拒绝和来源重授权；
- Agent Runtime 恢复和失败分类。

指标不得包含无界敏感正文。

## 15. 质量属性

### 安全

跨租户、Grant 扩权、过期/撤销 Grant、Prompt Injection、Tool 参数越界和 Artifact 泄漏必须有自动化测试。

### 可用性

Agent Runtime 不可用时业务平台正常；Workspace 故障不得阻断业务 API；长时业务任务继续恢复。

### 可替换性

Skill、Tool 和 Workspace API 不依赖特定模型或 Agent Runtime SDK。至少一个 Fake Runtime 能完成契约测试。

### 性能

上下文构建和 Tool 调用必须有预算；大内容使用引用；Workspace 列表和 Turn 查询使用 keyset pagination。

### 可审计性

一次 Agent 操作应可关联：

```text
User → Workspace → AgentRun → Turn → ToolInvocation
→ CapabilityGrant → Business Use Case → Audit/Outbox → Artifact
```

## 16. 实施阶段

### Phase A：Workspace Foundation

- Workspace/Conversation/Turn；
- Skill/Context/Tool Registry；
- Delegated Principal 和 Capability Grant；
- 只读 Tool；
- Observation 最小模型；
- Assistant API/SSE；
- 一个 Document Processing 垂直切片。

### Phase B：Controlled Actions

- ActionPlan UI；
- Prepare/Confirm/Execute；
- 写操作 Tool Policy；
- 版本冲突和确认过期；
- 审批和恢复。

### Phase C：Artifact Platform

- Artifact/Version；
- Source lineage；
- Sharing/Reauthorization；
- Report/Dashboard/Form；
- Blueprint。

### Phase D：Model Governance

- Model Registry；
- routing/fallback；
- quota/budget；
- cost attribution；
- evaluation。

### Phase E：Generated App

- Sandbox Runtime；
- Capability Binding；
- App Instance/Version；
- sharing；
- Blueprint；
- isolation evidence。

## 17. 禁止事项

- Workspace 直接写业务表；
- Skill 复制业务状态机；
- Agent 持有数据库或高权限长期凭证；
- Tool 暴露通用 SQL、Shell、文件系统或任意 HTTP；
- Gadget-like state 成为业务权威；
- 仅依赖前端 tenant/resource 参数授权；
- 将完整敏感内容无条件写入 Prompt/Audit/Observation；
- 在 `business-api` 中执行 Agent 生成代码；
- 未通过 ADR 就引入 workerd/WASI/microVM 作为全局运行时；
- 在 PLAN-0006 中提前实现通用 Workflow DAG 或 Generated App。

## 18. 架构适配门禁

后续 Fitness Functions 应增加：

1. `agent-integration` 不依赖业务 infrastructure；
2. Agent Adapter 不直接使用业务数据库连接；
3. Tool 实现只能调用公开 Application API/Port；
4. Capability Grant 必须租户、主体、任务和过期时间完整；
5. Tool 调用不存在通用 SQL/Shell/HTTP；
6. Workspace/Artifact 表不能成为业务上下文写入口；
7. 写 Tool 必须使用 ActionPlan；
8. Observation 不包含禁止的数据类别；
9. Sandbox 代码不得进入核心业务进程；
10. Agent Runtime 可替换契约通过 Fake/Contract Test。

## 19. 与 Cloudflare OS 的关系

Cloudflare OS 是参考项目，不是本架构的运行依赖。

采用其：

- Workspace；
- Gatekeeper 的 Capability 思想；
- Gadget/Blueprint 的产品价值；
- Observer/Observation 的派生数据保护；
- Agent-friendly typed API。

不采用其作为默认基础：

- Cloudflare Workers 云平台绑定；
- Durable Objects 作为业务权威状态；
- Gadget SQLite 保存正式业务事实；
- Dynamic Workers 直接成为当前阶段部署前提。

## 20. 最终原则

> Enterprise AI Workspace 负责让员工高效地使用 AI；Business Platform 负责让结果正确、可控、可恢复和可审计。

> Agent 可以提出意图、读取授权数据和生成产物，但任何正式业务事实只能由拥有该事实的 Application Service 改变。
