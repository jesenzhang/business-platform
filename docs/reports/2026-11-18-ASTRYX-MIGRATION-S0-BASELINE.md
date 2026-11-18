# S0 — Astryx 迁移前真实现状基准

- **文档 ID:** REPORT-ASTRYX-S0-BASELINE
- **状态:** 已验收（S0 完成，S1 授权实施）
- **日期:** 2026-11-18
- **范围:** `apps/business-console` 迁移前基准：分支基线、依赖、共享组件清单、迁移决策矩阵、依赖兼容性、CSS Layer 风险、质量门禁与浏览器截图、本轮不解决的问题
- **关联:** `docs/plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md`
- **证据目录:** `docs/reports/assets/astryx-migration/baseline/`

## 1. 基线标识

| 项目 | 值 |
| --- | --- |
| 迁移分支 | `feat/frontend-astryx-adoption-20261118` |
| 基线提交（分支起点） | `505d2039f5ba46f0d3023e3eaa31fb7891fd6580`（`origin/main`，2026-11-18 fetch） |
| 本地 `main` 原状 | `8365de0`（落后 `origin/main` 1 个提交，均为文档提交） |
| 起的旧工作分支 | `codex/plan-0014-wave5a-local-release-pilot`（63 个提交未进 main） |
| 前端与旧分支差异 | `git diff HEAD origin/main -- apps/business-console` 为空；前端源码在两条线上一致 |
| 包管理器 | npm 11.4.2，`package-lock.json` lockfileVersion 3（402 个包条目） |
| 运行时 | Node v24.15.0（Windows） |
| 工作树状态 | 创建分支时 clean；未 stash、未覆盖任何已有工作 |

## 2. 依赖与构建现实（迁移前）

| 依赖 | 声明 | 实际解析 |
| --- | --- | --- |
| react / react-dom | ^19.1.1 | 19.2.8 / 19.2.8 |
| vite | ^7.1.5 | 7.3.6 |
| typescript | ~5.8.3 | 5.8.3 |
| tailwindcss | ^3.4.17 | 3.4.19 |
| @playwright/test | ^1.55.0 | 1.62.1 |
| vitest | ^3.2.4 | 3.2.7 |
| @tanstack/react-query | ^5.90.2 | 5.101.4 |
| @tanstack/react-table | ^8.21.3 | 8.21.3 |
| echarts / echarts-for-react | ^6.0.0 / ^3.0.5 | 6.1.0 / 3.1.0 |
| zustand | ^5.0.8 | 5.0.14 |
| react-router-dom | ^7.8.2 | 7.9.x |
| @radix-ui/react-dialog | ^1.1.14 | 1.1.23 |

构建产物：`dist/index.html` 0.47 kB、CSS 17.74 kB（gzip 4.53 kB）、JS 1,551.49 kB（gzip 508.39 kB）。JS 体积主要来自 ECharts，属既有状态，不在本轮范围。

`tsconfig.app.json` 已设置 `moduleResolution: "Bundler"` 且 `strict: true`，可直接解析 Astryx 子路径导入。

## 3. 共享组件清单与实际调用者

统计来源：`Select-String` 对 `src/*.tsx` 的 `<Component` / `Component(` 计数。

| # | 组件 | 定义位置 | UI 职责 | 实际调用者（数量） | 说明 |
| --- | --- | --- | --- | --- | --- |
| 1 | `AppShell` | `src/components.tsx:19` | 布局 + 导航 | `src/App.tsx:8`（1 处路由元素） | 含侧栏、移动栏、工具栏、主题按钮 |
| 2 | `Brand` | `src/components.tsx:58` | 导航 | `AppShell`（2 处：桌面 + 移动） | 私有函数，未导出 |
| 3 | `Navigation` | `src/components.tsx:62` | 导航 | `AppShell`（2 处） | `NavLink` + `navItems` 常量（11 条路由） |
| 4 | `PageHeader` | `src/components.tsx:66` | 布局 / 文本 | 12 个页面（共 14 次）：Dashboard、Documents、DocumentDetail、Processing、JobDetail、CandidateReview、Findings、Repairs、Audit、Identities、Roles、Bindings、Organization、Explain | 全站共享，改动影响全部页面 |
| 5 | `StatCard` | `src/components.tsx:70` | 数据展示 | 仅 `DashboardPage`（4 次） | 迁移风险最低的共享组件 |
| 6 | `StatusPill` | `src/components.tsx:74` | 状态展示 | 15 次：pages.tsx 9 处、iam-pages.tsx 6 处 | 与 `.status-*` CSS 类强耦合 |
| 7 | `Loading` | `src/components.tsx:78` | 反馈 | 13 次（pages 6 / iam 7） | 含 `min-height:50vh` |
| 8 | `Empty` | `src/components.tsx:79` | 状态展示 | 12 次 | |
| 9 | `ErrorState` | `src/components.tsx:80` | 反馈 | 15 次 | 渲染 `error.message` |
| 10 | `IdLink` | `src/components.tsx:81` | 数据展示 | `JobDetail` + `JobsTable`（2 次） | UUID 截断显示 |
| 11 | `MutationError` | `src/iam-pages.tsx:16` | 反馈 | 9 次 | IAM 私有 |
| 12 | `DocumentTable` | `src/pages.tsx:56` | 数据展示 | `DocumentsPage`（TanStack Table） | headless 逻辑 + 自绘 `table` |
| 13 | `FindingsTable` | `src/pages.tsx:117` | 数据展示 | `FindingsPage`、`RepairsPage` | 手写 `table` |
| 14 | `JobsTable` | `src/pages.tsx:136` | 数据展示 | `DashboardPage`、`ProcessingPage`、`DocumentDetailPage` | 手写 `table` |
| 15 | `AuditList` | `src/pages.tsx:141` | 数据展示 | `DashboardPage`、`AuditPage` | 非表格列表 |
| 16 | `JobDetail` | `src/pages.tsx:91` | 数据展示 | `ProcessingPage` | 含固定 Pipeline 展示 |
| 17 | `CandidateReview` | `src/pages.tsx:103` | 表单 | `CandidatePage` | 复核决策写操作 |
| 18 | Radix `Dialog` | `src/components.tsx:32` | 弹出层 | 仅 `AppShell` 移动端抽屉 | 唯一 Radix 使用点 |
| 19 | `ReactECharts` | `src/pages.tsx:35` | 数据展示 | 仅 `DashboardPage` | 图表颜色硬编码 |
| 20 | 手写 `table` | `src/iam-pages.tsx`（5 处） | 数据展示 | Identities、Roles、Bindings、Organization、Explain | 无共享抽象 |

`styles.css` 角色分布：布局（`.app/.sidebar/.main-content/.toolbar/.page-content/.page-header/.stats-grid/.dashboard-grid/.detail-grid/.review-grid`）、导航（`.nav-list/.nav-item/.brand`）、数据展示（`table/th/td/.table-wrap/.mono`）、表单（`.text-input/.file-input/.chip/textarea`）、反馈（`.loading/.empty/.error-state/.inline-error/.success-note/.callout/.spinner`）、弹出层（`.dialog-overlay/.mobile-drawer`）、状态（`.status-pill/.status-*/.status-dot/.live-badge/.pipeline-step`）。

## 4. 迁移决策矩阵

决策含义：**KEEP** 本轮不动；**REPLACE** 用 Astryx 组件替换；**ADAPT** 保留行为、改用 Astryx 布局/Token；**DEFER** 明确延后到指定切片。

| 组件 / 样式 | 决策 | 切片 | 原因 |
| --- | --- | --- | --- |
| `StatCard` | **REPLACE** | S2 | 仅 Dashboard 使用，语义就是 Astryx `Card` + `Text`；替换无跨页影响 |
| Dashboard 三个展示面板（`article.panel` + `panel-heading`） | **REPLACE** | S2 | 页面私有结构；保留内部 `JobsTable`/`AuditList`/ECharts |
| Dashboard 页面级排版 | **ADAPT** | S2 | 用 Astryx `Grid`/`VStack`/`Heading`/`Text` + 间距 Token 取代硬编码 `stats-grid`/`dashboard-grid` |
| ECharts 配色 | **ADAPT** | S2 | 仅换 Token 颜色，保留数据口径、系列语义、图表类型 |
| `PageHeader` | **KEEP（S2）→ ADAPT（S3/S4）** | S3 | 14 个调用点，S2 内改动风险等同全站改版；S2 只消费不改契约 |
| `StatusPill` | **KEEP（S2）→ DEFER（S4）** | S4 | 15 个调用点且与 `.status-*` 类耦合；Astryx `Badge` 需先建立状态→色调映射表 |
| `Loading` / `Empty` / `ErrorState` | **KEEP → DEFER** | S4 | 全站反馈语义；替换应统一在 S4，配合 Skeleton/Banner 语义验证 |
| `IdLink` / `MutationError` | **KEEP** | — | 纯展示/私有，无设计系统收益 |
| `AppShell` / `Brand` / `Navigation` | **ADAPT** | S3 | 见 §7 导航耦合分析 |
| Radix `Dialog`（移动抽屉） | **DEFER** | S3 | 唯一使用点；替换需验证焦点/Esc/遮罩/触摸 |
| 手写 `table` + TanStack `DocumentTable` | **DEFER（保留 TanStack）** | S4 | TanStack Table 是 headless 行/列模型，Astryx `Table` 是表现层；两者职责不同层，不得互相替代。S4 先验证 Astryx `Table` 能否承接现有行模型，再决定是否替换 `<table>` 标记 |
| `pipeline` 展示 | **DEFER** | S4-B | 与 Job 状态语义绑定 |
| `CandidateReview` 表单 | **DEFER** | S4-B | 写操作，必须最后迁移 |
| 全局 `table`/`th`/`td`/`button`/`input`/`textarea` 选择器 | **ADAPT** | S1 | 见 §6，无层全局选择器会静默压制 Astryx |
| `.status-*`、`.chip`、`.callout` 等既有类 | **KEEP** | — | 仍有调用者；仅在迁移对应组件时收敛 |

## 5. Astryx 依赖兼容性结论

| 项目 | 事实 | 结论 |
| --- | --- | --- |
| 稳定版本 | `@astryxdesign/core`、`@astryxdesign/theme-neutral`、`@astryxdesign/cli` 均为 **0.7.16**（dist-tags `latest`） | 四包统一锁 0.7.16 |
| React peer | core: `react >=19.0.0`、`react-dom >=19.0.0` | 项目 19.2.8 ✅ |
| Stylex peer | core: `@stylexjs/stylex ^0.19.0` | 需显式安装 0.19.2（当前未安装） |
| theme-neutral peer | `react >=19`、`@astryxdesign/core` 精确 `0.7.16`；依赖 `lucide-react ^1.66.0` | 与 core 同步锁定；新增 1 个传递依赖 |
| CLI 运行时 | `engines.node >= 22.13.0` | Node 24.15.0 ✅ |
| 模块解析 | 子路径导入 `@astryxdesign/core/Button` 等 | `moduleResolution: Bundler` 已满足 ✅ |
| 重复 React 风险 | 单 workspace 包、无嵌套 node_modules 版本冲突；Astryx 只声明 peer | 低；安装后用 `npm ls react` 复核 |
| Tailwind 3 | Astryx 的 `tailwind-theme.css` 桥仅适用 v4 `@theme` | **不引入**该桥；采用官方 v3 配方：`@tailwind base` 进 `tw-preflight` 层 |
| SSR/Next 约束 | Astryx 文档中 layers.css 拆分针对 Next 等会提升 `@import` 的打包器 | Vite 也可能提升，因此仍按官方建议拆分 `layers.css` 并首个导入 |

## 6. CSS Layer 风险（实测）

现状 `src/styles.css` 顶部有三个无层 Tailwind 指令，且另有大量无层全局选择器。按 Astryx 文档的层叠规则：**无层样式胜过任何命名层**，因此以下规则会静默压制 Astryx 组件样式。

| 序 | 现有规则 | 位置 | 风险 | 处置（S1） |
| --- | --- | --- | --- | --- |
| 1 | `@tailwind base`（preflight） | `styles.css:1` | **高**：`button{background:transparent}`、`table{border-collapse}`、`*{border:0 solid}` 等直接抹掉 Astryx Button/Table 的背景与边框 | 包进 `@layer tw-preflight` |
| 2 | `button, input, textarea { font: inherit }` | `styles.css:8` | **高**：无层元素选择器覆盖 Astryx Button/TextInput 的字号/字重/字体 Token | 移入 `reset` 层 |
| 3 | `button { cursor: pointer }` | `styles.css:9` | 中：覆盖 disabled/loading 光标语义（Astryx 用 `aria-disabled`） | 移入 `reset` 层 |
| 4 | `table`、`th`、`td`、`tr:last-child td` | `styles.css:54` | **高**：`width:100%`、`padding`、`border-bottom`、`color`、`font-size` 会压制 Astryx Table 单元格 | 收敛为 `.table-wrap table` 等局部选择器（保留既有表格外观） |
| 5 | `textarea`、`textarea:focus` | `styles.css:61` | **高**：全局背景/边框/圆角/焦点环压制 Astryx TextArea | 收敛为 `.field-textarea` 或限定容器 |
| 6 | `:root { font-family/color/background }` | `styles.css:5` | 低：只影响继承；Astryx 组件自带层级化字体/颜色 | 保留 |
| 7 | `* { box-sizing: border-box }` | `styles.css:6` | 低：与 Astryx reset 同值 | 保留或移入 reset |
| 8 | `@media (prefers-reduced-motion) { *,*::before,*::after { … !important } }` | `styles.css:58` | 低：仅动画/过渡时长，符合无障碍预期 | 保留 |
| 9 | `.mono`、`.eyebrow`、`.panel` 等类选择器 | 多处 | 低：类选择器只作用于显式使用的元素 | 保留 |
| 10 | Tailwind utilities（无层） | `styles.css:3` | 中：项目当前**未使用**任何 Tailwind utility 类（`src/*.tsx` 出现的 64 个 className token 全部是自定义类名，仅 `@tailwind` 指令存在），因此无实际覆盖；保留无层以维持未来优先级 | 保留 |

补充：`styles.css` 无 `@apply`、无 `@import`，迁移只需重排层序与收敛全局选择器，不需要重写样式表。

## 7. AppShell 与 React Router 的耦合方式（S3 输入）

- 路由真相源：`src/App.tsx` 的 `<Routes>`；`navItems` 是 `components.tsx` 内的 11 条静态 `[path, label, icon]` 常量，两处独立维护（无单一来源）。
- active 状态：`NavLink` 的 `className={({isActive}) => …}` + `end={to === '/'}`。
- 移动抽屉：Radix `Dialog.Root/Trigger/Portal/Overlay/Content` + `<Dialog.Title className="sr-only">`；关闭依赖 `onClick` 里的 `document.body.click()`（Radix 外部点击关闭的隐式利用），**这是替换 Dialog 时最容易破坏的行为**。
- 主题按钮：`useConsoleStore` 的 `theme/setTheme`，仅切换根 `.app.dark` 类，不写 `data-*` 属性，也不与任何设计系统 Token 对接 → S1 需要把同一状态接到 `<Theme mode>`。

## 8. 测试基线

### 8.1 迁移前质量门禁（全部实际执行）

| 命令 | 结果 | 证据 |
| --- | --- | --- |
| `npm ci` | **PASS**（401 packages，39s） | lockfile 可复现 |
| `npm run lint` | **PASS**（无输出） | `eslint .` |
| `npm run typecheck` | **PASS** | `tsc -b` |
| `npm test` | **PASS**（2 文件 / 7 用例，1.55s） | vitest |
| `npm run build` | **PASS**（5.72s，771 modules） | CSS 17.74 kB / JS 1,551.49 kB |
| `npm run test:e2e` | **PASS**（4 用例，5.4s） | Playwright + Chromium Headless Shell 151 |
| `npm exec -- astryx doctor` | 迁移前不适用（未安装） | S1 执行 |

环境说明：本机原先没有 Playwright 浏览器，已执行 `npx playwright install chromium` 安装 `chromium_headless_shell-1234`；CI 需同等步骤。

### 8.2 现有 E2E 的真实覆盖度评估

| 用例 | 覆盖的行为 | 缺口 |
| --- | --- | --- |
| `smoke.spec.ts` | 只断言页面标题与 Documents 链接可见 | 不断言任何数据、状态、导航跳转、视口、主题 |
| `iam.spec.ts` › render from mocked admin APIs | 5 个 IAM 页面标题与关键空状态 | 不断言交互 |
| `iam.spec.ts` › role permission set-replacement | **真实行为**：打开编辑面板预勾选、新增键后提交完整集合、`expected_version` 正确、二次保存不丢键 | 无失败路径 |
| `iam.spec.ts` › deny explanation | **真实行为**：提交 explain 请求、渲染 bounded reason 与 policy reference | 无可视化回归 |
| `tests/api.test.ts` | envelope 解包、错误映射 | — |
| `tests/iam.test.ts` | 幂等键、版本透传、DELETE 查询参数、explain 契约 | — |

结论：IAM 有真实行为覆盖；**Dashboard 完全没有行为级 E2E**（无指标断言、空/错误状态、刷新、导航、响应式、主题）。S2 必须补齐。

### 8.3 浏览器截图基准

固定 Mock（`e2e/fixtures/console.ts`）：合成租户数据、固定时间戳（`2026-11-16T…Z`）、固定 locale `en-US`、timezone `UTC`、`reducedMotion`、`deviceScaleFactor: 1`。采集脚本：`e2e/screenshots.spec.ts`（`CAPTURE_DIR` 显式开启，默认跳过）。

| 视图 | 桌面 1440×900 | 移动 390×844 |
| --- | --- | --- |
| Dashboard `/` | [light](assets/astryx-migration/baseline/dashboard--desktop--light.png) · [dark](assets/astryx-migration/baseline/dashboard--desktop--dark.png) | [light](assets/astryx-migration/baseline/dashboard--mobile--light.png) · [dark](assets/astryx-migration/baseline/dashboard--mobile--dark.png) |
| Documents `/documents` | [light](assets/astryx-migration/baseline/documents--desktop--light.png) · [dark](assets/astryx-migration/baseline/documents--desktop--dark.png) | [light](assets/astryx-migration/baseline/documents--mobile--light.png) · [dark](assets/astryx-migration/baseline/documents--mobile--dark.png) |
| Roles `/roles` | [light](assets/astryx-migration/baseline/roles--desktop--light.png) · [dark](assets/astryx-migration/baseline/roles--desktop--dark.png) | [light](assets/astryx-migration/baseline/roles--mobile--light.png) · [dark](assets/astryx-migration/baseline/roles--mobile--dark.png) |

溢出度量见 [metrics.json](assets/astryx-migration/baseline/metrics.json)。

## 9. 迁移前发现的既有缺陷（非本轮引入）

### D1 — 移动端 shell 横向溢出（严重，P1）

`document.documentElement.scrollWidth = 569`，视口 390（三页一致）。实测根因：`.app` 是 `display:flex` 的行容器，移动端媒体查询隐藏了 `.desktop-sidebar`，但 `.mobile-bar` 与 `.main-content` 仍是同一 flex 行的两个子项：`.mobile-bar` 被压缩到 **178.5px**，`.main-content` 宽度 390px 且 `offsetLeft = 179`，因此右侧内容被推出视口 179px，页面出现横向滚动，表格被裁切（见基准移动端截图）。

影响：所有移动端页面。这是**迁移前就存在**的布局缺陷，不是 Astryx 引起。按 S1「不得造成无意回归」与 S2「无横向滚动」的要求，计划在 S1 以最小 CSS 变更修复（移动端 `.app` 改为纵向堆叠），并单独记录前后证据。

### D2 — ECharts 深色配色未适配

图表 `grid/axisLabel/splitLine` 固定为浅色值（`axisLabel: #6e6e73`、`splitLine: #e5e5ea`），深色面板上网格线仍是浅色。S2 允许「ECharts 颜色按主题 Token 适配」，数据口径不变。

### D3 — 主题状态与设计系统 Token 无连接

主题只切换根 `.app.dark` CSS 变量，没有可供组件库读取的 mode；S1 必须把 Zustand 状态接到 `<Theme mode>`，且不能让两套状态分叉。

### D4 — 无迁移前视觉基线

本轮之前仓库没有任何截图基线或视觉回归门禁；本轮建立（opt-in 截图脚本）。

### D5 — 表单/抽屉无键盘与焦点测试

移动抽屉的 Esc/焦点陷阱、表单必填与错误态均无自动化覆盖。S3 迁移导航时必须补齐。

### D6 — 空数据图表形态未定义

`processing_by_status = {}` 时图表渲染空坐标系；S2 需给出确定的空状态表现并测试。

## 10. 本轮不解决的问题

1. 共享 `AppShell`、`PageHeader`、`StatusPill`、`Loading/Empty/ErrorState` 的组件替换（S3/S4）；
2. TanStack Table → Astryx Table 的表现层替换（S4，且保留 TanStack 行模型）；
3. Radix Dialog 的移除（S3）；
4. Roles/Bindings/Organization 等写操作页面的迁移（S4-D）；
5. Tailwind 3 → 4、pnpm、构建器替换；
6. 历史 CSS 与依赖删除（S5，需确认无调用者）；
7. 服务端、OpenAPI、数据库、权限语义的任何变更（不在本计划范围）；
8. 全站无障碍审计（本轮只做已迁移 UI 的对比度与焦点验证）。

## 11. S0 验收对照

| 验收项 | 结果 | 证据 |
| --- | --- | --- |
| 现有前端质量检查有真实记录 | PASS | §8.1 六条命令逐条结果 |
| 所有拟替换组件识别了使用者 | PASS | §3 的 20 项清单与调用者计数 |
| 明确需变更 / 不得变更文件 | PASS | 计划 §4 |
| 识别兼容性阻碍与解决办法 | PASS | §5、§6、D1–D3 |
| 本阶段不修改用户可见业务页面 | PASS | S0 提交仅新增 e2e 固定 Mock、opt-in 截图脚本与文档；`src/**` 零改动 |
