# PR #19 合同前端候选方案重评估（Astryx 基线与编号冲突）

> 文档类型：Review（事实、结论与建议；不修改规范，不授权编码）
> 状态：Completed / Documentation-only recheck
> 日期：2026-11-19
> 对象：[PR #19 `docs: Web Office, frontend candidates, and Contract P0 alignment`](https://github.com/jesenzhang/business-platform/pull/19)，head `698a8d5ae571d8e0801d181d41cbee69f327f2b3`，base `main`，状态 OPEN
> 关联：[PLAN-0017 合同业务垂直切片](../plans/current/PLAN-0017-contract-business-vertical-slice.md)、[已归档 PLAN-0016（Astryx）](../plans/archive/2026/PLAN-0016-astryx-frontend-design-system-adoption.md)、[ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[前端设计系统标准](../standards/FRONTEND_DESIGN_SYSTEM_STANDARD.md)、[PR #19 P0 就绪审阅（原 PR 分支）](https://github.com/jesenzhang/business-platform/pull/19)

## 1. 结论摘要

1. PR #19 是**文档型提案**，其自述与文件清单均表明：只新增/修改 Markdown，无依赖安装、无代码、无公开 API/ADR 变更；其中 `docs/plans/current/PLAN-0016-contract-frontend-p0-validation.md` 明确为 **Proposed / NOT ACTIVE**，不构成实施授权。
2. 该前端候选子计划占用 **PLAN-0016** 编号，与 2026-11-18 已完成并归档的 **PLAN-0016 Astryx 前端设计系统采纳**冲突。按 `DOCUMENT_MANAGEMENT.md` §6.2「编号一经使用不得复用」的原则与 §2.4「不静默覆盖历史」（§6.3 仅规定 Plan 命名格式），编号一经使用不得复用；原提案写于 2026-10-08，早于 Astryx 采纳，必须在当前基线下重评估。
3. 建议：将其**并入 PLAN-0017 作为受门禁的前端工作包**（保留其 WP0/WP1/WP2 的“离线 PoC + 演示层”定位，WP3 保持阻断），或另取一个未占用编号并明确标注 Proposed/NOT ACTIVE 与 Superseded 关系；不得原样以 PLAN-0016 合并。
4. Astryx 基线下的硬边界：`src/api.ts`、`src/contracts.ts` 语义、请求头、幂等键、乐观锁版本、提交载荷、确认流程、权限要求与数据结构一律不改；不得引入页面级全局 CSS；不得建立第二套主题状态或组件包装层。
5. PR #19 已如实记录的关键阻断——**Contract Apply 与文档内容预览（Content Read/Preview）API 尚未落地**——在本次复核中仍然成立；相关前端能力必须标记 `BLOCKED / NOT RUN`，不得标 PASS。

## 2. 证据与事实核对

- PR #19 文件清单：`docs/reference/WEB_OFFICE_TECHNOLOGY_EVALUATION.md`、`docs/reference/FRONTEND_COMPONENT_CANDIDATES.md`、`docs/reviews/2026-10-08-contract-frontend-p0-readiness.md`、`docs/plans/current/PLAN-0016-contract-frontend-p0-validation.md`，以及 `docs/reference/README.md`、`docs/plans/README.md` 索引改动；PR body 明确 “Documentation only; no dependencies installed, no production candidate accepted, no code or public API/ADR changed”。
- 候选子计划前置写的是 `main @ 505d2039f5ba46f0d3023e3eaa31fb7891fd6580`（2026-10-08）；当前工作基线为 `e3d7c30`，期间已完成 Astryx S0–S5（PLAN-0016 归档，2026-11-18）。
- 当前 `docs/plans/archive/2026/PLAN-0016-astryx-frontend-design-system-adoption.md` 与 `docs/plans/README.md` 第 46 行均登记该编号；本工作区 `docs/plans/current/` 无 PR #19 的候选文件，未产生本地合并。
- 原 P0 审阅结论仍有效：`crates/contract/src/lib.rs` 当时仅有 TODO（本基线仍为 TODO）；`apps/business-api/src/routes/documents.rs` 无公开原始文件/预览内容接口；Contract Application/API、`ApplyCandidate → ContractVersion` 未实现；Candidate Review 接受/拒绝不等同合同版本更新。
- 抽取能力边界：`crates/document-processing/src/domain/pipeline.rs` 仅接受 `text/plain`、`text/markdown`、`application/json`，严格 UTF-8；Office/PDF 在该固定流水线为 unsupported，不能因上传成功推断抽取可用。
- 主干 CI：最近一次 main run `44310190041`（2026-11-18）失败作业为 `PostgreSQL + MinIO + E2E contracts`（Initialize containers）与 `Security scanning`（`cargo audit --deny warnings`）；PR #19 body 也写明 “CI must pass before considering merge… do not force merge”。因此 PR #19 的合并与任何实现激活都受 P0 CI 全绿门禁约束。

## 3. Astryx 基线下的逐项重评估

| PR #19 候选项 | 原定位 | 当前基线下的处理 |
|---|---|---|
| shadcn/ui + Design Tokens、Radix 组件薄封装 | PoC/Adapt | **需重评**：S4 已把全部业务页面迁移到 Astryx，不得引入第二套设计系统或组件包装层；只允许有直接使用者的“业务语义 → 组件色调/文案”映射，且不得改 `src/contracts.ts` |
| React Hook Form + Zod | PoC/Adapt | 仅在现有页面做等价行为回归；不得改请求头、幂等键、`expected_version`、提交载荷、确认流程、权限要求或数据结构；服务端校验仍是权威 |
| Sonner / Lucide | PoC/Adopt | 允许作为反馈/图标的局部替换，但错误提示只展示安全摘要与 request-id，不泄漏正文、Key、Secret 或 provider 错误 |
| TanStack Table/Query | Reuse | 维持复用，不新增同类包；server-side cursor/filter 以 Owner 契约为准，UI 不伪造全量数据；Astryx 组件用于表现层 |
| Open File Viewer / FilePreview | 离线 PoC；正式接入阻断 | **保持 BLOCKED**：仅允许脱敏本地 `File/Blob` 的隔离 PoC；无受权 Document Content Read/Preview API 时不得写伪 API、不得拼 S3 URL、不得上线预览入口 |
| Uppy / 分片续传 | Conditional / Defer | 默认 Defer；当前服务端仅单文件 10 MiB，Tus/分片需另行后端契约设计 |
| ONLYOFFICE / Univer / GenOffice / Collabora | Separate PoC | 维持延后与独立调研/ADR；不得把 Office 保真或在线编辑当作已实现能力 |
| i18n / Virtual / Agent UI / 低代码 | Foundation / Defer | 维持 Defer；不得借合同切片引入通用 Runtime 或新部署单元 |

统一约束（AGENTS.md §17、ADR-0027、STD-FRONTEND-001）：Tailwind 3 与固定层序 `reset → tw-preflight → astryx-base → astryx-theme` 保留；主题唯一来源为 Zustand `theme` + `localStorage["business-console-theme"]`；`src/styles.css` 只允许外壳类（由 `tests/styles.test.ts` 强制），新增页面直接使用 Astryx 组件，**不得引入页面级全局 CSS**；不得引入 Astryx Tailwind v4 桥；字体自托管。UI 变更必须执行 `npm run lint`、`npm run typecheck`、`npm test`、`npm run build`、`npm run test:e2e`、`npx astryx doctor` 并附真实 Chromium 截图与交互证据。

## 4. 编号冲突处置建议

1. **首选**：把 PR #19 中仍有价值的两部分——`docs/reference/*` 调研与 `docs/reviews/2026-10-08-contract-frontend-p0-readiness.md` 事实记录——作为 Reference/Review 保留；将前端验证内容改写为 [PLAN-0017](../plans/current/PLAN-0017-contract-business-vertical-slice.md) 的阶段 5「受门禁前端工作包」，删除其 `PLAN-0016` 标识，避免编号复用。
2. **备选**：若需要独立前端计划，另取未占用编号（当前已确认 0017 被本计划使用，下一个可用编号需在合并前再次核验），状态保持 Proposed / NOT ACTIVE，并在文档中标注与已归档 Astryx PLAN-0016 的关系。
3. 两种处置都必须在 `docs/plans/README.md` 保留已归档 Astryx PLAN-0016 条目不动；不得静默覆盖历史（DOCUMENT_MANAGEMENT §2.4）。

## 5. 阻断事实的如实记录

- **Contract Apply 阻断**：Contract Application/API 与 `ApplyCandidate → ContractVersion` 命令尚未实现（`crates/contract/src/lib.rs` 仍为 TODO）；前端不得伪造提交或把 Candidate Review 接受当作合同更新成功。
- **内容预览阻断**：未发现公开的、租户受权的 Document Content Read/Preview API；不得暴露 S3 key、无限期签名 URL 或由前端猜路由。
- **抽取格式阻断**：固定流水线仅支持纯文本/Markdown/JSON；Office/PDF 上传能力不等于抽取能力。
- 以上三项在 PLAN-0017 阶段 4/5 中保留为硬门禁；在 Owner 发布正式契约前，相关 UI 只做离线 PoC，并记录 `BLOCKED / NOT RUN`，不得宣称合同闭环完成。

## 6. 对合并/评审的建议（不给实施授权）

- 不建议原样合并 PR #19：需先解决 PLAN-0016 编号冲突，并按 Astryx 基线删改组件方案中与“不新增包装层、不改业务契约、无页面级全局 CSS”冲突的表述。
- 文档型 PR 的合并本身不等于计划激活；合并后 PLAN-0014 仍 Active、PLAN-0006 仍 BLOCKED、PLAN-0017 仍 Proposed / NOT ACTIVE。
- 合并前置：P0 主干 CI 全绿；CI 未绿时不 force merge，不启动任何前端依赖安装或代码分支。
- 独立评审者应确认：编号处置方式、前端 WP 归属、被阻断 API 的 Owner 承诺，以及 Astryx 证据（gates + Chromium）是否满足 UI 变更要求。

> 2026-11-19：本次仅做文档复核，未修改 PR #19、未合并、未激活实现、未写 Rust/前端代码。
