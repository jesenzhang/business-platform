# PLAN-0014 Wave 5A Repair 2 独立审阅与下一步指令

日期：2026-09-30。

## 1. 结论与固定范围

**PASS（Wave 5A Repair 2 有界修复） / NOT INTEGRATED。未发现需要阻止本轮修复收口的新缺陷。**

这不是 PLAN-0014 总验收或 R2 通过：真实 PostgreSQL、模块迁移隔离、仓库外目录复建、扫描等未执行项继续有效。

- 分支：`codex/plan-0014-wave5a-local-release-pilot`。
- 当前 HEAD：`c73955ec30f46d839ff33d44776cef142b7d6d77`。
- 修复代码候选：`c22939f0a51e3c7fa2ec424f3819c6e7113a9b66`。
- 审阅范围：`7180eec7fc813b33a748784ac806d5c95b4e359b..c73955ec30f46d839ff33d44776cef142b7d6d77`。
- 候选到 HEAD 的 tools/scripts/Cargo.toml/Cargo.lock 差异为空；开始审阅时工作树干净。
- 本轮未修改产品代码、迁移或历史证据，只新增本审阅文档；未提交、推送或集成。

## 2. 缺陷关闭验证

| 上轮发现 | 独立检查与本轮观察 | 判定 |
| --- | --- | --- |
| F1-A 派生 Cargo.toml 与提交未绑定 | `project()` 从固定提交的 spec、Cargo/lock/toolchain、迁移输入构造候选；verify 使用同一确定性派生过程，对完整派生成员与 transforms 比较。直接重跑上轮保留的 `derived-cargo-tamper`：exit 1，明确命中 `derived-members-match-revision` | CLOSED |
| F1-B 提交源码遗漏仍能生成 | `git ls-tree -r -z` 导出提交集合，generate 在创建输出之前双向比较相关 checkout 集合；verify 比较整个预期 payload 集合。直接重跑上轮隔离 checkout（仍缺 testing.rs）：exit 1，命中 `source-member-set-matches-revision` | CLOSED |
| 物理自洽不能替代来源正确 | 回归覆盖依赖版本/features/path、更换合法 spec、缺失/额外 payload 成员，均先重算物理摘要再验证来源拒绝 | PASS |
| 历史候选与当前工作树分离 | 回归在隔离 checkout 改 crate manifest、添加/删除源码后验证固定候选；缺 commit object 不回退磁盘。当前 HEAD 独立进程复验钉住 c22939f 的 identity/policy 候选，两者各 12 项 PASS | PASS |
| F2–F5 原修复 | compare 两侧物理检查、记录一致性、解析后的消费者图隔离与脚本原因断言保持通过 | 未发现回归 |

关键位置：`src/commit.rs`、`src/git.rs`、`src/packaging.rs::project/assert_checkout_matches_commit/assert_member_set_matches/assert_derived_members_match`；回归 `tests/source_derivation.rs`。工具仍只做本地发布候选，没有将签名、实际编译器身份或生产安装纳入验证声明。

## 3. 当前证据与历史证据

| 证据 | 本轮独立执行结果 |
| --- | --- |
| `cargo test -p module-release-tool --all-features` | exit 0；61 passed，0 failed，0 ignored，包含新增 9 项来源回归 |
| `scripts/test-plan0014-wave5a.ps1` | exit 0；14 PASS；报告在 `target/wave5a-repair2-independent-script-regression/` |
| `scripts/check-architecture.ps1` | exit 0；Architecture fitness PASS，Cargo metadata 与 OpenAPI PASS |
| 上轮 Cargo 篡改反例 | exit 1，派生来源检查拒绝 |
| 上轮源码删除反例 | exit 1，成员集合检查拒绝 |
| 两个 c22939f 固定候选 verify | 两个独立进程 exit 0，12 checks PASS；21 / 23 成员，原 integrity digest 保持 |
| fmt/check/clippy/全 workspace test | 本轮 NOT RUN；查阅 Repair 2 归档结果，不与本轮独立执行混计 |
| 完整 pilot 的构建/compare/消费者 lane | 查阅实现归档；本轮独立执行的是定向 suite、脚本回归与固定候选 verify，未完整重跑 pilot |

实现归档的 workspace 测试使用独立 target 后 exit 0，报告 864 passed / 0 failed / 70 ignored；默认 target 的二进制占用失败仍如实保留。独立 target 完整执行不是默认 target 失败的掩盖。70 个 ignored 分为 PostgreSQL 38、PostgreSQL+对象存储 3、S3/MinIO 28、模型 provider 1，均未执行。

仓库外目录消费者 lane 本轮实施选择 `-SkipOutsideConsumer`，因此是 NOT RUN；Repair 1 的历史外部 lane 不自动成为 Repair 2 当前证据。PostgreSQL 迁移/独立安装升级/rollback、CI、扫描和发布部署也未因本轮修复而通过。

## 4. 下一步：Wave 5A 收口 + Wave 5B-0 迁移隔离设计包

下一步不再修来源验证链。先把本结论落实到当前状态矩阵，再产出 **G-203 的可审阅实施设计**。原因：现有 shared PostgreSQL 019、public schema、runtime-migration 全量 embed 仍不能证明模块独立演进；先前实施记录 SP-3/SP-4 已将实际 schema/catalog 改动列为设计提案。AGENTS.md §3 要求关键归属、一致性与迁移策略确定后再编码。

以下指令可整体交给 zcode；本次授权范围是仓库文档收口、事实盘点与后续实施设计，不是数据库迁移执行。

### 4.1 起点与必读材料

1. 工作目录 `F:\Workspace\business-platform`。记录实际 branch/HEAD/status，确认与上述代码候选的差异。保留无关工作树修改。
2. 阅读 AGENTS.md、PLAN-0014、本文、Repair 2 README/F1-closure、正式 IAM 与 module-packaging 审阅、Wave 5A 实施记录 §12 的 SP-3/SP-4，以及 architecture manifest、data ownership、persistence、legacy migration、deployment 和安全 baseline；查阅适用 Accepted ADR 与文档治理规范。
3. 继续使用当前分支或按既有本地分支策略新建 `codex/` 分支；不得合并 main。实际起点 SHA 必须写入设计包。

### 4.2 本次允许修改的文件

- `docs/reviews/2026-09-30-wave5a-repair2-gates/README.md`：仅补独立结论指针，不改原日志/数字。
- `docs/plans/current/PLAN-0014-reference-conformance-and-r2-module-readiness.md`：追加 Wave 5A 独立收口与 Wave 5B-0 文档设计范围。
- `docs/architecture/ARCHITECTURE_STATUS.md` 与 `docs/README.md`：同步当前状态和导航。
- 新建 `docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md`。
- 新建 `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md`，状态 Proposed / READY FOR DESIGN REVIEW。
- 如方案确实需要新增架构决策，读取 ADR 索引后使用下一空闲编号，新增 Proposed ADR 并更新索引；不得自行标 Accepted。不要修改 ADR-0027/0028 的状态。

本次不改 Rust、Cargo 配置、脚本、SQL、迁移文件或 MANIFEST.sha256；不创建数据库、连接凭据或执行安装。无需为了文档收口重复完整 workspace 测试。

### 4.3 准入矩阵必须逐项给结论

以原编号建立表：G-01/G-201 对应关系、G-02、G-202、G-203、A-01..A-12、两模块各自 C0/C1 与相关 Wave 4 开放项。列出原关闭条件、当前源码/固定 SHA、证据路径、证据年代、实际执行范围、状态、剩余动作与 owner。

- F1-A/F1-B 根据本文 CLOSED；Wave 5A Repair 2 PASS / NOT INTEGRATED。
- G-201：明确分开“值契约/声明一致性”和“真实模块工件组装”；把本轮已取得的证据对应到残项，不用一句 R2 BLOCKED 抹掉已完成部分。
- G-202：区分 workspace 外消费者工程与仓库目录外复建。pilot 消费者有独立 `[workspace]`，路径指向 packed artifact，这是实际证据；目录外当前复建仍 NOT RUN。根据原验收逐子项判定，不能擅自要求联网发布，也不能以旧日志冒充新执行。
- G-203：声明 namespace 和 mapping/checksum 已有证据；物理 catalog、执行 ledger、安装升级隔离继续 OPEN。
- A-12 的真实 PostgreSQL 部分继续 NOT RUN/BLOCKED。70 个 ignored 保留四类分母。
- G-01/G-201 是承接关系，禁止当成两个独立成功模块计数；foundation 两个纯 crate 也不能凑两个 pilot。
- 不把针对本地工具的 PASS 扩大为模块 R2 admission。最终 R2 仍 BLOCKED，直到适用 C1 与真实验收被独立关闭。

### 4.4 只读盘点，产出可执行设计

阅读并给出精确文件/符号落点：`migrations/019_identity_authorization_foundation.sql`、workspace catalog manifest、`crates/runtime-migration`、`apps/migration`、identity/organization/policy PostgreSQL 适配器、相关 SQLite catalog、release specs 及工具 migration mapping。

设计必须完成下列十项，不得用“后续再考虑”代替关键决定：

1. **所有权表**：019 每张表、索引、约束、种子事实归哪个 context；跨 context FK/写入依赖单列。Organization 不能被两试点迁移顺带安装且不记账。
2. **历史兼容**：旧 001..019 与历史 checksum 原样保留。分别设计空库安装与已有宿主库过渡；旧历史不重放、不补写伪造成功记录、不修改 SQLx checksum。
3. **目标 catalog**：模块 namespace 目录、per-namespace manifest、revision/ordering/digest 精确定义；区分 ledger namespace、目录 namespace、实际 SQL schema。声明字段不代表表已隔离。
4. **runner 与 ledger**：保留 workspace runner/composition root；给出按 namespace 选择、版本状态、checksum 校验、依赖排序、并发锁、失败/dirty 拒绝与恢复的职责。说明谁写 ledger，避免两种 runner 同时管相同版本。
5. **最小方案比较**：至少比较“历史宿主 catalog 保持、未来迁移分 namespace”的增量路径与“额外物理 schema 迁移”路径。明确前者能否满足原 G-203 关闭条件；不能就给出完整过渡步骤，不把较小方案直接叫闭环。
6. **兼容与权限**：列出 repository SQL、search_path、连接角色、schema 授权受影响点；跨租户与跨模块读取/写入保持当前 Application 边界。禁止依赖全库超级权限掩盖模块越界。
7. **release 绑定**：新 catalog revision 如何进入现有 MigrationRevision/set_digest；候选描述与可执行迁移文件的对应关系；不修改 package_digest 的声明语义，不扩展 identity.v1 字段。
8. **失败与回滚**：明确失败事务、重试、进程中断、checksum 漂移、重复执行、并发 namespace 的处理；回滚不得默认 drop 业务数据。已有库过渡的不可逆动作须单独列为将来授权门禁。
9. **实施文件清单**：逐文件拟改内容、依赖先后、fixture/test 名称、兼容检查与提交切片。把下一轮实施拆为一个能独立审阅的最小包；其他提案 SP-1/2/5/6/7 不混入。
10. **真实验收矩阵**：identity 单独、policy 单独、两者共存、已有 legacy 宿主升级；tenant/version/idempotency、checksum 篡改、重复/并发、故障恢复，分别给预期结果与精确 cargo/CLI 命令。对于不可独立安装或有强依赖的模块，如实写清依赖并修订关闭判据提案。

数据库 fixture 只写规范：专用空测试库、显式 TEST URL、已授权角色、不使用其他项目数据库；没有当前授权就标 BLOCKED。本次不试探密码、不枚举 Secret、不执行迁移。

### 4.5 文档检查、交付与停止

逐条确认新文件存在、Markdown 链接有效、状态矩阵计数一致；运行 `rtk git diff --check`，记录命令/退出码。对不改运行代码的本次文档包，Rust workspace 门禁记“本轮未运行，产品代码无改动”，不要复用旧日志称当前通过。

交付：本地文档提交 SHA、起点/终点、修改清单、准入矩阵、设计方案、拟实施步骤、待决策项与未运行门禁。状态 **WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；WAVE 5B-0 DESIGN READY FOR REVIEW；R2 BLOCKED**。

停止在设计包交付。不得自动开始 Wave 5B 迁移代码、激活 Proposed ADR、关闭 G-203/A-12、推进 PLAN-0006、push/merge/tag/publish/deploy。

## 5. 审阅覆盖

采用 open-code-review-delegate：OCR 负责路径筛选和规则解析，宿主执行源码、记录与行为审阅。

total_files=143；reviewable_files=32；reviewed_files=32；skipped_files=0；coverage_rate=100%（reviewable 分母）。111 项 unsupported_ext 不在这个覆盖率中；相关计划、说明与关键日志另按任务读取，未宣称全部历史日志重执行。

reviewable 逐文件清单附后。

| 路径 | Git 状态 | 审阅状态 |
| --- | --- | --- |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/ignored-tests-inventory.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/ignored-tests-inventory.summary.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/25-consumer-identity-cargo-metadata.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/25-consumer-policy-cargo-metadata.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/artifact-manifest.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/contract-surface.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/migration-mapping.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/module.manifest.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/release-bundle.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/release.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/identity/verification.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/artifact-manifest.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/contract-surface.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/migration-mapping.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/module.manifest.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/release-bundle.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/release.json` | added | reviewed |
| `docs/reviews/2026-09-30-wave5a-repair2-gates/pilot/artifacts/policy/verification.json` | added | reviewed |
| `scripts/check-architecture.ps1` | modified | reviewed |
| `scripts/lib/plan0014-wave5a-lib.ps1` | modified | reviewed |
| `scripts/plan0014-wave5a.ps1` | modified | reviewed |
| `scripts/test-plan0014-wave5a.ps1` | modified | reviewed |
| `tools/module-release-tool/src/commit.rs` | added | reviewed |
| `tools/module-release-tool/src/git.rs` | modified | reviewed |
| `tools/module-release-tool/src/lib.rs` | modified | reviewed |
| `tools/module-release-tool/src/migrations.rs` | modified | reviewed |
| `tools/module-release-tool/src/packaging.rs` | modified | reviewed |
| `tools/module-release-tool/src/provenance.rs` | modified | reviewed |
| `tools/module-release-tool/src/repo.rs` | modified | reviewed |
| `tools/module-release-tool/src/spec.rs` | modified | reviewed |
| `tools/module-release-tool/tests/pilot_candidates.rs` | modified | reviewed |
| `tools/module-release-tool/tests/source_derivation.rs` | added | reviewed |
