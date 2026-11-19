# S5 — 默认开发规范固化（Astryx 成为默认设计系统）

- **文档 ID:** REPORT-ASTRYX-S5-STANDARDS
- **状态:** 已验收（S5 完成；PLAN-0016 归档）
- **日期:** 2026-11-18
- **范围:** `apps/business-console` 的包体、字体、CSS 边界、无障碍、CI 与前端规范
- **关联:** [ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[STD-FRONTEND-001](../standards/FRONTEND_DESIGN_SYSTEM_STANDARD.md)、[升级 Runbook](../operations/ASTRYX_UPGRADE_RUNBOOK.md)、[S4](2026-11-18-ASTRYX-MIGRATION-S4-PAGES.md)
- **证据目录:** `docs/reports/assets/astryx-migration/s5/`（可选截图）、CI 配置、测试文件

## 1. 身份

| 项目 | 值 |
| --- | --- |
| 分支 | `feat/frontend-astryx-adoption-20261118` |
| 起点 | `6fc89a8`（S4） |
| 切片提交 | `4780070` S5-1 路由分割 · `d3d1dc1` S5-2 字体自托管 · `7633650` S5-3 CSS 边界 · `633fb50` S5-4 CI/axe · 本报告（S5-5 规范与归档） |

## 2. S5-1 路由级代码分割

- Dashboard 独立成 `src/dashboard-page.tsx`（唯一使用 ECharts 的模块），全部路由改为 `React.lazy`；`AppShell` 在 outlet 外包一个 `Suspense`，外壳不会闪烁。
- 结果（生产构建，gzip）：

| chunk | S4 | S5 |
| --- | --- | --- |
| 主包 `index` | 2,245.1 kB (742.2) | **807.2 kB (263.2)** |
| `dashboard-page`（ECharts） | — | 1,149.5 kB (386.6)，仅 `/` 加载 |
| `pages`（documents/processing/governance） | — | 62.9 kB (20.1) |
| `iam-pages` | — | 58.0 kB (18.4) |
| `textEntryTraits` / `BottomSheet` | — | 163.4 / 72.0 kB（表单与触摸选择器按需） |

非 Dashboard 路由的首屏 JS 从 2.24 MB 降到约 0.81 MB。

## 3. S5-2 字体自托管

- 引入 `@fontsource/figtree@5.3.0`（400/500/600/700），在 `main.tsx` 引入。
- `@astryxdesign/theme-neutral/fonts.css` 是 Google Fonts `@import`；`vite.config.ts` 用 alias 将其重定向到空文件 `src/fonts-override.css`。生成的 `src/astryx-themes.ts` 保持与 CLI record 完全一致，`astryx doctor` 仍能校验生成物。
- `e2e/fonts.spec.ts`：断言无 `fonts.googleapis/gstatic` 请求、Figtree 已注册、h1 实际使用 Figtree。
- `astryx doctor` 恢复 **0 failure**（3 条已登记 warn）。
- 新增 `@types/node`（vite 配置类型）与 `tsconfig.node.json` 的 `types: ["node"]`。

## 4. S5-3 CSS 边界

- Dashboard 的 3 个 `.text-button` 链接改为 Astryx `Link`；`styles.css` 只剩外壳类。
- 新增 `tests/styles.test.ts`（3 个断言）：
  1. `styles.css` 中的类选择器必须属于外壳白名单（12 个），否则失败；
  2. `layers.css` 必须恰好是 `@layer reset, tw-preflight, astryx-base, astryx-theme;`；
  3. 两个样式表不得包含远程 `@import url(...)` 或字体 CDN。
- 该守卫进入 `npm test`，因此 CI 自动执行。

## 5. S5-4 CI 与无障碍底线

- `.github/workflows/ci.yml` 的 `frontend` 作业在 build 后执行 `npm exec -- astryx doctor`；YAML 已解析校验。
- 新增 `@axe-core/playwright@4.14.0` 与 `e2e/a11y.spec.ts`：对 Dashboard、Documents、Roles、Audit、Candidate、Findings 六个路由按 `wcag2a/2aa/21a/21aa` 扫描，serious/critical 必须为 0。
- 实测：六个路由 **0 violations（任意 impact 级别）**；另对 Dashboard/Documents/Roles 做了全量复查，同样为 0。
- 既有的 `frontend-e2e` 作业运行全部 41 条 Playwright 用例（导航、抽屉、上传、复核、IAM 写契约、字体、axe），交互契约与无障碍均 CI 门禁。

## 6. S5-5 规范与归档

| 产物 | 说明 |
| --- | --- |
| [`docs/standards/FRONTEND_DESIGN_SYSTEM_STANDARD.md`](../standards/FRONTEND_DESIGN_SYSTEM_STANDARD.md) | Baseline：样式层级、组件三层边界、Token、CLI 使用、门禁、例外登记、变更政策 |
| [`docs/operations/ASTRYX_UPGRADE_RUNBOOK.md`](../operations/ASTRYX_UPGRADE_RUNBOOK.md) | 升级步骤（版本组、生成物检查、静态/行为/视觉门禁、体积、回滚、失败处理、禁止事项） |
| [ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md) | 状态更新为“S0–S5 完成，默认设计系统”；证据与复审条件更新 |
| `PLAN-0016` | 归档至 `docs/plans/archive/2026/`，记录最终提交、验收、未完成项与承接文档 |
| `AGENTS.md` §17 | 指向标准与 Runbook，明确“升级不得批量改写业务页面”与字体自托管约束 |
| `docs/README.md` | 标准表、运行文档、当前实施与计划链接更新 |

## 7. 质量门禁（最终状态实跑）

| 命令 | 结果 |
| --- | --- |
| `npm run lint` | **PASS**（0 error / 0 warning） |
| `npm run typecheck` | **PASS** |
| `npm test` | **PASS**（10：api 2、iam 5、design-system guard 3） |
| `npm run build` | **PASS**（8.68s） |
| `npm run test:e2e` | **PASS**（41 passed / 19 skipped opt-in） |
| `npm exec -- astryx doctor` | **PASS**（0 fail / 3 warn / 18+ pass） |

## 8. 未解决与已知限制

1. **Dashboard chunk 仍约 1.15 MB**（ECharts 6 + 图表适配）：只在 `/` 加载；S5 已把它从主包剥离，进一步压缩需要评估 ECharts 按需引入（`echarts/core` + 具体图表）或图表懒挂载。
2. **axe 只对 serious/critical 设门禁**：moderate/minor 仍会输出但不阻塞（当前实测为 0）。
3. **视觉截图仍为人工评审**：`CAPTURE_DIR` 生成 11 个路由 + 抽屉截图，未建立像素快照 CI（避免跨平台字体渲染噪声）。
4. **`astryx doctor` 3 条 warn** 已登记在标准 §8：字体包探测、Windows CLI PATH 误报、聚合项。
5. **`@fontsource` 带来约 130 kB 字体资产**（woff/woff2，按 unicode-range 分片）；相比外部 CDN 是确定的体积成本。
6. **Safari/Firefox 仍未纳入 e2e**；当前门禁为 Chromium。
7. **组织树以文本缩进表达层级**，未实现展开/折叠交互。

## 9. S5 验收对照

| S5 验收项 | 结果 | 证据 |
| --- | --- | --- |
| 新页面默认 Astryx 组件 | PASS | STD-FRONTEND-001 §4/§9；全部路由已迁移 |
| 基础/业务/页面组件边界 | PASS | 标准 §4；实际代码分层 |
| 固化官方 CLI 使用方法 | PASS | 标准 §6 + Runbook §3 |
| 设计 Token 与主题规范 | PASS | 标准 §5；字体自托管、图表 Token 解析 |
| 移除无调用者历史 CSS/依赖 | PASS | S4 清理 + `.text-button` 移除；Radix/TanStack 已删 |
| 视觉与交互测试纳入 CI | PASS | `frontend-e2e` 作业 41 条用例；截图人工评审流程 |
| `astryx doctor` 纳入 CI | PASS | `frontend` 作业新增步骤 |
| Astryx 升级验证流程 | PASS | Runbook（版本组、检查、门禁、回滚） |
| 禁止升级触发批量改写 | PASS | 标准 §9 + 根 AGENTS §17 |

## 10. 结论

S5 完成，**Astryx 0.7.16 自 2026-11-18 起为 `apps/business-console` 的默认设计系统**。S0–S5 全部验收，PLAN-0016 归档；后续由 STD-FRONTEND-001 与升级 Runbook 承接，不再单列迁移计划。
