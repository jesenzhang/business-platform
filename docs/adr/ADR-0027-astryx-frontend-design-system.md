# ADR-0027：Astryx 作为前端默认设计系统

> 状态：Accepted（范围为 S0–S2 试点 + S3–S5 路线；“默认设计系统”的最终确立以 S5 退出条件满足为前提）
> 日期：2026-11-18
> 决策所有者：`apps/business-console` 前端表现层
> 关联文档：`docs/plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md`、`docs/reports/2026-11-18-ASTRYX-MIGRATION-S0-BASELINE.md`
> 替代：无
> 被替代：无

## 1. 背景

`apps/business-console` 的全部视觉实现来自两份本地资产：`src/components.tsx` 中的 10 个共享展示组件，以及 `src/styles.css` 中约 70 条手写类规则与全局元素选择器。仓库没有组件库、没有设计 Token 契约，也没有视觉回归门禁。

由此产生的实际成本已经可见：

- 同一语义（状态徽标、空状态、错误提示）在不同页面重复实现；
- 深色主题依赖根节点 CSS 变量覆盖，图表颜色未适配（`axisLabel: #6e6e73`、`splitLine: #e5e5ea` 固定为浅色）；
- 移动端 shell 存在迁移前既有的布局缺陷（390px 视口下 `scrollWidth = 569`）；
- 无组件级样式契约，任何全局 CSS 调整都可能静默破坏某个页面。

引入成熟设计系统可以一次性解决 Token、主题、可访问性与组件语义问题。但该变更影响所有页面、引入新的全局依赖与 CSS 级联机制，属于 DOCUMENT_MANAGEMENT §4.2 所列“引入影响全局的框架或 SDK”，必须先形成长期决策。

## 2. 决策驱动因素

- **正确性**：组件默认样式不得被既有 reset/preflight 静默覆盖；
- **可逆性**：迁移必须按切片独立提交、独立回滚，禁止一次性重写；
- **可维护性**：终止“每个页面各写一套徽标/空状态”的重复；
- **生态与许可证**：Astryx 为 Meta 开源设计系统，包体通过 npm 发布（`@astryxdesign/*`，MIT 系许可，需在 S5 复核许可证清单）；
- **交付成本**：不允许借迁移重排业务逻辑、API、权限或状态机；
- **兼容约束**：React 19、Vite 7、Tailwind 3、Zustand、TanStack Query、ECharts 必须继续工作。

## 3. 候选方案

### 方案 A：继续手写 CSS 与本地组件

优点是零新依赖、零级联风险。缺点是重复实现与主题/可访问性债务继续累积，且没有组件契约可测试。否决。

### 方案 B：一次性全量替换

把 14 个页面、AppShell、全部表格与表单在一个变更内替换为 Astryx。优点是终态一致；缺点是无法定位回归、无法回滚、违反“禁止顺手重构”，且写操作页面（Roles 权限集合替换、Repair、Review）风险不可控。否决。

### 方案 C：以 Astryx 为目标默认设计系统，按切片渐进迁移（选定）

先建立一个可验证的共存基础（层序、主题接线、Foundation Check），再用一个真实业务页面（Dashboard）证明迁移不改变业务语义，最后按“读优先、写后置”的顺序逐页推进。每个切片独立提交、独立验收、独立回滚。

## 4. 决策

### D1. Astryx 是 `apps/business-console` 的目标默认设计系统

新页面与新组件默认查询并使用 Astryx 组件；历史实现按 PLAN-0016 的切片顺序替换。

### D2. 采用“共存优先”而非“替换优先”

Tailwind 3 保留至 S5 之后单独评估。级联顺序固定为：

```text
reset → tw-preflight → astryx-base → astryx-theme → (utilities 无层)
```

`@tailwind base` 必须位于 `tw-preflight` 层内；utilities 保持无层。不引入 Astryx 的 Tailwind v4 桥（`tailwind-theme.css`），因为它会改变既有 utility 语义。

### D3. 主题状态单一权威

`apps/business-console` 的 light/dark 状态唯一来源仍是 Zustand `theme` + `localStorage["business-console-theme"]`。Astryx `<Theme mode>` 消费该状态，不新增第二套主题 store、不读取 `prefers-color-scheme` 覆盖用户选择。

### D4. 直接使用官方组件，不建包装层

禁止为每个 Astryx 组件编写同构包装器。允许存在的适配只有两类：

1. 语义适配（例如“业务状态 → Astryx Badge 色调”的映射表）；
2. 布局适配（Astryx 组件无法直接接入 React Router 时的轻量组合）。

任何适配都必须有直接使用者，且不得改变原组件的可访问性语义。

### D5. 业务契约冻结

迁移不得修改 `src/api.ts`、`src/contracts.ts` 的对外语义，不得修改请求头、幂等键、乐观锁版本、提交载荷、确认流程、权限要求或数据口径。前端迁移不触发 Rust/OpenAPI/数据库变更。

### D6. 导航真相源不变

React Router 仍是唯一路由与 active 状态来源。若 Astryx 导航组件不能自然接入，则保留现有导航行为，只采用 Astryx 布局与 Token，不建立复杂适配层。

### D7. “默认设计系统”的正式确立有退出条件

只有当 S3（AppShell）与 S4（业务页面）验收通过、且 S5 的清退项（无调用者的历史 CSS/Radix、CI 视觉门禁、升级验证流程）完成后，才可宣称 Astryx 为默认设计系统。在此之前状态是“目标默认 + 试点已验证”。

## 5. 边界与非目标

本 ADR 不决定：

- Tailwind 主版本升级、pnpm 迁移或构建器替换；
- 替换 ECharts、TanStack Query/Table、React Router、Zustand；
- 任何服务端、权限、数据模型或部署变更；
- 自建 Design System 或主题市场；
- 使用 experimental Theme Editor 作为生产必需能力。

## 6. 后果

### 正面

- 组件默认样式与 Token 由库保证，减少重复实现；
- 主题（含图表配色）可统一按 Token 驱动；
- 迁移过程本身产生可执行的视觉/交互回归门禁；
- 每个切片可独立回滚，业务风险可控。

### 负面与成本

- 新增 4 个直接依赖与 1 个传递依赖（`lucide-react`），CSS 体积增加；
- 引入 CSS 级联层这一新的调试维度；层序错误会静默失效；
- 团队需要遵守 CLI 文档查询与 Token 使用规范。

### 风险

- **级联层序错误**：无层 reset/preflight 压制组件样式。缓解：`layers.css` 固定层序 + Foundation Check 的 computed-style 断言 + 每切片真实 Chromium 验证。
- **版本错配**：CLI 与 Core 不同版本产生不一致生成物。缓解：四包锁定同一版本，`astryx doctor` 纳入门禁。
- **迁移诱发业务回归**：缓解：D5 冻结契约，S4 写操作后置并保留/扩充既有 e2e。
- **主题双写**：缓解：D3 单一状态来源。

## 7. 实施

- 修改范围：`apps/business-console` 的表现层文件（`styles.css`、新增 `layers.css`、`main.tsx`、`components.tsx` 增量、`pages.tsx` 的 Dashboard）、e2e/tests、`AGENTS.md` 的 Astryx 管理区块、`docs/`；
- 迁移步骤与切片：见 PLAN-0016 §5（S0 基准 → S1 基础接入 → S2 Dashboard → S3 AppShell → S4 业务页面 → S5 规范固化）；
- 兼容策略：见 PLAN-0016 §6；
- 测试与验收：lint / typecheck / vitest / build / Playwright / `astryx doctor` + 真实 Chromium 截图与交互；
- 回滚：按切片还原依赖、样式与页面文件；无数据迁移，无后端回滚。

## 8. 验证证据

- S0 基准：`docs/reports/2026-11-18-ASTRYX-MIGRATION-S0-BASELINE.md`（基线 `505d203`、六项门禁 PASS、20 项组件清单、12 张截图、既有缺陷 D1–D6）；
- S1 证据：依赖锁定 0.7.16、`astryx doctor` 输出、层序与 Foundation Check 的 Chromium 断言、全页面无回归截图；
- S2 证据：Dashboard 状态/视口/主题矩阵 e2e、前后截图与差异说明。

（S1/S2 证据在对应阶段报告中补充引用，本 ADR 不预写结论。）

## 9. 后续复审条件

出现以下任一事实时重新评估本决策：

1. Astryx 主版本升级导致 Tailwind 3 共存配方失效；
2. `@astryxdesign/*` 的许可证或维护状态发生变化；
3. S3 或 S4 出现无法在不改变业务行为的前提下解决的兼容阻碍；
4. 前端技术栈整体迁移（React 主版本、Tailwind 4、构建器替换）；
5. S5 完成并确立默认设计系统满一个发布周期。
