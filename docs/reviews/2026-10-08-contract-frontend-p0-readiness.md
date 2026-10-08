# Contract Business Vertical Slice — P0 Frontend Component Readiness Review

> 文档类型：Review（事实与缺口，不构成架构决策）
> 状态：Completed / Documentation review only
> 日期：2026-10-08
> Review base：\`jesenzhang/business-platform\` \`main\` at \`505d2039f5ba46f0d3023e3eaa31fb7891fd6580\`
> 关联参考：[FRONTEND_COMPONENT_CANDIDATES](../reference/FRONTEND_COMPONENT_CANDIDATES.md)、[WEB_OFFICE_TECHNOLOGY_EVALUATION](../reference/WEB_OFFICE_TECHNOLOGY_EVALUATION.md)
> 后续候选计划：[PLAN-0016](../plans/current/PLAN-0016-contract-frontend-p0-validation.md)（Proposed / NOT ACTIVE）

## 1. 审查范围与证据

基于 GitHub 源码及文档静态审阅，未运行本地构建、真实数据库或浏览器 E2E。

- \`apps/business-console/package.json\`：React 19、Vite、React Router 7、Radix Dialog、TanStack Query/Table、Zustand、ECharts、Tailwind、Vitest、Playwright 已存在。
- \`apps/business-console/src/App.tsx\` / \`components.tsx\`：AppShell、导航、通用状态与基础布局；当前还没有独立的可复用合同业务组件库。
- \`apps/business-console/src/pages.tsx\` / \`api.ts\` / \`contracts.ts\`：Document List/Detail、Upload、Processing、Candidate Review、Audit；Document Table 已使用 TanStack Table；上传为单文件；Candidate Review 使用 JSON + accept/reject，尚无 Contract Apply 流程。
- \`apps/business-console/src/iam-pages.tsx\`：已有用户、角色、组织、授权管理表单，可作为统一表单/设计系统安全 PoC 样本。
- \`apps/business-api/src/routes/documents.rs\`：已提供文档创建、列表、上传、详情与文档处理 Job 关联路由；**未找到公开的 GET 文档原始文件/预览内容接口**。
- \`apps/business-api/src/routes/upload.rs\`：上传 10 MiB 上限，支持 PDF / TXT / DOC / DOCX，含 Idempotency-Key；**不代表**上述所有格式已经可用于 AI 抽取。
- \`crates/contract/src/lib.rs\`：仅有领域定位说明与 TODO，尚无完整可供前端消费的 Contract Application/API。
- \`docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md\`：目前固定 MVP 实际可处理 text/plain、text/markdown、application/json；Office/PDF 在该流水线仍为 unsupported；不得因上传成功误判 Processing 能力。
- \`docs/plans/README.md\`：PLAN-0014 Active；Contract Business Vertical Slice 为下一项主要业务代码任务、计划尚未创建；PLAN-0006 Proposed/BLOCKED。

## 2. 业务流程与 P0 组件对齐

预期合同业务主链路仍为：

\`\`\`text
Contract List/Detail
  → Document Revision
  → AI Extraction + Evidence
  → Human Review
  → Apply Candidate (Contract owner Application Use Case)
  → Contract Version Update
\`\`\`

| 业务步骤 | 已有可复用前端能力 | 优先组件/是否本阶段采用 | API/服务端真实门禁 |
|---|---|---|---|
| 合同列表、详情 | React Router、TanStack Table/Query、Loading/Error/Empty | **优先复用原组件**；统一 DataTable/FilterBar 为受控薄封装；**无需重装表格库** | Contract public query/list/detail 尚未实现；过滤、分页、版本字段以 owner published contract 为准 |
| 合同字段/正式编辑 | IAM 表单模式、已有页面样式 | P0：React Hook Form + Zod PoC；shadcn/ui **Radix 配套**组件与设计 Token；不立即改写全部 IAM | Contract domain/application、字段约束、Policy 和 expected_version 必须先到位 |
| 文档上传、版本列表 | 单文件 FormData upload、Document list/detail | 当前单文件能力**继续使用**；Uppy **条件 PoC**（确实需要多文件/进度/失败重试时） | 当前服务端 10 MiB、单次 multipart；Tus/分片续传、跨文件原子批量未实现，不宣称支持 |
| 原件/附件预览 | Document 元数据和基本详情 | P0 候选：Open File Viewer / FilePreview **阻断接入**，可先用 File/Blob 脱敏样本做本地组件 PoC | 正式租户授权的 Document Content Read/Preview API 未见实现；不得暴露 S3 Key/无限期 URL 或前端猜路由 |
| 复杂 Office 预览、在线编辑 | 无 | **延后**：ONLYOFFICE 与 Collabora 保真对照，Univer/GenOffice 继续参考 | 独立许可、部署、Office 格式处理能力、版本化写回和业务确认均未通过 |
| 抽取候选与证据 | CandidateReview、Review API、Audit 视图 | P0：ReviewDiff / EvidencePanel / MutationFeedback 薄封装；Sonner 可用 | 现有 Review 接受/拒绝不能等同 Contract Version Update；证据必须绑定文档内容修订 |
| 正式应用候选 | 无 Contract Apply 页面/命令 | **阻断**：先设计 typed Review/Confirm UI 契约，不伪造提交 | Contract owner command、授权、expected_version、幂等、不可覆盖已签版本、Audit 必须后端实现 |
| 操作反馈与状态 | 自有 Loading/Error/StatusPill、ECharts Dashboard | 统一反馈/Toast + Lucide 可小规模采用；ECharts 保留 | 失败/拒绝不泄露敏感正文、凭据或底层存储细节 |
| 用户/组织/角色/权限 | IAM 页面与真实 API 已存在 | P0 **优先样本**：RHF/Zod、Radix Token + error states | 只能改善交互，不重写鉴权流程或重定义授权语义 |
| 大型列表与本地化 | TanStack Table、简单时间格式化 | TanStack Virtual **按性能证据再用**；react-i18next 可作为跨模块 i18n Foundation 候选 | server-side cursor/sort/filter 必须先由应用契约提供；禁止 UI 伪造全量数据 |

## 3. 纳入候选实施计划的组件裁剪

### 先执行（不需要等待合同业务 API，可作为 PoC）

1. **Radix-compatible design tokens + 基础 shadcn/ui 组件薄封装**：仅在选定 IAM / 文档现有页面验证，避免整站 UI 重做。
2. **React Hook Form + Zod**：统一 schema/error/pending/conflict 体验；先迁移单个 IAM 表单及 Candidate Review Comment 的局部交互。
3. **反馈和图标组件**：Sonner/Lucide 与现有 Loading/Error/Status 状态协调，敏感错误不进入 Toast。
4. **现有 Table/Query 复用**：为一页 Document 表格做 server-driven 查询契约清点；在没有可用 API 前不增加虚拟化。
5. **Open File Viewer isolated PoC**：使用**脱敏本地 File/Blob** 验证性能、安全和保真；不得和正式受权预览接口混同。

### 有明确 API/体验门禁后再采用

- **FilePreview 业务集成**：等待 Document Content Read/Preview 的 owner Application API 与授权/临时 URL 安全契约。
- **UploadQueue/Uppy**：是否真的需要并行、断点、暂停等由合同实际文件规格和服务端能力决定；默认不启用 Tus/分片。
- **Contract Form、ReviewDiff、ApplyCandidate**：等待 Contract list/detail/update/ApplyCandidate 的 published command/query、版本和 Policy 门禁；不得把候选审阅当正式合同行为。
- **国际化**：先沉淀 key/日期金额显示策略；引入 i18n runtime 与翻译资源按目标交付范围评审。

### 明确排除本轮

- TanStack Virtual 没有证明瓶颈前不引入；不新装另一套表格/图表/Routing/State SDK。
- Tiptap/Lexical、JSON Forms、React Flow、assistant-ui/CopilotKit、json-render、Monaco 仅保持 P1/P2 Reference；不在当前合同切片前置引入通用 Agent UI/低代码/流程设计器。
- 不将 ONLYOFFICE/GenOffice 当作普通文件预览组件的强依赖；不将 Office 解析与编辑能力假装为已实现。

## 4. 安全、所有权和架构约束

- Document Management 拥有文件、修订、访问授权；Document Intelligence 拥有候选/证据；Contract Management 拥有合同正式事实和合同版本；UI/Agent/Office 只是消费者或受控操作入口。
- 每次读写依赖可信 Principal/Tenant/Policy + resource scope，不以组件隐藏按钮作为授权依据；错误与事件不得泄露正文、Key、签名 URL 或密钥。
- 高风险写入按 \`Prepare → Preview → Confirm → Execute\`，并绑定用户、租户、资源版本、nonce/expiry 和命令 digest；禁止重复回调绕过幂等与确认。
- UI Contributions 第一阶段限宿主受控类型 Navigation/ListView/DetailSection/DetailTab/Action/Command，不允许 npm 组件选择改变公开业务 Module Contract 或注入任意 executable code。

## 5. 与当前计划关系 / Gate

- PLAN-0014 独立继续，不被 UI 组件选型抢占；Contract Business Vertical Slice 仍是下一项主要业务代码计划。
- PLAN-0016 是**此项的前端组件候选子计划**，状态 Proposed / NOT ACTIVE；不能单独宣布 Contract Vertical Slice 或 PLAN-0006 激活。
- 文档评估不会自动发起依赖安装/生产发布。进入编码前必须核验 npm lockfile 版本与 license，更新相关 API/架构/计划并通过独立审阅。
- 如果 Contract 主切片尚无正式 API，前端只可先做现有页面与离线 File/Blob 的有限 PoC。

## 6. 本轮审阅结果

- 静态源代码/路由/治理资料检查：**完成**。
- 组件依赖安装、编译、Vitest、Playwright、真实 SaaS 多租户、Office 视觉保真：**NOT RUN**。
- 关键阻断：Contract Application/API 未落地；正式文件预览内容 API 未落地；Candidate Review → Contract Version Apply 业务链路未落地。
- 建议：以 [PLAN-0016](../plans/current/PLAN-0016-contract-frontend-p0-validation.md) 提供分阶段、可验收候选任务；实施与最终供应商批准需后续 review/计划门禁。
