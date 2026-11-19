# PLAN-0017 独立评审报告（合同业务垂直切片）

> 文档类型：Independent Review（事实核对与评审意见；不修改 PLAN-0017，不授权编码/合并/激活）
> 评审日期：2026-11-19
> 评审者角色：独立评审者（非计划作者）
> 评审分支：`codex/contract-plan-review` @ `73c5caa5f17ffe104d3d3690d43e76621284f80b`（工作树 `E:\working\business-platform-wt\contract-plan-review`，未改主工作区，未 push）
> 评审对象：
> - `docs/plans/current/PLAN-0017-contract-business-vertical-slice.md`（248 行，Proposed / NOT ACTIVE）
> - `docs/reviews/2026-11-19-pr19-contract-frontend-recheck.md`（61 行）
> - `docs/plans/README.md` 索引改动（PLAN-0017 条目 + PLAN-0013 损坏行修复）
> 权威顺序：`AGENTS.md` §1（用户明确指令 → 已接受 ADR → 架构 Manifest/正文/Baseline → standards → 计划 → 实现）

## 1. Verdict

**PASS WITH COMMENTS（评审通过；计划保持 Proposed / NOT ACTIVE）**

- 未发现越界所有权、把 Candidate/Job Completed 冒充业务事实、跨上下文共享事务、与 Astryx 基线冲突、编号冲突、Proposed 写成已授权、或缺失 AGENTS.md §13 必备要素的问题。
- 共 7 条发现：1 条 C2（中等，基线元数据事实错误），6 条 C3（轻微，措辞/引用精度/可追溯性改进）。**全部不阻塞「评审通过但计划仍 NOT ACTIVE」的结论**；计划中的实现门禁（Gate-0 CI 全绿、Gate-1 Owner/独立评审冻结）依然有效。
- 结论口径：本报告只评价"计划是否可执行、边界是否正确、门禁是否诚实"；未把尚未实现的未来代码证据当作计划缺陷，也未要求与用户明确指令冲突的变更（首个闭环、PR #19 重评、不等 R2 准入、脱敏纯文本/Markdown/JSON 样本均按用户指令核对通过）。

严重度定义：C0 = 关键/阻塞；C1 = 重大（通常阻塞）；C2 = 中等（不阻塞，评审冻结前建议修订）；C3 = 轻微（可选改进）。

## 2. 核对过的事实与证据

以下均为实际读取仓库/gh 所得，非仅读计划自述：

### 2.1 权威架构与标准

| 核对项 | 证据 | 结论 |
|---|---|---|
| 合同正式字段与版本归 Contract Management；已签版本不可覆盖；文件通过 DocumentLink/明确 Revision 引用 | `docs/architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md:195-210` | 计划 §3.1/§5.1/§6.4 一致 |
| Context 不拥有项（Contract 不拥有 OCR/租约/文件字节；DI 不拥有正式字段；AI 输出始终是候选） | `docs/architecture/BOUNDED_CONTEXT_MAP.md:207-237,305-359` | 计划 §3/§3.2 一致 |
| DI 拥有 ProcessingJob/候选/审阅，DTE 只提供 lease/fence/retry 机制；本地事务 + Outbox；PG 多 Worker 权威、SQLite `BEGIN IMMEDIATE` 单进程 | `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md:206-216`；`AGENTS.md` §15 | 计划 §3.1/§4/§6.3/§6.4 一致 |
| 事件 Envelope 必备字段、事件过去时命名（含 `ContractFieldsApplied`）、`document.processing.*.v1`、§25 固定 processing-jobs/review 幂等与候选版本语义 | `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md:223-233,246-264,356-378` | 计划 §5.2/§5.1 一致 |
| 高风险写 Prepare → Preview → Confirm → Execute 及绑定项；§20 威胁建模流程 | `docs/architecture/SECURITY_ARCHITECTURE.md:116-135,304-317` | 计划 §7 一致（剩余风险可追溯性见 F4） |
| Tailwind 3 层序、主题唯一来源、无包装层、`styles.css` 仅外壳类、无页面级全局 CSS、门禁命令 | `docs/standards/FRONTEND_DESIGN_SYSTEM_STANDARD.md:22-30,33-50,108-109`；`AGENTS.md` §17 | 重评 §3/计划 §2.2/§5/§11 阶段 5 一致 |
| ADR/Plan 编号不复用、"不静默覆盖历史" | `docs/governance/DOCUMENT_MANAGEMENT.md:33-35,160-174` | 编号结论成立（引用精度见 F2） |

### 2.2 代码事实

| 核对项 | 证据 | 结论 |
|---|---|---|
| `crates/contract/src/lib.rs` 仅 TODO 骨架（7 行） | `crates/contract/src/lib.rs:1-7` | 计划 §4/§13 风险 3、重评 §2/§5 属实 |
| 固定流水线仅 `text/plain` / `text/markdown` / `application/json`，严格 UTF-8，超长/非法编码/旧 revision 各自报错 | `crates/document-processing/src/domain/pipeline.rs:19-56,181-192` | 计划 §2.2 与风险 4 属实；Office/PDF 为 unsupported |
| 确定性抽取只产 title/line_count/character_count 与行级证据 | `crates/document-processing/src/domain/pipeline.rs:81-142` | 计划 §13 风险 4 属实，未过度承诺真实合同字段 |
| 7 个 reserved 键 `document.read/review`、`contract.read/create/update/review/archive`；系统角色明确排除 reserved | `crates/policy/src/catalog.rs:59-91,108-118` | 计划 §7 的键名与状态属实 |
| 现有端点：`POST /api/v1/documents/{id}/processing-jobs`、`GET /api/v1/processing-jobs/{id}`、`.../candidate`（GET）、`.../review`（POST）；无文档内容读取/预览路由 | `apps/business-api/src/routes/mod.rs:50-66`；`apps/business-api/src/routes/documents.rs:138-154` | 计划 §5.1 下游端点属实；重评"内容预览阻断"属实 |

### 2.3 gh / Git 事实

- `gh pr view 19 --repo jesenzhang/business-platform`：PR #19 **OPEN**，title `docs: Web Office, frontend candidates, and Contract P0 alignment`，head `698a8d5ae571d8e0801d181d41cbee69f327f2b3`，base `main`；6 个文件（4 新增 + `docs/plans/README.md`、`docs/reference/README.md` 索引改动）；body 自述 "Documentation only; no dependencies installed… no code or public API/ADR changed" 与 "CI must pass before considering merge… do not force merge"。与重评 §1/§2 一致。
- PR #19 候选子计划头部（经 `gh api .../contents/...?ref=698a8d5` 读取）：`PLAN-0016`、`Proposed / NOT ACTIVE`、前置 `main @ 505d2039f5ba46f0d3023e3eaa31fb7891fd6580`。与重评 §2/§4 一致。
- `gh run view 44310190041`：main CI（2026-11-18T13:05:27Z）**失败**；10 作业中 `PostgreSQL + MinIO + E2E contracts`（`Initialize containers`，annotation 为 Docker pull failed）与 `Security scanning`（`cargo audit --deny warnings`）失败，其余 8 作业成功；`gh run list --branch main` 确认它是截至评审时最新 main run。与计划 §11 Gate-0、重评 §2/§6 一致。
- 编号：仓库内 PLAN-0001…PLAN-0017 各一份；`docs/plans/archive/2026/PLAN-0016-astryx-frontend-design-system-adoption.md` 头部为 Completed / Archived（2026-11-18），`docs/plans/current/` 无 PLAN-0016 文件；README:46 登记该归档条目。0017 未被占用，0016 已被占用且不可复用——计划与重评的编号结论成立。
- README 恢复行事实：`1990a33`、`d3a9196`（Merge PR #17）、`docs/reports/PLAN-0013-COMPLETION-AUDIT.md` 均存在；`gh run view 35418654378`、`35426688873` 均 success。PLAN-0013 损坏行修复内容是准确的，未引入假事实。

## 3. 逐条发现

### F1 — C2（中等，不阻塞）｜计划头部基线分支 SHA 与分支实际 HEAD 不符

- 位置：`PLAN-0017...md:8`。
- 断言：`计划基线：codex/contract-vertical-slice-plan @ e3d7c30（基于 main e3d7c30）`。
- 事实：`git rev-parse codex/contract-vertical-slice-plan` = `73c5caa5f17ffe104d3d3690d43e76621284f80b`（即写入本计划的提交本身）；`git merge-base main codex/contract-vertical-slice-plan` = `e3d7c30f4be5b68bc32ac15740cd7d14e3395b04`；`git rev-parse main` = 同一 `e3d7c30`。`e3d7c30` 是 merge-base/main HEAD，不是该分支当前 HEAD。
- 影响：不改动任何边界或门禁，但基线元数据在提交时即自相矛盾；本仓库历史评审把基线 SHA 错误视为可撤销候选的实质问题（参见 `docs/reviews/2026-08-11-plan-0010-accepted-candidate.md:18-27`）。
- 建议替换第 8 行为：
  `> 计划基线：main @ e3d7c30（GitHub main HEAD；CI run 44310190041 未全绿）；计划分支 codex/contract-vertical-slice-plan 由该提交分出，本计划文档提交 73c5caa`
- 是否阻塞：**否**。

### F2 — C3（轻微，不阻塞）｜PR #19 重评对 DOCUMENT_MANAGEMENT 条款的引用精度

- 位置：`2026-11-19-pr19-contract-frontend-recheck.md:12`。
- 断言："按 `DOCUMENT_MANAGEMENT.md` §6.2/§6.3，编号一经使用不得复用"。
- 事实："编号一经使用不得复用"字面位于 §6.2 ADR 小节（`DOCUMENT_MANAGEMENT.md:167`）；§6.3 Plan 小节（`:169-174`）只规定命名格式，未写不复用；Plan 编号不可复用需由 §2.4「不静默覆盖历史」（`:33-35`）与 §6.2 原则共同支撑。编号冲突的实质结论仍正确。
- 建议替换该从句为：
  `按 DOCUMENT_MANAGEMENT.md §6.2「编号一经使用不得复用」的原则与 §2.4「不静默覆盖历史」（§6.3 仅规定 Plan 命名格式），编号一经使用不得复用`
- 是否阻塞：**否**。

### F3 — C3（轻微，不阻塞）｜§8 可用性验收中"确认令牌不过期失效"语义含糊

- 位置：`PLAN-0017...md:166`。
- 原文：`故障注入下确认令牌不过期失效、不绕过权限`。字面可解释为"令牌过期也不失效"，与 §3.2/§7/SECURITY_ARCHITECTURE:135 的"确认过期必须重新准备"相反；测试实现可能据此写错断言。
- 建议替换为该行整句：
  `| 可用性/故障隔离 | DI Worker 停摆时 Contract list/detail/已存版本查询仍可用；ProcessingJob 状态查询不依赖业务写入；故障注入下过期确认令牌仍被拒绝、目标版本变化后确认失效、权限与版本校验不被绕过 |`
- 是否阻塞：**否**。

### F4 — C3（轻微，不阻塞）｜威胁建模"剩余风险进入 §13"缺少可追溯映射

- 位置：`PLAN-0017...md:152` 与 §13 风险列表（`:229-236`）。
- 事实：SECURITY_ARCHITECTURE §20（`:304-317`）要求"资产 → 信任边界 → 威胁 → 控制 → 剩余风险 → 验证"；计划 §7 已给出威胁与控制并称剩余风险进入 §13，但 §13 的 6 条风险未标注哪几条是威胁模型剩余风险，也未逐项给出验证引用。
- 建议在 §13 风险列表末尾增加一条（不改变现有编号）：
  `7. 威胁建模剩余风险（对应 §7）：跨租户 IDOR（控制：租户约束 Repository + 403/404；验证：§8 租户隔离负向矩阵）、陈旧候选/旧修订提交（控制：candidate_version + content_revision 校验；验证：CANDIDATE_STALE / CONTENT_REVISION_MISMATCH 用例）、确认令牌滥用（控制：nonce + 过期 + 版本绑定；验证：重放/过期用例）、内部 Key 泄漏（控制：DTO 脱敏；验证：§9 负向字段扫描）；冻结后随 Owner 决策更新。`
- 是否阻塞：**否**。

### F5 — C3（轻微，不阻塞）｜README 归档清单恢复行位置造成编号逆序

- 位置：`docs/plans/README.md:46-47`。PLAN-0013 损坏行被修复为独立行，但被追加在 PLAN-0016 之后（0016 → 0013），而其余归档清单为数字升序（0001…0012, 0016）。
- 事实：修复本身内容准确（见 §2.3），仅顺序问题；`docs/governance/DOCUMENT_MANAGEMENT.md` 未规定归档清单必须排序，故为 C3。
- 建议：把 `:47` 的 PLAN-0013 行移到 `:45` PLAN-0012 行与 `:46` PLAN-0016 行之间，并删除原 `:47` 行。
- 是否阻塞：**否**。

### F6 — C3（轻微，不阻塞）｜"暂不预期产生 ADR"与新增公开契约的治理表述张力

- 位置：`PLAN-0017...md:80`（§4 末条）。
- 事实：该行把"变更公开 API/事件语义"列为 ADR 触发条件，同时写"本计划暂不预期产生 ADR"；而计划 §5 引入整套新公开 Command/Event/错误码，并拟点亮 reserved 权限键。`AGENTS.md` §4 规定改变公开契约须同步 ADR；API 标准 §24（`API_AND_EVENT_CONTRACT_STANDARD.md:343-354`）规定破坏性变化或全局策略变化必须经 ADR。新增（非破坏性）契约不必然触发 ADR，故非阻塞，但"暂不预期"应说清适用边界。
- 建议替换该行为：
  `- ADR 触发条件：若评审得出“新增/合并/拆分 Context、变更数据所有权、变更公开 API/事件语义或幂等/版本规则、新增部署单元或基础设施、改变身份/租户/权限/数据分类”，必须先建 ADR；新增 Contract 公开契约在 Gate-1 冻结时按同一条件核对，若改变已发布契约语义或构成全局策略变化则创建/修订 ADR。当前计划不预期改变既有已发布契约，故暂不预期产生 ADR。`
- 是否阻塞：**否**。

### F7 — C3（轻微，不阻塞）｜完成定义中 API 层 E2E 与前端 BLOCKED 分支未显式耦合

- 位置：`PLAN-0017...md:221`（§12.2 第 2 项）与 `:224`（第 5 项）。
- 事实：第 2 项要求"固定闭环六步 E2E 通过"，第 5 项允许阶段 5 前端 `BLOCKED` 且"不宣称合同 E2E 完成"。两者未说明：当前端 BLOCKED 时，API 层六步 E2E 通过是否可勾选、以及整体 DoD 如何记录。重评 §5/§6 已把预览与 Apply 标为 BLOCKED，因此这是实际会遇到的分支。
- 建议替换两项为：
  `- [ ] 固定闭环六步 E2E 在 API/服务层通过；首轮脱敏纯文本/Markdown/JSON 样本与负例均有结果记录。`
  `- [ ] 前端受门禁工作包完成且 Astryx 门禁与 Chromium 证据齐备；若阶段 5 为 BLOCKED，则不得宣称含 UI 的合同端到端闭环完成，并在 DoD 中记录 BLOCKED 项与 Owner 承诺。`
- 是否阻塞：**否**。

## 4. 明确核查通过的关键门禁（无发现）

- **边界/所有权**：§3.1 七行所有者表与 §5/§6 的"只存引用 + Application API + 事件 + 幂等"未越界；无私有表 FK、无跨上下文 Repository、无共享可变 BusinessBase、无跨上下文写事务（§4/§6.1）。
- **Candidate / Job Completed 责任隔离**：§2.2、§3.2、§5.2、§6.4、重评 §1.5/§5 均明确"审阅通过 ≠ 合同写入"、"Job Completed ≠ 业务完成"；流水线事件负载只含引用与计数。
- **用户五项验收**：租户隔离、幂等、版本冲突、已签版本保护、审计追溯在 §8 有可测量阈值/负向矩阵，§12.2 有复现测试清单，符合"不接受高性能/高可用空话"的要求。
- **AGENTS.md §13 必备要素**：目标 Context/所有者（§3）、架构影响（§4）、API/Event（§5）、一致性与安全（§6/§7）、质量属性（§8）、Fitness Functions（§9）、文档同步（§9/§12.2）、回滚与完成定义（§12）全部具备；ADR 触发在 §4/§13 显式列出（精度见 F6）。
- **Astryx 基线**：计划与重评未要求改 `src/api.ts`/`src/contracts.ts` 语义、未引入第二设计系统/包装层/页面级 CSS/第二主题状态/Tailwind v4 桥；阶段 5 保持 BLOCKED 直到 Owner 契约发布，与 AGENTS.md §17、ADR-0027、归档 PLAN-0016、STD-FRONTEND-001 一致。
- **编号与状态**：0017 未占用、0016 不可复用已核实；全文与 README 均保持 Proposed / NOT ACTIVE / 不授权编码，无"已授权/已验收"表述；PLAN-0014 仍 Active、PLAN-0006 仍 BLOCKED，Gate-0 是阶段 2–6 硬门禁，未把 P1 并行编制写成 P0 完成。
- **PR #19 重评**：gh 事实（OPEN、head、base、6 文件、body 自述、候选子计划前置 SHA）与重评逐条一致；未建议原样以 PLAN-0016 合并，未把被阻断 API 写成可用。

## 5. lead 必须回答的开放问题是否被充分标注

**结论：是，充分标注。**

- `PLAN-0017...md:238-245` 有专门小节"需要我（lead）或独立评审者回答的开放问题"，共 7 条，覆盖：正式字段最小集与冻结人（Q1）、签署触发/补充协议/权限键或 ADR（Q2）、reserved 权限键点亮与 Policy 迁移责任（Q3）、Process Manager vs 轻量编排与持久化/超时（Q4）、性能/RPO/RTO 目标确认（Q5）、PR #19 处置与独立评审者（Q6）、被阻断 API 的 Owner 承诺与离线 PoC（Q7）。
- 正文对开放问题有回指而非孤立堆放：签署不可变（`:138` → §13）、质量属性阈值（`:155` → "待评审确认"）、Gate-1 冻结字段/版本/API/权限键/签署/SLO（`:200`）、阶段 4/5 阻断（`:204`）。重评 §6 也把编号处置、前端 WP 归属、被阻断 API Owner、Astryx 证据列为独立评审者应确认项（`:59`）。
- 因此本报告不要求把开放问题"补齐答案"作为评审通过条件；这些问题属于 Gate-1 Owner 冻结范围，保持 Proposed / NOT ACTIVE 正是诚实做法。（开放问题本身的措辞精度仅见 F3/F6/F7 的小建议。）

## 6. 评审结论与后续

- **Verdict：PASS WITH COMMENTS。计划评审通过，但 PLAN-0017 必须保持 Proposed / NOT ACTIVE；本报告不构成编码、依赖安装、API 激活、分支合并或 push 授权。**
- 7 条发现均不阻塞该结论；作者可选择在 Gate-1 Owner 评审冻结前一并吸收 F1–F7 的建议文本，其中 F1 建议在本次评审记录中同步修正，其余可在冻结版中处理。
- P0（恢复 main CI 全绿，run 44310190041 的两项失败作业）仍是阶段 2–6 的硬门禁；本报告未改变其优先级。

> 2026-11-19：本次仅做文档独立评审；未修改 PLAN-0017、PR #19、README 计划正文、架构基线、代码、权限、数据库、部署或 CI 配置；未 merge、未 push、未激活实现。
