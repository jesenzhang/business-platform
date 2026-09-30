# PLAN-0014 Wave 5B-0 独立设计审阅与下一实施包

日期：2026-09-30。审阅结论：**NEEDS REPAIR（设计包）**。

## 1. 范围与已确认事实

- 起点：`c73955ec30f46d839ff33d44776cef142b7d6d77`。
- 候选/当前 HEAD：`5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`。
- 分支：`codex/plan-0014-wave5a-local-release-pilot`，审阅开始时工作树干净。
- 核实差异为 9 个文档文件，+986 / -5；无 Rust、SQL、脚本或 Cargo 改动。
- `git diff --check c73955e..5c8355d` 本轮 exit 0。
- 已阅读全部 9 个变更文件，并对照 runtime-migration、migration config、现有测试 fixture 和适用持久化/安全/数据所有权 Baseline。
- 本轮没有连接数据库、执行 PostgreSQL DDL 或运行产品构建测试。下述问题来自设计控制流与当前代码的直接核对；不声称已执行未来 runner 的并发反例。

Wave 5A Repair 2 的有界 PASS 不受此次设计审阅影响。019 所有权盘点、历史 checksum 保留、目录/ledger/SQL schema 的区别和 NOT RUN 记录是有用的设计基础。推荐方案 A 的方向可继续细化；当前不能把整个设计包作为 S2/S3 runner 编码依据。

## 2. 阻止实施的设计问题

### B1 / P1：不同 namespace 的锁没有保护共享 ledger 的首次创建

定位：设计 `PLAN-0014-wave5b-migration-isolation-design.md:228`、`:237-239`；ADR-0029 §3.3/§3.5。

设计让 identity-management 与 policy 各取自己的 namespace advisory lock，再执行同一张 `public.platform_module_migrations` 的 `CREATE TABLE IF NOT EXISTS`。两个锁键不同，因此两进程可同时进入共享表创建。刚完成 host baseline、尚无模块 ledger 的数据库上，两个不同 namespace 的首次 up 存在并发 DDL 竞争。当前仓库 `crates/messaging/tests/inbox_idempotency.rs:18-21` 已明确记录同类 CREATE IF NOT EXISTS 竞争。

单 namespace 的并发 V8 不足以证明这一点。还必须规定同一物理连接持有会话锁并执行相关事务，不能从 pool 任意连接执行 SQL，再把持锁连接放回池。

修正要求：平台 ledger bootstrap 使用所有 namespace 共用的固定锁；bootstrap 校验现存表结构与提交结果；bootstrap 提交并释放公共锁后，再进入 namespace 锁及 revision 事务。写明锁获取/释放顺序、异常清理、连接取消/断开、超时和锁键域；只读 verify 不创建表。

### B2 / P1：方案 B 的搬迁步骤不可执行，也不能证明单模块冷安装

定位：设计 `:303-310`、`:318`、`:343-345`；准入矩阵、ADR 与状态摘要中对应结论。

设计 expand-2 要先创建 `public.platform_users` 兼容视图，指向尚未创建的 `identity_management.platform_users`；此时 public.platform_users 还是旧表，目标 schema 中也没有该表。视图名称冲突与目标不存在都使这条时间线不能执行。

此外，把共享 019 创建的 13 张表从 public 移到各 schema，并没有改变冷安装必须先运行 host baseline 1..19 的事实。方案 B 表格将“单模块冷安装（不建他人表）”写为“是”，没有独立安装 catalog/依赖/host bootstrap 支撑。物理 schema 搬迁与单模块冷安装是两个不同验收目标；后者也不必然以搬迁旧库为前提。

修正要求：方案 B 降为未完成的后续研究方案，删去已满足冷安装的断言；分别设计 legacy 升级与空库独立安装路径。若保留兼容视图方案，明确同一事务内移动表后再创建旧名视图，且迁移前后客户端路径与依赖对象都能成立；当前不授权执行该方案。

不能通过新增 G-203a/b 名称降低原 G-203/G-202/A-12 分母。可增加目录隔离、ledger 隔离、独立冷安装、既有库升级四个子项记账，整体条件仍未满足。不要把物理 schema 当成所有模块 R2 的唯一必选实现。

### B3 / P2：真实验收命令设置了互斥配置，fixture 范围也未对齐

定位：设计 `:403`、`:408-412`。当前 `apps/migration/src/config.rs:17-22` 在两个 URL 环境变量同时存在时直接返回 ConfigLoadError，即使值相同也拒绝。

因此按设计原文同时设置 MIGRATION__DATABASE__URL 与 DATABASE_URL 后运行 V1/V2/V3 CLI，会在连接前失败，无法验证 namespace 行为。

当前 `apps/migration/tests/migration_test.rs:11-18` 还会根据 DATABASE_URL 生成 sibling database `plan2_<uuid>`，与设计只允许 `plan0014_wave5b_<runid>` 的数据库清单不一致。不能把“same target”当成这些测试实际只操作给定数据库的证明。

修正要求：CLI lane 与测试 lane 分开环境作用域，逐 lane 只设一种变量，退出时恢复原环境；每套测试先盘点实际是否建 sibling DB，并明确授权对象与清理策略。本轮无数据库授权，不运行这些命令，不猜测凭据。

## 3. 交给 zcode 的唯一下一实施包

**执行 Wave 5B-0 Revision 1：修正设计控制流与验收命令，交付可供 S1/S2 分别审阅的实施规格。** 本次不进入真实 runner、ledger 或迁移目录实施。依据是 B1/B2 尚涉及关键并发与迁移正确性，AGENTS.md §3 规定关键项确定后才编码；ADR-0029 仍 Proposed，前轮停止条件也明确没有激活它。

### 3.1 文件责任与起点

从 `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601` 开始；先记录实际 branch/HEAD/status。只修改：

1. `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md`，Revision 升 1，保留历史修订说明。
2. `docs/adr/ADR-0029-module-namespaced-migration-catalogs-and-ledger.md`，保持 Proposed，同步 B1/B2。
3. `docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md`，修正独立安装判据和真实执行范围。
4. `docs/plans/current/PLAN-0014-reference-conformance-and-r2-module-readiness.md`、`docs/architecture/ARCHITECTURE_STATUS.md`、`docs/README.md`，追加本次审阅状态与修订指针，不改历史证据。
5. 新建 `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md`，逐项列出 B1/B2/B3 的修正落点、推演与待执行验收。

保留无关修改；不改 Rust、SQL、Cargo、脚本或旧 MANIFEST；不覆盖 Wave 5A 证据。

### 3.2 按顺序完成 B1

- 定义平台 bootstrap 锁与 namespace 锁为不同职责；固定公共键、明确 hash collision 的保守处理。统一顺序为：host baseline 完成 → 同一连接取公共 bootstrap 锁 → 校验/创建平台 ledger → 提交 → 释放公共锁 → namespace 锁 → 回读状态 → revision SQL + ledger INSERT 同一事务 → 明确释放。
- 不在持 namespace 锁时再取公共锁，避免不同入口形成锁序倒置。
- 专用持锁连接不得在锁仍持有时回池；异常/取消的释放或关闭策略明确。连接断开导致锁释放属于 PostgreSQL 语义，应用级取消不自动等于连接断开。
- verify/status 不执行 CREATE；表不存在只返回 Empty/Pending 等明确状态，权限、连接、表结构不符和 checksum 漂移均 fail closed。
- 在 V8 增加三条：同 namespace 两进程；不同 namespace 首次并发 bootstrap；bootstrap 持锁进程被终止后另一进程恢复。每条定义启动屏障、断言、残留对象/ledger 行和专用测试库要求，状态仍 NOT RUN。
- 写明测试目录存在空 catalog 与存在真实 revision 两种 fixture；空 catalog 不能证明安装/升级/失败事务路径。未来 fixture revision 用测试对象，不在正式模块中造无业务意义的迁移。

### 3.3 按顺序完成 B2

- 修正方案 A/B 对比和所有摘要：A 能提供未来 namespace 演进，B 描述的是既有表位置/权限隔离；二者都没有现成单模块冷安装证据。
- 空库独立安装给出依赖对象最小清单、模块独立初始化 catalog、与 legacy 历史的分别记账策略；这些只作为独立后续设计范围，不复制旧迁移冒充新历史。
- legacy 搬迁给出可成立的事务顺序及中间态对象存在性表。若未能完成，明确方案 B 未设计完成并从可执行实施步骤中移除，不保留错误序列。
- 保留原 G-203 整体 OPEN；a/b 等仅为 Proposed 子项，不标 Accepted，不把“目录存在/空 manifest”记为独立安装成功。
- S1 只负责离线 catalog 验证与静态门禁；S2 负责 runner 行为；S3 启动/readiness；S4 工件 schema；S5 真实 PG；各自准入与停止条件分开。
- 补 S4 的历史兼容要求：新工具按固定提交中的 spec schema 派生旧 v1 候选；新 spec/mapping v2 不得让既有 c22939f 固定候选失去 verify 能力。明确版本分派、未知版本拒绝与回归测试，不先实现。

### 3.4 按顺序完成 B3

- 提供两个独立的完整 PowerShell 示例：CLI 只设 MIGRATION__DATABASE__URL 且清除 DATABASE_URL；测试只设 DATABASE_URL 且清除 MIGRATION__DATABASE__URL。try/finally 恢复变量原来的“存在/不存在及值”，日志不输出真实 URL。
- 逐 suite 列出实际创建/访问/删除的数据库，不假设所有 `#[ignore]` 都访问同一个目标。新 namespace 测试计划限定显式授权的专用库；旧 sibling-DB 套件如需运行，授权清单包含确切新建库范围后才执行。
- V14 改为新增测试后的实际总数及增量核对，不能要求保持 864/191 不变；ignored 必须逐项分类，新增 PG ignored 使分母增加时如实解释，不能要求机械固定 70。
- 每条命令标明当前可执行或未来新增命令；`--namespace`/`verify` 当前未实现，不当成已存在 CLI。取消“V13 现在能验证新 mapping”的歧义。

### 3.5 把 S1 收窄为下一轮可编码包

Revision 1 必须附一份单独 S1 规格，但本次不实施：

- 固定相关 namespace/owner/catalog 元数据字段与未知字段拒绝策略；建议给 catalog 元数据显式 schema_version，不改变 identity.v1。
- 核心验证包括 owner/namespace 一致、路径逃逸/符号链接拒绝、manifest 与 SQL 双向集合、重复 revision、gap、未知依赖/环、checksum；空目录只能表明预留，无实际迁移证明。
- 历史保护同时覆盖文件集合与固定历史内容/checksum；仅断言恰好 19 个名称不足以阻止同时修改 SQL 与 MANIFEST。说明 Windows EOL 与已提交 blob 的对比口径。
- 正式 SQL owner 白名单扫描只是静态 fitness，不声称 SQL 字符串扫描能证明数据库权限隔离；对合法同 owner 新表的授权扩展机制写明，不能永远只允许 019 的 13 个旧表。
- 列出准确文件、红绿反例与命令；S1 不创建平台表、不接 CLI up、不改启动迁移、不升级 release spec。生产 catalog 激活与 baseline 全局冻结须依 Proposed ADR 的正式处理结果，不能以离线脚本通过代替接受。

### 3.6 交付与停止条件

完成全部修订后，检查本报告引用与 Markdown 导航，执行 `rtk git diff --check`，记录 actual HEAD、修改清单和本地提交 SHA。Rust/PG 门禁本轮 NOT RUN（无产品代码改动/无数据库执行），不冒用 Wave 5A 的通过数字。

closeout 必须包含 B1/B2/B3 的原问题→修正条款→静态推演→未来测试映射；未完成的研究结论明确留 OPEN。

状态：**WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW；WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；R2 BLOCKED**。

停止在 Revision 1 设计复审。不得自行标 ADR Accepted、不实施 S1/S2/S3、不连接数据库、不改 main、不 push/merge/tag/publish/deploy；不重开 Wave 5A Repair 2。

## 4. 审阅覆盖

open-code-review-delegate 的 OCR preview：total_files=9，reviewable_count=0，excluded_count=9（Markdown unsupported_ext）。OCR 规则覆盖率为 N/A，不能将零可审文件称为代码审阅 100%。宿主按设计任务完整读取 9 个变更文档：reviewed=9，skipped=0，文档范围覆盖率 100%。未宣称扫描整个仓库。

| 变更文档 | 状态 |
| --- | --- |
| docs/README.md | reviewed |
| docs/adr/ADR-0029-module-namespaced-migration-catalogs-and-ledger.md | reviewed |
| docs/adr/README.md | reviewed |
| docs/architecture/ARCHITECTURE_STATUS.md | reviewed |
| docs/plans/current/PLAN-0014-reference-conformance-and-r2-module-readiness.md | reviewed |
| docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md | reviewed |
| docs/reviews/2026-09-30-wave5a-repair2-gates/README.md | reviewed |
| docs/reviews/2026-09-30-wave5a-repair2-independent-review.md | reviewed |
| docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md | reviewed |
