# S1 — Astryx 基础接入（Tailwind 3 共存 + 主题 + Foundation Check）

- **文档 ID:** REPORT-ASTRYX-S1-FOUNDATION
- **状态:** 已验收（S1 完成；S2 授权实施）
- **日期:** 2026-11-18
- **范围:** `apps/business-console`：Astryx 0.7.16 安装、CLI 诊断、Agent 文档、主题接线、Tailwind 3 级联层、Foundation Check、既有页面视觉回归
- **关联:** [PLAN-0016](../plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md)、[ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[S0 基准](2026-11-18-ASTRYX-MIGRATION-S0-BASELINE.md)
- **证据目录:** `docs/reports/assets/astryx-migration/s1/`、`docs/reports/assets/astryx-migration/s1-diff/`

## 1. 身份

| 项目 | 值 |
| --- | --- |
| 分支 | `feat/frontend-astryx-adoption-20261118` |
| 基线 | `505d203`（S0 提交 `fd50fd6`） |
| 阶段 | S1（前端表现层） |

## 2. 实际安装版本

全部**精确锁定**，不使用 `^`/`~`：

| 包 | 版本 | 位置 |
| --- | --- | --- |
| `@astryxdesign/core` | `0.7.16` | dependencies |
| `@astryxdesign/theme-neutral` | `0.7.16` | dependencies |
| `@stylexjs/stylex` | `0.19.2` | dependencies（core 的 peer `^0.19.0`） |
| `@astryxdesign/cli` | `0.7.16` | devDependencies |

- `package-lock.json` 记录同一版本组，`npm ci` 可复现；
- 传递依赖新增 `lucide-react@1.74.0`（theme-neutral 依赖）；
- `npm ls react react-dom @stylexjs/stylex --all` 显示 **单一 React 19.2.8 / react-dom 19.2.8 / stylex 0.19.2**，全部 deduped，无重复实例；
- 未升级 Tailwind（仍 3.4.19）、未升级 React、未引入 pnpm。

## 3. `astryx doctor`

命令：`npm exec -- astryx doctor`（完整输出：[astryx-doctor.txt](assets/astryx-migration/s1/astryx-doctor.txt)）

| 结果 | 数量 |
| --- | --- |
| pass | 18 |
| warn | 3 |
| fail | **0** |
| info | 9 |

三条 warn 的事实与处置：

| warn | 内容 | 判定 |
| --- | --- | --- |
| `themes`（聚合项） | 指向下一条 `theme-fonts` | 见下 |
| `theme-fonts` | “External font imports prevent proof for neutral: Figtree” —— `theme-neutral/fonts.css` 通过 `@import url(https://fonts.googleapis.com/…)` 加载 Figtree，CLI 无法离线证明字体服务会返回该字族 | **接受**：不引入本地字体二进制；字体缺失时回退到主题 Token 定义的系统字体栈（`Figtree, -apple-system, …`）。S5 需决定自托管或移除该导入 |
| `cli-on-path` | 说 `node_modules/.bin/astryx.cmd` “不是 Astryx CLI” | **CLI 探测误报**：该 shim 实测执行 `@astryxdesign/cli@0.7.16` 且 `--version` 输出 `0.7.16`；`npx astryx` 与 `npm exec -- astryx` 均解析到同一 CLI。失败数为 0，不构成阻塞 |

`npm exec -- astryx doctor --built dist` 返回 19 pass / 3 warn / 0 fail，并明确说明 `dist` 不是源码、不做组件用法检查（符合预期）。

## 4. 已阅读的实时 CLI 文档

在 `apps/business-console` 内（由项目版本应答）：

| 命令 | 关键结论（实际采用） |
| --- | --- |
| `astryx docs migration` | 迁移按 surface 切片，先建立共存基础再替换组件 |
| `astryx docs migration/migration-setup` | Tailwind v3 配方：`layers.css` 先声明 `reset, tw-preflight, astryx-base, astryx-theme`；`@tailwind base` 包进 `tw-preflight`；utilities 保持无层；主题在应用根挂载 |
| `astryx docs styling-libraries/tailwind` | 层序必须由首个样式表声明；**不引入** v4 桥 `tailwind-theme.css`（会改写既有 utility 语义） |
| `astryx docs theme` / `use-a-theme` | `theme add neutral` 生成 `src/astryx-themes.ts`；`<Theme theme mode>`；light/dark 由应用状态驱动 |
| `astryx component Button` | `label` 必填；`variant/isDisabled/isLoading`；`data-variant` 反射；primary 为填充按钮（无边框） |
| `astryx component Card` | 默认 `padding` 来自主题、`data-variant`（default/muted/…）、`elevation` |

附加按需阅读：`component Theme`、`component TextInput`、`component Table`（用于 Foundation Check 的正确 API）。

## 5. Agent 文档（`astryx init --agent codex`）

- 命令在 `apps/business-console` 执行，生成 **`apps/business-console/AGENTS.md`**（ASTRYX 管理区块 + 组件发现工作流），未改动仓库根 `AGENTS.md`。
- 幂等性：连续执行两次，文件哈希不变（`init` 只更新管理区块）。
- 仓库根 `AGENTS.md` 追加 **§17 前端设计系统（Astryx）**，明确：只改表现层、Tailwind 3 层序固定、主题单一状态、不建包装层、未迁移页面保持既有视觉、迁移不按模板重写页面、门禁清单。
- 既有约束完整性：写入前备份原文并做前缀比较，`after.StartsWith(before)` 为 **true**；`git diff --stat AGENTS.md` 显示 **仅 12 行新增、0 行删除**，原有架构/安全/测试约束一字未改。

## 6. 主题接线

- 单一状态来源：Zustand `theme` + `localStorage["business-console-theme"]`（未新增 store、未新增 context）。
- `src/app-root.tsx` 的 `AppRoot` 是唯一接线点：`<Theme theme={themes[defaultThemeSlug]} mode={theme}>`，读取同一 Zustand 值。
- 边界放在文档根（而不是每个页面）：Astryx 的 portal（dialog/toast/tooltip）渲染在 React 树之外，只有根级 `Theme` 会把 `data-astryx-theme`/`data-theme` 同步到 `<html>`，portal 才能拿到 Token。
- 主题 CSS 由 CLI 生成的 `src/astryx-themes.ts` 导入（`@astryxdesign/theme-neutral/theme.css` + `fonts.css`），未手写主题、未覆盖 Token。
- 验证：`e2e/foundation.spec.ts` 断言 `<html data-astryx-theme="neutral" data-theme="light">`；点击工具栏按钮后变为 `dark`、`.app` 同时加 `dark` 类、localStorage 写入 `dark`、Astryx Card 背景色随之变化；刷新后仍为 `dark`。

## 7. Tailwind 3 共存与级联层

### 7.1 最终层序

`src/layers.css`（`main.tsx` 第一个导入）：

```css
@layer reset, tw-preflight, astryx-base, astryx-theme;
```

`src/styles.css`：

```css
@layer tw-preflight { @tailwind base; }
@tailwind components;
@tailwind utilities;   /* 保持无层 */
```

`src/main.tsx` 导入顺序：`layers.css` → `@astryxdesign/core/reset.css` → `@astryxdesign/core/astryx.css` → `styles.css`；主题 CSS 经 `astryx-themes.ts` 随后加载。

构建产物验证（`dist/assets/index-*.css` 首行）：`@layer reset,tw-preflight,astryx-base,astryx-theme;` 出现在最前，且存在 `@layer tw-preflight{…preflight…}` 区块 —— 层序在打包后仍然成立。

### 7.2 Reset 审计与处置（S0 §6 清单的实际结果）

| S0 风险项 | 处置 | 依据 |
| --- | --- | --- |
| `@tailwind base` 无层 | 包进 `@layer tw-preflight` | 预检不得高于 `astryx-base` |
| `button, input, textarea { font: inherit }` | 移入 `@layer reset` | Astryx reset 同样声明该规则；层级更低后 Astryx 组件可覆盖 |
| `button { cursor: pointer }` | 移入 `@layer reset` | 避免覆盖 `aria-disabled` 光标语义 |
| 全局 `table/th/td/tr:last-child td` | 收敛为 `.table-wrap table/th/td/…` | 全部 10 处既有表格都在 `.table-wrap` 内；Astryx Table 不再被命中 |
| 全局 `textarea` / `textarea:focus` | 收敛为 `.review-sidebar textarea` | 全仓库唯一 textarea 在 Candidate Review 侧栏；保持原样式不变 |
| 主题排版重置（h1–h6/p/small/code/pre） | 在 `reset` 层用 `.app :where(...)` 降级 | 见 §8 |
| `:root` / `*` / `body` | 移入 `reset` 层 | 同值，无渲染差异 |
| `prefers-reduced-motion` 全局 `!important` | 保留无层 | 无障碍预期，且不改变布局 |

## 8. 未迁移页面的排版保护（关键决策）

Astryx `Theme` 会把 `data-astryx-theme` 同步到 `<html>`，neutral 主题的 `reset` 层用 `:where(h1..h6, p, small, code, pre)` 设置了字体、字号、颜色与行高。若不处理，**所有未迁移页面**会一起换字体/字号（实测桌面整页高度 1228→1223px，文字整体位移）。

处置：在 `styles.css` 的 `reset` 层新增 `.app` 作用域降级规则（恢复 preflight 原有的 normalisation：字体/字号/行高/颜色继承、margin 0、`small` 用 `smaller`、`code/pre` 用等宽栈），并给 `.app` 显式设置系统字体栈。

排序依据（两层保险）：

1. `.app :where(...)` 的特异度 (0,1,0) 高于主题的 `:where(...)` (0,0,0)，同层内必胜；
2. Astryx 组件排版位于**更晚**的 `astryx-base` 层，层级高于 `reset`，因此迁移后的组件样式不受影响；
3. 既有 `.page-header h1`、`.panel-heading h2` 等无层类规则仍然最高优先级。

结果：桌面三页像素差 **0%**（Dashboard/Documents/Roles light），仅 Roles dark 0.062%（见 §10）。逐页迁移时删除对应降级条目。

## 9. Foundation Check

- 入口：`src/foundation-check.tsx`（默认导出，`src/App.tsx` 中通过 `import.meta.env.DEV ? lazy(...) : null` 注册 `__foundation-check` 路由，不进入主导航）。
- 生产隔离证据：
  - 构建产物只有 `index-*.js` 一个 chunk，**没有** foundation chunk；
  - 生产 JS 中 `Foundation check` 文本出现 **0** 次；
  - 生产 CSS 层序与 Token 仍存在（主题接线有效）。
- 内容：`Button`（primary/secondary/disabled）、`TextInput`、`Card`（default/muted）、`Table`（两行两列），全部为官方组件，无包装层、无内联样式。
- Chromium 计算样式断言（全部通过）：

| 断言 | 实测 |
| --- | --- |
| 主题边界存在且唯一 | `<html data-astryx-theme="neutral">` |
| Button padding | `12px`（非 0） |
| Button 表面 | primary `rgb(27,27,27)`，secondary `rgba(0,0,0,0.06)`（非透明） |
| TextInput 字段盒 `.astryx-text-input` | padding `4px 8px`、`1px solid`、白色背景（非透明） |
| Card(default) | padding `11px`、`1px solid`、`rgb(255,255,255)` |
| Card(muted) | padding `12px`、表面 `rgb(241,241,241)`（该变体本就无边框） |
| Table 单元格 | padding `12px`，表头 `Name` 可见 |
| Token | `--color-accent` 解析为 `light-dark(#1b1b1b, #f1f1f1)`（非空、非 initial） |
| 移动端 | 390×844 无横向溢出 |

## 10. 既有页面视觉回归（真实 Chromium，固定 Mock）

方法：在 S0 基线提交 `fd50fd6` 的独立 worktree 中，用**同一份**截图脚本（含 ECharts 动画稳定采样）重采基线；与 S1 逐像素比较（通道阈值 24/765）。

| 视图 | 视口 | light | dark |
| --- | --- | --- | --- |
| Dashboard | 1440×900 | **0.000%** | **0.000%** |
| Documents | 1440×900 | **0.000%** | **0.000%** |
| Roles | 1440×900 | **0.000%** | 0.062% |
| 三页 | 390×844 | 59–60%（布局修复，见 §11） | 59–61%（同上） |

页面高度完全一致（Dashboard 1228、Documents 900、Roles 939），无文字换行/位移变化。

S1 截图：[dashboard light](assets/astryx-migration/s1/dashboard--desktop--light.png) · [dark](assets/astryx-migration/s1/dashboard--desktop--dark.png)；`metrics.json` 记录每张图的 `horizontalOverflow=false`。

**Roles dark 0.062% 的差异已定位为唯一有意变化**：主题在根节点设置 `color-scheme: dark`，浏览器原生 `<input type="checkbox">` 由浅色控件改为深色控件（见对比图 [chips-dark-compare.png](assets/astryx-migration/s1-diff/chips-dark-compare.png)，上=基线，下=S1）。判定：符合深色主题预期，可读性未降低；影响范围仅原生 checkbox/select（Roles、Bindings、Organization），S4 迁移这些表单时由 Astryx 组件接管。

## 11. 修复的既有缺陷（D1，S0 记录）

- 现象：390px 视口下 `scrollWidth=569`，移动端内容右移 179px 并被裁切。
- 根因（S0 实测）：`.app` 为 `display:flex` 行容器，`.mobile-bar` 与 `.main-content` 同处一行，`.mobile-bar` 被压缩到 178.5px。
- 修复：移动端媒体查询新增 `.app { flex-direction: column; }`（1 条声明，无 JS、无业务语义变化）。
- 验证：三页 390×844 全部 `scrollWidth == clientWidth == 390`，`horizontalOverflow=false`；移动端整页高度变化（1663→1725 等）正是内容不再被压缩/裁切的结果。
- 修复后移动端截图：[dashboard](assets/astryx-migration/s1/dashboard--mobile--light.png) · [documents](assets/astryx-migration/s1/documents--mobile--light.png) · [roles](assets/astryx-migration/s1/roles--mobile--light.png)。

## 12. 构建与体积影响

| 产物 | 迁移前 | S1 | 说明 |
| --- | --- | --- | --- |
| CSS | 17.74 kB (gzip 4.53) | 326.40 kB (gzip 54.07) | Astryx 全部组件 CSS + 主题 Token（`astryx.css` 为预编译全集） |
| JS | 1,551.49 kB (gzip 508.39) | 1,651.17 kB (gzip 540.21) | 新增 Theme 运行时；ECharts 仍占主体 |
| chunk 数 | 1 | 1 | Foundation Check 未进入生产包 |

体积增加是一次性成本，S5 可评估按需（`@astryxdesign/core/astryx.css` 的裁剪或 code split）；本轮不做。

## 13. 质量门禁（全部实际执行）

| 命令 | 结果 |
| --- | --- |
| `npm ci` | PASS（lockfile 与 package.json 一致，Astryx 版本被锁定） |
| `npm run lint` | **PASS**（0 error / 0 warning） |
| `npm run typecheck` | **PASS** |
| `npm test` | **PASS**（2 文件 / 7 用例） |
| `npm run build` | **PASS**（7.07s） |
| `npm run test:e2e` | **PASS**（7 passed，12 skipped=opt-in 截图） |
| `npm exec -- astryx doctor` | **PASS**（0 fail；2 条实质 warn 已在上文判定） |

e2e 明细：原有 4 条（smoke 1 + IAM 3，全部保持通过）+ 新增 3 条 Foundation Check（组件样式、主题切换与刷新保持、移动端无溢出）。

## 14. 变更文件

新增：

- `apps/business-console/src/layers.css`、`src/app-root.tsx`、`src/foundation-check.tsx`、`src/astryx-themes.ts`（CLI 生成）
- `apps/business-console/e2e/foundation.spec.ts`
- `apps/business-console/AGENTS.md`（CLI 生成）
- `docs/reports/2026-11-18-ASTRYX-MIGRATION-S1-FOUNDATION.md`、截图与诊断证据

修改：

- `apps/business-console/package.json` / `package-lock.json`（4 个 Astryx 包 + stylex）
- `apps/business-console/src/main.tsx`（样式导入顺序 + `AppRoot`）
- `apps/business-console/src/App.tsx`（仅 dev-only foundation 路由）
- `apps/business-console/src/styles.css`（层序、reset 归位、表格/textarea 收敛、排版降级、移动端 D1 修复）
- `apps/business-console/e2e/screenshots.spec.ts`（新增画布稳定采样，供证据复用）
- `AGENTS.md`（仅追加 §17）

未修改：`src/api.ts`、`src/contracts.ts`、`src/store.ts`、`src/pages.tsx`、`src/iam-pages.tsx`、`src/components.tsx`、Rust crates、`openapi.json`、migrations。

## 15. 回滚

单次 revert 本提交即可：移除 4 个依赖、`layers.css`、`app-root.tsx`、`foundation-check.tsx`、`astryx-themes.ts`，恢复 `main.tsx`/`App.tsx`/`styles.css`/`AGENTS.md`。无数据迁移、无后端依赖，回滚后页面回到 S0 基线（含 D1 移动端缺陷）。

## 16. 本轮仍未解决 / 移交后续

1. **Google Fonts 外部请求**（`theme-neutral/fonts.css`）：由 CLI 生成的模块引入；字体不可用时回退系统栈。S5 决定自托管或移除。
2. **Astryx CSS 全集 326 kB**：未做裁剪；S5 评估。
3. **排版降级规则**：目前覆盖全部 `.app` 内容，逐页迁移时必须删除对应条目，否则迁移后的页面仍用旧排版（组件层不受影响，但页面级文本会）。
4. **原生 checkbox 深色外观变化**（§10）：S4 迁移 Roles/Bindings/Organization 表单时由 Astryx 组件取代。
5. **根级 Theme 与 S3 的关系**：S3 迁移 AppShell 时，需重新审视 `.app` 排版降级规则与 shell 自身排版，一次性把 shell 交给 Astryx。
6. `astryx doctor` 的 `cli-on-path` 误报属 CLI 在 Windows 的探测行为，需上游修复；本地已用 `npx astryx` 规避。

## 17. 验收对照

| S1 验收项 | 结果 | 证据 |
| --- | --- | --- |
| npm 安装与 lockfile 可复现 | PASS | §2、`npm ci` |
| Astryx doctor 无失败 | PASS | §3（0 fail；warn 已判定） |
| lint / typecheck / test / build | PASS | §13 |
| Chromium 下组件基础样式正常 | PASS | §9 计算样式断言 |
| light/dark 可切换且刷新后保持 | PASS | §6 |
| 旧页面无非预期布局/交互回归 | PASS | §10 桌面 0% 像素差；唯一差异为原生控件深色化 |
| 无重复 React 实例或 CSS Reset 覆盖错误 | PASS | §2 `npm ls`；§7–§8 层序与降级规则 |
