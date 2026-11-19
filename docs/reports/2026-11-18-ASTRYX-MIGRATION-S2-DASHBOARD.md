# S2 — Dashboard 真实页面试点

- **文档 ID:** REPORT-ASTRYX-S2-DASHBOARD
- **状态:** 已验收（S2 完成；本轮至此停止，不进入 S3）
- **日期:** 2026-11-18
- **范围:** `apps/business-console` 的 `/`（DashboardPage）及其页面私有展示代码；共享组件与其余页面保持不变
- **关联:** [PLAN-0016](../plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md)、[ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[S0](2026-11-18-ASTRYX-MIGRATION-S0-BASELINE.md)、[S1](2026-11-18-ASTRYX-MIGRATION-S1-FOUNDATION.md)
- **证据目录:** `docs/reports/assets/astryx-migration/s2/`（S2 截图）、`s2-diff/`（前后对比与差异图）

## 1. 身份

| 项目 | 值 |
| --- | --- |
| 分支 | `feat/frontend-astryx-adoption-20261118` |
| 基线 | `505d203`（S0 `fd50fd6`，S1 `2af503b`） |
| 迁移路由 | `/`（`src/pages.tsx` 的 `DashboardPage`） |
| 修改的源文件 | `src/pages.tsx`、`src/components.tsx`（删除无调用者的 `StatCard`）、`src/styles.css`（删除无调用者的 `.stat-*`/`.stats-grid`/`.dashboard-grid`） |

## 2. 业务契约保持不变

| 契约 | 处理 |
| --- | --- |
| 数据来源 | 仍是 `getOverview()` → `/api/v1/operations/overview`，Query key `['overview']` 未变 |
| 刷新机制 | `refetchInterval: 10_000` 原样保留（e2e 用 Playwright clock 快进 11s 验证） |
| 数据口径 | 四项指标仍为 `document_total`、`review_pending`、`unresolved_findings`、`processing_by_status.succeeded ?? 0`；`+N in the last day` 仍取 `document_created_today` |
| 图表数据 | 分类轴 = `Object.entries(processing_by_status)` 的 key（`_`→空格）与同序 value；图表类型、`minInterval: 1`、`barWidth: 24`、圆角、tooltip `axis` 全部保留 |
| 跳转 | `View jobs →` `/processing`、`Open audit →` `/audit`、`All processing →` `/processing`，仍使用 React Router `Link`（e2e 断言 URL） |
| 加载/错误 | `Loading` / `ErrorState` 分支与文案未改；错误仍显示 `error.message` 的有界文本 |
| 请求/状态管理 | 未新增请求、未新增 store、未改 `api.ts`/`contracts.ts`/`store.ts` |

## 3. 组件映射（页面私有结构）

| 区域 | 迁移前 | 迁移后 |
| --- | --- | --- |
| 页面标题 | `PageHeader`（共享，14 个调用者） | **保留 `PageHeader`**；仅把 action 的 `span.live-badge` 换成 Astryx `Badge variant="green"` |
| 四个指标卡 | `div.stats-grid` + `article.stat-card` + `strong` + `span.stat-label/detail`（含装饰性左色条 `tone-*`） | Astryx `Grid columns={{minWidth:150,max:4}}` + `Card padding={5}` + `VStack` + `Text type="supporting"` + `Text type="display-2" hasTabularNumbers` |
| 图表区 | `article.panel` + `.panel-heading` + `h2/p/a.text-button` | `Card` + `VStack` + `HStack` + `Heading level={2}` + `Text type="supporting"` + 既有 `ReactECharts` |
| 最近活动 | `article.panel` + 同上 + `AuditList` | `Card` + 相同头部组合 + **保留 `AuditList`**（共享组件，Audit 页仍在用） |
| 最近任务 | `article.panel` + 同上 + `JobsTable` | 全宽 `Card` + 相同头部组合 + **保留 `JobsTable`**（共享组件，Processing/DocumentDetail 仍在用） |
| 区块间距 | `.stats-grid`/`.panel` 的 `margin-bottom: 18px` | `VStack gap={4}`（Astryx 间距 Token，16px）+ `Grid gap={3}` |

新代码中没有内联样式、没有新增 CSS 类、没有包装组件；间距/圆角/表面来自组件属性与主题 Token（`Card padding` → `--spacing-*`，表面/边框/圆角 → `--color-background-surface`/`--color-border`/`--radius-container`）。

## 4. ECharts 主题适配

- **做法**：canvas 不能读 CSS 变量，因此用一个隐藏探针元素把 `--color-text-secondary`、`--color-border` 解析成具体颜色（浏览器会按当前 `color-scheme` 求值 `light-dark()`），并在 `theme` 变化时重新解析。
- **结果**：深色模式下坐标轴文字与网格线改为主题的 `rgb(158,158,158)` / `rgba(255,255,255,0.1)`，修复 S0 D2（原先固定 `#6e6e73`/`#e5e5ea`，深色下网格线过亮）。
- **不改变**：分类色板仍为 `#0071e3/#34c759/#ff9f0a/#af52de/#ff375f`；这些颜色承载状态分类语义，跨主题保持一致，不做重新映射。
- **空数据**：`processing_by_status` 为空时不再渲染空坐标系，改为 `Empty`（"No processing jobs in this scope yet."），并由 e2e 断言 canvas 数量为 0。

## 5. 移除的无调用者代码（已确认无其他使用者）

| 代码 | 确认方式 | 处置 |
| --- | --- | --- |
| `StatCard`（`components.tsx`） | 全仓 `<StatCard` 仅 Dashboard 4 处 | 删除 |
| `.stat-card`/`.stat-label`/`.stat-detail`/`.tone-*`/`.stats-grid` | `Select-String` 全 `src/*.tsx` 无引用 | 删除（含两个媒体查询片段） |
| `.dashboard-grid` | 仅 Dashboard 使用 | 从合并选择器与媒体查询中移除；`.detail-grid`/`.review-grid` 保留 |

## 6. Dashboard 上仍然保留的旧样式及原因

| 保留项 | 原因 |
| --- | --- |
| `PageHeader`（`.page-header`/`h1`/`.eyebrow`） | 14 个页面共用；S1 已明确“不在未检查全部调用者时重写”，迁移属 S3/S4 的共享切片 |
| `Link className="text-button"`（3 处） | 导航真相源必须是 React Router；Astryx `Link` 的自定义 `as` 需要传 `to` 适配。保留既有链接类，行为与视觉不变 |
| `AuditList`（`.audit-list/.audit-item/.audit-marker`） | 与 Audit 页共用；迁移需同时验证 Audit 页（S4-C） |
| `JobsTable`（`.table-wrap` + `table/th/td`） | 与 Processing、DocumentDetail 共用；TanStack/手写表格迁移属 S4 |
| `.status-pill`（表格内状态） | 15 个调用点，S4 统一映射到 Astryx `Badge` |
| `PageHeader` 的 h1 字体 | 受 S1 的 `.app` 排版降级保护，保持系统字体；S3 迁移 shell 时统一 |

## 7. 新增 Playwright 验证（`e2e/dashboard.spec.ts`，10 条全部通过）

| 用例 | 断言 |
| --- | --- |
| 正常加载 | 4 个指标值（128/4/3/41）按 Card 作用域断言；`+6 in the last day`；3 个区块标题；canvas 存在；最近活动与任务行可见 |
| 空数据 | 指标为 0；`+0 in the last day`；图表空状态文案；`No audit events yet.`；`No processing jobs in this scope.`；**canvas 数量为 0** |
| API 错误 | 渲染 `Couldn’t load this view.` 与有界错误消息，且不显示正常标题 |
| 刷新更新 | Playwright clock 快进 11s 后指标与图表数据更新为第二份 payload，且请求数 ≥ 2 |
| 导航 | 三个链接分别跳转 `/processing`、`/audit`、`/processing`（断言 URL 与目标页标题） |
| 桌面 1440×900 light/dark | `scrollWidth ≤ clientWidth`；7 个 Card 全部落在视口内且宽度 > 100px；`data-theme` 正确；表面非透明 |
| 移动 390×844 light/dark | 同上（含移动端单列/双列重排后的边界） |
| 主题 Token | light 与 dark 的 Card 背景色不同 |

固定 Mock 见 `e2e/fixtures/console.ts`：合成数据、固定时间戳、`en-US` + `UTC` + `reducedMotion`，无当前时间输入、无真实租户数据。

## 8. 前后截图与差异说明

| 视图 | 基线（S0/S1 后） | S2（迁移后） |
| --- | --- | --- |
| Dashboard 1440 light | [baseline](assets/astryx-migration/baseline/dashboard--desktop--light.png) | [s2](assets/astryx-migration/s2/dashboard--desktop--light.png) |
| Dashboard 1440 dark | [baseline](assets/astryx-migration/baseline/dashboard--desktop--dark.png) | [s2](assets/astryx-migration/s2/dashboard--desktop--dark.png) |
| Dashboard 390 light | [baseline](assets/astryx-migration/baseline/dashboard--mobile--light.png) | [s2](assets/astryx-migration/s2/dashboard--mobile--light.png) |
| Dashboard 390 dark | [baseline](assets/astryx-migration/baseline/dashboard--mobile--dark.png) | [s2](assets/astryx-migration/s2/dashboard--mobile--dark.png) |
| 空数据态 | — | [light](assets/astryx-migration/s2/dashboard-empty--desktop--light.png) · [dark](assets/astryx-migration/s2/dashboard-empty--desktop--dark.png) |
| 错误态 | — | [light](assets/astryx-migration/s2/dashboard-error--desktop--light.png) · [dark](assets/astryx-migration/s2/dashboard-error--desktop--dark.png) |

并排对比：[light](assets/astryx-migration/s2-diff/dashboard-before-after--light.png) · [dark](assets/astryx-migration/s2-diff/dashboard-before-after--dark.png)（左=基线，右=S2）。

像素差异（同一固定 Mock、同一截图脚本）：

| 视图 | light | dark |
| --- | --- | --- |
| Dashboard 1440×900 | 17.30% | 56.29% |
| Documents 1440×900 | **0.00%** | **0.00%** |
| Roles 1440×900 | **0.00%** | 0.06%（S1 已记录的原生 checkbox 深色化） |
| 三页 390×844 | 布局差异（S1 已修复的移动端布局 + 本页重排） | 同左 |

Dashboard 的差异**全部为有意设计变更**，逐项说明：

1. **指标卡**：装饰性左侧色条移除（Astryx Card 无该语义，用统一表面 + 排版层级替代）；数值字号 31px → `display-2`（34px）并使用等宽数字；标签/说明使用 `type="supporting"` Token。
2. **区块容器**：`.panel`（16px 圆角 + 大阴影）→ Astryx `Card`（主题圆角 12px、1px 边框、无重阴影）；卡片内边距由 24px → 间距 step 5。
3. **布局比例**：图表/活动由 `1.2fr : .8fr` 改为等宽两列（Astryx `Grid` 响应式），图表绘图区更大；页面总高 1228 → 1167px。
4. **标题层级**：区块标题由 17px 旧类改为 `Heading level={2}`（20px，主题字族 + 字重）；页面 h1 保持旧尺寸与字体（`PageHeader` 未改）。
5. **图表配色**：深色模式网格线/坐标文字改为主题 Token（S0 D2 的修复）；分类色板不变。
6. **Live 状态**：`span.live-badge` → Astryx `Badge variant="green"`（同一语义、同一文案）。
7. **空数据**：图表空状态从“空坐标系”改为明确的 Empty 文案；活动/任务空状态沿用既有 `Empty`。

未受影响：Documents、Roles（桌面 0% / 0.06%），证明 S2 没有泄漏到其他业务页面；`.panel`、`.table-wrap`、`.status-pill` 等共享样式仍被其他页面使用，未改动行为。

## 9. 质量门禁（全部实际执行）

| 命令 | 结果 |
| --- | --- |
| `npm run lint` | **PASS**（0 error / 0 warning） |
| `npm run typecheck` | **PASS** |
| `npm test` | **PASS**（7 用例） |
| `npm run build` | **PASS**（7.31s） |
| `npm run test:e2e` | **PASS**（17 passed / 16 skipped-opt-in） |
| `npm exec -- astryx doctor` | **PASS**（18 pass / 3 warn / 0 fail，warn 同 S1） |

构建产物：`index-*.js` 1,703,202 B、`index-*.css` 325,727 B、Astryx `Tooltip-*.js` 58,737 B（Astryx 懒加载 chunk）；生产 JS 中 `Foundation check` 出现 0 次（dev-only 入口未泄漏）。

## 10. 回滚

单独 revert S2 提交即可：恢复 `DashboardPage` 的旧实现、恢复 `StatCard` 与 `.stat-*`/`.stats-grid`/`.dashboard-grid` CSS、删除 `e2e/dashboard.spec.ts` 与截图证据。S1 的层序/主题/Foundation Check 不受影响。

## 11. 未解决与移交

1. `PageHeader`、`StatusPill`、`Empty/ErrorState/Loading`、`AuditList`、`JobsTable` 仍是旧实现 → S3/S4。
2. 图表 X 轴标签在窄容器下由 ECharts 自动抽稀（基线行为，未改变）。
3. Astryx CSS/JS 体积已在 S1 记录，S5 评估裁剪。
4. Google Fonts 外部请求仍存在（S1 限制），S5 决定自托管或移除。
5. Dashboard 在 1050–1300px 中间宽度未单独截图（门禁要求为 1440/390），如需可加入 S5 视觉矩阵。

## 12. 验收对照

| S2 验收项 | 结果 | 证据 |
| --- | --- | --- |
| 页面仍使用真实原有业务契约 | PASS | §2，e2e 断言真实字段与刷新 |
| 所有原有测试保持通过 | PASS | 原 4 条 e2e + 7 条 vitest 全绿 |
| 新增浏览器测试通过 | PASS | §7，10 条 |
| 视觉和主题达到一致性要求 | PASS | §8 截图 + 并排对比；浅/深色表面 Token 断言 |
| 不出现重复的布局/状态管理/请求代码 | PASS | 无新请求/store/包装层；删除旧 StatCard 而非并存 |
| 新增依赖和抽象均有直接使用者 | PASS | S2 未新增依赖；`resolveChartColors` 仅服务于本页图表 |
