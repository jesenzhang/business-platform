# S4 — 逐业务页面迁移（Documents → IAM）

- **文档 ID:** REPORT-ASTRYX-S4-PAGES
- **状态:** 已验收（S4 完成；S5 未实施）
- **日期:** 2026-11-18
- **范围:** `apps/business-console` 全部 11 个业务路由的页面内容与共享展示组件
- **关联:** [PLAN-0016](../plans/current/PLAN-0016-astryx-frontend-design-system-adoption.md)、[ADR-0027](../adr/ADR-0027-astryx-frontend-design-system.md)、[S3](2026-11-18-ASTRYX-MIGRATION-S3-APPSHELL.md)
- **证据目录:** `docs/reports/assets/astryx-migration/s4/`、`s4-diff/`

## 1. 身份

| 项目 | 值 |
| --- | --- |
| 分支 | `feat/frontend-astryx-adoption-20261118` |
| 起点 | `611d32f`（S3） |
| 切片提交 | `c67d7aa` S4-0 共享原语 · `96a28fe` S4-A1 Documents · `9234efd` S4-A2/B1/B2 · `5718d35` S4-C1 · `8ddc4f0` S4-C2 · `ac365fa` S4-D1..D4 · 本报告 |
| 修改文件 | `src/components.tsx`、`src/pages.tsx`、`src/iam-pages.tsx`、`src/styles.css`、`package.json`/`package-lock.json`、`e2e/**` |

**切片粒度说明**：S4-A2/B1/B2 共用 `pages.tsx`、S4-D1..D4 共用 `iam-pages.tsx`，各自以一个提交交付（回滚时可整段 revert）；其余切片独立提交。

## 2. 共享原语（S4-0）

| 组件 | 迁移后 | 说明 |
| --- | --- | --- |
| `PageHeader` | `Heading level={1}` + `Text` label/supporting + `HStack` | props 不变，12 个路由同时获得设计系统标题 |
| `StatusPill` | `Badge` + 一张显式 `STATUS_TONE` 映射表 | 未知值降级 neutral；不再依赖 `.status-*` 类 |
| `Loading` | `Spinner`（居中） | 文案保留 `Loading live data…` |
| `Empty` | `EmptyState isCompact` | 文案由调用方提供 |
| `ErrorState` | `Banner status="error"` | 标题 + 有界 `error.message` |
| `IdLink` | Astryx `Link`（经 LinkProvider → React Router） | 保留短 UUID 显示 |

## 3. 逐页迁移与契约保持

| 路由 | 迁移内容 | 保持的契约 |
| --- | --- | --- |
| `/documents` | Card + `FileInput` + `Button` + Astryx `Table`（替换 TanStack core-row-model 包装） | 上传 multipart、`Idempotency-Key`、成功后整页跳转、有界错误、`limit=50` |
| `/documents/:id` | Grid/Card/`MetadataList`/`Button` + 迁移后的 `JobsTable` | `getDocument`、`listDocumentJobs`、`startProcessing` 携带 `content_revision` |
| `/processing` | Card + `JobsTable` | `listJobs(50)` |
| `/processing/:id` | `MetadataList` + 用 `StatusDot`/`Badge` 渲染的固定 6 步 pipeline + `Button` 跳转 | 步骤顺序、`done/current` 展示规则、`candidate_available` 判定 |
| `/processing/:id/candidate` | `CodeBlock`（payload）+ `MetadataList` + `TextArea` + `Button` | `submitReview(job_id, decision, candidate.version, comment)` 一字未改；提交后锁定与可重试错误 |
| `/findings` | Card + Astryx `Table`（规则/资源/严重度/状态/次数/时间） | `listFindings(50)`、Repair controls 链接 |
| `/repairs` | 警告 `Banner` + 复用 `FindingsTable` | `status !== 'repaired'` 过滤，只读边界 |
| `/audit` | `List`/`ListItem` 事件流（状态点 + 结果 Badge） | `listAudit(50)`；failure_code 仅在展开视图出现 |
| `/identities` | Card/Grid + 两张 `Table` + `TextInput`/`Button` | 附加成员、挂起/恢复的 `expected_version` |
| `/roles` | Card/Grid + `Table` + `CheckboxInput` 权限集 + 独立编辑卡（不再嵌套 Card） | 创建角色的 `permission_keys`；**编辑权限的完整集合替换 + `expected_version`**；system 角色不可变；禁用角色版本 |
| `/bindings` | Card + `TextInput`/`Selector`×2 + `Table` | 三种 scope 序列化、revoke `expected_version` |
| `/organization` | Card + 树形 `Table`（扁平化缩进）+ 成员卡 | 创建单位（`parent_id: null`）、禁用版本、成员 add/remove 的 `membership_type` + `expected_version` |
| `/explain` | Card + `MetadataList` + 评估 `Table` | `explainDecision` 载荷、bounded reason 渲染 |

## 4. 重要技术决策与修正

1. **TanStack Table 移除**：项目只用到 core row model，Astryx `Table` 的 `data`/`columns`/`renderCell` 完全覆盖；删除 `@tanstack/react-table` 依赖（无其它调用者）。
2. **Selector 命名缺口**：Astryx `Selector` 的可视 label 不会成为 combobox 的可访问名（实测 `role=combobox` 且无 `aria-labelledby`），因此每个 Selector 显式传入 `aria-label`（Role/Scope/Unit type/Parent unit/Membership type）。这是组件库行为，不改业务。
3. **角色编辑器不再嵌套 Card**：按 Astryx「不要 Card 套 Card」原则改为同级 Card；e2e 用标题过滤定位，不依赖结构类名。
4. **组织树扁平化**：递归 `<tr>` 改为 `flattenUnits()` 生成带 `depth` 的行，用 `— ` 前缀表达层级（Astryx Table 不渲染嵌套行）。排序/父子关系语义不变。
5. **全局样式清理**：`styles.css` 从 105 行降至 55 行，只保留 Tailwind 层序、reset 与外壳几何（`.app/.sidebar/.brand/.main-content/.toolbar/.page-content/.mobile-bar`）。删除全部 `.panel/.table-*/.status-*/.chip/.text-input/.pipeline/.json-view/.audit-*/.loading/.empty/.error-state` 等历史规则。
6. **排版降级移除**：S1 引入的 `.app .page-content :where(...)` 保护规则已删除——所有业务页面都由 Astryx 组件渲染，主题排版（Figtree/字号/颜色）统一生效；`.app` 不再固定系统字体。
7. **保留的旧样式**：`.text-button` 仍用于 Dashboard 三个内联链接（React Router 链接 + 控制台 accent token）；S5 可替换为 Astryx `Link` 并删除。

## 5. 测试

### 5.1 新增 Playwright（本轮 +18）

| 文件 | 用例 | 覆盖 |
| --- | --- | --- |
| `e2e/documents.spec.ts` | 4 | 列表/上传禁用→启用、multipart + Idempotency-Key + 跳转、有界错误、空状态 |
| `e2e/processing.spec.ts` | 4 | 队列、Job 详情与 pipeline、**复核决策精确载荷**（decision/candidate_version/comment + 幂等键）、409 后可重试 |
| `e2e/governance.spec.ts` | 4 | Findings 徽标与跳转、Audit failure code、Repairs 过滤与边界、三处空状态 |
| `e2e/iam-writes.spec.ts` | 4 | 挂起版本、绑定创建 scope、revoke 与成员移除版本、explain allow + evaluations |
| `e2e/iam.spec.ts`（改造） | 3 | 保留原断言：权限预勾选、完整集合替换 + 版本、二次保存不丢键；仅更新到新控件定位 |

### 5.2 既有测试

Dashboard 10 条、Shell 7 条、Foundation 3 条、smoke 1 条全部保持通过。

## 6. 视觉证据

每个路由的桌面浅色全页截图（11 张）：[overview](assets/astryx-migration/s4/route--overview.png) · [documents](assets/astryx-migration/s4/route--documents.png) · [processing](assets/astryx-migration/s4/route--processing.png) · [findings](assets/astryx-migration/s4/route--findings.png) · [repairs](assets/astryx-migration/s4/route--repairs.png) · [audit](assets/astryx-migration/s4/route--audit.png) · [identities](assets/astryx-migration/s4/route--identities.png) · [roles](assets/astryx-migration/s4/route--roles.png) · [bindings](assets/astryx-migration/s4/route--bindings.png) · [organization](assets/astryx-migration/s4/route--organization.png) · [explain](assets/astryx-migration/s4/route--explain.png)

并排对比（左=S3，右=S4）：[documents](assets/astryx-migration/s4-diff/documents-before-after--light.png)。

像素差异（S3 → S4，同 Mock、同脚本）：

| 视图 | 桌面 light | 桌面 dark | 移动 light | 移动 dark |
| --- | --- | --- | --- | --- |
| Dashboard | 28.07% | 18.22% | 30.10% | 21.43% |
| Documents | 17.38% | 39.90% | 30.00% | 63.48% |
| Roles | 24.57% | 50.52% | 23.80% | 70.61% |

差异全部来自本轮的**有意的设计系统替换**：panel→Card、手写表格→Astryx Table（紧凑密度、滚动区）、状态 pill→Badge、标题/正文改为主题排版（Figtree）、表单控件更换。页面高度随之变化（如 Dashboard 1167→1035；Roles 939→900）。

## 7. 构建与依赖

| 产物 | S3 | S4 |
| --- | --- | --- |
| CSS | 324.53 kB (gzip 53.67) | 315.37 kB (gzip 51.76) |
| JS 主包 | 2,027.94 kB (gzip 667.04) | 2,245.12 kB (gzip 742.16) |
| 异步 chunk | 1（Tooltip） | 3（BottomSheet 71.9 kB、ItemSwipeLayer 9.6 kB、MenuBottomSheet 0.6 kB） |
| 依赖变更 | — | 删除 `@tanstack/react-table` |

JS 增长来自 `Selector`（触摸端 BottomSheet + swipe 层）与全部业务组件的引入；S5 应评估路由级 code split 与组件按需加载。

## 8. 质量门禁（最终 HEAD 实跑）

| 命令 | 结果 |
| --- | --- |
| `npm run lint` | **PASS**（0 error / 0 warning） |
| `npm run typecheck` | **PASS** |
| `npm test` | **PASS**（7） |
| `npm run build` | **PASS**（10.47s） |
| `npm run test:e2e` | **PASS**（40 passed / 19 skipped opt-in） |
| `npm exec -- astryx doctor` | **PASS**（0 fail；warn 同 S1：外部字体、CLI PATH 探测） |

## 9. 回滚

- 逐切片的提交可单独 revert；
- S4-A2/B1/B2 与 S4-D1..D4 各自整段 revert；
- 清理提交（CSS/TanStack/排版降级）单独 revert 只会恢复旧样式与依赖，不回退页面逻辑。

无数据、后端、API 或权限语义变更。

## 10. 未解决 / 移交 S5

1. **JS 体积** 2.25 MB（gzip 742 kB）：路由级 code split、Astryx 组件按需加载、`Selector` 触摸端依赖评估。
2. **Google Fonts 外部请求**：自托管或移除。
3. **`.text-button`**（Dashboard 3 处）与品牌区少量自绘样式仍在；S5 统一到 Astryx Link/组件。
4. **未做 axe/E2E 键盘全量审计**：本轮覆盖焦点陷阱、Esc、触摸、表单标签；完整无障碍审计留给 S5 CI。
5. **组织树用文本缩进**：若需要真正的树形交互（展开/折叠），应使用 Astryx 的层级组件单独设计。
6. **移动端表格**：Astryx Table 自带横向滚动区，窄屏需要横向滚动查看所有列（与旧表格行为一致）。
7. **CI 尚未纳入 `astryx doctor` 与视觉/交互门禁**：S5 范围。
