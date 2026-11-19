# PLAN-0014 Wave 5B-S1 实施 closeout — 离线 catalog 门禁、静态 SQL fitness、单一 baseline embed 修复

状态：**WAVE 5B-S1 READY FOR INDEPENDENT REVIEW / NOT INTEGRATED；R2 BLOCKED**

本轮不是设计轮：不重启设计、不改变任何判据方向或分母，只执行 `docs/reviews/2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md` §3 的唯一下一实施包，并落实其 §2 的 C1 历史摘要兼容规则。实现者不自填独立 PASS。

---

## 0. 起止 SHA 与范围

| 项 | 值 |
|---|---|
| 起点（父提交，未改动） | `6decf1ad9ea05233d66dd3539594ff210afd99f2` |
| 实现候选（本地，未集成） | `b25e2d4a9b455b776934495b8f3dfd2b71a13fe2` |
| 文档/证据提交 | ``本次文档/证据提交（紧随实现候选 b25e2d4 之后，SHA 以 `git log --oneline` 为准；本轮不 amend 候选）`` |
| 固定历史基线（钉住的 baseline 提交） | `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`（baseline revision 19） |
| 分支 | `codex/plan-0014-wave5a-local-release-pilot` |
| 执行包 | 上述审阅文档 §3（§3.1–§3.10） |
| 历史摘要规则 | 同上 §2 C1 |
| 门禁证据目录 | `docs/reviews/2026-09-30-wave5b-s1-gates/` |

**本轮明确未做**：S2（runner / `platform_module_migrations` / bootstrap 锁 / 运行时接线）；任何数据库连接（含只读 SQL、建库、`sqlx` 连库编译）；ADR-0029 的接受（**仍 Proposed / NOT ACCEPTED**）；`main` 的任何改动；push / merge / tag / publish / 部署；Wave 5A 归档证据（`docs/reviews/2026-09-29-wave5a-*`、`2026-09-30-wave5a-repair2-gates/`）的任何改写；把 `migrations/modules` 说成已安装 catalog；把 baseline 全局冻结设为默认策略（只在显式 `--only baseline-candidate-freeze` 时评估）；任何 gap / 准入子项的关闭。

---

## 1. 逐文件职责（谁负责哪条规则）

| 文件 | 变更 | 负责的具体规则（review §3 条目） | 失败方向 |
|---|---|---|---|
| `tools/module-release-tool/src/catalog.rs` | 新增（约 3.3k 行） | §3.3 catalog 元数据 / §3.4 manifest·revision·依赖 / §3.5 SQL owner 静态 fitness / §3.6 钉住历史 / §3.7 单一 embed / §3.1 freeze（可选） | 任何一条不满足即 `ToolError::Verification{ check, reason }`，进程 exit 1 |
| `tools/module-release-tool/src/lib.rs` | `pub mod catalog;` | 模块挂载 | — |
| `tools/module-release-tool/src/main.rs` | `catalog-check` 子命令、`--only` 解析、usage | §3.2 CLI 与 check 选择；未知 check 拒绝 | usage error，exit 1 |
| `tools/module-release-tool/tests/fixtures/wave5b/catalogs/**` | 50 个 case 目录 + `EXPECTATIONS.json`（驱动侧 catalog 家族跑出 51 条 = 50 个 fixture 目录 + 1 条由 freeze 克隆派生的 `--only module-catalog` 探针 `F3-module-sql-catalog`） | §3.8 的共享正/负例 fixture | 期望表是唯一真源，两个读取者都必须同意 |
| `tools/module-release-tool/tests/fixtures/wave5b/history/pinned-baseline-5c8355d….json` | 钉住的 20 条摘要表（19 SQL + manifest；12 LF / 7 CRLF 形） | §3.6 + C1 | 与内嵌常量漂移即 Rust 测试红 |
| `tools/module-release-tool/tests/wave5b_catalog.rs` | 新增 19 个测试 | §3.8 第二读取者；C1 反例；13 表归属 ↔ `019` 对账；默认 check 集合；仓库自身 | 任一不符即 `cargo test` 红 |
| `scripts/test-plan0014-wave5b.ps1` | 新增门禁驱动 | §3.8 第一读取者：catalog / history / embed / freeze / path 五族；scratch 隔离；NOT RUN 诚实 | 任一 case 与期望不符 → exit 1；平台不能跑的 case 记 BLOCKED 并如实播报 |
| `scripts/check-architecture.ps1` | 追加 catalog-check 调用 + namespace manifest canonical-LF-only 断言 | §3.8 接入门禁；C1「容错不外溢」 | 既有门禁全部保留；新断言独立，不复用历史 manifest 规则 |
| `crates/messaging/tests/outbox_reliability.rs`、`crates/audit-postgres/tests/chain_pg.rs` | `sqlx::migrate!("../../migrations")` → `runtime_migration::MIGRATOR.run(pool)` | §3.7 / P-4 的两处既有违规 | `#[ignore]` 保留，PG 行为仍 NOT RUN |
| `crates/audit-postgres/Cargo.toml`、`Cargo.lock` | dev-dependency `runtime-migration` | 上一条所需 | — |
| `migrations/modules/{identity-management,policy}/{catalog.json,MANIFEST.sha256}` | 新增 6 字段 catalog + 空 manifest | §3.3 正式 catalog 预留 | 只能打印 `RESERVED` |
| `.gitattributes` | 新增，path 作用域两条 `text eol=lf` | 使摘要 fixture 与 namespace manifest 不随 `core.autocrlf` 漂移；**历史 migrations 故意不列入** | 缺失则 fixture 摘要随开发者配置漂移 |

---

## 2. C1 历史摘要兼容规则（落实方式与不改动证明）

C1 的事实（本轮在 `5c8355d` 上实测，非引用）：`migrations/MANIFEST.sha256` 的 19 条里**恰好 7 条**只与 **CRLF 形**摘要相符——`002_document_metadata`、`003_outbox_reliability`、`004_outbox_state_reconciliation`、`005_document_idempotency`、`006_inbox_idempotency`、`007_document_integrity_constraints`、`008_document_query_indexes`；其余 12 条（含盘上本就是 LF 的 `001`、`018`）与 LF 形一致。仓库 `core.autocrlf=true` 且无历史 `.gitattributes`，因此这个分布是 checkout 配置差异的来源，不是数据缺陷。

落实的四条：

1. **历史 manifest 的三形容错保留**（raw / LF / CRLF 之一即可），`verify_history` 对 `baseline-sql` 条目使用 `manifest_digest_accepts`，并在日志里打印 `legacy_manifest_form_entries=7`，使这个容错是**可见、可数**的，不是静默通过。
2. **每个历史文件另被 blob 摘要钉住**：`git cat-file blob 5c8355d:<path>` 的字节做 SHA-256，逐条与钉住表比对；比对同时覆盖 钉住 commit / `HEAD` / 索引 / 工作树（LF 折叠）四方。因此"容错"不可能被用来掩盖内容改动——改内容必改 blob。
3. **容错不外溢**：新建 namespace catalog 的 `MANIFEST.sha256` 只接受规范 LF 摘要，文件本身含 CR 即拒；由 `Assert-ModuleManifestCanonicalLf`（check-architecture）与 catalog 校验各自独立实现，历史规则**不被复用**（fixture `manifest-non-canonical-digest` 拒绝、`crlf-checkout-validated` 通过，证明这条边界真的在中间）。
4. **不改写历史**：`git diff 6decf1a..b25e2d4 -- migrations/*.sql migrations/MANIFEST.sha256` 为空（见 §6）；`.gitattributes` 故意不覆盖 `migrations/` 根，历史文件仍按原样 checkout 为 CRLF（本轮在 fresh clone 中实测：历史 manifest 含 CR，fixture 树不含 CR）。

**OID ≠ 摘要**：审阅 C1 警告的失败模式是把 `git show <sha>:<path>` 的输出或 `git rev-parse <sha>:<path>` 的 **40 位对象 id** 当成摘要。本轮以测试 `a_git_object_id_is_never_a_content_digest` 钉住：同一文件的 blob OID（40 hex）与 blob SHA-256（64 hex）不同，且 `validate_sha256_hex` 拒绝 40 hex。

---

## 3. 正/负例对账（按 case ID）

期望表 `tools/module-release-tool/tests/fixtures/wave5b/catalogs/EXPECTATIONS.json` 是唯一真源，两个读取者读同一张表：

- 读取者 A：`scripts/test-plan0014-wave5b.ps1`（`catalog-cases` 家族直接来自该表；`history-cases` / `embed-cases` / `freeze-cases` / `path-cases` 是只能在真实 Git/文件系统上构造的家族）。
- 读取者 B：`tools/module-release-tool/tests/wave5b_catalog.rs::every_shared_fixture_case_produces_the_recorded_verdict`（同一张表、同一批目录，但走 Rust API）。

一条规则若只在一侧绿、另一侧红，对账立刻暴露；`no_fixture_directory_is_left_uncovered` 进一步要求每个 fixture 目录都在期望表里有一行（新增 fixture 忘记写期望 = 红）。


- catalog-cases：51 个 case，MATCH 51，BLOCKED 0，MISMATCH 0

| case | 期望 | 实得 | 判定 | 依据（截断） |
|---|---|---|---|---|
| `reserved` | pass | pass | MATCH | status=RESERVED |
| `validated` | pass | pass | MATCH | status=VALIDATED |
| `crlf-checkout-validated` | pass | pass | MATCH | status=VALIDATED |
| `catalog-unknown-field` | fail | fail | MATCH | matched-substring='table_prefix' |
| `catalog-missing-field` | fail | fail | MATCH | matched-substring='ledger' |
| `catalog-duplicate-field` | fail | fail | MATCH | matched-substring='declared twice' |
| `catalog-wrong-type` | fail | fail | MATCH | matched-substring='invalid type' |
| `catalog-unknown-schema` | fail | fail | MATCH | matched-substring='plan-0014.wave5b.catalog.v2' |
| `catalog-owner-mismatch` | fail | fail | MATCH | matched-substring='does not equal namespace' |
| `catalog-directory-mismatch` | fail | fail | MATCH | matched-substring='does not equal the directory name' |
| `catalog-unknown-context` | fail | fail | MATCH | matched-substring='authoritative contexts' |
| `catalog-bad-ledger` | fail | fail | MATCH | matched-substring='platform_module_migrations' |
| `catalog-bad-sql-schema` | fail | fail | MATCH | matched-substring='sql_schema' |
| `catalog-not-a-directory` | fail | fail | MATCH | matched-substring='is not a namespace directory' |
| `manifest-path-entry` | fail | fail | MATCH | matched-substring='plain 'NNN_lower_name.sql' basename' |
| `manifest-absolute-entry` | fail | fail | MATCH | matched-substring='plain 'NNN_lower_name.sql' basename' |
| `manifest-parent-entry` | fail | fail | MATCH | matched-substring='plain 'NNN_lower_name.sql' basename' |
| `manifest-backslash-entry` | fail | fail | MATCH | matched-substring='plain 'NNN_lower_name.sql' basename' |
| `manifest-dot-entry` | fail | fail | MATCH | matched-substring='plain 'NNN_lower_name.sql' basename' |
| `manifest-ghost-entry` | fail | fail | MATCH | matched-substring='no such file is in' |
| `manifest-missing-entry` | fail | fail | MATCH | matched-substring='absent from' |
| `manifest-duplicate-entry` | fail | fail | MATCH | matched-substring='lists '001_wave5b_fixture_foundation.sql' twice' |
| `revision-duplicate` | fail | fail | MATCH | matched-substring='revision 001 is used by more than one file' |
| `revision-gap` | fail | fail | MATCH | matched-substring='leaves a gap' |
| `manifest-non-canonical-digest` | fail | fail | MATCH | matched-substring='canonical LF digest' |
| `manifest-empty-not-zero-bytes` | fail | fail | MATCH | matched-substring='blank line' |
| `manifest-unknown-digest` | fail | fail | MATCH | matched-substring='canonical LF digest' |
| `dependency-unknown-namespace` | fail | fail | MATCH | matched-substring='unknown namespace 'acme'' |
| `dependency-baseline-zero` | fail | fail | MATCH | matched-substring='positive revision' |
| `dependency-baseline-unavailable` | fail | fail | MATCH | matched-substring='pinned baseline provides 19' |
| `dependency-without-revision` | fail | fail | MATCH | matched-substring=''<name>@<revision>'' |
| `dependency-self` | fail | fail | MATCH | matched-substring='depends on itself' |
| `dependency-duplicate` | fail | fail | MATCH | matched-substring='more than once' |
| `dependency-cycle` | fail | fail | MATCH | matched-substring='form a cycle' |
| `dependency-revision-unavailable` | fail | fail | MATCH | matched-substring='requires revision 1 of 'identity-management'' |
| `dependency-same-target-twice` | fail | fail | MATCH | matched-substring='more than once' |
| `sql-renames-whole-table` | fail | fail | MATCH | matched-substring=''RENAME TO' renames the whole table' |
| `sql-likes-other-table` | fail | fail | MATCH | matched-substring=''LIKE <table>' copies another table's definition' |
| `sql-partitions-of-host-table` | fail | fail | MATCH | matched-substring=''PARTITION OF' inherits a parent table' |
| `sql-drops-host-table` | fail | fail | MATCH | matched-substring='may not destroy it, even as its owner' |
| `sql-drops-own-revision-table` | pass | pass | MATCH | status=VALIDATED |
| `sql-references-other-owner` | fail | fail | MATCH | matched-substring='gives to owner 'identity-management'' |
| `sql-creates-other-owner-table` | fail | fail | MATCH | matched-substring='host baseline table owned by 'identity-management'' |
| `sql-references-platform-ledger` | fail | fail | MATCH | matched-substring='platform ledger' |
| `sql-undeclared-new-table` | fail | fail | MATCH | matched-substring='without an '@bp-owned: wave5b_fixture_policy_t4' declaration' |
| `sql-declared-not-created` | fail | fail | MATCH | matched-substring='never creates it' |
| `sql-dynamic-sql` | fail | fail | MATCH | matched-substring='dollar-quoted' |
| `sql-qualified-reference` | fail | fail | MATCH | matched-substring='schema-qualified' |
| `sql-unsupported-statement` | fail | fail | MATCH | matched-substring='is outside the supported SQL subset' |
| `table-name-conflict` | fail | fail | MATCH | matched-substring='two namespaces may not create the same table' |
| `F3-module-sql-catalog` | pass | pass | MATCH | namespace=policy owner=policy status=VALIDATED revisions=001+002 depends_on=platform-baseline@19 |

- embed-cases：7 个 case，MATCH 7，BLOCKED 0，MISMATCH 0

| case | 期望 | 实得 | 判定 | 依据（截断） |
|---|---|---|---|---|
| `E1-working-tree` | pass | pass | MATCH | embed scanned_rs_files=261 baseline_embed_points=1 crate_relative_embeds=2 |
| `E2-second-baseline-embed` | fail | fail | MATCH | module-release: refused by the [baseline-embed] check: 2 call sites embed the host baseline catalog ('crates/messagin… |
| `E3-non-literal-argument` | fail | fail | MATCH | module-release: refused by the [baseline-embed] check: the 'migrate' macro in 'crates/messaging/src/second_embed.rs:1… |
| `E4-embed-leaves-owning-crate` | fail | fail | MATCH | module-release: refused by the [baseline-embed] check: the embed at 'crates/runtime-migration/src/lib.rs:6' names '..… |
| `E4b-baseline-embed-missing` | fail | fail | MATCH | module-release: refused by the [baseline-embed] check: no file embeds the host baseline catalog; the single embed poi… |
| `E5-outbox_reliability.rs` | pass | pass | MATCH | applies the baseline through runtime_migration::MIGRATOR |
| `E5-chain_pg.rs` | pass | pass | MATCH | applies the baseline through runtime_migration::MIGRATOR |

- freeze-cases：5 个 case，MATCH 5，BLOCKED 0，MISMATCH 0

| case | 期望 | 实得 | 判定 | 依据（截断） |
|---|---|---|---|---|
| `F2-extra-baseline-sql` | fail | fail | MATCH | module-release: refused by the [baseline-candidate-freeze] check: the flat baseline directory must hold exactly the 1… |
| `F3-module-sql-not-baseline` | pass | pass | MATCH | freeze baseline_sql_files=19 subdirectories=modules |
| `F1-clean-copy` | pass | pass | MATCH | freeze baseline_sql_files=19 subdirectories=modules |
| `F4-extra-subdirectory` | fail | fail | MATCH | module-release: refused by the [baseline-candidate-freeze] check: 'migrations/archive' mixes a subdirectory into the … |
| `F5-stray-file` | fail | fail | MATCH | module-release: refused by the [baseline-candidate-freeze] check: 'migrations/NOTES.txt' is not part of the pinned ba… |

- history-cases：11 个 case，MATCH 11，BLOCKED 0，MISMATCH 0

| case | 期望 | 实得 | 判定 | 依据（截断） |
|---|---|---|---|---|
| `H1-clean-clone` | pass | pass | MATCH | history[5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601] baseline_revision=19 blob_entries_verified=40 working_tree_entries_… |
| `H2-worktree-drift` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: the working tree file 'F:\Workspace\business-platform… |
| `H3-worktree-line-endings-only` | pass | pass | MATCH | CRLF checkout of the pinned LF blob |
| `H4-staged-drift` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: the index stores '897d572b790833271eff4811b28db90830c… |
| `H5-committed-drift` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: 'HEAD:migrations/013_runtime_governance_revision1.sql… |
| `H6-worktree-deleted` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: the working tree no longer contains 'F:\Workspace\bus… |
| `H7-new-baseline-sql-history` | pass | pass | MATCH | immutability covers the pinned 19; the count rule is the explicit freeze |
| `H8-manifest-extended` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: the working tree file 'F:\Workspace\business-platform… |
| `H11-manifest-only-drift` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: the working tree file 'F:\Workspace\business-platform… |
| `H12-manifest-line-removed` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: the working tree file 'F:\Workspace\business-platform… |
| `H10-no-pinned-commit` | fail | fail | MATCH | module-release: refused by the [pinned-baseline-history] check: pinned commit 5c8355dceba9cf062a63fa6a2b5c6ef1ff41b60… |

- path-cases：2 个 case，MATCH 1，BLOCKED 1，MISMATCH 0

| case | 期望 | 实得 | 判定 | 依据（截断） |
|---|---|---|---|---|
| `P1-namespace-junction` | fail | fail | MATCH | module-release: refused by the [module-catalog] check: namespace directory 'C:\Users\admin\AppData\Local\Temp\plan001… |
| `P2-catalog-symlink` | fail | not-run | BLOCKED | the host refused to create a file symbolic link: 此操作需要管理员权限。 (the repository volume is exFAT, which holds no reparse … |

合计 **76** 个 case：MATCH **75**，BLOCKED **1**，MISMATCH **0**（全部 76 个 case 的机器可读原文见 `wave5b-cases/*.csv`）。

### 3.4 规则 → case ID 映射（review §3 逐条）

| review 条目 | 规则 | 正例 case | 负例 case |
|---|---|---|---|
| §3.3 | 6 字段严格解析、未知/缺失/错类型/重复字段拒绝 | `reserved`、`validated` | `catalog-missing-field`、`catalog-unknown-field`、`catalog-wrong-type`、`catalog-duplicate-field`、`catalog-bad-ledger`、`catalog-bad-sql-schema`、`catalog-unknown-schema` |
| §3.3 | 目录名 == namespace == owner，且 owner ∈ 权威 context | `reserved`、`validated` | `catalog-directory-mismatch`、`catalog-owner-mismatch`、`catalog-unknown-context` |
| §3.3 / §11.5 | 空 manifest 只能 `RESERVED`，且必须是 0 字节 | `reserved` | `manifest-empty-not-zero-bytes` |
| §3.4 | manifest ↔ SQL 双向集合相等 | `validated` | `manifest-missing-entry`、`manifest-ghost-entry`、`manifest-duplicate-entry` |
| §3.4 / C1 | 新 catalog 只认 canonical LF 摘要；历史容错不外溢 | `validated`、`crlf-checkout-validated`（字节 CRLF、摘要记 LF） | `manifest-non-canonical-digest`、`manifest-unknown-digest` |
| §3.4 | manifest 条目必须是裸 `NNN_name.sql` | `validated` | `manifest-path-entry`、`manifest-absolute-entry`、`manifest-parent-entry`、`manifest-backslash-entry`、`manifest-dot-entry` |
| §3.4 | revision 号不得重复、不得缺口 | `validated`（001、002 连续） | `revision-duplicate`、`revision-gap` |
| §3.4 | 依赖语法精确、无环、同目标不重复、被依赖 revision 必须存在 | `reserved`、`validated`（`platform-baseline@19`） | `dependency-without-revision`、`dependency-baseline-zero`、`dependency-baseline-unavailable`、`dependency-unknown-namespace`、`dependency-self`、`dependency-duplicate`、`dependency-cycle`、`dependency-revision-unavailable`、`dependency-same-target-twice` |
| §3.4 | 跨 namespace 同表名冲突拒绝 | — | `table-name-conflict` |
| §3.5 | owner 白名单 + `@bp-owned` 授权扩展 | `validated`、`sql-drops-own-revision-table` | `sql-creates-other-owner-table`、`sql-references-other-owner`、`sql-undeclared-new-table`、`sql-declared-not-created`、`sql-qualified-reference`、`sql-references-platform-ledger` |
| §3.5 | 无法归因的结构点名拒绝（不猜） | — | `sql-renames-whole-table`、`sql-likes-other-table`、`sql-partitions-of-host-table`、`sql-dynamic-sql`、`sql-unsupported-statement` |
| §3.5 补 | host baseline 表禁止 schema 级销毁（owner 也不行） | `sql-drops-own-revision-table` | `sql-drops-host-table` |
| §3.6 | 钉住历史在干净仓库通过 | `H1-clean-clone` | `H2-worktree-drift`、`H4-staged-drift`、`H5-committed-drift`、`H6-worktree-deleted`、`H11-manifest-only-drift`、`H12-manifest-line-removed` |
| §3.6 | EOL-only 差异按文档容错通过（blob 为准） | `H3-worktree-line-endings-only` | — |
| §3.6 | 不可变性覆盖 19 条；数量规则属显式 freeze | `H7-new-baseline-sql-history`（history 通过） | `F2-extra-baseline-sql`（freeze 拒绝） |
| §3.6 | manifest 与 blob 交叉一致 | `H1` | `H8-manifest-extended` |
| §3.6 / §3.2 | 无钉住 commit 时 fail closed，不回落到 HEAD | — | `H10-no-pinned-commit`、Rust `a_repository_without_the_pinned_commit_is_refused_without_a_fallback` |
| §3.7 | 唯一 baseline embed 点 + crate 只能 embed 自有 catalog | `E1-working-tree`、`E5-*` 两条 | `E2-second-baseline-embed`、`E3-non-literal-argument`、`E4a-embed-leaves-owning-crate`、`E4b-baseline-embed-missing` |
| §3.7 / P-4 | 两个 PG 测试改走 `runtime_migration::MIGRATOR` | `E5-outbox_reliability.rs`、`E5-chain_pg.rs`（回归守卫） | 同两条：若被改回 `sqlx::migrate!` 即红 |
| §3.1 | freeze 只认扁平 baseline，模块 SQL 不算 | `F3-module-sql-not-baseline`、`F1` | `F2-extra-baseline-sql`、`F4-extra-subdirectory`、`F5-stray-file` |
| §3.8 | 路径逃逸 / 符号链接 / junction 拒绝；祖先被替换也必须拒 | — | `P1-modules-junction`（真实 junction，实测拒绝）；`P2-catalog-symlink`（**BLOCKED**，见 §5）；manifest 路径条目族（同上） |
| §3.2 | 默认 check 集合不含 freeze | Rust `the_default_gate_excludes_the_candidate_freeze`；`20-catalog-check-default.log` | `25-catalog-check-unknown-check.log`（未知 check 拒绝，exit 1） |

---

## 4. 实际门禁结果（逐条命令与真实退出码）

所有日志在 `docs/reviews/2026-09-30-wave5b-s1-gates/gates/`，每个文件头部记录 `# command:` / `# started:` / `# finished:` / `# exit-code:`，可逐行复跑。宿主：Windows，`pwsh 7.6.6`，`git 2.50.1.windows.1`，Rust `1.94.1`（`x86_64-pc-windows-msvc`）。所有命令经 `rtk proxy` 包裹（本仓所有 shell 命令的统一入口）。

三条如实记录的"未一次成功"，日志全部保留，不做删改：

- `37-test-workspace.log` / `37-test-workspace-attempt-2.log`：默认 target 目录下 `cargo test --workspace` 报 `failed to remove file target\debug\business-api.exe` / `business-worker.exe`——本机此刻正有这两个开发进程在跑（PID 见 `00-anchors.txt`），二进制被自己占住。本轮**没有**终止用户进程，而是按设计包 §9.4 对 V14 的规定改用隔离 target 目录重跑（`37b-test-workspace-isolated-target.log`），这与 Wave 5A 的 `38-test-workspace-isolated-target.log` 做法一致。
- `41-wave5b-offline-catalog-gate.log`：首次跑出 2 条 MISMATCH，根因是**本轮自己新写的两个 case**（`H11` / `H12`）按 LF 解析克隆出来的 `migrations/MANIFEST.sha256`，而该文件在 `core.autocrlf=true` 下克隆为 CRLF，导致 fixture 实际什么都没改。校验器行为正确（`H8` 从另一条路径证明 manifest 漂移会被拒），缺陷在驱动侧，修正见 attempt-2。
- 证据目录保护的一次真实触发：不带 `-Force` 重跑驱动时它直接拒绝启动——`evidence directory '…\wave5b-cases' already holds 7 item(s); pass -Force to replace it, or choose a new directory`（exit 1）。这条拒绝本身是期望行为（保护上一轮证据不被递归清空）。**如实记录一处证据缺陷**：该次拒绝的日志随后被带 `-Force` 的同名重跑覆盖（`41-…-attempt-2.log` 现在是 exit 0 的通过记录），因此这条拒绝只有本段的文字记录，没有独立日志文件；下一轮若再触发，应写到新目录或换日志名以保留证据。

| 日志 | 命令 | 退出码 | 用时 |
|---|---|---:|---:|
| `20-catalog-check-default.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform` | 0 | 7s |
| `21-catalog-check-only-catalog.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform --only module-catalog` | 0 | 1s |
| `22-catalog-check-only-history.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform --only pinned-baseline-history` | 0 | 7s |
| `23-catalog-check-only-embed.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform --only baseline-embed` | 0 | 1s |
| `24-catalog-check-only-freeze.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform --only baseline-candidate-freeze` | 0 | 1s |
| `25-catalog-check-unknown-check.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform --only everything` | 1（负例：未知 check 必须失败，退出码 1 即预期） | 1s |
| `26-catalog-check-fresh-clone.log` | `rtk proxy cargo run -q -p module-release-tool -- catalog-check --repo F:\Workspace\business-platform\target\tmp\wave5b-fresh-clone` | 0 | System.Object[]s |
| `30-fmt-check.log` | `rtk proxy cargo fmt --all -- --check` | 0 | 2s |
| `31-check-workspace.log` | `rtk proxy cargo check --workspace --all-targets --all-features` | 0 | 5s |
| `32-clippy-workspace.log` | `rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | 5s |
| `33-test-runtime-migration.log` | `rtk proxy cargo test -p runtime-migration --all-features` | 0 | 1s |
| `34-test-module-release-tool.log` | `rtk proxy cargo test -p module-release-tool --all-features` | 0 | 322s |
| `35-build-no-run-outbox-reliability.log` | `rtk proxy cargo test -p messaging --test outbox_reliability --no-run` | 0 | 1s |
| `36-build-no-run-chain-pg.log` | `rtk proxy cargo test -p audit-postgres --test chain_pg --no-run` | 0 | 1s |
| `37-test-workspace-attempt-2.log` | `rtk proxy cargo test --workspace --all-features` | 101（默认 target 目录被本机正在运行的 `business-api` / `business-worker` 进程占住二进制而中断；按 §9.4 V14 改用隔离 target 目录，见 37b-*.log） | 23s |
| `37-test-workspace.log` | `rtk proxy cargo test --workspace --all-features` | 101（默认 target 目录被本机正在运行的 `business-api` / `business-worker` 进程占住二进制而中断；按 §9.4 V14 改用隔离 target 目录，见 37b-*.log） | 43s |
| `37b-test-workspace-isolated-target.log` | `rtk proxy cargo test --workspace --all-features --target-dir F:\Workspace\business-platform\target\gate-target` | 0 | 277s |
| `40-check-architecture.log` | `rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | 17s |
| `41-wave5b-offline-catalog-gate-attempt-2.log` | `rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -EvidenceDir docs/reviews/2026-09-30-wave5b-s1-gates/wave5b-cases -Force` | 0（第二次尝试被本轮自己的证据目录保护挡住：已存在的 gate 证据不会被清空，需 -Force；这条拒绝本身是期望行为，日志保留） | 212s |
| `41-wave5b-offline-catalog-gate.log` | `rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -EvidenceDir docs/reviews/2026-09-30-wave5b-s1-gates/wave5b-cases` | 1（首次尝试：本轮自查出 2 条驱动 fixture 缺陷（clone 的 manifest 是 CRLF），已在 attempt-2 修正；失败日志保留） | 200s |
| `41-wave5b-offline-catalog-gate-attempt-2.log` | 同上 + `-Force` | 0（带 `-Force` 重跑通过；76 case：75 MATCH / 1 BLOCKED / 0 MISMATCH） | 212s |
| `42-test-module-release-tool-at-candidate.log` | `rtk proxy cargo test -p module-release-tool --all-features` | 0 | 308s |
| `43-test-module-release-tool-after-tooling-added.log` | `rtk proxy cargo test -p module-release-tool --all-features` | 0（新增生成器入库后复跑：新文件不改变 packaging 的源输入，`source-inputs-match-revision` 仍绿） | 299s |
| `44-check-architecture-after-tooling-added.log` | `rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1` | 0（同上，架构门禁对新增脚本不敏感） | 16s |
| `50-git-diff-check.log` | `git diff --check` | 0 | 0s |
| `51-git-status-short.log` | `git status --short` | 0 | 0s |
| `52-git-diff-check-at-candidate.log` | `git diff --check` | 0 | 0s |
| `53-git-status-short-at-candidate.log` | `git status --short` | 0 | 0s |
| `54-git-candidate-anchors.log` | `git log --oneline -n 3 --format=%H %ad %s --date=iso-strict` | 0 | 0s |
| `55-git-diff-check-at-docs-commit.log` | `git diff --check`（文档提交后，工作树干净） | 0 | 0s |
| `56-git-status-short-at-docs-commit.log` | `git status --short`（文档提交后，工作树干净） | 0 | 0s |

**`git diff --check` 的两处如实例外**：本轮文档与代码自身不含空白问题，但 `git diff --cached --check` 仍会在两类文件上报警——① 归档的审阅指令文档 `docs/reviews/2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md` 头部两空格换行（Markdown 硬换行，**该文件按原样入库，一字不改**）；② 门禁日志原文里的行尾空格（如 `41-wave5b-offline-catalog-gate.log` 的 MISMATCH 行），日志是原始证据，**不做清理**，清理即伪造证据。两类都在此处显式登记，不靠"整仓 exit 0"混过去；`gates/50-*.log` 与 `gates/52-*.log` 记录的是候选提交时受版本控制的产品与脚本文档树，那里 exit 0。
对账口径（V14 增量对账，**不要求**等于历史数字）：`docs/reviews/2026-09-30-wave5a-repair2-gates/ignored-tests-classified.md` 的基线是 `c22939f0` 处 **864 passed / 0 failed / 70 ignored / 191 suites**。本轮工作区实得 **883 passed / 0 failed / 70 ignored，192 个测试套件**（口径：把日志里每条 	est result: 行的 passed/failed/ignored 相加，套件数 = Running/Doc-tests 行数；日志 37b-test-workspace-isolated-target.log）；差值逐条为：新增测试全部来自新测试目标 `tools/module-release-tool/tests/wave5b_catalog.rs`（19 条，逐条：`a_git_object_id_is_never_a_content_digest`、`a_new_namespace_digest_is_canonical_lf_only`、`a_repository_without_the_pinned_commit_is_refused_without_a_fallback`、`dependency_grammar_is_fixed_and_does_not_guess`、`every_pinned_entry_is_the_blob_of_the_pinned_commit`、`every_shared_fixture_case_produces_the_recorded_verdict`、`exactly_one_file_embeds_the_host_baseline_catalog`、`head_and_index_still_hold_the_pinned_baseline`、`host_table_ownership_is_unique_and_inside_the_authoritative_contexts`、`migration_019_is_not_a_module_catalog`、`no_fixture_directory_is_left_uncovered`、`the_attribution_table_is_migration_019_itself`、`the_conformance_tests_do_not_embed_a_second_baseline`、`the_default_gate_excludes_the_candidate_freeze`、`the_embedded_pinned_table_is_the_checked_in_fixture`、`the_freeze_only_counts_the_flat_baseline`、`the_repository_itself_passes_the_default_checks`、`the_seven_c1_entries_are_pinned_by_blob_even_though_the_manifest_is_crlf`、`unattributable_sql_forms_are_refused_by_name`），套件数因此 +1（该文件是一个新的 test target）。除此之外本轮未新增、未删除任何测试，也未把任何 `#[ignore]` 去掉，因此 ignored 计数不变；若与实际差值不符，以日志为准并如实修正本段。。历史 864 / 191 / 70 是**对账基线，不是本轮目标**，本轮不机械固定它们，也不把它们当通过判据。

---

## 5. NOT RUN 与 BLOCKED（不得声称通过的部分）

| 项 | 状态 | 原因 |
|---|---|---|
| `chain_pg`（audit-postgres PostgreSQL 契约） | **NOT RUN** | 本轮无数据库授权；`#[ignore]` 保留，只做到 `--no-run` 编译通过（`36-build-no-run-chain-pg.log`） |
| `outbox_reliability` 的 PostgreSQL lane | **NOT RUN** | 同上（`35-build-no-run-outbox-reliability.log`） |
| 设计包 §9 的 V1–V9、V11（DB 部分）、V12、V13、V14 全部数据库项 | **NOT RUN / BLOCKED** | 未连接任何数据库（含只读）；S2 未实施，runner/ledger 不存在 |
| `P2-catalog-symlink`（catalog 目录为符号链接） | **BLOCKED** | 本机创建文件符号链接需管理员权限（`UnauthorizedAccessException`），且仓库卷为 **exFAT**，不持有 reparse point。驱动如实输出 BLOCKED 行并在末行声明"能跑的都对上，1 条 BLOCKED"，不记作 MATCH。同族的 junction 变体 `P1-modules-junction` 能真实创建并已实测拒绝 |
| `cargo test --workspace` 的 `--include-ignored` 运行 | **NOT RUN** | 需要 PostgreSQL / 对象存储 / S3 授权；本轮无授权，未以任何方式部分冒充 |
| 大小写重复目录（`Policy` vs `policy`）负例 | **BLOCKED（宿主不可表达）** | 规则已实现（`assert_no_case_collisions`），但 NTFS/exFAT 大小写不敏感，无法在同一目录同时放两个 twin；该规则的验证停留在单元测试级别 |
| 钉住历史表的生成器 | 已入库 | 提交到 `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1`（fixture 生成器、门禁驱动与 closeout 对账器同目录）；再 pin 仍是设计级动作，须经独立审阅（§9.6） |

---

## 6. 固定历史证明（可复跑）

```powershell
git diff --stat 6decf1ad9ea05233d66dd3539594ff210afd99f2..b25e2d4a9b455b776934495b8f3dfd2b71a13fe2 -- migrations/*.sql migrations/MANIFEST.sha256   # 空输出
git diff --name-only 6decf1a..b25e2d4 -- migrations/                                                                                                 # 只有 migrations/modules/**
```

```text
# git diff --stat 6decf1a..b25e2d4 -- migrations/*.sql migrations/MANIFEST.sha256
[rtk] /!\ No hook installed — run `rtk init -g` for automatic token savings
# (empty above = 历史 SQL 与历史 manifest 一字未改)

# git diff --name-only 6decf1a..b25e2d4 -- migrations/
[rtk] /!\ No hook installed — run `rtk init -g` for automatic token savings
migrations/modules/identity-management/MANIFEST.sha256
migrations/modules/identity-management/catalog.json
migrations/modules/policy/MANIFEST.sha256
migrations/modules/policy/catalog.json
```

门禁自身的打印（`22-catalog-check-only-history.log`）：

```
history[5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601] mode=blob+index+working-tree
history[5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601] baseline_revision=19 blob_entries_verified=40 working_tree_entries_verified=20 legacy_manifest_form_entries=7
```

`blob_entries_verified=40` = 20 个路径 ×（钉住 commit↔HEAD、钉住 commit↔索引）两组比对；`working_tree_entries_verified=20` = 工作树 LF 折叠后与钉住字节一致；`legacy_manifest_form_entries=7` 即 C1 的七条。

**checkout 端稳定性**（`.gitattributes` 只作用于新路径的证据）：在候选提交上做一次本地 fresh clone（`--no-hardlinks`，`core.autocrlf=true`）后实测：fixture `MANIFEST.sha256` **无** CR，历史 `migrations/MANIFEST.sha256` **有** CR（未被规范化），普通 `.rs` 文件仍为 CRLF；并在该 clone 上以 `catalog-check` 默认 check 复跑通过（`26-catalog-check-fresh-clone.log`，exit 0）。

---

## 7. 两份正式 catalog 的实际输出（RESERVED，非安装证明）

```
checks=module-catalog,pinned-baseline-history,baseline-embed
namespace=identity-management owner=identity-management status=RESERVED revisions= depends_on=platform-baseline@19
namespace=policy owner=policy status=RESERVED revisions= depends_on=platform-baseline@19
order=identity-management>policy
history[5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601] baseline_revision=19 blob_entries_verified=40 working_tree_entries_verified=20 legacy_manifest_form_entries=7
embed scanned_rs_files=261 baseline_embed_points=1 crate_relative_embeds=2
status=ok
```

这两行只说明：两份 catalog 元数据自洽、`depends_on = platform-baseline@19`、manifest 为 0 字节、因此状态是 `RESERVED`。**它们不证明**：模块能在 PostgreSQL 上独立安装 / 升级 / 回滚；`migrations/modules` 是已安装 catalog；G-203a-i/ii 或任何准入子项被关闭；ADR-0029 被接受。

---

## 8. 回滚方法

本轮全部为本地提交，回滚不需要数据库或部署动作：

```powershell
# 整轮回滚（回到起点）
git reset --hard 6decf1ad9ea05233d66dd3539594ff210afd99f2

# 只回滚文档/证据提交，保留实现候选
git reset --hard b25e2d4a9b455b776934495b8f3dfd2b71a13fe2

# 只回退门禁接入（保留 validator 与 fixture，让 check-architecture 不再调用它）
git checkout 6decf1a -- scripts/check-architecture.ps1
```

按提交内容看，风险面各自独立：`crates/messaging/tests/outbox_reliability.rs` 与 `crates/audit-postgres/tests/chain_pg.rs` 的改动只影响 `#[ignore]` 的 PG 测试（回滚只需一并还原 `crates/audit-postgres/Cargo.toml` 与 `Cargo.lock`）；`migrations/modules/**` 与 `tools/module-release-tool/**` 是纯新增；`.gitattributes` 只覆盖两条新路径，删除它不会规范化任何历史文件。历史 baseline 从未被写入，因此不存在"回滚迁移内容"的问题。

---

## 9. 已知限制与接受的风险（诚实清单）

1. **owner 扫描是文本 fitness，不是隔离证明**：它阻止"把别人的表写进自己的迁移"，不阻止数据库层的跨 owner 写入；真正边界是角色/grant（D-5 / OPEN-B2-3）。
2. **`%` 在字符串字面量之外被拒**：SQL 子集不容纳 `LIKE … '%'` 之外的裸 `%`（避免把模式当成语法噪声跳过），因此是**具名拒绝**而非静默通过；未来 revision 若需要，须扩子集并补 fixture。
3. **`RENAME TO` 整表一律拒绝**：历史 baseline 里 `019` 的四条 v0 demo rename 因此**在模块子集之外**——这是对的（模块 revision 不该整表改名），但意味着"模块改自己的表名"目前没有合法路径，需要时要先扩规则并记账（Rust 测试 `migration_019_is_not_a_module_catalog` 把这个事实显式钉住，而不是偷偷放宽子集去适配历史文件）。
4. **数据语句（`INSERT` / `UPDATE` / `DELETE`）对自有 host 表仍允许**：本轮只把 schema 级销毁（`DROP` / `TRUNCATE` / 整表 `RENAME`）钉死；数据面清理属 S2 的策略面。
5. **H7 的判读**：新增第 20 个 baseline SQL 时 `pinned-baseline-history` 记 **pass**（不可变性覆盖钉住的 19 条），拒绝它的是 `baseline-candidate-freeze`（F2）。§3.6 第 3 点若被读成"历史检查自己就该数数量"，则本轮实现取的是第 5 点（数量属显式 freeze）这一条更具体的规则。
6. **钉住表的生成器已入库，但再 pin 仍是设计级动作**：`pinned-baseline-*.json` 的生成脚本已提交到 `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1`（fixture 生成器、门禁驱动与 closeout 对账器同目录，见其 README）。把钉住点换到另一个 SHA 等于改变"历史"的定义，必须经独立审阅，不能自行换。
7. **freeze 不是默认策略**：默认 check 集合是 catalog + history + embed；`baseline-candidate-freeze` 只在显式 `--only` 时评估（ADR-0029 未 Accepted 前不得成为默认）。
8. **`cargo test -p module-release-tool` 在提交前会红**：`tests/packaging.rs::source-inputs-match-revision` 把工作树 `Cargo.lock` 与 `6decf1a` 处已提交版本比较；本轮改了 `Cargo.lock`，因此该测试只在候选提交之后才绿。这既是门禁真实性的证据（它确实比对的是钉住 revision 的输入），也提醒 S2 每次改锁文件后必须在**提交后**复跑（`42-test-module-release-tool-at-candidate.log`）。
9. **同一 target 目录并发跑 cargo 会污染门禁结果**：validator 在并发下可能无输出，驱动把它判为"无法计分"直接抛错，而不是当成 fail；本轮所有门禁串行执行，并使用独立 `-TargetDir`。
10. **符号链接家族的宿主依赖**：见 §5；换到支持 reparse point 的卷 + 特权环境后 `P2` 才能跑。

---

## 10. 停止点

实现与门禁已到位，等待**独立复审**对候选 `b25e2d4a9b455b776934495b8f3dfd2b71a13fe2`（文档轮见 §0）的判定。本轮不进入 S2、不连接数据库、不自判 ADR-0029、不改 `main`、不 push / merge / tag / publish / 部署，也不重开 Wave 5A Repair 2 或改写其归档证据。
