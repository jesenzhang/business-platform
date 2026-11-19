# PLAN-0017 — Contract Business Vertical Slice

> 文档 ID：PLAN-0017
> 版本：0.1（评审稿）
> 状态：**Proposed / NOT ACTIVE / 待独立评审**。本文件是文档计划，不授权编码、依赖安装、API 激活或部署。
> 日期：2026-11-19
> 责任模块：Contract Management（正式字段与版本所有者）；协同：Document Management、Document Intelligence（`document-processing`）、Policy/IAM、Audit、Business Console Frontend
> 计划基线：`codex/contract-vertical-slice-plan` @ `e3d7c30`（基于 main `e3d7c30`）；GitHub main 最近一次 CI `44310190041`（2026-11-18）未全绿
> 权威输入（冲突时按 AGENTS.md §1）：`ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md`（§5 合同领域基线）、`BOUNDED_CONTEXT_MAP.md`（§8/§12/§13）、`DATA_OWNERSHIP_AND_CONSISTENCY.md`、`SERVER_BACKEND_ARCHITECTURE.md`、`CODE_ARCHITECTURE.md`、`SECURITY_ARCHITECTURE.md`、`FRONTEND_DESIGN_SYSTEM_STANDARD.md`、`API_AND_EVENT_CONTRACT_STANDARD.md`、`ARCHITECTURE_FITNESS_FUNCTIONS.md`、`DOCUMENT_MANAGEMENT.md`、ADR-0010/0011/0012/0013/0019/0022/0024/0027
> 关联计划：PLAN-0014（Active / Review-first，**非本计划准入条件**）、PLAN-0006（Proposed / BLOCKED，等待本切片）、PLAN-0008（已归档，文档修订与证据基础）、[PR #19 前端候选重评估](../../reviews/2026-11-19-pr19-contract-frontend-recheck.md)
> 编号说明：0017 未占用；已归档的 [PLAN-0016](../archive/2026/PLAN-0016-astryx-frontend-design-system-adoption.md) 属于 Astryx 前端设计系统，编号不可复用

## 1. 定位与状态

截至 2026-11-19，**P0 是恢复主干 CI 全绿**；**P1 是与 P0 并行编制并评审本合同业务垂直切片计划**。本计划先完成设计与评审，不激活实现。

- 合同切片**无需等待 PLAN-0014 的 R2 准入**即可并行编制与评审；PLAN-0014 的 review/test 工作不替代本合同链路验证。
- **主干 CI 全绿是进入本计划实现阶段的硬前置门禁**（见 §11 Gate-0）。CI 未恢复前只允许文档评审与测试设计，不允许写入口、迁移、API、Worker 或前端代码。
- 不激活 PLAN-0006、不重开 PLAN-0009、不改变 PLAN-0014 状态、不合并或 push 任何分支。
- 本计划不改变任何长期边界；若评审中发现需要新增 Bounded Context、变更所有权、公开契约语义或幂等/版本规则，先停下并通过 ADR，再恢复实现。

## 2. 目标与非目标

### 2.1 目标

以**一个固定闭环**验证平台现有 Document / Processing / Policy / Audit / Frontend 基础，并交付合同领域第一个真实业务垂直切片：

```text
合同列表与详情
  → 文档修订（Document Management 拥有的内容修订）
  → 抽取与证据（document-processing 的 Candidate + Evidence）
  → 人工审阅（CandidateReview 决定，仍不是业务事实）
  → 应用候选（Contract Management 的 Application Use Case）
  → 生成合同新版本（ContractVersion，正式字段与版本由 Contract 拥有）
```

该闭环必须同时证明：租户隔离、幂等、版本冲突、已签版本保护、审计追溯五项验收；证明跨上下文协作只通过 Application API + 事件 + 幂等，不使用共享事务。

### 2.2 非目标

- 不实现 Approval / Finance / Legal / Business Assurance / Party 主数据的完整业务闭环；PartyId 只作为引用。
- 不扩展抽取流水线：`ValidateSource → DetectType → ExtractText → ExtractFields → ValidateCandidate → AwaitReview` 固定管线不变；不引入通用 DAG、调度器、Workflow Designer、OCR 平台、Model Gateway。
- 不在首轮处理 PDF / Office / 图片：现有 `crates/document-processing/src/domain/pipeline.rs` 仅接受 `text/plain`、`text/markdown`、`application/json`，并强制严格 UTF-8。
- 不新增 Bounded Context、crate、数据库、进程、部署单元、消息 Topic 或第三方组件。
- 不修改 `apps/business-console/src/api.ts` / `src/contracts.ts` 的语义，不引入页面级全局 CSS，不改变 Astryx/Tailwind 层级与主题单一状态来源（见 AGENTS.md §17、ADR-0027）。
- 不把 Candidate Review accepted、ProcessingJob Completed 或 UI 提交成功表述为合同正式写入成功。
- 不使用真实客户合同、密钥、生产 URL 或未脱敏样本；不声称本计划已验收或已激活。

## 3. 目标 Bounded Context、数据所有者与统一语言

### 3.1 唯一权威所有者

| 数据 / 事实 | 唯一所有者 | 其他上下文允许方式 |
|---|---|---|
| Contract、ContractVersion、ContractTerm、Amount/Period、ContractLifecycleState、ContractParty（引用 PartyId）、合同侧文档关系 | **Contract Management** | ContractId/Version 引用、公开 Query、版本化事件；不得直接写私有表 |
| Document、DocumentVersion / DocumentRevision、文件字节与存储引用、内容访问授权、文档生命周期 | **Document Management** | DocumentId + 明确 Revision 引用；公开内容授权 API；不得复用或覆盖其他上下文写入 |
| ProcessingJob、步骤/checkpoint、AI Task、ExtractionCandidate、CandidateEvidence、CandidateReview、Worker lease/fence/retry 业务记录 | **Document Intelligence**（`document-processing`） | 公开候选/审阅 Query 与应用命令；返回带来源 Revision/Evidence 的候选，不返回 raw text |
| Lease、fence、重试、取消、恢复的**执行机制** | Durable Task Execution | 仅为 DI 提供机制，不成为第二业务状态所有者（AGENTS.md §15 / ADR-0010/0011） |
| Principal、TenantContext、RoleBinding、ResourceScope、Policy Decision | Identity / Organization / Policy | 进入 Application 前建立；业务上下文拥有“何种状态下允许何种操作” |
| Runtime AuditEvent、Outbox 发布事实 | Audit / 基础设施适配 | 应用用例产生审计意图；与业务事件可关联但不可互相替代 |
| Party 主数据 | Party & Counterparty | ContractParty 仅保存 PartyId 引用；本切片不写 Party |

### 3.2 统一语言与关键不变量

- **Candidate 不是正式业务事实**。候选只表达“抽取系统认为可能是什么”，必须带 `content_revision`、证据行号/片段来源、pipeline version 与生成主体。
- **ProcessingJob Completed ≠ 业务完成**。执行状态的成功不自动推进合同状态；只有 Contract Application 在校验权限、版本、冲突、业务规则后写入才形成正式事实。
- **修订绑定**：合同只能引用明确的 Document Revision；新修订不得静默复用旧修订的 OCR/抽取结果；证据必须能追溯到该 Revision。
- **已签版本不可覆盖**：任何字段应用、修订替换、删除都不得修改已签署 ContractVersion；变更必须生成新版本并保留历史。
- **Reference + Snapshot**：跨上下文长期结论保存 SourceRef（context、aggregate_id、resource_version、observed_at）与最小 Snapshot（字段、来源版本、checksum），不只是指向“当前值”。
- **Review 与 Apply 分离**：审阅决定留在 DI；正式字段写入由 Contract Use Case 执行，使用 Prepare → Preview → Confirm → Execute，确认绑定主体、租户、目标版本、nonce、过期时间。

## 4. 架构影响

- **边界不变**：不新增 Bounded Context、不拆微服务；沿用模块化单体。`crates/contract/src/lib.rs` 当前仅为 TODO 骨架，实现时在该 crate 内按 Domain / Application / Infrastructure / API 逻辑分层展开，迁移文件标注 `contract` 命名空间。
- **依赖方向**：Domain 不依赖 Axum/SQLx/Reqwest/SDK/`apps/*`；Application 只依赖 Domain 与端口；API 只做协议转换；Infrastructure 实现端口；`apps/*` 作为 composition root 装配。核心层不得引用 Document Management / document-processing 的私有 Row。
- **跨上下文方式**：Contract ↔ Document Management ↔ Document Intelligence 仅通过公开 Application API、版本化集成事件、幂等消费者；同一上下文内使用本地事务 + Outbox；**禁止跨上下文共享写事务、私有表 FK、任意 JOIN 或共享可变 BusinessBase 表**。
- **进程编排**：首个闭环由应用层的 Process Manager / 用例协调（记录流程实例、关联 ContractId/JobId/Document Revision、接收事件、发命令、超时与补偿），不把 Broker 消费端当作业务规则所有者。Durable Task Execution 只提供 lease/fence/retry 机制。
- **持久化**：PostgreSQL 是多 Worker 生产权威；SQLite 仅本地单进程（`BEGIN IMMEDIATE`），不得用于生产、并行或多 AI 配置。写命令与 Outbox 在同一本地事务提交；消费端按至少一次交付设计。
- **前端**：仅表现层。PR #19 的前端候选需在 Astryx 基线（ADR-0027 / PLAN-0016 已归档 / STD-FRONTEND-001）下重评估，作为本计划**受门禁的前端工作包**，不单独形成合同 E2E 声明；见配套重评估笔记。
- **ADR 触发条件**：若评审得出“新增/合并/拆分 Context、变更数据所有权、变更公开 API/事件语义或幂等/版本规则、新增部署单元或基础设施、改变身份/租户/权限/数据分类”，必须先建 ADR，本计划暂不预期产生 ADR。

## 5. 公开 Application API 与事件（草案，评审冻结）

以下为计划契约草案，非已发布 API；实施前由各 Owner 评审、冻结为版本化 OpenAPI / JSON Schema 并纳入 CI 破坏性差异检测。

### 5.1 Commands / Queries（Contract Management 拥有）

- `CreateContract`（201；`Idempotency-Key`，租户/主体来自可信上下文；初始 Draft 版本）
- `ListContracts` / `GetContractDetail`（契约列表与详情；稳定 Read DTO、opaque versioned cursor、过滤白名单；详情含 ContractVersion 列表与文档 Revision 引用）
- `LinkDocumentRevision`（只存引用：DocumentId + content_revision + 关系角色；不复制字节、不写 DM 私有表）
- `UpdateContractDraftFields`（`expected_version`；已签版本禁止）
- `ApplyCandidate`（**核心**：输入 candidate_id、candidate_version、content_revision、evidence refs、expected_version、确认令牌；输出新 ContractVersion 或稳定冲突错误）
- `RecordContractVersionSigned`（由 Owner 决策后标记签署；签署后进入不可变保护，见 §6.4）

相关现有/下游端点（不属于 Contract 所有权，仅被调用）：
`POST /api/v1/documents/{document_id}/processing-jobs`（固定 `content_revision`、`Idempotency-Key`）、`GET /api/v1/processing-jobs/{job_id}`、`.../candidate`、`POST .../review`（review 需 `Idempotency-Key` 与 candidate version，见 API 标准 §25）。

### 5.2 事件（版本化，过去时事实）

- Contract 域：`ContractCreated.v1`、`ContractDocumentRevisionLinked.v1`、`ContractFieldsApplied.v1`、`ContractVersionCreated.v1`、`ContractVersionSigned.v1`、`ContractCandidateApplyRejected.v1`。
- 处理域沿用 `document.processing.*.v1` 命名空间（API 标准 §25），负载只含引用与有限计数，不含文档字节、raw text、对象 key、lease 身份或凭证。
- 集成事件 Envelope 必备：`event_id`、`event_type`、`schema_version`、`occurred_at`、`producer`、`correlation_id`、`causation_id`、`trace_id`、`tenant_id`、`subject_ref`、`payload`；敏感内容最小化。
- 消费者忽略未知可选字段，按 `event_id` 与业务唯一键去重；重复、乱序、重放必须被测试覆盖；Dead Letter 不自动成为业务终态。

### 5.3 DTO / 错误约束

- 金额使用 decimal + currency，时间用带时区 ISO-8601，ID 为稳定字符串；DTO 与 Domain Model / DB Row 分离，输入使用允许列表。
- **禁止**对外返回 `object_key`、`storage_key`、bucket、内部路径、raw text、signed URL、prompt、credential、lease token、provider 原始错误。
- 稳定错误码至少：`CONTRACT_VERSION_CONFLICT`、`CANDIDATE_STALE`、`SIGNED_VERSION_IMMUTABLE`、`TENANT_MISMATCH`、`EVIDENCE_MISSING`、`CONTENT_REVISION_MISMATCH`、`IDEMPOTENCY_KEY_REUSE`，统一错误体含 `code`/`message`/`request_id`/`trace_id`。409 表示版本/幂等/业务冲突，403/404 避免资源枚举。

## 6. 一致性、幂等、版本、并发与补偿

### 6.1 一致性与事务边界

- **强一致**：Contract 聚合状态转换、`expected_version` 校验、ApplyCandidate 写入与其 Outbox、权限确认、签署标记；同一聚合/上下文内用本地事务。
- **最终一致 / 有界陈旧**：跨上下文通知、候选生成、审阅完成后回到 Contract 的命令、Read Model/360 视图；允许明确滞后窗口并告警。
- **读己之写**：用户在 Contract Query 中立即可见自己完成的写入；不依赖异步投影判定写入成功。
- **无共享事务**：即使模块化单体共享一个 DB 实例，也不以跨上下文事务掩盖边界；不通过直接写对方表完成闭环。

### 6.2 幂等

- 所有可重试正式写命令（`CreateContract`、`UpdateContractDraftFields`、`ApplyCandidate`、`RecordContractVersionSigned`）接受 `Idempotency-Key`，作用域含 tenant + principal + 用例名 + 请求摘要。
- 相同键 + 相同摘要返回原结果；相同键 + 不同摘要返回 409；保存状态、结果引用和过期策略。客户端超时后可安全查询结果。
- 事件消费者按 `event_id` / 业务唯一键 / 聚合版本去重，重复事件不得产生第二个 ContractVersion 或第二份正式副作用。
- 外部 Provider 回调（若未来引入）验证签名、时间戳与重放窗口，不直接信任内容做正式写入。

### 6.3 版本、并发与冲突

- 重要聚合带版本号；写命令携带 `expected_version`，不匹配返回稳定冲突错误，不自动覆盖。
- 应用候选前重新校验：candidate version、content_revision、证据是否仍属于该修订、Contract 目标版本、审阅决定是否仍有效；任一变化即 `CANDIDATE_STALE` / `CONTENT_REVISION_MISMATCH`。
- 并发 Apply：同一 `expected_version` 下最多一次提交成功，其余冲突；不允许后写覆盖先写。审阅决定同样带 candidate version，冲突时返回冲突而非覆盖更新决策（API 标准 §25）。
- DI 的 ProcessingJob 写入使用 adapter-owned `ProcessingExecutionUnitOfWork`，按 lease 与 fence 版本写入；stale worker fail closed。PostgreSQL 为多 Worker 权威；SQLite 仅本地单进程。

### 6.4 已签版本保护

- 签署完成后的 ContractVersion 为不可变：禁止字段更新、修订替换、候选应用、物理删除；任何此类命令返回 `SIGNED_VERSION_IMMUTABLE`（或 409/422，按冻结的契约统一），并产生审计事实。
- 变更只能创建新版本，保留原版本、来源修订、Snapshot 与替代关系；已签版本的字节/修订不得被覆盖。
- 谁能触发签署、是否需要 Approval、签署后是否允许补充协议生成新版本，列为本计划开放问题（§13），评审冻结前不得实现。

### 6.5 补偿（不是技术回滚）

- 跨上下文失败以新的业务动作处理：记录 Apply 失败/拒绝事件、保留 Candidate 与 Review 证据、允许以新版本重新应用；不删除仍被证据引用的文件。
- Process Manager 记录参与上下文、关联 ID、超时、重试与人工介入点；不可逆结果（已签版本、已发布事件）不得静默回滚。
- 文件与业务元数据通过状态机、checksum、一致性扫描协调；扫描覆盖“DB 有引用但对象缺失 / 对象存在但无引用 / Outbox 长期未发布 / lease 异常”，结果可观测、可审计、可重试。

## 7. 身份、租户、授权与数据分类

- 身份来自外部 IdP 的可信 OIDC/JWT（ADR-0024）， Delivery 提取 Principal / Tenant / RequestId / TraceId；**tenant 不得来自请求体或前端参数**。生产配置 fail-closed，禁止无签名 Token 与开发万能用户。
- 授权为 RBAC + ABAC + 业务状态规则组合：Policy Context 提供机制，Contract 拥有“何种状态下允许 Apply/签署/变更”的规则。复用目录中的保留键 `contract.read/create/update/review/archive`、`document.read/review`（`crates/policy/src/catalog.rs` 标记为 reserved，当前无消费用例）；是否点亮、绑定到哪些 RoleBinding/ResourceScope 由 Owner + Reviewer 决定，不通过 UI 隐藏按钮改变授权结果。
- 资源级 scope 包含 tenant、contract id、document id、组织范围与业务状态；跨租户访问统一 403/404，避免资源枚举；Agent / CLI / MCP 权限不超过原用户。
- 数据分类：合同与文档正文按 Confidential 处理（涉法务/财务字段不得因进入 UI、事件或分析而降级）；Secret/Credential 禁止进入业务 DTO、日志、trace、事件与报告。
- 轻量威胁建模（SECURITY_ARCHITECTURE §20）：提取文本属于不可信输入（prompt injection / 恶意内容）；威胁包括跨租户 IDOR、旧版本提交、重复确认、伪装回调、绕过授权预览、对象 key 泄漏；控制为租户约束 Repository、版本/幂等校验、确认令牌、公开 DTO 脱敏与审计，剩余风险进入 §13。
- 高风险写（ApplyCandidate、签署）执行 Prepare → Preview → Confirm → Execute，绑定主体、租户、资源版本、操作摘要、风险、过期时间、一次性 nonce 与 trace；目标版本变化或确认过期后必须重新准备。

## 8. 质量属性与可测量验收（目标值待评审确认，不接受“高性能/高可用”表述）

| 属性 | 可测量场景 / 验收阈值 |
|---|---|
| 租户隔离 | 负向用例矩阵覆盖跨租户 list/detail/processing-job/candidate/review/apply/preview；**越权成功次数 = 0**，全部返回 403/404；Repository、ObjectKey、Cache、Event、日志均带 tenant 约束 |
| 幂等 | 同一 `Idempotency-Key` 重放 N 次：正式 ContractVersion 新增数 = 1，响应一致；相同键不同摘要 100% 返回 409；事件重复投递不产生第二个版本 |
| 版本冲突 | 两个 concurrent Apply 使用同一 `expected_version`：恰好 1 成功、1 冲突；无 lost update；`CANDIDATE_STALE` / `CONTENT_REVISION_MISMATCH` 可复现 |
| 已签版本保护 | 对所有写命令尝试修改已签版本：成功次数 = 0；每次尝试均有审计事实；原版本 checksum/引用不变，新变更生成新版本 |
| 审计追溯 | 每个成功 Apply 可沿 `ContractVersion → Candidate(id/version) → Evidence(content_revision + line) → Reviewer principal → decision → event_id → trace_id` 完整追溯；E2E 样本追溯完整率 = 100%，任一缺失即失败 |
| 首轮样本与格式 | 脱敏样本：纯文本 / Markdown / JSON 各 ≥ 2 份（含至少 1 份字段缺失或冲突样本），负例含不支持类型、非法 UTF-8、超长内容、`content_revision` 不匹配、证据缺失；PDF/Office/图片标 `NOT RUN` 而非 PASS |
| 性能/容量（待确认） | 测试环境 1,000 contracts/tenant：List p95 ≤ 300 ms、Detail p95 ≤ 200 ms、Apply 排除处理时间 p95 ≤ 500 ms；超出阈值记录实测值并由 Owner 决定调整，不宣称通过 |
| 可用性/故障隔离 | DI Worker 停摆时 Contract list/detail/已存版本查询仍可用；ProcessingJob 状态查询不依赖业务写入；故障注入下确认令牌不过期失效、不绕过权限 |
| 恢复与 RPO/RTO | 崩溃/重复/重放后 Outbox 事件不丢不重；lease 过期由新 Worker fence 接管；目标为已提交业务事实零丢失，RPO/RTO 数值由运维基线与评审确认 |
| API/Event 兼容 | 现有端点无破坏性差异；OpenAPI/JSON Schema 差异检测进入 CI；未知事件字段可忽略；历史消息重放测试通过 |
| 前端（受门禁 WP） | 仅表现层变更；`npm run lint/typecheck/test/build/test:e2e`、`npx astryx doctor`、`tests/styles.test.ts` 通过；附真实 Chromium 截图与交互证据；`src/api.ts`/`src/contracts.ts` 语义零差异 |

## 9. Fitness Functions（架构门禁）

- 依赖方向：`scripts/check-architecture.ps1` 或等价检查证明 Domain 无 Axum/SQLx/Reqwest/供应商 SDK/`apps/*`；Application 不依赖 API/Infrastructure；基础设施类型不进入核心接口。
- 数据所有权：静态检查证明无跨上下文私有表写入、无跨 Context Repository 引用、无任意 JOIN 后直接写回；migration 文件标注所属上下文。
- 契约：Rust OpenAPI 生成 + 破坏性差异检测；事件 JSON Schema 校验；公开 DTO 中 `object_key|storage_key|bucket|raw_text|prompt|credential|lease` 关键字/字段的负向测试。
- 安全：未认证、跨租户、IDOR、Mass Assignment、重复/过期确认、Secret 泄漏、生产 fail-closed 配置测试。
- 幂等/版本：相同键重放、摘要冲突、`expected_version` 冲突、已签版本不可变、事件重复/乱序/重放的自动化用例。
- 长任务：ProcessingJob 崩溃、重复、重试、取消、lease/fence recovery 与 SQLite 生产配置拒绝用例。
- 文档同步：计划索引、ADR 索引、OpenAPI/Schema、迁移清单与本文状态一致；Review 记录引用实际 commit/CI。
- 前端：`styles.test.ts` 仅允许外壳类、无新增页面级全局 CSS；Astryx doctor 与 Chromium 证据齐备。
- 提交前门禁：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets --all-features`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test --workspace --all-features`；无法执行时记录 `NOT RUN` / `BLOCKED`，不得宣称通过。

## 10. 测试分层

1. **Domain 纯单元测试**：Contract 状态机、已签版本不可变、Candidate/Evidence 绑定不变量、版本号递增、拒绝非法转换；无 DB/网络/Tokio Runtime。
2. **Application + Fake Ports**：Create/List/Detail/Link/Update/Apply/Sign 用例；幂等记录、expected_version、Outbox 写入、授权决策、Process Manager 流程与补偿分支。
3. **真实 PostgreSQL + MinIO 契约测试**：Repository、Outbox 多 Worker、lease/fence/崩溃恢复、取消与重放、对象一致性扫描、迁移 status（仅允许明确缺失 migration table 的忽略）；CI 必须 `--include-ignored`，本地无 DB 时带原因 `#[ignore]`。
4. **API 安全测试**：OIDC/租户/资源权限、403/404 无枚举、错误码、无内部 key/正文泄漏、opaque cursor、幂等与乐观锁、OpenAPI 兼容。
5. **审计测试**：业务写入与 Audit/Outbox 原子性、追溯链完整、审计与 Runtime Audit 可关联但不互相替代。
6. **E2E（首轮脱敏样本）**：固定闭环全链路；纯文本/Markdown/JSON 各自成功路径 + 负例；重复事件、审阅冲突、旧版本提交、已签保护；前端 Playwright 需真实或确定性授权 fixture，否则记 `BLOCKED/NOT RUN`。
7. **依赖/安全扫描**：secret、漏洞、许可证、镜像扫描；前端依赖 lockfile/license 记录。

本任务为文档计划，以上命令本次均 **NOT RUN**；实现阶段按 §11 门禁逐项执行并记录证据。

## 11. 分阶段步骤与依赖门禁

| 阶段 | 步骤 | 门禁 / 证据 | 状态 |
|---|---|---|---|
| Gate-0（P0） | 恢复 main CI 全绿：处理 `44310190041` 中失败的 `PostgreSQL + MinIO + E2E contracts`（Initialize containers）与 `Security scanning`（`cargo audit --deny warnings`） | 主干 CI 10 作业全绿并记录 run ID；未全绿则**不得进入阶段 2 及以后** | P0，进行中 |
| Gate-1（P1 并行） | 完成本计划与 PR #19 重评估的 Owner/独立评审；冻结字段集、版本规则、API/Event、权限键、签署定义、SLO 目标 | 评审记录；若需改边界/契约先建 ADR；本计划保持 Proposed / NOT ACTIVE | P1，进行中 |
| 阶段 2 | Contract Domain + Application 用例与端口骨架（不接 HTTP、不启 Worker） | Domain 单测 + Application Fake Port 测试；架构依赖检查 | 未开始 |
| 阶段 3 | 持久化适配器与 `contract` 迁移命名空间；Outbox/Inbox、幂等存储、版本并发 | 真实 PG + MinIO 契约测试、幂等/冲突/重放测试、迁移兼容性 | 未开始 |
| 阶段 4 | 公开 Contract List/Detail、LinkDocumentRevision、ApplyCandidate、Sign；与 DM/DI 公开 API 和事件集成 | API 安全/契约测试、事件 Schema、审计追溯、OpenAPI 差异检测 | 未开始 |
| 阶段 5（受门禁前端 WP） | 按 Astryx 基线重做 PR #19 的受控前端工作包：仅展示层，等待 Owner API 发布 | `src/api.ts`/`src/contracts.ts` 零语义变更、无页面级全局 CSS、全部前端门禁 + Chromium 证据；Contract Apply/内容预览 API 未发布则保持 `BLOCKED` | 未开始 |
| 阶段 6（收口） | 脱敏样本 E2E、全部门禁、文档同步、独立验收评审、计划状态迁移 | 完成定义 §12 全部满足；归档与否由评审决定 | 未开始 |

**依赖规则**：PLAN-0014 R2 准入不是本计划前置条件；PLAN-0006 必须继续 BLOCKED；Gate-0 是阶段 2–6 的硬门禁；阶段 5 依赖阶段 4 的 Owner 契约发布。

## 12. 回滚与完成定义

### 12.1 回滚

- 迁移先兼容扩展、再迁数据、再切换读取、最后清理；禁止修改历史迁移文件；保留旧读路径与兼容窗口。
- 提供按用例关闭的开关/路由下线能力：关闭新增 Contract 端点或前端入口不影响既有 Document/Processing 行为；已写入数据不物理删除。
- 已发布事件不可撤回：出错以新事件与新业务动作补偿（拒绝/重新应用/新版本），Dead Letter 不自动成为终态；保留 Candidate、Evidence、Review 与审计事实。
- 回滚需同时回滚 OpenAPI/Schema、权限绑定、前端入口与 Runbook；回滚后重跑契约与安全门禁。

### 12.2 完成定义（DoD）

- [ ] Gate-0 主干 CI 全绿且有 run ID 证据；所有 Rust 门禁与架构 Fitness 全绿（无 `NOT RUN`/`BLOCKED` 遗留）。
- [ ] 固定闭环六步 E2E 通过，首轮脱敏纯文本/Markdown/JSON 样本与负例均有结果记录。
- [ ] 五项验收（租户隔离、幂等、版本冲突、已签版本保护、审计追溯）有可复现测试与证据。
- [ ] API/Event、OpenAPI、Schema、错误码、权限键、迁移清单与本文同步；需要 ADR 的变更已接受。
- [ ] 前端受门禁工作包完成且 Astryx 门禁与 Chromium 证据齐备；或明确标记 `BLOCKED` 且不宣称合同 E2E 完成。
- [ ] 独立评审者接受，计划状态才可从 Proposed 迁移；**本文件当前不表示已验收、已激活或可合并实现**。

## 13. 风险与开放问题

**风险**

1. Gate-0 CI 失败可能暴露基础设施/安全债（容器初始化、依赖漏洞），修复时间与所有权不在本计划内。
2. 权限键虽已目录化但为 reserved，缺少消费用例与 RoleBinding；点亮错误会造成越权或全量拒绝。
3. Contract Apply 与 Document Content Read/Preview API 当前被阻断（PR #19 审阅事实），阶段 4/5 可能被推迟。
4. 现有确定性抽取只产出标题/行数等有限字段与行级证据，可能不足以支撑真实合同字段应用；需评审最小字段集与证据阈值。
5. 前端候选方案早于 Astryx 采纳，若直接合并会与已归档 PLAN-0016 编号冲突并引入重复设计系统依赖。
6. 首轮样本脱敏质量与代表性不足会削弱 E2E 说服力；不得用伪造数据宣称 PASS。

**需要我（lead）或独立评审者回答的开放问题**

1. 合同正式字段的最小集合（合同列表/详情展示字段、可应用字段）由谁冻结？金额/日期/相对方字段是否进入首轮？
2. “已签署”由哪个命令/角色/审批结果触发？是否允许通过补充协议生成新版本（而不改已签版本）？是否需要新权限键或 ADR？
3. `contract.*` / `document.*` reserved 权限键是否点亮、绑定到哪些角色与 ResourceScope？谁负责 Policy 数据迁移？
4. 跨上下文闭环的首轮协调是否使用完整 Process Manager，还是应用用例内的轻量编排 + Outbox？持久化位置与超时阈值？
5. §8 中性能/RPO/RTO 阈值只是建议值，是否按测试环境实测校准并由 Owner 确认？
6. PR #19 如何处置：将其前端候选内容改为并入 PLAN-0017 的受门禁工作包，还是另取未占用编号并标 Superseded/Proposed？谁担任独立评审者？
7. Document Content Read/Preview 与 Contract Apply 两个被阻断 API 的 Owner 与交付计划由谁承诺？在它们发布前是否允许阶段 5 的离线 PoC？

> 2026-11-19：仅创建文档计划，未写 Rust/前端代码，未修改公开契约、权限、数据库、部署或 CI 配置，未 push，未激活实现。
