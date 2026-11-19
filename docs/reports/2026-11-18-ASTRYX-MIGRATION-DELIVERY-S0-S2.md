# Astryx 前端迁移 — S0–S2 交付报告与 S3–S5 路线

- **文档 ID:** REPORT-ASTRYX-DELIVERY-S0-S2
- **状态:** 本轮交付完成（S0/S1/S2 已验收；S3–S5 未实施）
- **日期:** 2026-11-18
- **范围:** `apps/business-console` 前端表现层
- **关联:** [PLAN-0016](../plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md)、[ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[S0](2026-11-18-ASTRYX-MIGRATION-S0-BASELINE.md)、[S1](2026-11-18-ASTRYX-MIGRATION-S1-FOUNDATION.md)、[S2](2026-11-18-ASTRYX-MIGRATION-S2-DASHBOARD.md)

## 1. 基线与提交

| 项目 | 值 |
| --- | --- |
| 远端 | `https://github.com/jesenzhang/business-platform.git` |
| 基线（分支起点） | **`505d2039f5ba46f0d3023e3eaa31fb7891fd6580`**（`origin/main`，2026-11-18 fetch；本地 `main` 原为 `8365de0`，落后 1 个文档提交） |
| 工作分支 | `feat/frontend-astryx-adoption-20261118` |
| 代码 HEAD（S2） | **`51b951d`** |
| 提交列表 | `fd50fd6` S0 基准 → `580ee92` 基线截图重采（证据修正）→ `2af503b` S1 基础接入 → `51b951d` S2 Dashboard → 本报告提交（docs） |

每个阶段独立提交、独立可回滚；未 force push、未合并、未推送 main、未部署。

## 2. S0 组件清单与迁移决策（摘要，完整见 S0 报告）

清点 20 项共享组件/样式（`components.tsx` 10 个组件、`pages.tsx` 6 个私有展示组件、`iam-pages.tsx` 1 个错误提示、Radix Dialog、ECharts、手写表格）。

| 决策 | 组件 | 切片 |
| --- | --- | --- |
| REPLACE | `StatCard`、Dashboard 三个 `article.panel` 区块、Dashboard 排版 | **S2 已执行** |
| ADAPT | ECharts 配色（仅轴/网格）、AppShell/Brand/Navigation | 图表 S2 已执行；shell S3 |
| KEEP（S2 内）→ ADAPT/DEFER | `PageHeader`（14 调用点）、`StatusPill`（15）、`Loading/Empty/ErrorState`（12–15）、`AuditList`、`JobsTable`、TanStack `DocumentTable` | S3/S4 |
| DEFER | Radix Dialog（移动抽屉）、`CandidateReview`、`pipeline`、`MutationError` | S3/S4 |

## 3. Astryx 版本、Tailwind 3 共存与兼容验证

| 项目 | 结论 |
| --- | --- |
| 版本 | `@astryxdesign/core` 0.7.16、`theme-neutral` 0.7.16、`cli` 0.7.16、`@stylexjs/stylex` 0.19.2；全部**精确锁定**，`npm ci` 可复现 |
| Peer/环境 | React 19.2.8（单实例，deduped）、Node 24.15.0（CLI 要求 ≥22.13）、`moduleResolution: Bundler` 满足子路径导入 |
| Theme | `src/astryx-themes.ts`（CLI 生成，默认 neutral）；根级 `<Theme mode={Zustand theme}>`；`data-astryx-theme`/`data-theme` 同步到 `<html>`，portal 可继承 Token |
| 层序 | `src/layers.css` 首个加载：`reset → tw-preflight → astryx-base → astryx-theme`；`@tailwind base` 在 `tw-preflight`；utilities 无层；**未引入** v4 桥 |
| Reset 归位 | `:root`/`*`/`body`/`button,input,textarea`/`button` 移入 `reset` 层；`table/th/td` 收敛为 `.table-wrap …`；`textarea` 收敛为 `.review-sidebar textarea` |
| 未迁移页面保护 | `.app :where(h1..h6,p,small,code,pre)` 在 `reset` 层降级主题排版；`.app` 显式声明系统字体栈 |
| 验证 | 构建后 CSS 首行仍为 `@layer reset,tw-preflight,astryx-base,astryx-theme;` 且存在 `@layer tw-preflight{}`；Chromium computed-style 断言 12 项（padding/border/background/Token）全部通过 |
| doctor | 18 pass / 3 warn / **0 fail**；warn = 外部字体 Figtree、CLI-on-path 探测误报、聚合项 |

## 4. 实际迁移的组件与保留原因

已迁移（S2）：四个指标卡（`StatCard` → `Card`+`Grid`+`Text`）、图表/活动/最近任务三个区块容器（`.panel` → `Card`+`VStack`+`HStack`+`Heading`+`Text`）、`Live` 状态（`.live-badge` → `Badge`）、图表轴/网格颜色（→ 主题 Token）。

保留原因（逐项见 S2 报告 §6）：`PageHeader`/`StatusPill` 调用点跨 12–15 个页面；`AuditList`/`JobsTable` 与其他页面共用；导航必须保持 React Router 唯一真相源；`Loading/Empty/ErrorState` 的语义替换需要与 S4 页面一起验证。

## 5. S1 Foundation Check 真实浏览器结果

- 入口仅 dev：`import.meta.env.DEV` 路由 `__foundation-check`，生产 JS 中 `Foundation check` 出现 **0 次**、无独立 chunk。
- Chromium 计算样式：Button padding 12px、primary 背景 `rgb(27,27,27)`、secondary `rgba(0,0,0,0.06)`；TextInput 字段盒 `4px 8px` + `1px solid` + 白底；Card default padding 11px + 1px 边框 + 白底，muted 12px + `rgb(241,241,241)`；Table 单元格 padding 12px；`--color-accent` 解析为 `light-dark(#1b1b1b,#f1f1f1)`。
- 主题：点击工具栏切换后 `<html data-theme>` 变为 `dark`、`.app` 同步 `dark`、localStorage 写入、刷新保持。
- 旧页面：桌面三页像素差 **0.000%**（Dashboard/Documents/Roles light；Roles dark 0.062% 为原生 checkbox 深色化）；移动端修复 S0 的 D1 横向溢出。

## 6. S2 Dashboard 前后截图与差异

并排：[light](assets/astryx-migration/s2-diff/dashboard-before-after--light.png) · [dark](assets/astryx-migration/s2-diff/dashboard-before-after--dark.png)。逐项差异（指标卡视觉、容器、比例、标题层级、图表深色适配、Live 徽标、空状态）见 S2 报告 §8。

像素差异（同脚本、同 Mock）：Dashboard 桌面 17.30%（light）/ 56.29%（dark，整页表面与图表配色变化）；Documents 0%/0%、Roles 0%/0.06% —— 迁移未泄漏到其他页面。页面总高 1228 → 1167px。

## 7. 逐项质量门禁

| 阶段 | 命令 | 结果 |
| --- | --- | --- |
| S0 | `npm ci` / `lint` / `typecheck` / `test` / `build` / `test:e2e` | PASS / PASS / PASS / PASS(7) / PASS / PASS(4) |
| S1 | 同上 + `astryx doctor` | PASS / PASS / PASS / PASS(7) / PASS / PASS(7, 12 skipped) / PASS(18/3/0) |
| S2 | 同上 + `astryx doctor` | PASS / PASS / PASS / PASS(7) / PASS / PASS(17, 16 skipped) / PASS(18/3/0) |

无 FAIL；无 BLOCKED；无 NOT RUN。两条 doctor warn 已判定为“外部字体（接受）”和“CLI PATH 探测误报（已用 `npx astryx` 规避）”，失败数为 0。

## 8. S3–S5 详细切片、依赖与风险

### S3 — 共享 AppShell（下一轮，预计 4 个可独立提交的切片）

| 切片 | 内容 | 依赖 | 验收 | 风险/回滚 |
| --- | --- | --- | --- | --- |
| S3-1 | 评估 `AppShell`/`Layout`/`TopNav`/`SideNav`；先只替换布局骨架，导航仍用 `NavLink` | S1 层序/主题；S2 模式 | 11 条路由与 active 状态不变；桌面/移动截图 0 意外差异 | 若 Astryx 布局与 fixed sidebar 冲突 → 只取 Token/间距，保留现结构 |
| S3-2 | 侧栏导航（`SideNav` 或 `Navigation` + Astryx Token），合并 `navItems` 单一来源 | S3-1 | 所有路由可达、active 正确、键盘焦点顺序正确 | 组件若不能接 `NavLink` → 保留 `NavLink` + Token（ADR D6） |
| S3-3 | 移动导航：Radix Dialog → Astryx `Dialog`（或保留 Radix） | S3-1；portal Token 已验证 | 打开/关闭/遮罩点击/焦点陷阱/Esc/跳转后关闭/触摸滑动 | `document.body.click()` 关闭技巧必须移除并改为显式 `close()`；否则抽屉不关 |
| S3-4 | 主题切换（`IconButton`/`Switch`）+ 迁移 shell 排版：移除 `.app` 排版降级、shell 采用 Astryx 字体/字号 | S3-1..3 | 14 个页面逐一截图审阅；light/dark；无横向滚动 | 这是**全站视觉变化**切片，必须单独提交、独立截图，不能与布局替换混在一次提交 |
| S3-5 | 回归矩阵：路由/主题/移动抽屉/键盘 + 全量门禁 | 全部 | e2e 增补；`astryx doctor` 0 fail | 可整段回滚到 S2 的 shell |

依赖：S1 的层序与主题仍是唯一基础；S2 的图表 Token 解析方式可复用；Radix Dialog 是 S3 唯一的外部行为依赖。

### S4 — 逐业务页面（读优先、写后置；每页一个提交）

| 切片 | 页面 | 关键保留项 | 风险 |
| --- | --- | --- | --- |
| S4-A1 | Documents List（含上传面板） | `listDocuments`、上传 `Idempotency-Key`、`window.location.href` 跳转 | 上传失败/禁用态；表格滚动 |
| S4-A2 | Document Detail | `getDocument`/`listDocumentJobs`、`startProcessing` 的 `content_revision` | 版本参数误改会导致处理错误版本 |
| S4-B1 | Processing List/Detail | 固定 pipeline 展示、状态语义 | pipeline 的 `done/current` 判定是展示逻辑，不得改 |
| S4-B2 | Candidate Review | `submitReview` 的 `candidate_version`、决策载荷、成功/错误态 | 写操作；必须保留确认与错误处理 |
| S4-C1 | Findings、Audit | 只读；`StatusPill` → `Badge` 映射表 | 状态→色调映射错误会改变可读性 |
| S4-C2 | Repairs | Prepare→Preview→Confirm→Execute 边界文案 | 高风险写；不得因 UI 迁移放宽确认 |
| S4-D1 | Identities、Organization | 成员/单位写操作的 `expected_version` | 版本参数遗漏 → 409/覆盖 |
| S4-D2 | Bindings | scope 三种形态、revoke 版本 | scope 序列化错误 → 授权边界变化 |
| S4-D3 | Roles | **`setRolePermissions` 完整集合替换**、自升级保护、`expected_version` | 最危险切片：UI 迁移可能导致权限键被清空。必须保留并扩充现有 e2e（S0 基线已有 2 条真实行为断言） |
| S4-D4 | Explain | evaluate 请求/响应只读展示 | 无写风险 |

依赖：S3 的 shell 必须先稳定（否则每页都要重测 shell）；S1 的排版降级条目必须随页面迁移逐条删除。

### S5 — 默认规范固化（S3/S4 验收后）

1. 新页面默认 Astryx（模板 + CLI 查询）；
2. 基础组件 / 业务组件 / 页面组件边界文档化（禁止包装层，允许语义映射）；
3. Token 与主题规范（何时用 `defineTheme`、何时用组件属性）；
4. 清理无调用者的历史 CSS/Radix 依赖（当前 `.live-badge`、`.stat-*` 已清；`.chip/.callout/.status-*` 待 S4）；
5. CI 纳入 `astryx doctor`、视觉快照与交互测试；
6. Astryx 升级验证流程（`astryx upgrade --from <ver> --apply` + doctor + 全量截图）；
7. 字体自托管决策（当前依赖 Google Fonts）；
8. CSS 体积裁剪（当前全量 `astryx.css` 326 kB gzip 54 kB）。

风险：S5 的“删除历史代码”必须以调用者普查 + 回归通过为前提，禁止仅凭组件库升级批量改写业务页面（ADR-0027 D4/D7）。

## 9. 本轮仍未解决的限制

1. **Google Fonts 外部请求**（`theme-neutral/fonts.css`）：Astryx 组件字体依赖外网；不可用时回退系统栈。S5 决定自托管或移除。
2. **`astryx doctor` 两条 warn**：外部字体、Windows 下 CLI-on-path 探测误报（本地 `npx astryx` 正常）。
3. **排版降级规则是临时机制**：`.app :where(...)` 让未迁移页面保持旧排版；S4 必须逐页删除，否则迁移后的页面仍用旧排版（组件不受影响）。
4. **原生 checkbox/select 深色化**：根级 `color-scheme: dark` 的预期结果，S4 表单迁移后由 Astryx 组件接管。
5. **CSS 体积**：326 kB（gzip 54 kB）全量组件样式；未做按需裁剪。
6. **中间宽度未纳入截图矩阵**：门禁只覆盖 1440/390；1050–1300px 的过渡布局未固定证据。
7. **`PageHeader`/`StatusPill` 等仍是旧实现**：Dashboard 标题字体沿用旧系统栈（S3 统一）。
8. **浏览器矩阵只有 Chromium**：未在 Firefox/WebKit 验证（与既有 e2e 基线一致）。

## 10. 是否建议继续 S3：**GO**

证据：

1. **基础可靠**：Astryx 0.7.16 与 React 19/Tailwind 3/Vite 7 共存通过构建级验证（层序在产物中成立），`astryx doctor` 0 fail，无重复 React。
2. **真实页面已验证**：Dashboard 使用真实 `getOverview` 契约完成迁移，10 条 Chromium 测试覆盖正常/空/错误/刷新/导航/两视口/两主题，全部通过；历史 7 条 vitest 与 4 条既有 e2e 未回归。
3. **无外溢**：Documents/Roles 桌面像素差 0%（Roles dark 0.06% 已解释），证明迁移边界可控。
4. **可回滚**：S0/S1/S2 三个代码提交独立，任一阶段可单独 revert，无数据/后端迁移。
5. **路线可执行**：S3 的四个切片边界、验收与风险已明确，唯一硬依赖（portal Token、导航真相源、Dialog 行为）均有已验证的替代方案。

条件（不满足则 S3 暂停并回报）：S3-3 若无法在不改变焦点/Esc/触摸行为的前提下替换 Radix，则保留 Radix，仅迁移布局与 Token；S3-4 的全站排版变化必须单独切片并提供 14 页截图，不得与布局替换合并且不得静默进行。
