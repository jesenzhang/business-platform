# Wave 5A Repair 2 — F1-A / F1-B 逐项关闭表（含 §5 门禁逐条）

- 审阅：`docs/reviews/2026-09-30-wave5a-repair1-independent-review.md`（结论 **NEEDS REPAIR**，
  本轮由其 §5 指令驱动，原文口径不扩大）。承载该审阅的提交（审阅起点）：
  `7180eec7fc813b33a748784ac806d5c95b4e359b`。
- 修复前基线：`77a4fbd2ae89b5cec8ac2e6f482eba8722295387`（Repair 1 候选）。
- 代码候选：`c22939f0a51e3c7fa2ec424f3819c6e7113a9b66`（提交链见 `README.md` §1）。
- 文档 closeout HEAD：`6c63ba1598d38e71913ad633eb8a46002b35c7d3`；该提交之后对同一候选的复验见
  `closeout/`（两模块各 exit 0），其头部同时记录 HEAD、候选钉住的 SHA，以及该提交 123 个路径全在 `docs/` 下。
- 命令约定：下表命令即审阅 §5.5 的确切命令行，RTK 前缀 `rtk proxy` 略去不影响语义；
  `日志` 列是本目录内相对路径。

## 0. 先失败回归（修复前的旧代码，见 `red/`）

新增回归文件 `tools/module-release-tool/tests/source_derivation.rs`（9 例）。修复前运行
（`red/10-red-unit-regressions.log`）：**`0 passed; 9 failed`，exit 101**。每条都先证明候选
能 `verify`，再注入一处损坏，并用工具自己的 `plan-0014.artifact-integrity.v1` 算法重算
inventory 与 integrity digest 使损坏在物理层面自洽——失败原因只能来自来源派生。

| # | 用例 | 注入 | 修复前观察 | 修复后期望 |
|---|---|---|---|---|
| 1 | `packed_manifest_is_not_read_from_the_commit` | packed `Cargo.toml` 的版本 pin 改写 | **exit 0 通过**（RED） | 由 `derived-members-match-revision` 拒绝 |
| 2 | 同上（依赖 pin 改写为不存在的版本） | `async-trait = "=0.1.92"` → `"=99.0.0"` | **exit 0 通过**（RED） | 同上 |
| 3 | 同上（整段依赖表重排 + pin 改写） | 两个依赖同时改 | **exit 0 通过**（RED） | 同上 |
| 4 | `dependency_bump_survives_regeneration_for_both_pilots` | identity / policy 各一处 | **exit 0 通过**（RED） | 同上 |
| 5 | `alternative_spec_candidate_is_self_consistent` | 换一份合法 spec | 旧代码产出**另一套**工件却自洽 | 拒绝：成员集合与提交不符 |
| 6 | `generate_refuses_checkout_missing_a_committed_source`（F1-B） | 删掉 `crates/identity/src/testing.rs` | **生成成功，20 成员**（RED） | `source-member-set-matches-revision` 拒绝且不落盘 |
| 7 | `generate_refuses_checkout_with_uncommitted_source` | 多加一个未提交的 `.rs` | **生成成功**（RED） | 同上（多成员方向） |
| 8 | `missing_member_still_refused_after_rehashing` | 删成员 + 重算摘要 | **exit 0 通过**（RED） | `source-member-set-matches-revision` |
| 9 | `pinned_candidate_still_verifies_after_the_checkout_moves` + `missing_commit_object_has_no_disk_fallback` | checkout 前移 / 提交对象缺失 | — | 候选仍通过 / 无磁盘回退 |

修复后同一命令：**`9 passed; 0 failed`，exit 0**（`green/20-green-source-derivation.log`）。

## 1. F1-A — 派生成员必须按固定提交重算（§3）

| 项 | 内容 |
|---|---|
| 缺陷 | packed `Cargo.toml` 的字节来自磁盘工作树，验证只把工件里那份 `module.manifest.json` 重新编译一遍；两者都不回答「packed `Cargo.toml` 是不是那个提交里的内容」。审阅注入 `async-trait = "=99.0.0"` 并重算摘要后 `verify` 仍 exit 0。 |
| 代码 | `tools/module-release-tool/src/commit.rs`（`Inputs::Commit`：`git cat-file blob` 取字节、`git ls-tree -r -z` 取成员、`120000` / `160000` 一律拒绝）；`packaging.rs::project()`（唯一候选生成路径，`generate` 与 `verify` 共用）；`packaging.rs::pack_crate_manifest()` 改从 `Inputs::text("<crate>/Cargo.toml")` 读；`packaging.rs::assert_derived_members_match()` 逐成员全量比较。 |
| 新检查 ID | `derived-members-match-revision`（`src/provenance.rs::CHECK_DERIVED`）。比较的是**完整内容**，不是字段黑名单。 |
| 单元回归 | `source_derivation.rs` 例 1–5，`green/20-green-source-derivation.log` |
| 脚本回归 | `script-regression/script-regression-log.md` `R7a`（脚本自己重算必须与工具记录逐字节一致）、`R7b`（注入后必须被该检查拒绝） |
| 编排负例 | `pilot/negatives/n13-packed-manifest-not-from-the-commit/`（`case.md` 记录注入与 sha256 前后值） |
| 修复前证据 | `red/10-red-unit-regressions.log`；注入后的完整性摘要 `03cae522…`（`red/README.md`） |

## 2. F1-B — 成员集合来自 Git 树，不是磁盘 glob（§4）

| 项 | 内容 |
|---|---|
| 缺陷 | 源文件集合由磁盘目录扫描得出；checkout 少一个已提交文件时 `generate` 照样成功（20 成员），产出的候选没有任何提交与之对应，`cargo check` 在解包后 E0583。 |
| 代码 | `commit.rs::paths_under()`（提交侧 `ls-tree -r -z --full-name` + `tree_object` 目录存在性；工作树侧递归且拒绝符号链接）；`packaging.rs::assert_checkout_matches_commit()`（**在 `create_dir_all` 之前**：`.rs` 成员集合双向比较 + 逐字输入 blob 比较）；`packaging.rs::assert_member_set_matches()`（出厂成员集合双向 + 覆盖不变式：每个成员要么被重算、要么被 blob 比对）；`packaging.rs::crate_sources_at()`（列目录改从输入集合，`lib.rs` 缺失即拒）。 |
| 新检查 ID | `source-member-set-matches-revision`（`src/provenance.rs::CHECK_MEMBERS`）。 |
| 单元回归 | `source_derivation.rs` 例 6–9 |
| 脚本回归 | `R8a`（隔离 checkout 控制例可生成）、`R8b`（缺文件被拒且**不创建工件目录**）、`R8c`（多文件被拒） |
| 编排负例 | `pilot/negatives/n14-isolated-checkout-control/`、`n15-checkout-dropped-a-committed-source/`、`n16-checkout-added-an-uncommitted-source/`；三份 `case.md` 都带 `--repo <隔离 checkout>` 的原始命令行 |
| 修复前证据 | `red/10-red-unit-regressions.log`；从缺文件 checkout 生成的工件摘要 `350da720…`、`member_count: 20` |

## 3. §5.4.4 / §5.4.5 — 三类比较与两个独立检查标识

`verify` 现在按固定顺序跑 12 项检查（`pilot/13-verify-*.log` 逐条打印 ID）：

| 类别 | 检查 ID | 比较对象 |
|---|---|---|
| 物理完整性 | `artifact-record-format`、`artifact-integrity-digest`、`artifact-inventory`、`contract-surface-file-digest`、`release-identity-consistency`、`record-consistency`、`packed-crate-bindings` | 工件自身与它的记录 |
| 来源（成员集合） | `source-revision-exists`、**`source-member-set-matches-revision`** | 出厂成员集合 ↔ `git ls-tree -r -z` |
| 来源（未转换字节） | `source-inputs-match-revision` | verbatim 成员 ↔ `git cat-file blob` |
| 来源（派生字节） | **`derived-members-match-revision`** | packed `Cargo.toml` / `module.manifest.json` / `contract-surface.json` / `migration-mapping.json` ↔ 由固定提交重算 |
| 声明 | `toolchain-declaration` | 明细自述「只验声明」 |

§5.4.5 要求两类结论不混用：`artifact-integrity-digest` 的明细只说「payload 与自身记录一致」，
来源三项各说各的比较对象；一条通过不等于另一条通过。§5.4.4 三类**全部**通过才有来源 PASS——
`tests/pilot_candidates.rs` 断言 12 个 ID 齐全且顺序固定，缺任何一项即测试失败。

## 4. §5.4.6 — 既有协议与既有摘要保持不变

`plan-0014.artifact-integrity.v1` 的 preimage 构造、成员排序（路径字节序）、
`normalize_text_bytes` 规则一字未改；两个候选的四类摘要与 Repair 1 逐字相同：

| 模块 | 成员 | contract | package | artifact integrity | migration set |
|---|---|---|---|---|---|
| identity | 21 | `3fed976b0ac5e040366f696a1c4a7fe6140bd5bc956996fc8b51a141eba31107` | `b9dfa42ccd09190020c39ee8bf412507de2107810fc2414efdc3415761053c70` | `68deba81c942aaf624f599cd191ef6b09855438b99035cca7010ae4b7d9cc700` | `ef43cfa9…` |
| policy | 23 | `52e788ff…` | `37e95a19…` | `f8add9e0ce13f0c1e33a52eb2f28a85f726c649534e8d10584cdaf4ff6ecb243` | `1a5a43f8…` |

唯一必要且已记录的规范化口径调整：`migration-mapping.json` 里 `digest_form` 的判定顺序。
旧实现先试「原始字节摘要」，于是该字段取决于检查者恰好怎么 checkout 文件（`core.autocrlf=true`
时是 `raw`，普通 checkout 是 `lf-normalized`），映射摘要随主机漂移。现在先试**规范 LF 折叠**形式，
`digest_form` 成为提交的属性而不是主机的属性；两个候选因此回到上表记录的取值。
## 5. §5.5 兼容矩阵逐条落点

| 要求 | 证据 | 结果 |
|---|---|---|
| identity / policy 正常生成 | `pilot/10-generate-run-a-*.log`、`pilot/10-generate-run-b-*.log`（两次独立生成） | 4 × exit 0 |
| 独立进程 verify | `pilot/13-verify-identity.log`、`pilot/13-verify-policy.log`（`module-release verify` 独立进程，12 项检查全 PASS） | exit 0 |
| 重复生成 compare | `pilot/12-compare-identity.log`、`pilot/12-compare-policy.log` | exit 0，两侧各自重算后一致 |
| packed crate 构建 | `pilot/40-crate-build-identity.log`、`pilot/40-crate-build-policy.log`（以 packed crate 为 workspace root） | exit 0 |
| 消费者构建 + 测试 + `--locked --offline` 复建 + guard | `pilot/20-*`、`21-*`、`22-*`、`25/26-*-cargo-metadata*`、`26-*-guard.log`、`23/24-*` | exit 0（仓库内 lane） |
| 构建候选后固定候选仍通过 | `pilot/41-verify-after-build-identity.log`、`41-verify-after-build-policy.log` | exit 0 |
| 固定候选在后续文档 closeout HEAD 仍通过 | 本目录 `closeout/40-verify-after-closeout-*.log`（文档提交之后再验证同一候选） | exit 0 |
| checkout 输入变动后历史候选仍按原提交验证 | 单元 `pinned_candidate_still_verifies_after_the_checkout_moves`；编排 `n14/n15/n16`（`--repo <隔离 checkout>`） | 通过 / 被拒（按预期） |
| 提交对象缺失时拒绝 | 编排 `n09-zero-revision`；单元 `missing_commit_object_has_no_disk_fallback`；`red/11-red-missing-commit-guard.log` | exit 1，`source-revision-exists` |
| F2–F5 原回归保持通过 | `green/21-green-release-tool-suite.log`（`verification_guards`、`pilot_candidates`、`isolation`、`compatibility` 等全套） | 61 passed / 0 failed |
| 仓库外消费者 lane | — | **NOT RUN**（`-SkipOutsideConsumer`；`pilot/assembly-log.md` 内有一行同义记录） |

## 6. §5.5 门禁命令与退出码

| 命令 | 退出码 | 日志 |
|---|---|---|
| `cargo test -p module-release-tool --all-features` | 0 | `gates/37-test-release-tool.log`（61 passed） |
| `pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1 -EvidenceDir …/script-regression` | 0 | `gates/36-script-regression.log`、`script-regression/script-regression-log.md`（14 条全 PASS） |
| `cargo fmt --all -- --check` | 0 | `gates/30-fmt.log` |
| `cargo check --workspace --all-targets --all-features` | 0 | `gates/31-check.log` |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | `gates/32-clippy.log` |
| `cargo test --workspace --all-features`（默认构建目录） | **101，环境阻塞** | `gates/33-test-workspace-attempt-file-lock.log`（见 §7 第一行） |
| 同上，改用独立构建目录 `--target-dir target/gate-target` | 0 | `gates/38-test-workspace-isolated-target.log`（**864 passed / 0 failed / 70 ignored**，191 suites） |
| `pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | `gates/34-check-architecture.log` |
| `git diff --check` | 0 | `gates/35-git-diff-check.log` |
| pilot（候选固定后） `scripts/plan0014-wave5a.ps1 … -SkipOutsideConsumer` | 0 | `pilot/assembly-log.md` + `pilot/negatives/*/case.md`（19 例全 OK） |

架构门禁本轮新增的可执行约束（`scripts/check-architecture.ps1`）：`git.rs` 必须出现
`ls-tree` / `"-r"` / `"-z"` 且**不得**出现 `show`；`src/commit.rs` 必须存在并包含
`120000` 与 `160000`（符号链接与 gitlink 的拒绝分支）。

## 7. 未运行 / 被阻塞（逐项，不给总验收）

| 项 | 状态 | 原因 |
|---|---|---|
| `cargo test --workspace --all-features`（默认 `target/debug`） | **BLOCKED（环境）** | `target/debug/business-api.exe` 与 `business-worker.exe` 被本会话之外的进程占用（`拒绝访问 (os error 5)`，4 次重试同样结果；进程枚举在本会话内看不到该持有者），cargo 无法重链接这两个 bin，构建阶段即失败：`gates/33-test-workspace-attempt-1-file-lock.log`（第一次）与
`gates/33-test-workspace-attempt-file-lock.log`（最后一轮，逐字相同的失败）。同一代码改用独立构建目录跑完整：`gates/38-test-workspace-isolated-target.log` **exit 0，864 passed / 0 failed / 70 ignored，191 suites**（Repair 1 为 855 / 190，增量正是本轮新增的 9 例）。 |
| 仓库外目录消费者复建 lane | **NOT RUN** | 本轮按审阅指令使用 `-SkipOutsideConsumer`；Repair 1 的仓库外 lane 证据单独留在 `docs/reviews/2026-09-29-wave5a-repair1-gates/3*-outside-*`，不与本轮混计。 |
| PostgreSQL 迁移执行、单模块独立安装 / 升级 /  rollback | **BLOCKED** | 无数据库授权；identity 与 policy 与 organization 共享 `019_identity_authorization_foundation.sql`、同 `public` schema、被 `runtime-migration` 全量 embed。 |
| 70 个 `#[ignore]` 测试 | **NOT RUN** | 在候选上重新生成清单并与 Repair 1 逐项比对：新增 0、消失 0、类别漂移 0；分母 PostgreSQL 38 / PostgreSQL 且对象存储 3 / 仅 S3/MinIO 对象存储 28 / 内网模型 provider 1（`ignored-tests-classified.md`、`ignored-tests-inventory.json`）。一条也未执行，不计入通过。 |
| publish / 注册 / install / upgrade / rollback | **NOT RUN** | 本轮范围禁止，也未发生。 |
| push / merge / tag / deploy / 改 main | **NOT RUN** | 本轮范围禁止，未执行。 |
| 运行时动态加载 / 签名 | **不适用** | 工具不生成签名，工件不加载代码；`identity` / `policy` 仍是普通路径依赖 crate。 |
| 漏洞 / 许可证 / Secret / 镜像扫描 | **NOT RUN** | 无授权与凭据。 |
| CI | **NOT RUN** | 本轮为本地执行。 |
| Wave 5B、PLAN-0006、G-01 / G-02 / G-201 / G-202 / G-203 | **未启动 / 未关闭** | 按 §5.6 停在 Repair 2。 |
| ADR-0027 / ADR-0028 | 仍 Proposed / NOT ACCEPTED | 未修改。 |
| 独立审阅 | `<pending>` | 实现者不自行填写 PASS / accepted。 |
