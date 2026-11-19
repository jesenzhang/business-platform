# S3 — 共享 AppShell 迁移（导航 / 工具栏 / 移动抽屉 / 主题切换）

- **文档 ID:** REPORT-ASTRYX-S3-APPSHELL
- **状态:** 已验收（S3 完成；本轮停在 S4 之前）
- **日期:** 2026-11-18
- **范围:** `apps/business-console` 的共享外壳：侧栏导航、工具栏、主题切换、移动端导航抽屉；不迁移业务页面内容
- **关联:** [PLAN-0016](../plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md)、[ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[S2](2026-11-18-ASTRYX-MIGRATION-S2-DASHBOARD.md)
- **证据目录:** `docs/reports/assets/astryx-migration/s3/`、`s3-diff/`

## 1. 身份

| 项目 | 值 |
| --- | --- |
| 分支 | `feat/frontend-astryx-adoption-20261118` |
| 起点 | `51b951d`（S2） |
| 切片提交 | `d49ed0f` S3-1 · `e093266` S3-2 · `f40c2d2` S3-3 · `d08375f` S3-4 · 本报告（S3-5） |
| 修改文件 | `src/components.tsx`、`src/app-root.tsx`、`src/styles.css`、`package.json`/`package-lock.json`、`e2e/shell-navigation.spec.ts`、`e2e/fixtures/console.ts`、`e2e/screenshots.spec.ts` |

## 2. 关键决策：不引入 AppShell 整体框架，采用组件级外壳迁移

对 `AppShell` / `Layout` / `TopNav` / `SideNav` / `MobileNav` / `Drawer` 逐一评估后，结论是**不替换外壳几何**，只把内部导航、工具栏、抽屉与主题切换换成 Astryx 组件：

| 评估项 | 事实 | 结论 |
| --- | --- | --- |
| `AppShell height="fill"` | 主内容改为内部滚动（100dvh），与本应用“文档滚动 + sticky toolbar”的既有行为不同 | 会改变滚动容器、粘性行为与移动端地址栏体验，超出安全切片 |
| `AppShell height="auto"` | 需要把品牌、导航、工具栏全部重排到 TopNav + SideNav 结构 | 14 个页面的 chrome 一次性变化，S3 无法逐页审阅 |
| 品牌位置 | 当前品牌在 248px 侧栏顶部；AppShell 的 TopNav 才是产品身份位 | 迁移会改变信息架构，不属于设计系统替换 |
| 移动栏 | 当前移动栏（菜单 + 品牌 + 主题）由应用自己拥有 | AppShell 的移动栏是自带 toggle 的紧凑栏，替换即改移动端 chrome |
| 导航接入 React Router | `SideNavItem` 支持 `as`/`href`，且 Astryx 提供 `LinkProvider` 把 `href` 传给路由器 | ✅ 组件级接入是官方路径，无需 AppShell |

因此按 PLAN-0016 的兜底条款执行：**保留现有导航几何与行为，采用 Astryx 导航/布局/抽屉/Token**，不引入适配层。保留的几何：260px 固定侧栏（原 248px 改为组件自身宽度）、sticky 工具栏、移动栏、`main` 唯一滚动容器。

## 3. 切片明细与真实验证

### S3-1 导航（`d49ed0f`）

- `LinkProvider component={RouterLink}` 包裹应用：所有 Astryx 链接（SideNav 项，以及后续 Link/Button `href`）都走 React Router，`as` 适配层为零。
- `SideNav` + `SideNavItem`：`navItems` 单表、lucide 图标、`isSelected` → `aria-current="page"`、真实 `navigation` landmark、品牌进 header 槽、API 状态进 footer 槽。
- 选中匹配规则与旧 `NavLink` 等价：根路径精确匹配，其余按路径段前缀（`/documents/42` 选中 Documents）。
- 删除 `.nav-list/.nav-item/.nav-icon` 样式；侧栏宽度 248 → 260 以匹配组件宽度。
- 新增直接依赖 `lucide-react@1.74.0`（`theme-neutral` 已依赖同一版本，Astryx 图标体系即 lucide）。
- 既有 smoke / IAM / Dashboard 共 14 条 Playwright 用例不改一行全部通过。

### S3-2 工具栏与主题（`e093266`）

- 工具栏：`Text type="label"` + `Text type="supporting"`；租户状态 `StatusDot variant="success"` + `Text`。
- 主题切换：两处 `button` → `IconButton`（lucide Moon/Sun，可访问名固定 `Toggle theme`，tooltip 说明目标主题）。
- **移动端新增主题入口**：工具栏在 ≤760px 隐藏，旧版移动端无法切主题；移动栏现在提供 `IconButton`。
- 删除 `.toolbar-title/.secure-label/.toolbar-actions`；修复误删的 `.subtle` 工具类（与 `.secure-label` 共用选择器）。

### S3-3 移动抽屉（`f40c2d2`）

- Radix `@radix-ui/react-dialog` → Astryx `Drawer`（`side="start"`、300px、默认模态 scrim、`DrawerHeader` + `Layout/LayoutContent` + `SideNav`）。
- 关闭方式：Escape、scrim 点击、关闭按钮、**路由变化**（用 `useLocation` effect 替代旧的 `document.body.click()` 外部点击技巧）。
- 焦点：Drawer 自带 focus trap；关闭后焦点回到“打开导航”按钮；`data-autofocus` 由组件管理。
- 触摸：移动端链接用 `tap()` 验证可导航并关闭抽屉。
- 仪表/行为证据：`e2e/shell-navigation.spec.ts` 7 条用例（路由清单、aria-current、客户端导航、抽屉焦点/Esc、遮罩/导航关闭、触摸、移动主题+无溢出）。
- 移除 `@radix-ui/react-dialog` 及 `.dialog-overlay/.mobile-drawer/.sr-only/.icon-button` 死代码；`npm ls @radix-ui/react-dialog` 为空。
- 已记录的分层语义：Astryx 一次 Escape 只关一层。键盘 Tab 到带 tooltip 的控件时，第一次 Esc 关闭 tooltip、第二次关闭抽屉；直接 Esc（无 tooltip）一次关闭。抽屉测试同时覆盖这两种路径。

### S3-4 排版作用域（`d08375f`）

- S1 为保护未迁移页面，把主题排版重置降级在 `.app` 作用域；外壳已用 Astryx 组件，因此把降级范围收窄为 `.app .page-content`：**外壳跟随主题，页面内容保持旧排版直到 S4 迁移**。
- 证据：收窄前后 12 张截图逐像素对比 **0.00%**（三视图 × 两视口 × 两主题），证明这是无视觉影响的纯作用域变更。
- 意义：S4 逐页删除降级规则时不再需要触碰外壳。

### S3-5 回归矩阵（本提交）

- 新增“每个顶级路由在外壳内可渲染”矩阵：11 条路由逐一断言 `h1` 可见、对应导航项 `aria-current="page"` 且唯一、无未捕获错误。
- `e2e/fixtures/console.ts` 补齐数组型 admin 端点（`role-bindings`、`organization-units`）：此前 catch-all 返回分页信封会让 Organization 页在 `.filter` 上崩溃，把 fixture 缺口伪装成产品回归。

## 4. 视觉证据

并排对比（左 = S2，右 = S3）：[light](assets/astryx-migration/s3-diff/shell-before-after--light.png) · [dark](assets/astryx-migration/s3-diff/shell-before-after--dark.png)。

| 视图 | S2 → S3 像素差 | 说明 |
| --- | --- | --- |
| Dashboard 1440 light/dark | 6.08% / 5.20% | 侧栏图标+选中态、260px 宽度、工具栏排版、移动栏（截图不含） |
| Documents 1440 light/dark | 5.28% / 5.39% | 同上（外壳变化，页面内容未动） |
| Roles 1440 light/dark | 5.93% / 6.15% | 同上 |
| 三页 390 light/dark | 0.38–0.68% | 侧栏隐藏；只有移动栏（菜单按钮、主题按钮）变化 |

页面高度全部不变（1167 / 900 / 939），业务内容未被外壳迁移带动。

截图集：

- 桌面 3 视图 × 2 主题：[s3](assets/astryx-migration/s3/dashboard--desktop--light.png)
- 移动 3 视图 × 2 主题：[s3](assets/astryx-migration/s3/dashboard--mobile--light.png)
- **移动抽屉（新增）**：[light](assets/astryx-migration/s3/mobile-drawer--mobile--light.png) · [dark](assets/astryx-migration/s3/mobile-drawer--mobile--dark.png)
- 空/错误态回归：[empty](assets/astryx-migration/s3/dashboard-empty--desktop--light.png) · [error](assets/astryx-migration/s3/dashboard-error--desktop--light.png)

## 5. 质量门禁（HEAD 复跑）

| 命令 | 结果 |
| --- | --- |
| `npm run lint` | **PASS**（0 error / 0 warning） |
| `npm run typecheck` | **PASS** |
| `npm test` | **PASS**（7 用例） |
| `npm run build` | **PASS**（9.48s） |
| `npm run test:e2e` | **PASS**（24 passed / 18 skipped opt-in） |
| `npm exec -- astryx doctor` | **PASS**（18 pass / 3 warn / 0 fail） |

e2e 构成：既有 4 条 + Foundation 3 条 + Dashboard 10 条 + **Shell 7 条**。

## 6. 体积影响

| 产物 | S2 | S3 | 说明 |
| --- | --- | --- | --- |
| CSS | 325.7 kB (gzip 53.9) | 324.5 kB (gzip 53.7) | 死代码清除抵消新增组件样式 |
| JS | 1,703.2 kB (gzip 556.8) | 2,027.9 kB (gzip 667.0) | +324.7 kB：Drawer/SideNav/Layout/focus/scroll-lock 进入主包（外壳每页都用） |
| chunk | 2（主 + Tooltip） | 1 | Drawer 把 Tooltip 并入主图 |

体积是 S3 的主要成本。S5 应评估按路由/组件拆分，或与 Astryx 上游确认 `astryx.css`/运行时裁剪；本轮不优化。

## 7. 回滚

每片独立提交，可单独 revert：

- S3-1 → 恢复到 `.nav-*` 手写导航与 248px 侧栏；
- S3-2 → 恢复工具栏 span 与两个原生按钮；
- S3-3 → 恢复 Radix 抽屉（需重新安装依赖）；
- S3-4 → 把降级作用域改回 `.app`；
- S3-5 → 删除新增用例与 fixture 端点。

无数据/后端变更。

## 8. 验收对照

| S3 验收项 | 结果 | 证据 |
| --- | --- | --- |
| React Router 仍是唯一导航真相源 | PASS | `LinkProvider` + `useLocation` 匹配；客户端导航（无整页刷新）用例 |
| 路由与 active 状态不变 | PASS | 11 条路由矩阵；嵌套路由 `/documents/:id` 仍选中 Documents |
| 移动抽屉：打开/关闭/遮罩/焦点/Esc/路由/触摸 | PASS | 抽屉 4 条用例（含 touch context） |
| 若无法自然接入则不引入复杂适配层 | PASS | 无 `as` 包装组件；LinkProvider 为官方机制 |
| Astryx doctor / 全部门禁 | PASS | §5 |
| 无意外布局回归 | PASS | §4 高度不变、页面内容像素级稳定（S3-4 0.00%） |

## 9. 未解决 / 移交 S4

1. **AppShell 未整体采用**：决策与理由见 §2；若未来要统一 TopNav 信息架构，需要单独评审，不属于 S4。
2. **JS +325 kB**：外壳组件进入主包；S5 处理。
3. **抽屉 Esc 分层语义**：tooltip 打开时首按 Esc 只关 tooltip（Astryx 设计），已在测试与报告中固化。
4. **页面内容仍是旧排版**：`.app .page-content` 降级继续保护未迁移页面，S4 逐页删除。
5. **`PageHeader`/`StatusPill`/`Empty`/`ErrorState`/`Loading` 仍是旧实现**：S4 逐页迁移。
6. Google Fonts 外部请求与 doctor 两条 warn：同 S1，S5 处理。
