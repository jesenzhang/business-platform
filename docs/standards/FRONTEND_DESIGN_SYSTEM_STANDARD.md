# 前端设计系统标准（Astryx）

> 文档 ID：STD-FRONTEND-001
> 版本：1.0
> 状态：Baseline
> 生效日期：2026-11-18
> 所有者：`apps/business-console` 前端表现层
> 关联：ADR-0027、PLAN-0016（已归档）、`apps/business-console/AGENTS.md`
> 替代：无

## 1. 目的与范围

本文件定义 `apps/business-console` 的前端设计系统基线：组件边界、主题与 Token、样式层级、CLI 使用、测试门禁和升级规则。适用于该应用的全部页面与共享组件；不改变 `src/api.ts`、`src/contracts.ts`、后端、权限或数据结构契约。

## 2. 权威顺序

发生冲突时：用户指令 → Accepted ADR（ADR-0027）→ 本标准 → 当前计划 → 实现。Astryx 官方文档（`npx astryx docs …`）是组件 API 的第一来源；本仓库的约束优先于 Astryx 的通用工作流建议（例如“每个页面从模板重建”不适用于本仓库的既有页面）。

## 3. 样式层级（不可更改的部分）

```text
layers.css（首个加载）: reset → tw-preflight → astryx-base → astryx-theme
styles.css:              @layer tw-preflight { @tailwind base }
                         @tailwind components; @tailwind utilities;  ← 无层
```

- 层序由 `src/layers.css` 唯一声明，必须先于任何 Astryx 样式表加载；
- `@tailwind base` 必须在 `tw-preflight` 层内，utilities 保持无层；
- 不得引入 Astryx 的 Tailwind v4 桥 `tailwind-theme.css`；
- 不得在页面级新增全局 CSS。`src/styles.css` 只允许外壳类（`.app/.sidebar/.brand/.main-content/.toolbar/.page-content/.mobile-bar*`），由 `tests/styles.test.ts` 强制；
- 应用 reset 只能位于 `reset` 层或 `src/styles.css` 的 `reset` 块内，不得无层，也不得定义会命中 Astryx 组件的元素选择器。

## 4. 组件三层边界

| 层 | 位置 | 职责 | 规则 |
| --- | --- | --- | --- |
| Foundation | Astryx 包（`@astryxdesign/core/*`） | Button、TextInput、Selector、CheckboxInput、Table、Card、Grid、Stack、Banner、EmptyState、Badge、Drawer、SideNav、CodeBlock、MetadataList 等 | 直接用官方组件与 props，不复制样式；不建同构包装层 |
| Business | `src/components.tsx` | `PageHeader`、`StatusPill`、`Loading`、`Empty`、`ErrorState`、`IdLink`；`AppShell` 外壳 | 只做语义映射（状态→Badge 色调）与跨页面一致的组合；任何新增必须至少有两个路由使用，否则留在页面模块 |
| Page | 路由模块（`pages.tsx`、`iam-pages.tsx`、`dashboard-page.tsx`） | 路由级数据获取、布局组合、页面私有渲染 | 每页只组合 Foundation/Business 组件；不写全局 CSS、不新增请求、不改契约 |

补充规则：

- 共享的领域视图（`JobsTable`、`AuditList`）放在使用它们的页面模块并导出，供同域页面与 Dashboard 复用；
- 路由级代码分割：每个路由模块可被 `React.lazy` 加载，`AppShell` 提供一个 `Suspense` 边界；
- 不嵌套 `Card`；需要并列区块时用同级 `Card` 或 `Section`；
- `Selector` 的可视 label 不会命名 combobox，必须显式传 `aria-label`。

## 5. 主题与 Token

- 主题唯一来源：Zustand `theme` + `localStorage["business-console-theme"]`，根级 `<Theme mode>` 消费；不得新增第二套主题状态；不得用 `prefers-color-scheme` 覆盖用户选择。
- 颜色/间距/圆角/字体一律使用 Astryx Token 或组件 props；页面代码不得出现硬编码 `#hex`/`px`（例外见 §8）。
- 字体：Figtree 通过 `@fontsource` 自托管（400/500/600/700）。`@astryxdesign/theme-neutral/fonts.css` 是 Google Fonts `@import`，由 `vite.config.ts` 的 alias 重定向到 `src/fonts-override.css`；生成的 `src/astryx-themes.ts` 保持与 CLI record 一致，不得手改。`e2e/fonts.spec.ts` 阻止字体 CDN 请求回归。
- ECharts 颜色：坐标轴/网格从主题 Token 解析（隐藏探针求值 `light-dark()`），分类色板保持稳定以承载状态语义。

## 6. CLI 使用规范

| 场景 | 命令 |
| --- | --- |
| 找组件/文档/模板 | `npx astryx search "<query>"` |
| 读组件 API | `npx astryx component <Name>` |
| 读专题文档 | `npx astryx docs <topic>`（migration、theme、tokens、styling-libraries/tailwind…） |
| 健康检查 | `npm exec -- astryx doctor`（必须 0 failure；已知 warn 见 §8） |
| 主题变更 | `npx astryx theme add/use <slug>`，随后检查 `src/astryx-themes.ts` 是否仍与 record 一致、字体 alias 是否仍然生效 |
| 升级 | 按 `docs/operations/ASTRYX_UPGRADE_RUNBOOK.md` 执行 |

`apps/business-console/AGENTS.md` 由 `npx astryx init --agent codex` 生成或更新；其通用工作流建议不得覆盖本标准的迁移约束。

## 7. 测试与质量门禁

在 `apps/business-console` 下，每个前端变更至少执行：

```text
npm run lint
npm run typecheck
npm test
npm run build
npm run test:e2e
npm exec -- astryx doctor
```

其中：

- `npm test` 包含设计系统守卫（`tests/styles.test.ts`：CSS 类白名单、层序、无远程样式表）；
- `npm run test:e2e` 包含交互契约（导航、抽屉、表单、上传、复核）、字体守卫（`fonts.spec.ts`）与无障碍底线（`a11y.spec.ts`，serious/critical 为 0）；
- 视觉证据通过 `CAPTURE_DIR=<dir> npx playwright test e2e/screenshots.spec.ts` 生成，用于评审与前后对比，不作为像素快照门禁；
- 更新或删除任何断言必须说明原因，禁止用删除测试、放宽断言或更新快照掩盖失败。

## 8. 例外登记（需评审）

| 例外 | 位置 | 原因 |
| --- | --- | --- |
| 外壳几何（fixed rail、sticky toolbar、移动栏）与 shell 变量 | `src/styles.css` | 设计系统不拥有应用外壳布局；由 CSS 白名单测试约束范围 |
| `.brand` 的自绘标记与两行文字 | `src/styles.css`、`src/components.tsx` | 品牌标识是产品资产，不映射到 Astryx 组件 |
| ECharts 分类色板 | `src/dashboard-page.tsx` | 承载状态语义，跨主题保持一致 |
| 组织树以文本缩进渲染 | `src/iam-pages.tsx` | Astryx Table 不支持嵌套行；层级语义保留 |
| `astryx doctor` 的 3 条 warn（字体包、CLI PATH 探测、聚合项） | 环境 | 字体已自托管；CLI PATH 为 Windows 探测误报，已在本地验证 |

## 9. 变更政策

1. 新页面默认使用 Astryx 组件，并按本标准的组件分层放置。
2. 不允许“为未来复用”提前抽象；新增 Business 组件必须有至少两个路由的使用者。
3. 组件库升级不得自动批量改写业务页面；升级必须走 Runbook，并逐切片评审。
4. 任何改变样式层级、主题权威或组件边界的变更必须先更新 ADR-0027 或本文件。
5. 删除历史代码仅在确认无调用者且回归通过后进行。

## 10. 验收

- `src/styles.css` 仅包含外壳类，且守卫测试通过；
- 全部业务路由由 Astryx 组件渲染，无页面级全局 CSS；
- 字体无外部 CDN 请求；
- `astryx doctor` 0 failure；
- CI 的 frontend 与 frontend-e2e 作业通过（含 axe 底线）；
- 升级 Runbook 可执行、可回滚。
