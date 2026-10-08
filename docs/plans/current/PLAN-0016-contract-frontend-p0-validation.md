# PLAN-0016 — Contract Vertical Slice: P0 Frontend Component Validation

> 文档 ID：PLAN-0016
> 版本：0.1（候选）
> 状态：Proposed / NOT ACTIVE
> 日期：2026-10-08
> 责任：Business Console Frontend / Contract Module Owner / Document Management Owner
> 前置条件：正式 Contract Business Vertical Slice 计划的契约与优先级评审；当前主线 `505d2039f5ba46f0d3023e3eaa31fb7891fd6580`
> 关联：[P0 就绪审阅](../../reviews/2026-10-08-contract-frontend-p0-readiness.md)、[前端候选参考](../../reference/FRONTEND_COMPONENT_CANDIDATES.md)、[Web Office/文件预览调研](../../reference/WEB_OFFICE_TECHNOLOGY_EVALUATION.md)
> 与 PLAN-0014 关系：可并行进行小范围 UI PoC，但**不抢占** PLAN-0014 的平台治理与门禁工作；本计划无自动实施授权

## 1. 目标 / 非目标

**目标：** 为真实 Contract Business Vertical Slice 验证可复用的前端 P0 能力，以既有业务 Console 为载体验证组件可用性、可访问性、授权边界及性能；把组件采用决策和合同业务服务端 API 的交付节奏分开。

**非目标：** 一次性安装全部候选；全面重做 Console；迁移 Next.js/Tailwind 主版本；改写既有 IAM 授权；上线通用 Form Builder、工作流设计器、协同编辑器、Agent Runtime/Generative UI；补全尚未落地的 Contract 领域模型；在只有模拟数据时宣称合同业务链路已完成。

## 2. 基线与依赖

1. 已有：React 19 + Vite 7、Radix Dialog、TanStack Query/Table、ECharts、Zustand、Tailwind 3、Vitest/Playwright；合同/文档相关现有页面见 `apps/business-console/src/pages.tsx`，当前 CLI/FrontEnd/API 不可隐式改变。
2. 已有：Document 列表、上传（10 MiB；PDF/TXT/DOC/DOCX）、详情元数据；Processing/Review 的候选查询与接受/拒绝。
3. **缺失/阻断：** Contract Application/API（`crates/contract/src/lib.rs` 为 TODO），Document Content Read/Preview 受权 API（`routes/documents.rs` 尚无），正式 `ApplyCandidate → ContractVersion` 的应用命令；Office/PDF 的 ExtractText 仍受固定处理流水线输入限制。
4. 所有者不变：Contract Management 正式字段/版本；Document Management 文档修订/访问授权；Document Intelligence 证据/候选；Policy/IAM 授权；Durable Task Execution 执行状态。前端不得通过直接操作对象存储绕过应用边界。

## 3. 组件范围：立即 PoC / 按门禁 / 延后

| 组件/能力 | 此计划动作 | 决策条件 | 需要的外部依赖 |
|---|---|---|---|
| shadcn/ui（Radix 兼容组件）+ Design Tokens | `PoC / Adapt`：仅现有 IAM 和 Document 页面局部验证，复用当前 Radix | 无障碍、暗色模式、样式污染、React19/现有 Tailwind 兼容通过 | 无新业务 API |
| React Hook Form + Zod | `PoC / Adapt`：1 个 IAM 表单 + 1 个 Candidate Review 输入用例 | 双端校验语义、异步错误映射、旧行为/E2E 回归 | 现有 API |
| Sonner、Lucide | `PoC / Adopt`：反馈、错误提示、图标替换小切片 | 不泄漏服务端细节、屏幕阅读器可用、UI 一致性 | 无 |
| TanStack Table/Query | `Reuse`：整理 DataTable/FilterBar 边界，不新增同类包 | 服务端分页/filter Query 契约明确；小列表不使用 Virtual | 已有 Query；Contract API 待发布 |
| Open File Viewer | `PoC / Conditional`：脱敏离线 File/Blob、CSS/懒加载、PDF/DOCX/XLSX/PPTX 验证 | 安全/CSP、文件类型/大小、排版保真及降级显式 | 正式页面接入需新 Document Read/Preview API |
| Uppy | `Conditional / Defer default`：真实批量/进度诉求出现才验证 | 当前单文件上传是否足够；是否能仅复用现有 API | Tus/分片上传须另设计后端契约；当前不支持 |
| react-i18next | `Foundation only`：整理翻译 key/本地化策略；是否安装按 UI 交付要求确认 | 国际化、金额/时区/日期需求实证与测试 | 翻译资源/语言目标 |
| TanStack Virtual | `Defer` | 真实审计/合同列表性能瓶颈和无障碍证据 | 现有 Table |
| ONLYOFFICE / Univer / GenOffice / Collabora | `Separate PoC` | 复杂 Office 排版/在线编辑、许可与正式版本写回 | Office/Document 独立调研和 Proposed ADR（必要时） |
| Agent UI / json-render / React Flow / JSON Forms / 富文本 | `Defer` | Contract 切片结束、PLAN-0006 激活或对应业务能力需求明确 | 现有 Workspace/Module UI Contribution 架构 |

## 4. 工作包与交付物（只有激活后才编码）

### WP0 — Baseline & 对照样本（入场门禁）

- 固定目标 `main` commit 与 `npm package-lock`；确认当前 Console 的 API/E2E、IAM 操作、Document/Processing 行为及已有异常分支。
- 收集 **脱敏** 的 DOCX/XLSX/PDF/PPTX 及合同元数据样本；保护修订版本与对象所有权，测试无跨租户文件访问。
- 固定页面：1 个 IAM form、1 个 Documents List/Upload、1 个 Candidate Review。记录现有截图、HTML/a11y 状态、关键交互。
- 校准计划编号/Contract 主计划引用；若 Contract 主计划尚未存在，本计划仅允许隔离 PoC，不允许对外承诺合同完整切片。

**DoD：** baseline 和测试样本、现状缺口、影响文件清单进入 Review；未验证的外部能力标 NOT RUN。

### WP1 — UI Primitives / Form / Feedback（无需新后端 API）

- 试用 Radix-compatible shadcn/ui 组件，最小化 Token 层，局部替换页面控件；保留现有 URL/Route/Query 与 Theme 行为。
- 对一个 IAM 表单使用 React Hook Form + Zod 校验，同时保留原服务端 validation/expected_version、默认拒绝与审计逻辑。
- 对 Candidate Review 输入、disabled/pending/error/conflict/duplicate response 做等价行为回归；以 Typed Props 封装薄组件而不是创建新通用表单运行时。
- 可选 Sonner/Lucide，在敏感错误提示处只展示安全摘要和关联 request-id，不展示 provider/storage secret。

**DoD：** 对照用例在 keyboard/ARIA/Theme/mobile 下无倒退；授权与审阅 API 负例 PASS；新增包 licenses/NOTICE/版本记录完整。

### WP2 — 文件预览 / 上传独立 PoC（与 Business API 分离）

- 使用 Open File Viewer 在离线 Test Harness 或已有前端 Client-only 测试视图中读取本地 `File/Blob`，测试 PDF/图片/DOCX/XLSX/PPTX；记录复杂合同的页级差异、文本缺失/降级和内存/首帧时延。
- 评估后端**是否存在**授权安全的内容查询与传输契约；在当前未提供时仅留禁用的业务预览入口/显式待办，不写伪 API、不直接拼 S3 URL。
- 使用现有单文件上传接口验证错误、取消（仅浏览器请求层）、10 MiB 限制和幂等；只有确定需要批量/恢复上传时才扩展到 Uppy。
- 非受信 OOXML/URL/CSS/HTML 的跨域、CSP、XSS 防护、对象 URL cleanup、浏览器 Worker 资源约束必须测。

**DoD：** 离线预览 PoC 有明确 PASS/FAIL、对照截图、格式支持边界、许可风险；正式 FilePreview 按 API 门禁阻断。

### WP3 — Contract Slice 业务接入（取决于 Contract Owner 契约）

仅当独立 Contract Business Vertical Slice 主计划批准并交付以下 published API/commands 后才进入：

1. `Contract List/Detail`：租户/资源级 Policy、稳定 Read DTO、分页过滤、ContractVersion、DocumentLink；
2. `ApplyCandidate`：候选/证据与 Document revision 绑定，合同正式字段变更由 Contract Application 接受，expected_version/幂等/冲突策略与审批状态明确；
3. `Document Preview Content`：Document owner 发布受权内容访问 API、短时访问/流式读取/审计和禁止 S3 key 暴露；复杂文件格式仍需通过 Office 保真 PoC；
4. `Review/Audit`：能从具体合同版本追溯操作主体、原文内容修订、AI Candidate、证据和审阅决策。

UI 仅使用平台 Query/Command 与 typed client contract，不在 UI 中推导业务规则；高风险动作执行前必须准备计划、预览差异、确认和提交后版本校验，防重复和 stale-version 写入。

**DoD：** Contract List → Document Revision → Extraction/Evidence → Human Review → Apply Candidate → Contract Version Update 真实 E2E；无权限访问/过期授权/旧版本提交/重复请求必须 fail closed，不能把 Review accepted 当作 Contract 更新成功。

## 5. 质量、测试与验收

### 前端固定命令（激活后的代码分支）

在 `apps/business-console` 目录：

```bash
npm ci
npm run lint
npm run typecheck
npm run test
npm run build
npm run test:e2e
```

运行 Playwright 前须配置真实或确定性的授权 API/fixture；浏览器缺失或无测试服务时记录 `BLOCKED/NOT RUN`，不得宣称 PASS。

### 新增验证证据

- 单测：Schema、错误映射、失效状态、可访问性与常规交互；Upload/Preview 失败显示与资源清理。
- E2E：权限不足不加载合同/文档正文；表单版本冲突、重复提交、拒绝/接受候选不伪造业务状态。
- 对照：真实文件截图及内容缺失；JS 首包与按需 Chunk 记录；大文件峰值内存、主线程耗时、首屏时间（PoC 先记录基线，再为正式验收设阈值）。
- 外部依赖：license/SBOM、安全扫描、CSP/XSS 负例、版本与 npm lockfile 同步。
- 后端 API/存储/业务代码若被修改，额外执行仓库规定的 Rust fmt/check/clippy/tests、Architecture Fitness、PostgreSQL/MinIO 契约与恢复门禁。

### 准入判定

- **PASS** 仅限经测试完成的具体能力；候选项目的 Stars/README 演示或源码存在不能作为保真/安全证据。
- **BLOCKED**：未提供 Document Preview Content 或 Contract Apply Command 时，相关 UI 不得标记可用。
- **FAIL**：跨租户泄漏、绕过授权、覆盖签署合同、候选直接写正式数据、不可追溯更新必须阻断集成。
- 独立 Reviewer 评估 P0 组件重复依赖、升级成本、许可、可访问性和产品范围后，再决定 `Adopt/Adapt/Defer`。

## 6. 风险与回滚

- 外部组件升级破坏 React/Tailwind/SSR：只在隔离分支局部安装，npm lockfile 与组件代码版本可回退；使用薄适配器降低绑定面。
- 富文本文档/OXML 浏览器安全与 Office 排版误差：显示“非高保真”的可用性提示，必要时使用专业 Office 引擎；不自动覆盖正式版本。
- 重复上传/误导性的续传体验：原接口继续保持单文件 10 MiB；Uppy 仅在真实协议支持后开放功能。
- UI 权限提示与 Rust Policy 不一致：服务器永远为唯一授权权威；隐藏/禁用按钮仅 UX，不得修改权限结果。
- 合同域能力尚未交付：WP1/WP2 可独立撤回，WP3 被阻断；不发布“Contract E2E 完成”声明。

## 7. 完成定义与计划治理

1. P0 组件逐项记录候选/采用/拒绝与依据；P1/P2 仍留 Reference。
2. 至少选定的 IAM 表单、文档列表、候选审阅三个 UI 交互有真实回归证据。
3. 文件预览 PoC 有脱敏附件截图、安全、错误、性能与许可证记录；未交付 Document Preview API 时不承诺正式内容预览。
4. 如果 Contract Owner 公共契约已经交付，则 WP3 全链路真实 E2E；否则标 `BLOCKED` 且本计划不得声明整个 Contract Slice 已完成。
5. 架构/公开 Contract 若变化，先由对应 Owner 制定/接受 ADR（如需要）并同步 Baseline，再激活代码和测试任务。
6. 执行与计划顺序仍遵循 [docs/plans/README](../README.md)：PLAN-0014 Active，Contract Business Vertical Slice 是下一主要业务代码计划，本 PLAN 仅是子计划/预检。

> 2026-10-08：仅创建 Proposed 计划文档，没有批准实施或更改代码、依赖、API、数据库、业务模块、部署单元。
