# 服务端后端架构清单与执行契约

> 文档 ID：ARCH-MANIFEST-001  
> 版本：1.1  
> 状态：Baseline  
> 生效日期：2026-08-06  
> 适用范围：所有服务端设计、计划、代码、测试、部署与审查任务

## 1. 目的

本文定义完整服务器后端架构设计由哪些权威文档共同组成，以及后续实现任务必须如何证明符合架构。

完整架构不是单一技术方案，而是以下视角的组合：

```text
业务能力与领域边界
+ 应用用例与依赖方向
+ 数据所有权与一致性
+ 接口、事件与集成契约
+ 长时任务与流程协调
+ Enterprise AI Workspace 与 Agent Capability
+ 安全、质量属性与可运维性
+ 部署、可观测性与演进治理
+ 自动化架构适配门禁
```

## 2. 权威架构文档集

以下文档共同构成服务端后端架构 Baseline：

| 领域 | 权威文档 | 主要问题 |
|---|---|---|
| 总体服务端架构 | `SERVER_BACKEND_ARCHITECTURE.md` | 系统采用什么架构及依赖方向 |
| 业务边界 | `BOUNDED_CONTEXT_MAP.md` | 系统由哪些业务上下文组成 |
| 数据与一致性 | `DATA_OWNERSHIP_AND_CONSISTENCY.md` | 谁拥有数据、如何保证一致性 |
| 长时任务 | `WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md` | 长任务、流程与业务状态如何分离 |
| Enterprise AI Workspace | `ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md` | Workspace、Skill、Context、Capability、Observation、Artifact 和 Generated App 如何与业务平台分层 |
| 质量属性 | `QUALITY_ATTRIBUTE_SCENARIOS.md` | 性能、可用性、恢复、安全目标 |
| 安全架构 | `SECURITY_ARCHITECTURE.md` | 身份、租户、授权和数据保护 |
| 部署架构 | `DEPLOYMENT_ARCHITECTURE.md` | 进程、节点、环境与扩缩容 |
| 可观测性 | `OBSERVABILITY_ARCHITECTURE.md` | 日志、指标、追踪和审计 |
| Runtime Audit | `RUNTIME_AUDIT_ARCHITECTURE.md` | 统一 AuditEvent、原子写入和查询验证 |
| 数据完整性与修复 | `DATA_INTEGRITY_AND_REPAIR_ARCHITECTURE.md` | Finding、受控修复、Repair 和 Lease/Fence 边界 |
| 审计保留与篡改证据 | `AUDIT_RETENTION_AND_TAMPER_EVIDENCE.md` | 保留、归档和 Hash Chain 证据 |
| 遗留迁移 | `LEGACY_MIGRATION_ARCHITECTURE.md` | 从现有系统如何渐进迁移 |
| 代码架构 | `CODE_ARCHITECTURE.md` | crate、分层与代码依赖规则 |
| 持久化、查询与多数据库 | `PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md` | Command/Query seam、Projection、跨 Context 读取与数据库适配策略 |
| 数据治理、分析与可视化 | `DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md` | 可重建分析投影、指标语义、受控查询、Dashboard、报表和 Agent 边界 |
| API 与事件 | `../standards/API_AND_EVENT_CONTRACT_STANDARD.md` | 对外协议和兼容性规则 |
| 架构门禁 | `../standards/ARCHITECTURE_FITNESS_FUNCTIONS.md` | 如何自动证明架构符合性 |
| Rust 编码 | `../standards/RUST_CODING_STANDARD.md` | 具体编码和测试规范 |
| 查询与数据库适配 | `../standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md` | Read DTO、分页、查询性能、SQL/ORM 与层级数据规则 |

任何单份文档都不能脱离其余文档单独解释为完整架构。

涉及 AI Workspace、Agent、Skill、Context、Tool、Capability、Observation、Artifact、Blueprint、Model Gateway 或 Generated App 的任务，必须同时遵守：

```text
ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md
SECURITY_ARCHITECTURE.md
OBSERVABILITY_ARCHITECTURE.md
ADR-0018-enterprise-ai-workspace-and-capability-security.md
```

## 3. 架构决策层级

发生冲突时按以下顺序处理：

1. 用户当前明确要求；
2. 已接受 ADR；
3. 本架构清单；
4. `SERVER_BACKEND_ARCHITECTURE.md`；
5. 专题架构 Baseline；
6. `CODE_ARCHITECTURE.md` 与标准文档；
7. 当前执行计划；
8. 当前代码实现。

当前实现与 Baseline 冲突时，现状不能自动覆盖架构。应修正实现，或通过 ADR 修改架构基线。

## 4. 根本设计原则

### 4.1 业务优先

业务边界由业务能力、统一语言、业务不变量和数据所有权决定，不由数据库表、页面、框架、消息主题或部署产品决定。

### 4.2 核心独立

核心系统只表达业务规则、应用用例和稳定能力语义。交付协议、持久化、通信、存储和供应商实现属于外层适配。

### 4.3 依赖倒置

外层依赖内层，具体实现依赖核心定义的抽象。核心不得通过方便性反向依赖具体技术实现。

### 4.4 单一数据所有者

每份权威业务数据必须有且只有一个 Bounded Context 负责写入规则和生命周期。

### 4.5 明确一致性

同一聚合和同一上下文内优先本地事务；跨上下文默认通过事件、幂等、流程协调和补偿实现最终一致。

### 4.6 业务状态与执行状态分离

业务过程状态由业务上下文拥有；任务、重试、租约和调度状态由通用执行能力拥有。两者不得混为一个模型。

### 4.7 安全默认拒绝

身份、租户、权限、敏感数据、外部输入和高风险写操作采用 fail-closed 原则。

### 4.8 质量属性可验证

性能、可用性、恢复、安全和容量目标必须写成可测试场景，不能只使用“高性能”“高可用”等描述。

### 4.9 模块化单体优先

Bounded Context 首先表现为代码、数据和治理边界。只有出现客观部署需求时才拆分微服务。

### 4.10 架构规则自动化

能够通过依赖检查、契约测试、集成测试和 CI 验证的规则，不应长期只依赖人工记忆。

### 4.11 Agent 权限必须任务化和资源化

Agent 传播原用户委托身份，但不得自动继承用户全部环境权限。每个 Agent Run 只能通过短期、可撤销、租户和资源范围明确的 Capability Grant 调用白名单业务 Tool。

### 4.12 Workspace 和 Artifact 不得成为第二业务内核

Workspace、Conversation、Skill、Context、Observation、Artifact 和 Generated App 可以拥有自身产品状态，但不得拥有合同、客户、审批、财务、项目或文档正式状态。正式业务写入只能进入拥有该事实的 Application Use Case。

### 4.13 生成代码必须独立隔离

任何 Agent 生成代码不得在 `business-api`、业务 Worker、AI Worker 或 Agent Adapter 进程内执行。未来 Generated App 必须通过独立 Sandbox Runtime 和 Capability Binding 访问业务能力。

## 5. 新任务架构准入

任何新增能力在进入编码前必须回答：

1. 属于哪个业务能力和 Bounded Context？
2. 谁拥有其权威数据？
3. 业务不变量和生命周期是什么？
4. 是业务状态、流程状态还是执行状态？
5. 需要哪些同步接口、异步事件或防腐层？
6. 事务、幂等和并发边界是什么？
7. 失败、重试、取消和补偿语义是什么？
8. 涉及哪些安全和租户边界？
9. 需要满足哪些质量属性场景？
10. 是否改变公开契约、数据所有权、部署边界或长期技术决策？

Agent/Workspace 相关任务还必须回答：

11. Workspace、Agent Run、Skill、Context、Tool 或 Artifact 中谁拥有该状态？
12. Agent 使用什么 Delegated Principal 和 Capability Grant？
13. Grant 的资源、动作、字段、期限和撤销边界是什么？
14. Tool 是否只调用公开 Application API/Port？
15. 读取数据是否产生 Observation，派生产物如何重新授权？
16. 是否意外引入通用 SQL、Shell、文件系统、任意 HTTP 或数据库凭证？
17. Agent Runtime 是否仍可替换？
18. 生成代码是否被错误地放入核心业务进程？

缺少以上关键答案时，不应直接以数据库表、Handler、Prompt、Skill 文件、MCP 配置或 SDK 为起点实施。

## 6. 计划文档要求

后续 `docs/plans/current/` 中的计划必须包含“架构符合性”章节，至少列出：

- 目标 Bounded Context；
- 数据所有者；
- 影响的用例、接口与事件；
- 依赖方向；
- 一致性与幂等；
- 安全影响；
- 质量属性影响；
- 新增或修改的 ADR；
- 架构适配测试；
- 文档同步清单。

Agent/Workspace 计划还必须列出：

- Workspace/Registry/Run/Artifact 数据所有者；
- Delegation 与 Capability 模型；
- Tool 白名单和风险等级；
- Observation/Artifact lineage；
- Agent Runtime 可替换契约；
- Prompt Injection 和敏感数据最小化；
- Generated App 是否明确排除或具有独立 ADR。

计划不得用具体基础设施操作代替业务和能力设计。

## 7. 代码评审要求

代码审查除功能正确性外必须检查：

- 是否按 Bounded Context 组织；
- 是否存在跨上下文直接写表；
- 是否将数据库模型作为领域模型；
- 是否有基础设施类型向核心泄漏；
- 是否在入口、Worker 或 Adapter 中复制业务规则；
- 是否区分业务状态和执行状态；
- 是否实现必要的幂等、冲突和补偿；
- 是否更新接口、事件、迁移和架构文档；
- 是否增加相应层级的测试；
- 是否通过架构 Fitness Functions；
- Agent Adapter 是否直接访问业务数据库；
- Skill 是否复制业务规则；
- Tool 参数是否可以扩大 Capability 范围；
- Observation 是否泄漏无界敏感内容；
- Artifact 分享是否重新验证来源访问要求；
- Agent 生成代码是否进入核心业务进程。

## 8. 完成定义

一个实现任务只有同时满足以下条件才可声明完成：

```text
业务验收通过
+ 分层和数据所有权符合
+ 协议兼容性明确
+ 安全与质量属性验证通过
+ 测试和 CI 通过
+ 架构文档同步
+ 必要 ADR 已接受
```

Agent/Workspace 任务还必须证明：

```text
Task Capability 不扩权
+ Agent Runtime 可替换
+ Tool 无直接持久化绕过
+ Observation/Artifact 不泄漏
+ 业务平台在 Agent 停止后仍正常
```

仅“代码可以运行”不能视为架构完成。

## 9. 变更控制

以下变化必须通过 ADR：

- 新增、合并或拆分 Bounded Context；
- 改变数据所有权；
- 改变跨上下文一致性模型；
- 新增独立部署单元或拆分微服务；
- 改变身份、租户或授权模型；
- 引入全局框架或基础设施产品；
- 改变长时任务可靠性语义；
- 改变公开 API 或事件的兼容性策略；
- 接受偏离质量属性基线的长期风险；
- 选择 Agent Runtime 作为长期强依赖；
- 选择 workerd、WASI、容器、isolate 或 microVM 作为 Generated App 全局运行时；
- 改变 Capability、Observation 或 Artifact 的长期所有权与安全语义。

## 10. 对当前阶段的约束

PLAN-0001 至 PLAN-0005 已完成基础服务、持久化查询、Durable Document Processing 与 Runtime Governance。

下一阶段 PLAN-0006 只允许建立 Enterprise AI Workspace 的最小基础和一个只读业务垂直切片。其结果必须：

- 不改变现有业务数据所有权；
- 不复制 Runtime Audit 或 Durable Processing；
- 不让 Agent Adapter 直接持久化业务数据；
- 证明 Workspace、Registry、Capability、Tool、Observation 和流式恢复边界；
- 不引入任意 Generated App、通用工具或高风险业务写入；
- 在合并前通过新增 Agent/Workspace Fitness Functions。

## 11. 合并与采用

本清单和专题 Baseline 位于文档分支时，PLAN-0006 不得被描述为 Active。文档变更进入 `main` 后，后续 Agent/Workspace 实现必须以 `origin/main` 中的以下文件为权威来源：

```text
docs/architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md
docs/adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md
docs/plans/current/PLAN-0006-enterprise-ai-workspace-foundation.md
```

正在实施的后续分支必须在最终审查前同步合并后的 `main`，解决冲突并重新运行架构门禁。

## 12. 最终原则

> 后续任务不是“参考”本架构，而是必须证明符合本架构。

> 架构 Baseline 定义长期边界，计划定义阶段实施，代码提供实现证据，测试和 CI 提供持续证明。

> Agent 负责理解和协助，Capability 限制其可做之事，业务 Application Service 对最终事实负责。
