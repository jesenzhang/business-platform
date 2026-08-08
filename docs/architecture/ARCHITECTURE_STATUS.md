# 架构实施状态

> 文档类型：Living Document
> 最后更新：2026-08-07
> 当前阶段：Phase 5 Enterprise AI Workspace Architecture — Proposed
> 当前计划：PLAN-0007 Active；PLAN-0006 继续 Proposed / NOT ACTIVE
> 集成方式：local solo fast-forward，无 PR
> Analytics/Visualization：Baseline 已建立，运行时尚未实现

> 2026-08-03: PLAN-0001 and PLAN-0002 are Integrated and archived. PLAN-0002
> was fast-forwarded at `ad47544505b66d577ccdcb8f300812c294d3d7bf`; main CI
> run 30784568762 passed all six jobs, including PostgreSQL, MinIO, Document
> E2E, and architecture fitness. The repository is in Phase 2 preparation.
>
> 2026-08-03: PLAN-0003 Revision 1 is Integrated and archived at
> `f6dbc693da42d0f9a7566739f5a4169c6a86f880` via local solo fast-forward. Main
> CI run 30796583865 passed all six jobs, including PostgreSQL, MinIO, Document
> E2E, and architecture fitness. At that historical checkpoint Search remained
> Deferred and PLAN-0004 had not started.
>
> 2026-08-03: PLAN-0004 Revision 1 reached Accepted Candidate at
> implementation `a0bb0ad9374e87b1225e26ccbfbd44f4d616ebf2`. Feature CI run
> `30833527820` passed all six jobs, including PostgreSQL, MinIO, the
> multi-process crash/reclaim E2E, and Architecture Fitness. SQLite remains
> local single-process and PostgreSQL is the production multi-worker authority;
> no PR or main integration is used.
>
> 2026-08-04: PLAN-0004 Revision 1 was integrated and archived by local solo
> fast-forward at main `12454709a88fde16f7769af27a75e79c4bc0981a`. Feature CI
> `30833916455` and Main CI `30868701290` passed all six jobs, including the
> GitHub Linux PostgreSQL/MinIO E2E and Architecture Fitness. Local SQLite
> process E2E passed; Windows PostgreSQL/MinIO remains NOT RUN.
>
> 2026-08-04: PLAN-0005 started from main `6fd065c33471665e828a3da3a2cc3fae8d6d2afc`.
> Runtime Governance owns Audit, integrity scan/finding, repair run and ledger
> state; Document Intelligence remains the owner of processing state.
>
> 2026-08-04: PLAN-0005 reached Accepted Candidate at implementation
> `71a2a8495033aec7d8e40752fc80fd2ba30dc485`. Feature CI `30879843925` passed
> format, check, clippy, workspace tests, Architecture Fitness, and the real
> PostgreSQL/MinIO governance E2E. Windows PostgreSQL/MinIO remains NOT RUN;
> metadata-only text-artifact verification and the separate owner/Governance
> local transaction boundary are recorded accepted risks in the current plan;
> SQLite also proves expired-lease reclaim and stale-fence rejection.
>
> 2026-08-04: PLAN-0005 was reopened as Revision 1, `Active`, for Governance
> trust and correctness hardening. The previous implementation candidate is
> `71a2a8495033aec7d8e40752fc80fd2ba30dc485` and the previous evidence HEAD is
> `ab01288cd3477107c567e342ff45b7f624ec5396`. It is not an integration
> candidate until the Revision 1 evidence and Feature CI are green.
>
> 2026-08-04: Revision 1 local evidence is green for formatting, workspace
> check, Clippy, workspace tests, the SQLite Governance E2E, management
> security tests, and Architecture Fitness. Feature CI remains PENDING for
> this revision; PostgreSQL/MinIO integration cases remain NOT RUN locally.
> Implementation evidence is committed at `f1e10a0942c18715f5591b56619d5c6dae21f06e`;
> the push attempt could not reach GitHub HTTPS/443, so no new Feature CI run
> was created.
>
> 2026-08-06: PLAN-0005 Revision 2 was integrated and archived at main
> `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac` by solo fast-forward. Implementation
> SHA is `24e70f4182ca3315d94033178952113c4faba717`; Candidate SHA is
> `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac`; Main CI `31026047403` passed all
> six jobs, including GitHub Linux PostgreSQL/MinIO/E2E and Architecture Fitness.
> Evidence CI `31022731371` and Feature CI `31021778597` also passed. Windows
> PostgreSQL/MinIO remains NOT RUN.
>
> Runtime Audit, Integrity Finding, Controlled Repair, Repair Ledger, and
> Lease/Fence Recovery are integrated. The accepted boundaries remain:
> PROC-INT-008 is metadata/checkpoint based; the hash chain is tamper evidence,
> not WORM; and cross-bounded-context coordination uses local transactions,
> leases, idempotency, and reconciliation rather than distributed transactions.
>
> 2026-08-06: Cloudflare OS was reviewed as an Enterprise AI Workspace reference.
> ADR-0018 accepts an explicit Workspace product layer, task-scoped Capability
> Grants, Agent Observation lineage, non-authoritative Artifact/Blueprint state,
> and a future independent Generated App sandbox. Cloudflare OS remains a
> reference only and is not a runtime dependency. PLAN-0006 is Proposed on the
> documentation branch; no Agent implementation, activation, integration, or
> PLAN-0007 start has occurred.

> 2026-08-07: PLAN-0007 is Active for the external Business API surface, independent React console, remote CLI, and read-only MCP adapter. PLAN-0006 remains Proposed / NOT ACTIVE; no Workspace or Capability runtime is introduced.

> 2026-08-09: PLAN-0008 reached Accepted Candidate on branch
> `codex/plan-0008-document-lifecycle-revision-and-evidence-foundation`, with
> implementation checkpoint `bfc5f2c` based on `35d1d01`. Document Management now
> owns immutable DocumentRevision and typed links; Document Intelligence binds
> ProcessingJob/Run/Artifact/Evidence to an exact revision. Local format, check,
> Clippy, workspace tests, OpenAPI, and Architecture Fitness are green. PostgreSQL,
> MinIO, and real-infrastructure E2E remain NOT RUN locally.

Revision 1 records the Audit history boundary explicitly: migration 013
backfills deterministic tenant-local sequence values but marks pre-existing
rows `chain_version=0`; only new sequence-based rows are chain-protected.
The processing state matrix is now the owner rule for PROC-INT-006, while
PROC-INT-008 remains PARTIAL (metadata/checkpoint only; object-store probing
is deferred). Resolved findings reopen as explicit recurrence episodes.

## 1. 当前权威结论

- Rust 业务平台是系统主体，Agent 和 Enterprise AI Workspace 是可选产品层；
- 关闭 Workspace/Agent Runtime 后，Web、OpenAPI、Worker 和业务流程必须继续运行；
- 服务端采用战略 DDD 主导的模块化单体；
- 服务端内部采用 Domain、Application、Delivery、Infrastructure 和 Composition Root 分层；
- 业务能力、统一语言和数据所有权决定 Bounded Context；
- 核心层只表达业务和通用能力语义；
- 基础设施产品通过适配器、配置、部署和 ADR 接入；
- 战术 DDD 按复杂度采用，不对简单 CRUD 过度建模；
- 每份可变权威业务数据只有一个拥有上下文；
- 同一上下文内优先本地事务，跨上下文通过事件、幂等、Process Manager 和补偿协作；
- 长时任务区分业务状态、业务流程状态、人工工作流和执行机制状态；
- API、事件、安全、质量属性、部署和可观测性属于正式架构资产；
- Web、Worker、OpenAPI 和 Agent 复用 Application 用例；
- Agent 使用原用户委托身份，但每个任务还必须获得更窄的 Capability Grant；
- Workspace、Skill、Context、Observation、Artifact 和 Generated App 不拥有正式业务事实；
- Agent 生成代码不得进入核心业务进程；
- 后续任务必须通过架构 Fitness Functions 提供持续符合证据。

## 2. 完整服务端架构文档集

入口：

- `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`

总体与专题 Baseline：

- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/BOUNDED_CONTEXT_MAP.md`
- `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`
- `docs/architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`
- `docs/architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`
- `docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`
- `docs/architecture/SECURITY_ARCHITECTURE.md`
- `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/architecture/LEGACY_MIGRATION_ARCHITECTURE.md`
- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`
- `docs/architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`
- `docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`
- `docs/architecture/RUNTIME_AUDIT_ARCHITECTURE.md`
- `docs/architecture/DATA_INTEGRITY_AND_REPAIR_ARCHITECTURE.md`
- `docs/architecture/AUDIT_RETENTION_AND_TAMPER_EVIDENCE.md`

标准：

- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
- `docs/standards/RUST_CODING_STANDARD.md`
- `docs/standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`

关键决策：

- `docs/adr/ADR-0003-domain-driven-layered-backend.md`
- `docs/adr/ADR-0010-durable-processing-job-and-fixed-pipeline.md`
- `docs/adr/ADR-0011-worker-leases-fencing-and-crash-recovery.md`
- `docs/adr/ADR-0013-unified-runtime-audit-model.md`
- `docs/adr/ADR-0017-platform-native-analytics-and-visualization.md`
- `docs/adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md`

外部参考和审查：

- `docs/reference/CLOUDFLARE_OS_REFERENCE_ANALYSIS.md`
- `docs/reviews/2026-08-06-cloudflare-os-and-enterprise-ai-workspace-gap-review.md`

## 3. 当前实现状态

当前仓库已完成 Phase 1 Foundation Integrity、Phase 2 Persistence and Query
Hardening、Phase 3 First Durable Business Flow 与 Phase 4 Runtime Governance
Foundation；PLAN-0001 至 PLAN-0005 均已集成并归档。

已具备：

- `apps/*` 和领域/能力 crate 初始划分；
- Domain/Application/Infrastructure/API 的设计方向；
- 统一配置、错误、对象存储和消息基础；
- PostgreSQL 生产权威与 SQLite 本地适配；
- Document Management 最小垂直切片；
- 固定 Durable Document Processing Pipeline；
- Worker Lease/Fence/Heartbeat/Crash Recovery；
- AI Task、Candidate 和 Review 基础；
- Runtime Audit、Integrity Finding、Controlled Repair 和 Repair Ledger；
- 真实 PostgreSQL/MinIO E2E 和 Architecture Fitness CI；
- 完整服务端架构 Baseline 和文档治理。

Enterprise AI Workspace 现状：

- 架构 Baseline、ADR-0018、参考分析和 PLAN-0006 Proposed 已形成；
- `crates/agent-integration` 仍只有 TODO 骨架；
- apps/agent-adapter 在 PLAN-0007 中提供窄的 HTTP MCP read-only adapter；PLAN-0006 的 Workspace/Agent Runtime 仍未实现；
- Workspace/Conversation/Thread/Turn 尚未实现；
- Skill/Context/Tool Registry 尚未实现；
- Delegated Agent Capability Grant 尚未实现；
- ToolInvocation/Observation/Artifact 尚未实现；
- Assistant UI/SSE/业务卡片尚未实现；
- Model Gateway 和 Generated App Sandbox 尚未实现。

仍需收敛：

- 当前领域 crate 尚未按 Bounded Context Map 完成全部统一语言和数据所有权落实；
- `workflow` 通用能力仍未实现通用 Durable Task Execution；PLAN-0004 的固定
  Document Processing 执行边界已落在独立 processing adapters 中；
- Worker 与 Migration 已具备该固定切片的运行实现，其他 Agent Adapter 能力
  仍处于骨架阶段；Runtime Governance 的统一 Audit、Integrity 与 Repair
  能力已由 PLAN-0005 收敛并集成；平台原生 Analytics/Visualization 架构 Baseline 已建立，运行时实现尚未开始；
- API/Event 契约尚未全部形成可生成 Schema；
- 质量属性尚未形成完整性能和容量证据；
- 生产 Runbook 尚未完成；
- Enterprise AI Workspace 仅有文档设计，没有运行证据。

## 3.1 总体架构第 19 章后续路线

与总体架构第 19 章保持一致，以下能力仍未完成，不能从当前 Runtime Governance 或
Document Processing 实现推断为已交付：

- 核心业务领域迁移与建模：Identity/Organization、Customer、Contract、Approval、
  Project、Finance、Notification 和其他 Bounded Context；
- AI 业务能力扩展：Provider、OCR/LLM/VLM/Parser、抽取/分类/摘要、候选复核、成本和恢复；
- 通用 Durable Task / Workflow：定时任务、Process Manager、重试、补偿、取消、恢复和人工工作流；
- 平台原生 Analytics/Visualization：投影基座、指标语义、Analytics Query Service、
  Dashboard/Report 和受控 Agent 分析技能，Runtime 尚未实现；
- Agent 只读与分析、Agent 受控写入，以及高级智能化和桌面/移动/语音入口。

当前只有 PLAN-0004 的固定 Document Processing Pipeline 和 PLAN-0005 的 Runtime
Governance Foundation 具备集成运行实现；通用 Workflow、完整 Agent 和 Analytics
Runtime 均仍需独立 PLAN。

## 4. 已完成计划的持续约束

PLAN-0001 至 PLAN-0005 的约束继续有效：

1. 业务上下文拥有自己的正式状态和规则；
2. Domain/Application 不依赖具体 Delivery/Infrastructure；
3. Application 用例定义权限、租户、版本、幂等、事务和审计意图；
4. Infrastructure 类型、错误和 DTO 不向核心泄漏；
5. API 按契约规范实现认证、错误、版本、幂等和乐观锁；
6. 数据写入、Audit 和 Outbox 的一致性符合数据架构；
7. Local/S3 Adapter 符合安全、流式和契约测试要求；
8. Processing 和 Governance 使用明确 Owner Port，不直接写其他上下文私有表；
9. 多 Worker 写入使用 Lease/Fence/Optimistic Version；
10. Repair 只允许明确类型和 Allow-list，不提供任意 SQL；
11. CI 必须继续提供真实 PostgreSQL/MinIO 和架构证据。

PLAN-0006 还必须保持：

12. Agent Adapter 不直接访问业务数据库；
13. Skill 不复制业务状态机；
14. Capability Grant 不超过原用户权限且绑定任务、资源、动作、字段和期限；
15. Tool 无通用 SQL、Shell、文件系统和任意 HTTP；
16. Observation 不保存无界敏感正文；
17. Workspace/Artifact 不成为第二业务内核；
18. Agent Runtime 可替换；
19. Generated App 明确排除在 PLAN-0006 外。

## 5. 后续任务强制规则

所有计划必须包含：

- 目标 Bounded Context/平台能力边界；
- 数据所有者；
- 业务不变量；
- Commands、Queries、API 和 Events；
- 事务、一致性、幂等和补偿；
- 安全与数据分类；
- 质量属性；
- 部署和可观测性影响；
- Fitness Functions；
- 文档和 ADR 更新。

Agent/Workspace 计划还必须包含 Delegation、Capability、Tool allow-list、
Observation、Artifact lineage、Runtime replaceability 和 Prompt Injection 威胁模型。

缺少这些内容的计划不能直接进入实现。

## 6. 架构适配门禁

当前最低门禁：

- Domain 不依赖 Delivery、Infrastructure 和供应商实现；
- Application 不依赖具体 Adapter；
- Handler 和 Worker 入口不承载业务规则；
- 基础设施错误和 DTO 不向核心泄漏；
- 业务用例能以 Fake/In-Memory Ports 运行；
- 适配器有真实依赖契约测试；
- 跨上下文没有直接写入对方私有数据；
- 新增长时任务区分业务状态和执行状态；
- API/Event 兼容性有测试；
- 安全和租户边界 fail-closed；
- 架构相关文档与代码同步；
- Agent Adapter 不依赖业务持久化适配器；
- Agent Integration core 不依赖 Axum/SQLx/Reqwest/Provider SDK；
- Capability 使用和 Tool 参数范围可自动验证；
- Generated App 运行时依赖不会提前进入 PLAN-0006。

## 7. 当前判定

```text
完整服务端架构 Baseline：已形成并生效
业务能力和 Context Map：已形成初始 Baseline
数据所有权和一致性：已形成 Baseline
长时任务架构：已形成 Baseline，固定 Document Processing 已实现
Runtime Audit / Integrity / Repair：已形成 Baseline 且实现基础已集成
Enterprise AI Workspace：Baseline/ADR/Proposed Plan 已形成，代码未开始
API/Event 契约：已形成 Baseline，Schema 尚待全面落地
安全架构：已形成 Baseline，业务和 Governance 部分符合，Agent Capability 待实现
质量属性：已形成初始目标，Workspace 性能/恢复证据尚无
部署和可观测性：已形成 Baseline，Workspace/Sandbox 部署未实现
遗留迁移：已形成 Baseline，具体切片尚待计划
代码骨架：已存在
分层依赖：部分符合
基础设施隔离：部分符合
自动化架构门禁：已实现基础，本地与 GitHub Actions 均 PASS；Agent 门禁待 PLAN-0006
PLAN-0001：Integrated / Archived
PLAN-0002：Integrated / Archived
PLAN-0003：Integrated / Archived
PLAN-0004：Integrated / Archived（main `12454709a88fde16f7769af27a75e79c4bc0981a`；Feature CI `30833916455`；Main CI `30868701290` 全绿；Windows PostgreSQL/MinIO NOT RUN）
PLAN-0005：Integrated / Archived（main `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac`；Implementation `24e70f4182ca3315d94033178952113c4faba717`；Candidate `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac`；Main CI `31026047403`；Windows PostgreSQL/MinIO NOT RUN）
Analytics/Visualization：Baseline（ADR-0017）；运行时实现尚未开始
PLAN-0006：Proposed / NOT ACTIVE（Architecture Decision ADR-0018；Base `a3f78a7d6e1a745d30cd0e6cf257a870fc95aa58`）

PLAN-0007：Active / local solo fast-forward（Business Console、Public REST Contract、CLI、read-only MCP；未归档）
```

## 8. PLAN-0006 采用前动作

PLAN-0006 进入 Active 前：

1. 保持 ADR-0018、Workspace Baseline、Cloudflare OS 参考分析和 Proposed 计划文档的语义一致；
2. 对 PLAN-0006 的 Workspace、Capability、Observation 和 Tool 所有权做独立审查；
3. 确认第一垂直切片只读且只使用 Document Processing 公共 Query Port；
4. 明确 Agent Runtime Port 与 deterministic Fake Runtime；
5. 完成轻量威胁模型；
6. 明确迁移、API、SSE、Crash Recovery、PostgreSQL E2E 和 Fitness Functions；
7. 激活后建立独立实现分支，不在文档分支直接编码。

## 9. 下一次更新条件

出现以下事件时更新本文：

- PLAN-0002 成为 Accepted Candidate；
- PLAN-0002 本地 fast-forward 集成并完成 main CI；
- 首个垂直切片通过架构验收；
- 架构适配测试进入 CI；
- Bounded Context 或数据所有权调整；
- 新增部署单元或重大基础设施；
- 质量属性目标被实测或调整；
- PLAN-0002 完成并归档；
- PLAN-0004 Gate 0 通过并进入 durable processing implementation；
- PLAN-0004 Revision 1 集成并归档，或开始下一项明确计划；
- PLAN-0005 Runtime Governance Foundation 建立并通过架构门禁；
- 平台原生 Analytics/Visualization Baseline 建立；后续投影、指标、查询、Dashboard、报表和 Agent 技能必须由独立 PLAN 推进；
- 指标语义、投影、Analytics Query Service、Dashboard/Report 或 Agent 分析技能开始实现；
- 开始第一个遗留业务迁移切片。
- PLAN-0006 被激活；
- Workspace/Capability 数据模型形成候选；
- 第一个 Agent read-only Tool 通过授权和 Adapter 契约测试；
- Agent Runtime crash/recovery 与 SSE reconnect 有实测证据；
- PLAN-0006 成为 Accepted Candidate 或被取消/替代；
- 选择长期 Agent Runtime；
- 开始 Artifact/Blueprint 阶段；
- 提议 Generated App Sandbox 或选择 workerd/WASI/container/isolate/microVM；
- Bounded Context、数据所有权、授权模型或部署单元发生变化。
