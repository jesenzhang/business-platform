# Astryx 升级验证 Runbook

> 文档 ID：RUNBOOK-ASTRYX-UPGRADE
> 版本：1.0
> 状态：Baseline
> 生效日期：2026-11-18
> 适用：`apps/business-console`
> 关联：`docs/standards/FRONTEND_DESIGN_SYSTEM_STANDARD.md`、ADR-0027

## 1. 何时使用

升级 `@astryxdesign/core`、`@astryxdesign/cli`、`@astryxdesign/theme-neutral`、`@stylexjs/stylex`，或升级会改变 Astryx 传递依赖的包时。日常业务改动不需要本 Runbook。

## 2. 前置条件

- 工作树干净，当前分支可回滚；
- Node ≥ 22.13，npm 可用；
- 已了解目标版本的变化（`npx astryx docs`/发布说明）；
- 一次只升级一个版本组，不与业务改动混合提交。

## 3. 步骤

### 3.1 锁定新版本（同一提交）

```bash
cd apps/business-console
npm install --save-exact @astryxdesign/core@<v> @astryxdesign/theme-neutral@<v> @stylexjs/stylex@<v>
npm install --save-exact --save-dev @astryxdesign/cli@<v>
```

四个包的版本必须一致（core/theme/cli/stylex 版本组），`package-lock.json` 必须更新。

### 3.2 检查生成物与主题接线

```bash
npx astryx doctor          # 必须 0 failure；记录 warn 变化
git diff -- src/astryx-themes.ts
```

- `src/astryx-themes.ts` 是 CLI 生成文件，必须与其 record 一致；若 CLI 改写了它，提交生成结果，不要手改。
- 确认 `vite.config.ts` 中 `@astryxdesign/theme-neutral/fonts.css` 的 alias 仍然生效（字体自托管，禁止外部 CDN）。
- 若 `theme add/use` 重新引入 `fonts.css` 导入：保持生成文件不变，让 alias 继续生效；只有 alias 失效时才调整 `vite.config.ts`。

### 3.3 静态与单元门禁

```bash
npm run lint
npm run typecheck
npm test                   # 含设计系统守卫与 CSS 白名单
npm run build
```

### 3.4 行为与无障碍门禁

```bash
npx playwright install chromium   # 首次
npm run test:e2e                  # 交互契约 + 字体守卫 + axe serious/critical
```

任何失败都必须查明根因；不允许删除测试、放宽断言或更新快照来“修复”。

### 3.5 视觉评审（必须人工查看）

```bash
CAPTURE_DIR=../../docs/reports/assets/astryx-migration/upgrade-<v> npx playwright test e2e/screenshots.spec.ts
```

- 逐张查看 `route--*.png`（11 个路由）与 `mobile-drawer--*.png`；
- 与上一版本截图并排对比，记录差异与原因；
- 关注：组件默认样式、深浅色 Token、表格密度、表单控件、抽屉层级、图表坐标轴配色。

### 3.6 体积检查

```bash
npm run build
```

记录主包、`dashboard-page`、`pages`、`iam-pages` 的 gzip 体积；若主包增长超过 10%，在合并前说明原因或先做按需加载。

### 3.7 提交

单次升级一个提交，信息包含：目标版本组、doctor 结果、e2e 结果、体积变化、截图目录、回滚方式。

## 4. 回滚

```bash
git revert <upgrade-commit>
cd apps/business-console && npm ci
```

生成物（`astryx-themes.ts`）随提交一起回滚；无数据/后端变更。

## 5. 失败处理

| 现象 | 处理 |
| --- | --- |
| `astryx doctor` failure | 停止升级；检查版本组是否一致、生成文件是否被手改、层序是否被改动 |
| 组件样式被 reset 压制（padding/border 为 0） | 检查 `layers.css` 层序与 `@tailwind base` 是否仍在 `tw-preflight` 层 |
| 字体 CDN 请求重新出现 | 检查 `vite.config.ts` alias 与 `e2e/fonts.spec.ts` |
| axe 新增 serious/critical | 修复组件命名/landmark/ARIA，不允许仅下调阈值 |
| 视觉差异无法解释 | 回滚升级，改为单组件试验分支定位 |

## 6. 禁止事项

- 不得因为升级而批量重写业务页面；
- 不得跳过视觉评审直接合并；
- 不得在升级提交中混入业务逻辑或数据结构变更；
- 不得使用 `--force` 推送到共享分支。
