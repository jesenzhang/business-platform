# 企业 SaaS 前端组件能力与候选项目初步调研

> 文档类型：Reference（不构成架构 Baseline/ADR/实施授权）
> 状态：Draft / 候选清单，待业务切片与 PoC 验证
> 调研日期：2026-10-08
> 适用：`apps/business-console`、未来 Business Module UI Contributions、Enterprise AI Workspace
> 关联：[Web Office / File Preview 选型](WEB_OFFICE_TECHNOLOGY_EVALUATION.md)

## 1. 核心判断与治理边界

**企业 SaaS 需要完整的前端能力，不等于所有候选库都必须安装。** 先复用现有 React/Vite 架构，再按真实业务场景补齐组件；候选排名是 PoC 优先级，不是生产采用决定。

- P0：合同垂直切片和基础管理控制台近期会复用的组件能力。优先评估，不等于自动引入依赖。
- P1：Agent Workspace、配置驱动业务模块、Wiki/流程等明确需求出现时评估。
- P2：专业复杂交互，仅有落地业务证明或技术缺口时使用。
- Adopt：经 PoC 直接采用；Adapt：通过自己的可替换封装采用；Reference：研究设计，不安装；Defer：当前不实施。
- 本文 **不改变** Accepted ADR、数据所有权、Published UI Contribution 语义或现有 PLAN 优先级。任何全局 UI SDK/API、部署边界变化另行评审。

## 2. 当前技术基线（依据仓库 main 源码）

源码：[package.json](../../apps/business-console/package.json)、[App.tsx](../../apps/business-console/src/App.tsx)、[components.tsx](../../apps/business-console/src/components.tsx)、[pages.tsx](../../apps/business-console/src/pages.tsx)；审查时主分支 Git Tree 为 `505d2039f5ba46f0d3023e3eaa31fb7891fd6580`。

| 层次 | 已安装/已实现 | 本次建议 |
|---|---|---|
| 框架与路由 | React 19、TypeScript、Vite 7、React Router 7 | 保留；不因组件选型转 Next.js |
| UI 基础 | Radix Dialog、Tailwind CSS 3、自有 AppShell/状态/弹窗 | 优先统一 Token、无障碍、对话框/抽屉规范 |
| 表格与服务端数据 | TanStack Table、TanStack Query | 复用；仅大数据时补虚拟滚动，不再安装第二套 DataGrid |
| 应用状态 | Zustand | 保留，服务端缓存继续由 Query 管理 |
| 图表 | ECharts + React 封装 | 保留，图表是展示层而非数据查询权威 |
| 测试 | Vitest、Playwright、ESLint/TypeScript | 纳入组件回归、视觉及可访问性测试 |
| 当前页面 | Dashboard、Document、Processing、Candidate Review、Audit、IAM 等 | 使用真实合同业务切片检验复用边界，不从抽象组件库独立扩建平台 |

**实际缺口（能力级，不等于已证实的产品缺陷）：** 复杂业务表单的统一校验、上传队列/进度/重试、统一文件预览、消息反馈、国际化、可复用数据表格与权限感知组件、Agent 流式交互和动态业务模块 UI。现有页面有简单表单/上传和表格，应以渐进改造替代全面重写。

## 3. P0 基础组件候选

| 能力 | 候选与来源 | 开源许可（上游仓库/主体） | 初步采用方式 | 验证重点 |
|---|---|---|---|---|
| 设计系统 / primitives | [shadcn/ui](https://github.com/shadcn-ui/ui) + 已有 [Radix](https://github.com/radix-ui/primitives) | MIT / MIT | **Adapt**：沿用 Radix 体系；平台自有 Design Tokens 与组件对外契约 | React 19、现有 CSS、主题、键盘、焦点、移动端 |
| 业务表单/验证 | [React Hook Form](https://github.com/react-hook-form/react-hook-form) + [Zod](https://github.com/colinhacks/zod) | MIT / MIT | **Adapt**：FormField、FormError、校验与异步提交的薄封装 | 服务端错误映射、租户字段、长表单、版本冲突、a11y |
| 文件上传 | [Uppy](https://github.com/transloadit/uppy) | MIT | **Adapt**：UploadQueue；复用既有 Document API，不直接改存储所有权 | 大文件、取消、并发、断线、重试；Tus/分片需兼容的服务端协议 |
| 文件查看 | [Open File Viewer](https://github.com/xushanpei/open-file-viewer) | MIT 核心；可选插件另审 | **Adapt**：`FilePreview`，只读通用附件；详见独立 [Office 调研](WEB_OFFICE_TECHNOLOGY_EVALUATION.md) | PDF/Office 真实文件保真、恶意内容、CSP、性能、版本 |
| 用户反馈 | [Sonner](https://github.com/emilkowalski/sonner) | MIT | **Adopt/Adapt**：统一异步状态、成功/失败通知；不代替错误审计 | 屏幕阅读器、错误去敏感信息、重复事件 |
| 图标 | [Lucide](https://github.com/lucide-icons/lucide) | ISC（以仓库 LICENSE 为准） | **Adopt**：统一图标语义与尺寸 | 包体积、语义按钮、授权文本 |
| 国际化 | [react-i18next](https://github.com/i18next/react-i18next) | MIT | **Adapt**：先建立词条 key、日期/金额/时区本地化策略 | 缺失翻译、复数/区域、格式化、语言切换 |
| 大表格性能 | [TanStack Virtual](https://github.com/TanStack/virtual) + 已有 TanStack Table | MIT / MIT | **条件采用**：仅万行级或明显滚动瓶颈出现时 | 键盘导航、行高、黏性列、筛选分页、屏幕阅读器 |

**注意：** shadcn/ui 是把可维护组件源码引入项目的工作流，不是无需治理的一体化成品 UI 库。已有的 Radix、TanStack Table/Query、ECharts、React Router 和 Zustand **不应重复选型**。文件预览 != 文件编辑，Uppy 的断点续传 != 当前 Rust API 已支持断点续传。

## 4. P1 与 P2：业务模块 / Agent Workspace 候选

| 组件能力 | 候选（默认仅 Reference） | 许可边界 | 触发条件与取舍 |
|---|---|---|---|
| JSON Schema 驱动表单 | [JSON Forms](https://github.com/eclipsesource/jsonforms) / [RJSF](https://github.com/rjsf-team/react-jsonschema-form) | MIT / Apache-2.0 | Business Module 的发布 Schema、可编辑字段和授权约束稳定后再验证；表单 Schema 不可成为绕过后端 API 的写入途径 |
| 富文本与 Wiki/合同条款编辑 | [Tiptap](https://github.com/ueberdosis/tiptap) / [Lexical](https://github.com/facebook/lexical) | 开源核心 MIT / MIT；Tiptap 商业扩展单独许可 | Wiki、非 Office 原生条款编辑有真实需求时比较；复杂 Word 保真仍走 Web Office 专用能力 |
| Agent Chat / Tool UI | [assistant-ui](https://github.com/assistant-ui/assistant-ui) / [CopilotKit](https://github.com/CopilotKit/CopilotKit) | MIT / MIT；官方平台/云增值另审 | 企业 AI Workspace 开始实现时，对照流式响应、工具调用、恢复、人工介入、安全契约；不让 UI SDK 决定 Agent Runtime |
| Generative UI | [json-render](https://github.com/vercel-labs/json-render) | Apache-2.0 | 白名单组件 Registry、版本化 Schema、可信渲染和授权边界设计后才允许 Agent 动态 UI；不执行任意 JSX/JS |
| 图形流程/依赖视图 | [React Flow / XYFlow](https://github.com/xyflow/xyflow) | MIT 开源核心 | 有审批视图/执行拓扑需要时展示/编辑图；前端图不拥有任务调度或业务状态 |
| 拖拽看板与排序 | [dnd-kit](https://github.com/clauderic/dnd-kit) | MIT | 字段顺序、看板、列表拖动发生明确需求时引入；要有键盘等价操作 |
| 日历 / 合同履约 | [FullCalendar](https://github.com/fullcalendar/fullcalendar) | 核心 MIT，Premium 另行许可 | 履约日期、审批期限与日历实际出现时启用 |
| JSON/代码/配置编辑 | [Monaco Editor](https://github.com/microsoft/monaco-editor) | MIT | 仅开发者控制台、复杂 JSON 工具 Schema 才引入，避免重型包进入普通页面 |

**P2 延后原则：** 未出现对应真实功能需求前，不为了“系统功能完整”预装流程编辑器、低代码布局器、协作富文本或 Monaco。

## 5. 平台自有组件契约建议（候选，不是已实现清单）

第三方组件不得直接构成业务模块公共契约；建议由平台提供可演进的薄层：

| 平台能力/候选公共组件 | 必需语义 | 权威来源与特殊约束 |
|---|---|---|
| `DataTable` / `FilterBar` | 服务端过滤、排序、游标分页、批量选择、空/加载/错误状态 | Query/Read DTO；过滤条件不能直接拼任意 SQL |
| `SchemaForm` / `ResourcePicker` | typed values、用户可见错误、租户/组织/资源选择、字段级权限展示 | 业务 Application API + Policy；UI 的禁用状态不等于授权 |
| `UploadQueue` / `FilePreview` | 临时受权文件访问、格式路由、进度/错误/取消、浏览器资源清理 | Document Management 持有内容修订；不要传播对象存储 key/永久 URL |
| `ReviewDiff` / `ApprovalPanel` | 修订差异、证据引用、确认动作、冲突与版本绑定 | Human Review / ActionPlan；`Prepare → Preview → Confirm → Execute` |
| `TaskTimeline` / `AuditViewer` | 任务状态、步骤、失败、重试和事件时间线 | Durable Execution / Audit read API；不能反向修改事实 |
| `AgentPanel` / `ArtifactViewer` | 流式消息、tool call 状态、可追溯的 Artifact 和人工介入 | Workspace/Capability；资源分类随结果继承、遵守 default-DENY |
| `UIContributionRenderer` | 受控模块 UI Contribution（Navigation、ListView、DetailSection、DetailTab、Action、Command） | 已接受的 [Business Application Platform Architecture](../architecture/BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md)；仅宿主允许的 typed slots，不注入任意可执行组件代码 |

组件和产品能力必须与 [Enterprise AI Workspace Architecture](../architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md)、[Identity and Authorization Architecture](../architecture/IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md)、[Security Architecture](../architecture/SECURITY_ARCHITECTURE.md) 一致。**Policy/Capability/数据所有权最终由 Rust 后端裁决。**

## 6. 最小验证路径（不激活新 Plan）

1. **基础体验**：在现有 IAM 页面挑一个表单试用 Radix/shadcn + React Hook Form/Zod、通知/图标与 i18n；验证键盘、焦点、错误和深浅色主题，不要求替换全部页面。
2. **合同切片**：复用现有文档上传和列表 API 验证 UploadQueue、FilePreview、DataTable 的协作；批量/断点上传仅在后端支持时开启；与 Office 调研共用脱敏文件样本。
3. **管理与证据**：以真实 Candidate Review、Audit、Policy Explain 证明 `ReviewDiff` / `AuditViewer` 的数据和授权接口；无相应 API 时先做设计，不假造接口。
4. **Agent / 动态 UI**：等待 Contract Business Vertical Slice 与 Enterprise AI Workspace 的前置门禁，选 assistant-ui/CopilotKit 各做同输入 PoC，验证 AG-UI/Client Contract 适配、工具状态和人工确认；json-render 仅在可信 schema/白名单/审计通过后考虑。
5. **质量/准入**：性能（JS 初始包与懒加载、表格交互）、可访问性（键盘/焦点/ARIA）、SSR/Vite 兼容、UI 回归、API 契约、CSP/XSS、跨租户访问、依赖安全和许可证清单；提供明确的 PASS/FAIL 样本证据。
6. **决策归档**：PoC 后更新本表 Adopt/Adapt/Reference/Defer、固定 npm 版本或 Git SHA；变动公开模块/架构边界时再开 Proposed ADR 和实施计划，不擅自改变 [计划顺序](../plans/README.md)。

## 7. 来源、许可与检查版本

本表在 **2026-10-08** 检查了公开仓库元数据、分支 HEAD 和主仓库许可证（许可按上游声明，仅用于初查；商用前检查具体包、NOTICE、子依赖及收费模块）。SHA 为**检查时点的 12 位短前缀**，不是集成锁定版本；PoC 必须使用完整 commit SHA/npm lockfile 并复核依赖许可。

| 项目 | 参考 Revision | 项目 | 参考 Revision |
|---|---|---|---|
| [shadcn/ui](https://github.com/shadcn-ui/ui) | `main 0132174664c0` | [React Hook Form](https://github.com/react-hook-form/react-hook-form) | `master e76876b02550` |
| [Zod](https://github.com/colinhacks/zod) | `main 0b216ef674e2` | [Uppy](https://github.com/transloadit/uppy) | `main e3191129e041` |
| [TanStack Virtual](https://github.com/TanStack/virtual) | `main 78371e851e90` | [Sonner](https://github.com/emilkowalski/sonner) | `main 8e4662b39255` |
| [Lucide](https://github.com/lucide-icons/lucide) | `main a04f228cd011` | [react-i18next](https://github.com/i18next/react-i18next) | `master c4ee2c94ef84` |
| [RJSF](https://github.com/rjsf-team/react-jsonschema-form) | `main 1280ca7fc799` | [JSON Forms](https://github.com/eclipsesource/jsonforms) | `master 47f77687320f` |
| [Tiptap](https://github.com/ueberdosis/tiptap) | `main 5e4c11237821` | [Lexical](https://github.com/facebook/lexical) | `main b36ab04d4e81` |
| [CopilotKit](https://github.com/CopilotKit/CopilotKit) | `main 1e9a2af00f86` | [assistant-ui](https://github.com/assistant-ui/assistant-ui) | `main 20f0e5bcbef2` |
| [json-render](https://github.com/vercel-labs/json-render) | `main fc2a696a50a3` | [React Flow](https://github.com/xyflow/xyflow) | `main 3d35b5731757` |

补充候选来源：[Radix](https://github.com/radix-ui/primitives)、[dnd-kit](https://github.com/clauderic/dnd-kit)、[FullCalendar](https://github.com/fullcalendar/fullcalendar)、[Monaco Editor](https://github.com/microsoft/monaco-editor)；Open File Viewer 的单独版本与许可见 [Web Office 参考材料](WEB_OFFICE_TECHNOLOGY_EVALUATION.md)。

> **Contract Vertical Slice 的 P0 执行对齐：** 已完成 [2026-10-08 现状与 API 门禁审阅](../reviews/2026-10-08-contract-frontend-p0-readiness.md)，并形成 [PLAN-0016 P0 前端组件验证子计划](../plans/current/PLAN-0016-contract-frontend-p0-validation.md)（**Proposed / NOT ACTIVE**）。组件 PoC 与正式业务 API 阶段分开；未授权自动安装依赖或宣称合同切片已完成。

> 当前仅形成可验证的参考组件候选清单。未安装组件、未重构前端、未新增 API 或部署、未执行 PoC、未激活实施计划或接受 ADR。
