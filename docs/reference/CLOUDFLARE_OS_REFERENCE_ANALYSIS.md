# Cloudflare OS 参考项目分析

> 文档类型：Reference Analysis
> 状态：Current
> 检查日期：2026-08-12
> 检查对象：`cloudflare/cloudflare-os`
> 默认分支：`main`
> 固定提交：`213ea6aa0a0e29d91d72832dcc9871432c1e01c5`
> 许可证边界：仓库代码与仓库内文档为 Apache-2.0；Cloudflare 官方 blog 文章仅作为外部产品说明来源使用，不复制代码
> 结论用途：Enterprise AI Workspace 架构输入，不作为直接依赖决策

## 1. Executive Conclusion

**FACT**: Cloudflare OS 公开定位为 “an AI productivity environment”，其核心由三部分组成：company-context-aware 的 agent workspace、Gatekeepers 安全框架，以及可生成和共享的 gadget/app 平台。其仓库 README 和官方 blog 都明确说明它不是传统操作系统，而是面向组织生产力的 AI 工作空间与应用平台。
来源：
- https://github.com/cloudflare/cloudflare-os
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md
- https://blog.cloudflare.com/cloudflare-os/

**INFERENCE**: Cloudflare OS 更接近 “企业 AI Workspace + 受控应用生成平台 + Capability Gateway”，而不是业务事实系统。它的强项在于工作空间、工具接入、应用沙箱和观察/重授权，不在合同、审批、财务、客户等正式业务状态所有权。
这一判断由 README、`docs/blueprints.md`、`packages/workshop-backend/src/overseer.ts`、`packages/workshop-shared/src/gatekeeper.ts` 以及 Cloudflare 官方 blog 的共同证据支持。
来源：
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/docs/blueprints.md
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-backend/src/overseer.ts
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-shared/src/gatekeeper.ts

**PROJECT DECISION**: 本项目吸收 Cloudflare OS 的 Workspace / Capability / Observation / Blueprint / Generated App 设计，但不直接依赖 Cloudflare OS 作为核心业务平台，也不把 gadget state、workspace state 或 observation state 提升为正式业务事实。正式业务写入仍必须进入拥有该事实的 Business Context Application Use Case。
该决定与本项目现有 ADR/Baseline 保持一致，尤其是 Enterprise AI Workspace 与 Capability Security 的边界定义。
来源：
- `docs/architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`
- `docs/adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md`

## 2. Research Scope

本轮二次深审固定了以下边界：

1. 固定仓库 `cloudflare/cloudflare-os` 的默认分支 `main`；
2. 固定 exact commit `213ea6aa0a0e29d91d72832dcc9871432c1e01c5`；
3. 固定检查日期为 `2026-08-12`；
4. 只使用 Cloudflare 官方仓库源码、官方文档和 Cloudflare 官方 blog；
5. 不复制源码，只做事实总结与架构映射；
6. 将本地已有分析统一成单一权威结论，不生成第二份 V2 冲突文件。

仓库元数据由 GitHub 官方 API 确认：
- 仓库：`cloudflare/cloudflare-os`
- 默认分支：`main`
- 许可证：Apache-2.0
- 仓库主页：https://github.com/cloudflare/cloudflare-os

## 3. Evidence Base

### 3.1 官方产品与架构说明

- Cloudflare 官方 blog：`Cloudflare OS: an open platform for agents, apps, and work`
  - URL: https://blog.cloudflare.com/cloudflare-os/
  - 价值：产品定位、Workspace / Gatekeeper / Gadget / Blueprint / Observation / sandbox 的官方语义。
- 仓库 README
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md
  - 价值：产品定位、workspace/gadget/blueprints/security framework、Workers / Durable Objects / Dynamic Workers / Facets / Cap'n Web、sandbox 和 capability-based access control。
- 蓝图说明
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/docs/blueprints.md
  - 价值：Blueprint 共享边界、`.gadget` 格式、R2/KV 传播、版本化、导入导出、feature/publish 机制。

### 3.2 源码路径证据

- `packages/workshop-backend/src/overseer.ts`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-backend/src/overseer.ts
  - 价值：Workspace/Gadget/Gatekeeper 数据结构、ObservationAuthorizer、ApprovalQueue、addObserver、Blueprint 创建与传播、AgentSpawner gatekeeper、UseGadgetClient 边界。
- `packages/workshop-shared/src/gatekeeper.ts`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-shared/src/gatekeeper.ts
  - 价值：capability-based access control、resource introduction、observation authorization、side-effect approval、hook binding、sandboxed iframe / RPC 语义。
- `packages/workshop-backend/src/blueprint-archive.ts`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-backend/src/blueprint-archive.ts
  - 价值：`.gadget` archive 格式、metadata/content 分离、R2/KV 存储边界、content length / metadata size 上限。
- `packages/workshop-frontend/src/routes/workspaces.tsx`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-frontend/src/routes/workspaces.tsx
  - 价值：workspace UI 的隔离环境定位。
- `packages/workshop-frontend/src/routes/workspace.$id.tsx`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-frontend/src/routes/workspace.%24id.tsx
  - 价值：workspace editor 路由。
- `packages/workshop-frontend/src/routes/gadget.$id.tsx`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-frontend/src/routes/gadget.%24id.tsx
  - 价值：legacy /gadget 路由重定向到 /workspace，说明 workspace 是当前权威产品入口。
- `packages/workshop-frontend/src/routes/gatekeepers.tsx`
  - URL: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-frontend/src/routes/gatekeepers.tsx
  - 价值：Gatekeepers 是单独的产品可视化和管理域，不是隐式环境级工具列表。

## 4. FACT / INFERENCE / PROJECT DECISION

### 4.1 FACT

1. Cloudflare OS 由 workspace、gadget、gatekeeper、blueprint、observations、AI Gateway、Workers Runtime 组成。
   来源：README、blog、`overseer.ts`、`gatekeeper.ts`

2. 其默认安全姿态是 “agents start with no access”，外部资源必须通过显式 introduction / capability binding 引入。
   来源：blog、`gatekeeper.ts`

3. Gatekeeper 持有 credential，并通过 typed API、OAuth、resource scoping、logging 和 approval 介入外部系统。
   来源：blog、README、`gatekeeper.ts`

4. Observation 会跟踪 agent/gadget 实际读取了什么，而不只是调用了什么工具。
   来源：blog、`gatekeeper.ts`

5. Blueprint 只携带代码/模板/元数据，不携带 SQLite 数据、聊天历史或 credentials。
   来源：blog、`docs/blueprints.md`

6. 每个 gadget 是独立实例，运行在 Dynamic Worker / Facet / sandboxed iframe 约束内。
   来源：README、blog

7. Workspace 是用户、session、persistent state、outputs、resource access 和 isolated runtime 的容器。
   来源：blog、`workspaces.tsx`

8. Cloudflare OS 的仓库是 Apache-2.0。
   来源：GitHub repository metadata

### 4.2 INFERENCE

1. Cloudflare OS 的“操作系统”更像企业内部生产力平台的产品隐喻，不应按传统 OS 内核理解。
   依据：README 首段、blog 的 “not a traditional computer operating system” 叙述。

2. Cloudflare OS 的核心创新点不是单个 agent，而是把 workspace、capability、observation 和 generated app 组织成一个完整工作系统。
   依据：README 和 blog 都把三大部分并列，而不是以聊天为中心。

3. 其安全模型本质上是 object-capability / resource introduction + share-time reauthorization，而不是 ambient ACL / ambient API keys。
   依据：blog 的 “start with no access”、Gatekeeper 设计、Observation 重新授权。

4. `.gadget` 与 Blueprint 说明 Cloudflare OS 更接近 “个人可修改应用” 平台，而不是单纯文档工具。
   依据：`docs/blueprints.md` 关于 code/template、share link、import/export、app instance 的描述。

5. 对本项目最有价值的部分不是 Workers 运行时本身，而是 “Workspace + Capability + Observation + non-authoritative app/artifact” 的边界模型。
   依据：本项目已有 Rust/PostgreSQL/MinIO/Durable Task 基础，不需要直接吸收 Cloudflare 运行时。

### 4.3 PROJECT DECISION

1. 本项目采用 Cloudflare OS 的产品抽象，不采用 Cloudflare OS 的业务事实所有权模型。
2. 本项目采用 Capability-based security、Observation lineage、Blueprint/Generated App 的分层思想。
3. 本项目不采用 Cloudflare OS 作为正式业务系统，不把 gadget SQLite、workspace state 或 observation log 作为正式业务事实。
4. 本项目生成应用阶段如落地，必须通过本项目自己的 `sandbox-runtime` / `artifact` / `app-blueprint` / `policy` 边界实现。
5. 本项目不把 `workerd` 作为核心业务平台依赖写入当前边界决策，除非后续 ADR 明确授权。

## 5. Cloudflare OS Concept to Platform Mapping

| Cloudflare concept | Cloudflare meaning | Platform Capability | Business Module | Contribution | Agent / Workspace role | Semantic layer |
|---|---|---|---|---|---|---|
| Workspace | 载体，包含 sessions、persistent state、outputs、resource access、isolated runtime | `ai-workspace`, `conversation/session registry` | `ai-workspace`, `agent-integration`, `policy` | 统一的 AI 工作容器，承载会话、上下文、输出与资源绑定 | Agent 在 workspace 中运行、协作、继续任务 | 只承载工作态与引用，不承载正式业务事实 |
| Gadget | Agent 生成的私有全栈应用，独立实例、独立状态 | `artifact`, `app-blueprint`, `sandbox-runtime` | `artifact`, `app-blueprint`, `sandbox-runtime` | 可交互、可共享、可修改的应用产物 | Agent 可创建、修改、继续执行 app | Artifact / Blueprint / UI 产物，非业务事实 |
| Blueprint | 共享 gadget 代码模板，不含数据、凭证、历史 | `artifact blueprint registry` | `artifact`, `app-blueprint` | 可复用应用模板，支持导出/导入/实例化 | Agent/用户可从模板生成实例 | 模板定义层，不是事实层 |
| Gatekeeper | External service capability gateway，持 credential、scoping、approval、logging | `capability gateway`, `tool policy`, `delegated principal` | `policy`, `agent-integration` | 把外部系统包装成最小权限的类型化能力 | Agent 只能通过受控 binding 访问外部资源 | 授权语义与审计语义，不是业务事实 |
| Capability introduction | 从无权限到具体资源的显式引入 | `capability grant` | `policy`, `agent-integration` | 让资源访问短期、定向、可撤销 | Agent 请求引入，用户批准/拒绝 | 授权元数据、授权记录 |
| Observation | 记录 agent/gadget 实际读过什么，并要求 share-time reauthorization | `runtime audit + observation lineage` | `audit`, `agent-integration` | 数据血缘、二次授权、泄漏阻断 | Workspace 分享必须重新证明访问权 | lineage / provenance / authorization dependency |
| Agent / App boundary | Agent 通过 typed API / Cap'n Web 与 app 交互，不直接拿 DB / shell / internet | `typed app RPC`, `tool schema`, `public read DTO` | `agent-integration`, `artifact`, `policy` | 稳定协议，UI 和 agent 复用同一用例 | Agent 调用 app 方法，而不是读表或操作前端 DOM | Read DTO / public projection / semantic object |
| Generated application | Agent 自动生成、继续编辑、可共享的 app | `generated app pipeline`, `sandbox runtime` | `sandbox-runtime`, `artifact`, `app-blueprint` | 将分析、表单、dashboard、workflow 变成持续可运行的应用 | Agent 把工作成果固化为 app | 非权威应用状态；最多是分析产物或 artifact |
| Security isolation | 无 ambient access；server code 无外网；client code 受限 iframe；按 observation 重新授权 | `least privilege runtime`, `network egress control`, `share-time reauth` | `policy`, `security`, `audit`, `sandbox-runtime` | Fail-closed 的执行边界 | Agent 和 app 都在受控 binding 内活动 | 安全政策与审计事实 |

## 6. Detailed Concept Notes

### 6.1 Workspace

**FACT**: Cloudflare OS 的 workspace 同时容纳 session、persistent state、outputs/files、resource access 和 isolated runtime。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md

**INFERENCE**: 这与本项目的 Enterprise AI Workspace 目标一致，但 Cloudflare OS 更偏产品化工作容器，而本项目必须继续把正式业务事实留在各自业务 Bounded Context。
**PROJECT DECISION**: `ai-workspace` 可以承载对话、任务、资源、应用与输出，但不能成为合同、客户、审批、财务等事实源。

### 6.2 Gadget

**FACT**: Gadget 是 agent 生成的小型全栈应用，具有独立服务器、前端和持久化状态。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-backend/src/overseer.ts

**INFERENCE**: Gadget 在 Cloudflare OS 中更接近 “用户可持续修改的私有 app 实例”，不是一次性生成物。
**PROJECT DECISION**: 本项目若实现 Generated App，应把其定位为受控 Artifact + 独立 sandbox runtime，且只拥有 UI、草稿、布局、临时分析和用户偏好状态。

### 6.3 Blueprint

**FACT**: Blueprint 共享的是 gadget 的代码定义，不共享 SQLite 数据、聊天历史或 credentials。
来源：
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/docs/blueprints.md

**INFERENCE**: Blueprint 的真正价值是“分享如何构建”，不是“分享当前运行中的事实”。
**PROJECT DECISION**: 本项目的 blueprint/app-blueprint 只应保存版本化定义和连接需求，不保存正式业务数据。

### 6.4 Gatekeeper

**FACT**: Gatekeeper 负责把外部服务包装为受控 capability，处理 OAuth / credential / resource scoping / approval / logging。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-shared/src/gatekeeper.ts

**INFERENCE**: Cloudflare OS 这里的设计比普通 MCP server 更强，因为它把权限、资源、观察和副作用审批连成一个统一层。
**PROJECT DECISION**: 本项目对应实现应命名为 `policy` / capability gateway / integration gateway，而不是通用 HTTP、SQL、Shell 工具箱。

### 6.5 Observation

**FACT**: Cloudflare OS 记录资源观察，而不是只记录调用日志；共享时还要检查观察过的资源是否仍可被协作者访问。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-shared/src/gatekeeper.ts

**INFERENCE**: 这意味着观察是可传播约束，不能只当审计字段；它会影响后续分享、协作和数据出站。
**PROJECT DECISION**: 本项目应把 observation 视为 runtime audit 的上层扩展，形成 `Agent Observation -> Artifact Source -> Derived Access Requirement` 的链路。

### 6.6 Agent / App Boundary

**FACT**: Cloudflare OS 的 gadget 客户端与服务端通过 Cap'n Web RPC 通信，agent 可以调用同一结构化 API。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/packages/workshop-shared/src/gatekeeper.ts

**INFERENCE**: Agent 与 App 共享同一 typed API 边界，能减少重复 adapter，但前提是这个 API 不是私有 DB 结构，也不是前端 DOM 协议。
**PROJECT DECISION**: 本项目的 agent-facing 入口必须复用 Application Use Case、Tool Schema 和 Read DTO，不允许 Agent 直连数据库模型或未经控制的页面协议。

### 6.7 Generated Application

**FACT**: Cloudflare OS 的 app/gadget 是可生成、可运行、可共享、可继续修改的“个人可变应用”。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md

**INFERENCE**: 这类应用适合承载 dashboard、form、whiteboard、slides、report 等工作型产物，而不适合承载正式业务事实。
**PROJECT DECISION**: 本项目 generated app 阶段应优先服务于分析、展示、协作和草稿，不应形成第二套业务写入核。

### 6.8 Security Isolation

**FACT**: Cloudflare OS 默认没有 ambient access；server code 在 Dynamic Worker 中禁用外网，client code 在 sandboxed iframe 中运行。
来源：
- https://blog.cloudflare.com/cloudflare-os/
- https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md

**INFERENCE**: 它的安全模型依赖 runtime isolation + capability introduction + reauthorization，而不是只靠 UI 提示或 ACL 列表。
**PROJECT DECISION**: 本项目的生成应用、Agent 工具和外部连接必须 fail-closed；不得默认开放网络、数据库或 shell。

## 7. Repository-Level Technical Findings

### 7.1 `README.md`

**FACT**:
- Cloudflare OS 有三项核心能力：agent chat UI、sandboxed gadget development、Gatekeepers security framework。
- 它基于 Workers、Durable Objects、Dynamic Workers、Facets 和 Cap'n Web。
- 它支持“每个用户一份私有 gadget 实例”和“blueprint 作为可共享模板”。

**意义**: README 已经足以证明这是一个 workspace/app 平台，不是单纯的聊天产品。

### 7.2 `docs/blueprints.md`

**FACT**:
- Blueprint 只保留 code snapshot 和 binding metadata。
- 共享 blueprint 生成独立 gadget 实例。
- `.gadget` archive 里有固定 magic/version/metadata/content 格式，数据和元数据分离存储。
- blueprint 内容通过 R2/KV 等存储层级传播，并支持版本保留和导入导出。

**意义**: 这说明 Cloudflare OS 对“应用定义”和“应用状态”分离得很明确，这一点对本项目的 Generated App / Artifact / Blueprint 边界非常重要。

### 7.3 `packages/workshop-backend/src/overseer.ts`

**FACT**:
- `Workspace` / `Gadget` / `Gatekeeper` / `Blueprint` 都是显式的数据结构。
- `ObservationAuthorizer`、`ApprovalQueue`、`GatekeeperClient`、`UseGadgetClient` 都体现了能力边界。
- `createBlueprint` 会收集 binding metadata，生成随机 ID，并传播到多个存储层。
- `AgentSpawnerGatekeeper` 体现了 agent 的受控生成能力。

**意义**: 后端把 workspace、gadget、gatekeeper 和 blueprint 组织成一个完整的工作系统，而不是把它们当 UI 模块。

### 7.4 `packages/workshop-shared/src/gatekeeper.ts`

**FACT**:
- capability introduction、resource selection、observation authorization、side-effect approval 都在同一协议族内。
- `Gatekeeper.addObserver()`、`ObservationAuthorizer.authorizeObservation()` 和 `ApprovalQueue.submitAction()` 说明 observation / action / sharing 是联动的。

**意义**: 这比传统 MCP 只管“能否调用工具”更完整，适合我们借鉴为 capability gateway + observation lineage。

### 7.5 `packages/workshop-frontend/src/routes/workspaces.tsx`

**FACT**: UI 明确把 workspace 定义为 isolated environment with conversations, gatekeepers, and outputs。
**意义**: 前端命名也证明 workspace 是产品主容器，而不是 gadget 的别名。

### 7.6 `packages/workshop-frontend/src/routes/gadget.$id.tsx`

**FACT**: 旧 `/gadget/$id` 路由被重定向到 `/workspace/$id`，说明 workspace 已成为当前权威路由语义。
**意义**: 当前产品主体已经从“单 gadget”演进为“workspace 容器”。

## 8. Acceptance Matrix for This Project

### 8.1 Adopt

- Workspace / session / output container
- Capability introduction / delegated principal
- Gatekeeper-style capability gateway
- Observation lineage and share-time reauthorization
- Blueprint as shareable application template
- Generated app as sandboxed, non-authoritative artifact
- Typed RPC / API boundary for agent and UI reuse

### 8.2 Adapt

- Gatekeeper -> 本项目的 capability gateway / policy gateway
- Gadget -> 受控 artifact / future generated app
- Blueprint -> artifact blueprint registry
- Observation -> runtime audit + lineage
- AI Gateway -> 本项目的 model gateway / budget / usage ledger
- Dynamic Worker / Facet -> 本项目的 sandbox runtime / process isolation strategy

### 8.3 Reject

- 让 Cloudflare OS 成为业务权威系统
- Gadget SQLite 作为正式业务事实存储
- Agent ambient access 到 DB / shell / unrestricted HTTP
- 仅靠 ACL 或 UI 控件代替 capability binding
- 让 generated app 直接写入正式业务上下文

### 8.4 Defer

- 任意全栈应用生成的普遍开放平台
- 多人实时编辑作为默认能力
- 第三方 connector 市场化分发
- 自动模拟全部副作用并延迟批量审批的复杂工作流

## 9. Mapping to This Repository

| Cloudflare concept | Business Platform target | Contribution | Current/Planned module boundary | Semantic layer relation |
|---|---|---|---|---|
| Workspace | `ai-workspace` | Agent 会话容器、资源绑定、输出与协作边界 | `ai-workspace` | 工作态容器，不是业务事实 |
| Skills / context | `agent-integration` + `knowledge-context` | 组织上下文、可执行技能 | `agent-integration`, `knowledge-context` | 作为可复用上下文，不覆盖业务规则 |
| Capability introduction | `policy` + `agent-integration` | 短期、资源范围、可撤销授权 | `policy`, `agent-integration` | 授权元数据 |
| Gatekeeper | `policy` / integration gateway | 外部系统受控访问、审批、日志 | `policy`, `agent-integration` | 授权与审计事实 |
| Observation | `audit` + `agent-integration` | 资源观察记录、继承访问约束 | `audit`, `agent-integration` | provenance / lineage |
| Gadget | `artifact` + `sandbox-runtime` | 可持续修改的受控应用 | `artifact`, `sandbox-runtime` | Artifact / UI 产物，非正式事实 |
| Blueprint | `app-blueprint` + `artifact` | 共享模板、版本化定义 | `artifact`, `app-blueprint` | 模板语义层，不是事实层 |
| Generated application | `sandbox-runtime` | 沙箱运行、可继续修改、可共享 | `sandbox-runtime` | 只保留产物与引用 |
| Agent/App RPC boundary | `agent-integration` + `application use case` | agent 和 UI 复用同一结构化 API | `agent-integration`, business app use cases | 公开 DTO / semantic read model |
| Model/cost control | `ai-application` / gateway | provider routing、budget、usage ledger | `ai-application` | 成本和路由语义 |

## 10. Risks and Non-Goals

1. 误把 AI Workspace 做成新的业务内核，产生双重事实源。
2. 把 skill 写成重复业务规则。
3. 只做 RBAC，不做任务级 / 资源级 capability。
4. 把敏感业务数据无条件写进 prompt、log 或 observation。
5. 过早实现任意代码沙箱，扩大攻击面和维护面。
6. 在 Business Platform 和其他 Agent Runtime 中重复实现同一套运行时。
7. 过早绑定特定运行时或模型 gateway。
8. Artifact 分享不继承来源访问约束，导致数据泄漏。

## 11. Follow-up Triggers

以下事件发生后，应重新审查 Cloudflare OS 参考结论：

- Cloudflare OS 的正式自托管方案发布；
- Gatekeeper / Observation 安全模型发生重大变化；
- Generated App 沙箱接口稳定；
- 本项目启动 Generated App 阶段；
- 本项目决定是否采用 `workerd` 或其它特定 runtime。

## 12. Source Index

### Cloudflare official

- Repository: https://github.com/cloudflare/cloudflare-os
- README: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/README.md
- Blueprints: https://raw.githubusercontent.com/cloudflare/cloudflare-os/213ea6aa0a0e29d91d72832dcc9871432c1e01c5/docs/blueprints.md
- Blog: https://blog.cloudflare.com/cloudflare-os/

### Cloudflare source paths used for this analysis

- `packages/workshop-backend/src/overseer.ts`
- `packages/workshop-shared/src/gatekeeper.ts`
- `packages/workshop-backend/src/blueprint-archive.ts`
- `packages/workshop-frontend/src/routes/workspaces.tsx`
- `packages/workshop-frontend/src/routes/workspace.$id.tsx`
- `packages/workshop-frontend/src/routes/gadget.$id.tsx`
- `packages/workshop-frontend/src/routes/gatekeepers.tsx`

### Local project reference

- `docs/architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`
- `docs/adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md`
