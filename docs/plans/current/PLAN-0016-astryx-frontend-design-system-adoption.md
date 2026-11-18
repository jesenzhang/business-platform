# PLAN-0016 — Astryx 前端设计系统渐进迁移

- **文档 ID:** PLAN-0016
- **版本:** 1.0
- **状态:** Active（S0 已完成，S1/S2 授权实施；S3–S5 仅制定路线）
- **日期:** 2026-11-18
- **所有者:** `apps/business-console`（前端表现层）
- **关联文档:** `docs/reports/2026-11-18-ASTRYX-MIGRATION-S0-BASELINE.md`、`docs/adr/ADR-0027-astryx-frontend-design-system.md`、`docs/governance/DOCUMENT_MANAGEMENT.md`
- **替代/被替代:** 无
- **迁移分支:** `feat/frontend-astryx-adoption-20261118`（基线 `505d203`）

## 1. 目标

把 Astryx 建立为 `apps/business-console` 的默认前端设计系统，并用可回滚的切片逐步替换历史 UI 实现。成功标准是：

1. Astryx 与现有 React 19 / Vite 7 / Tailwind 3 / Zustand / TanStack Query / React Router / ECharts 可靠共存；
2. 至少一个真实业务页面（Dashboard `/`）在保持 API 契约、数据口径、加载/错误状态和导航行为不变的前提下完成迁移，并有真实 Chromium 验证；
3. 形成 S3–S5 可继续执行、每一步都能独立验收与回滚的路线。

## 2. 非目标

本计划不授权：

- 修改业务逻辑、状态机、API、权限、数据结构、后端或数据库；
- 修改 Accepted ADR、服务端架构 Baseline 或其他执行计划；
- Tailwind 3 → 4 升级、pnpm 迁移、构建工具替换；
- 自建 Design System 或为每个 Astryx 组件编写包装层；
- 替换 ECharts、TanStack Query、TanStack Table、React Router、Zustand；
- 使用 experimental Theme Editor 作为生产必需能力；
- 引入微服务、Agent Runtime 或额外后端进程；
- 在 S2 验收前迁移共享 AppShell（S3）或业务页面（S4）。

## 3. 架构预检

| 项目 | 结论 |
| --- | --- |
| 目标能力 | 前端表现层组件与主题统一；不新增业务能力 |
| Bounded Context / 数据所有者 | 不涉及。前端只消费既有 Query API；Document Management、Document Processing、Governance、Identity & Authorization 的权威数据所有者不变 |
| 统一语言 / 业务不变量 | 不变。`setRolePermissions` 仍为完整集合替换；乐观锁版本、幂等键、确认流程、权限边界全部保持 |
| 公开契约 | 不变。`src/api.ts` / `src/contracts.ts` 不因迁移修改；不触碰 `openapi.json` |
| 一致性与安全 | 不变。迁移不新增请求、不缓存业务数据、不改变 Authorization 头或租户范围；截图与测试只使用合成固定数据 |
| 质量属性 | 影响：前端可维护性与可替换性、首屏 CSS 体积与渲染正确性、无障碍对比度、深色主题一致性。不影响服务端性能、可用性、RPO/RTO |
| Fitness Functions | 新增 CSS 层序与组件基础样式断言；其余沿用 lint / typecheck / vitest / build / Playwright |

## 4. 修改范围

允许变更：

- `apps/business-console/package.json`、`package-lock.json`（仅新增 Astryx 相关依赖）；
- `apps/business-console/src/styles.css`（层序与 reset 归属调整）、新增 `src/layers.css`；
- `apps/business-console/src/main.tsx`（Theme Provider 与样式导入顺序）；
- `apps/business-console/src/components.tsx`（仅增量：主题接线、新增 Adapter 之一，不重写既有组件契约）；
- `apps/business-console/src/pages.tsx`（S2 仅 `DashboardPage` 及其私有展示代码）；
- `apps/business-console/e2e/**`、`apps/business-console/tests/**`（新增验证，不删除既有断言）；
- `AGENTS.md`（仅追加 Astryx 管理区块）；
- `docs/`（本计划、ADR-0027、S0/S1/S2 报告与截图资产）。

不得变更：

- `crates/**`、`apps/*-api|worker|cli|migration/**`、`migrations/**`、`openapi.json`；
- `apps/business-console/src/api.ts`、`contracts.ts`、`store.ts` 的对外语义；
- `apps/business-console/src/iam-pages.tsx`；
- `docs/architecture/**` 与既有 Accepted ADR。

## 5. 切片

### S0 — 真实现状盘点与迁移基准（已完成）

- 从 `origin/main` `505d203` 建分支；记录基线 HEAD、依赖版本、质量门禁结果；
- 清点 `components.tsx` / `pages.tsx` / `iam-pages.tsx` / `styles.css` 全部共享组件与调用者；
- KEEP / REPLACE / ADAPT / DEFER 决策矩阵；
- Astryx 0.7.16 依赖兼容性与 Tailwind 3 共存策略；
- CSS Layer 风险清单、移动端既有布局缺陷、测试覆盖缺口；
- 12 张固定 Mock 的桌面/移动 × 浅色/深色基准截图 + 溢出度量。

验收：不做用户可见业务页面修改；所有拟替换组件识别了使用者；明确变更/禁改文件；兼容性阻碍有解决办法。

### S1 — Astryx 基础接入

1. 安装 `@astryxdesign/core@0.7.16`、`@astryxdesign/theme-neutral@0.7.16`、`@stylexjs/stylex@0.19.2`、devDependency `@astryxdesign/cli@0.7.16`；`package-lock.json` 必须记录同一版本组。
2. `npm exec -- astryx doctor` 输出留档。
3. 阅读实时 CLI 文档：`astryx docs migration`、`astryx docs styling-libraries/tailwind`、`astryx docs theme`、`astryx component Button`、`astryx component Card`。
4. `npm exec -- astryx init --agent codex`，只向 `AGENTS.md` 追加 Astryx 管理区块；`git diff` 必须显示既有架构/安全/测试约束零删除。
5. 主题：复用 Zustand `theme` 与 `business-console-theme` localStorage key 驱动 `<Theme mode>`，不新建主题状态。
6. 层序：`src/layers.css` 声明 `@layer reset, tw-preflight, astryx-base, astryx-theme;` 并先于 `styles.css` 加载；`@tailwind base` 包进 `tw-preflight`；`@tailwind utilities` 保持无层。审计 `styles.css` 中所有无层全局选择器，归入 `reset` 层或收敛作用域。
7. Foundation Check：dev-only 路由 + 独立 e2e fixture，生产构建不得包含该入口。
8. Chromium 断言 Button / Card / TextInput / Table 的 `padding`、`border`、`background` 非零，排除 CSS 层冲突。
9. 修复 S0 发现的既有移动端 shell 横向溢出（1 处 CSS，独立记录，不含业务语义变化）。

验收（全部必须 PASS）：`npm ci` 可复现；`astryx doctor` 无失败；lint / typecheck / test / build / test:e2e 通过；Chromium 组件基础样式正确；light/dark 切换且刷新保持；旧页面无非预期回归；无重复 React 实例。

### S2 — Dashboard 真实页面试点

- 保留 `getOverview`、TanStack Query `refetchInterval`、ECharts 图表语义、加载/错误状态、跳转行为；
- 四个统计指标卡片与页面展示区域改用 Astryx `Card` + 布局/文本基础组件；
- 使用 Astryx Token（间距/颜色/字体/圆角），删除本页对新代码无必要的硬编码样式；
- 不重写共用 `PageHeader` / `StatusPill`（除非确认全部调用者）；
- ECharts 颜色按主题 Token 适配，数据口径不变；
- 新增 Playwright：正常、空数据、错误、刷新更新、到 Processing/Audit 导航、1440×900、390×844、light/dark、无横向滚动、无裁切溢出；
- 前后截图 + 差异说明。

验收：真实业务契约不变；原有测试全绿；新增浏览器测试通过；无重复布局/状态/请求代码；新增依赖与抽象都有直接使用者。

### S3 — 共享 AppShell（下一轮，本轮不实施）

- 评估 Astryx `AppShell` / `TopNav` / `SideNav` / `MobileNav` / `Layout`；
- React Router 保持唯一导航真相源；路由表与 active 状态不变；
- 移动端导航若替换 Radix Dialog，必须验证打开/关闭/遮罩/焦点管理/Esc/路由跳转/触摸；
- 若 Astryx 导航无法自然接入 React Router，则保留现有导航行为，只采用 Astryx 布局与 Token，不引入复杂适配层。

### S4 — 逐业务页面（本轮不实施）

顺序与切片边界：

| 切片 | 页面 | 优先级 |
| --- | --- | --- |
| S4-A | Documents List/Detail → Upload | 读优先，写后置 |
| S4-B | Processing List/Detail → Candidate Review | 写操作保持版本/幂等/确认 |
| S4-C | Findings、Audit → Repairs | Repairs 为高风险写 |
| S4-D | Identities、Roles、Bindings、Organization、Explain | Roles 权限集合替换必须有扩充电 e2e |

约束：所有写操作保持既有授权要求、乐观锁 `expected_version`、`Idempotency-Key`、提交载荷、确认流程与错误处理；Roles 的完整集合替换不得因 UI 迁移清空既有权限键。

### S5 — 默认规范固化（本轮不实施）

- 新页面默认使用 Astryx 组件；
- 明确基础组件 / 业务组件 / 页面组件边界；
- 固化 CLI 使用与升级验证流程；
- 建立 Token 与主题规范；
- 删除确认无调用者的历史 CSS 与 Radix 依赖；
- 将 `astryx doctor`、视觉与交互测试纳入 CI；
- 禁止仅因组件库升级而批量改写业务页面。

## 6. 兼容策略

### 6.1 Tailwind 3 共存

Astryx 官方 Tailwind v3 配方（`astryx docs migration/migration-setup`）：

```css
/* layers.css，先加载 */
@layer reset, tw-preflight, astryx-base, astryx-theme;
```

```css
/* styles.css */
@import '@astryxdesign/core/reset.css';
@import '@astryxdesign/core/astryx.css';
@import '@astryxdesign/theme-neutral/theme.css';

@layer tw-preflight { @tailwind base; }
@tailwind components;
@tailwind utilities;
```

要点：层序必须在任何 `@import` 之前声明；预检进 `tw-preflight`（低于 `astryx-theme`）；utilities 保持无层以维持既有 utility 优先级；**不引入** `tailwind-theme.css` 桥（它会改写既有 utility 语义）。

### 6.2 主题

单一主题状态：Zustand `theme` + `business-console-theme`；`<Theme theme={neutralTheme} mode={theme}>` 位于 `QueryClientProvider` 内、`BrowserRouter` 外，确保所有路由共享同一 Token 集。

## 7. 风险与回滚

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 无层 reset/preflight 覆盖组件样式 | 组件失去 padding/边框，静默失效 | 层序 + Foundation Check computed-style 断言 |
| `styles.css` 无层全局元素选择器（`button`/`textarea`/`table`）压制 Astryx | 局部样式异常 | 归入 `reset` 层或收敛选择器；逐个验证 |
| CLI 与 Core 版本错配 | 生成物不一致 | 四包锁定 0.7.16，doctor 校验 |
| 主题双写 | light/dark 状态分裂 | 复用 Zustand，不新增 provider 状态 |
| 迁移引入业务回归 | 契约/权限破坏 | 禁改 api/contracts/iam；e2e 行为断言 |
| Dashboard 视觉改动被误判为回归 | 验收争议 | 前后截图 + 文字差异说明 |

回滚：每个切片独立提交。S1 回滚 = 还原 `package.json`/`package-lock.json`/`layers.css`/`styles.css`/`main.tsx`/`AGENTS.md`；S2 回滚 = 仅还原 `DashboardPage` 与其私有展示代码 + 新增 e2e。业务数据与后端无迁移，无数据回滚需求。

## 8. 测试与质量门禁

在 `apps/business-console` 下每个切片执行：

```text
npm run lint
npm run typecheck
npm test
npm run build
npm run test:e2e
npm exec -- astryx doctor
```

涉及已迁移 UI 时额外执行真实 Chromium 交互与截图验证。仅前端变更不要求重跑全部 Rust 集成测试；一旦触碰 Rust、OpenAPI、安全契约或跨层行为，按 `AGENTS.md` 执行完整门禁。

## 9. 文档同步

- S0/S1/S2 各自产出 `docs/reports/` 报告；
- 本计划在 S2 验收后更新状态，S5 完成后归档；
- 引入全局前端框架由 `ADR-0027` 记录；
- `docs/README.md` 登记本计划与 ADR。

## 10. 完成定义

S0–S2 完成定义：

1. 三个切片各自独立提交，均可单独回滚；
2. 质量门禁全部 PASS（或明确记录 FAIL/BLOCKED/NOT RUN 及原因）；
3. Dashboard 在真实 Chromium 下通过状态/视口/主题矩阵；
4. Tailwind 3 与 Astryx 层序可验证、无静默覆盖；
5. 形成 S3–S5 切片、依赖与风险清单，并给出继续与否的 GO / NO-GO 结论。
