# Wave 5A Repair 1 — F1…F5 逐项关闭表

代码候选：`69d9a6b01184db7d9f3d4aa3b688fb569387b2fc`（修复提交链见 `README.md` §1）。
审阅指令：`docs/reviews/2026-09-29-wave5a-independent-review.md` §4（原文照录，不扩大）。
审阅结论：**NEEDS REPAIR / NOT PUBLISHED**（Wave 5A），本轮由实现者修复并交付复审。

命令约定：下表 `命令` 列即评审 §4.3 的确切命令行（RTK 前略去 `rtk proxy` 不影响语义）；
`日志` 列是本目录内相对路径。`negatives/<case>/` 内每个负例都有 `case.md`（注入说明 +
注入实际改动的文件与其 sha256 前后值 + 两条命令与退出码）与该两步的原始日志。

## 0. 先失败回归（修复前的旧代码）

`red-run/reproduce-red-counterexamples.ps1` 把审阅反例重新打在**修复前**候选
`8036bc65ba7c77666e48afaa6439911ba7c083d7` 上（一次性 worktree，运行完即删），结果：

| finding | 反例 | 命令 | 日志 | 修复前实际结果 |
|---|---|---|---|---|
| F1 | 两份身份记录都把 `source_revision` 写成 40 个 0 | `module-release verify` | `red-run/03-f1-verify-zero-revision.log` | **exit 0，验证通过**（RED CONFIRMED） |
| F2 | 右侧副本 packed 源里加一行 | `module-release compare` | `red-run/04-f2-compare-damaged-side.log` | **exit 0，宣告两侧一致**（RED CONFIRMED） |
| F2 对照 | 同一份损坏改走 verify | `module-release verify` | `red-run/05-f2-verify-same-damage.log` | exit 1（缺陷只在 compare，不在 verify） |
| F3 | `artifact_integrity_algorithm` 改成没人实现的算法 | `module-release verify` | `red-run/06-f3-verify-unknown-algorithm.log` | **exit 0，验证通过**（RED CONFIRMED） |
| F3 | `artifact-manifest.json` 声称另一个 `module_version` | `module-release verify` | `red-run/07-f3-verify-record-version-claim.log` | **exit 0，验证通过**（RED CONFIRMED） |
| F3 | bundle 绑定 `crate_version = 9.9.9` 而 packed Cargo.toml 是 `0.1.0-rc.1` | `module-release verify` | `red-run/08-f3-verify-bundle-crate-binding.log` | **exit 0，验证通过**（RED CONFIRMED） |

F4 与 F5 的修复前行为另有两份不可改的既有证据：`red-run/pre-fix-orchestration-script.ps1`
（第 253-277 行的字符串扫描即 F4 的全部实现；第 289-343 行的四个负例共用一个 `$tampered`
目录，第 322/333 行是 `.Replace()` 字符串改 JSON，且只断言 `-ExpectExit 1`）；
以及旧证据目录 `docs/reviews/2026-09-29-wave5a-gates/` 里 `25-consumer-identity-path-guard.txt`
（`hits : 0 / result: PASS`）与 `52-tamper-wrong-version-identity.log`、
`53-tamper-wrong-module-id-identity.log`（两份日志的拒绝原因**逐字相同**，都是
`[bundle-identity]`：模块 ID 那条从没走到模块 ID 检查）。详见
`red-run/F4-F5-from-retained-history.md`。

过程说明（不等同证据）：修复过程中先在新工作树里跑过一次新回归，`verification_guards`
当时 1 passed / 7 failed（`exit=101`），F1 双零 revision 当时 `status=verified`；那次是
会话内运行，未建证据目录、未保留完整控制台输出。上表是**可复现**版本的红证据。

## 1. F1 — 钉住的源提交必须真的是提交，且真的包含被打进工件的输入

| 项 | 内容 |
|---|---|
| 缺陷 | `source_revision` 只当字符串写进记录，从不解析；`generate` 也从不确认提交里有它声称打包的输入 |
| 修复位置 | 新增 `tools/module-release-tool/src/git.rs`（工具内唯一触达子进程的模块：只跑 `git`，参数数组，`--no-optional-locks`，`cat-file` 读 blob）；新增 `src/provenance.rs`（`source-revision-exists` / `source-inputs-match-revision` / `toolchain-declaration`）；`src/packaging.rs` 的 `generate` 在写第一个字节之前先比对输入，`verify` 用提交内容而非工作树内容判定；`src/repo.rs` 删除手写 `.git`/`HEAD`/`refs`/`packed-refs` 解析 |
| 回归名称 | `tests/provenance.rs`：`a_pinned_revision_must_be_a_real_commit_object`、`an_input_modified_but_never_committed_is_refused`、`an_input_the_pinned_commit_does_not_contain_is_refused`、`committed_inputs_match_and_the_check_reports_what_it_compared`、`a_detached_head_and_a_linked_worktree_mean_the_same_commit_as_git`、`a_blob_is_committed_content_not_working_tree_content`（6）；`tests/verification_guards.rs`：`a_pinned_source_revision_that_is_not_a_real_commit_is_refused`、`the_pinned_revision_must_actually_contain_the_inputs_it_claims`；脚本负例 `n09-zero-revision`、`n11-stale-revision` |
| 命令 | `cargo test -p module-release-tool --all-features`；`pwsh -NoProfile -File scripts/plan0014-wave5a.ps1 -OutRoot target/wave5a-repair1 -EvidenceDir docs/reviews/2026-09-29-wave5a-repair1-gates` |
| 日志 | `gates/01-test-module-release-tool.log`；`negatives/n09-zero-revision/`、`negatives/n11-stale-revision/`；`artifacts/identity/verification.json` 的 `source-revision-exists` / `source-inputs-match-revision` 两行 |
| 实际结果 | 40 个 0 → `[source-revision-exists]` 拒绝；真实但更早的提交 `e2ce866…` 的父提交 → `[source-inputs-match-revision]`，原因点名 `tools/module-release-tool/specs/identity.` 与 "does not contain"；正向候选记 `17 payload member(s) and 9 feeding file(s) compared with commit '69d9a6b0…'` |

## 2. F2 — compare 必须真的读两侧 payload

| 项 | 内容 |
|---|---|
| 缺陷 | `compare` 只比两份 manifest 记录里的摘要字符串，payload 被改也宣告一致 |
| 修复位置 | `src/packaging.rs::check_side`（每侧先重跑清单 + 逐成员重算摘要，失败即 `compare-payload-integrity` 并指明 left/right）；`compare` 比的是**重算后**的摘要与清单，且 `Comparison::scope` 明确写出 `verification.json` 与 `source_revision` 声明不在其比较范围内 |
| 回归名称 | `tests/verification_guards.rs`：`compare_reads_the_real_payload_of_the_right_side`、`compare_reads_the_real_payload_of_the_left_side`、`two_independent_candidates_still_compare_identical`；脚本负例 `n12-compare-right-damaged` |
| 命令 | 同上两条 |
| 日志 | `negatives/n12-compare-right-damaged/`；`assembly-log.md` 的 `n12 : both sides must be identical before the damage` 行（exit 0）与损坏后那行（exit 1） |
| 实际结果 | 注入前两侧确实 `identical`（exit 0）；右侧 `lib.rs` 追加一行后 → `[compare-payload-integrity]`；左侧删 `CHANGELOG.md` 同拒绝（Rust 侧用例） |

## 3. F3 — 描述性记录必须互相自洽，且与打包内容自洽

| 项 | 内容 |
|---|---|
| 缺陷 | 无人交叉核对 `artifact-manifest.json` / `release.json` / `release-bundle.json` / 载荷内 `module.manifest.json`；未知 schema/algorithm 照算摘要；bundle 的 crate 版本绑定不与 packed `Cargo.toml` 对照 |
| 修复位置 | `src/packaging.rs::validate_artifact_record`（`artifact-record-format`）、`::assert_records_agree`（`record-consistency`）、`::assert_packed_bindings`（`packed-crate-bindings`，逐成员读 payload 内 `contract/packages/<name>/Cargo.toml` 的 `[package]`） |
| 回归名称 | `tests/verification_guards.rs`：`an_unknown_artifact_record_schema_or_algorithm_is_refused`、`the_descriptive_records_must_describe_one_module_and_version`、`the_bundle_crate_bindings_must_match_the_packed_cargo_manifests`；脚本负例 `n03`…`n08` |
| 命令 | 同上两条 |
| 日志 | `negatives/n03-unknown-algorithm/`、`n04-unknown-schema/`、`n05-record-module-id/`、`n06-record-package-digest/`、`n07-bundle-crate-version/`、`n08-bundle-crate-name/` |
| 实际结果 | 未知 algorithm/schema → `[artifact-record-format]`；`module_id` / `package_digest` 与身份不一致 → `[record-consistency]`；bundle 版本 9.9.9 与改名 → `[packed-crate-bindings]`；正向 `record-consistency` 明细同时列出 module_id / version / 两个摘要 |

## 4. F4 — 隔离由完整解析图判定

| 项 | 内容 |
|---|---|
| 缺陷 | 脚本用 `-like "*<repo>\crates*"` 扫消费者 `Cargo.toml` / `Cargo.lock` 文本；反向斜杠、相对路径、传递性 path 依赖一概看不见 |
| 修复位置 | 新增 `src/isolation.rs`（解析 `cargo metadata` 的完整包图，`source == null` 的每个包都要落在消费者目录或声明的工件目录内；绝对/相对、正/反斜杠、`\\?\` verbatim、盘符大小写、路径边界而非字符串前缀、无法解析即拒绝、`workspace_root` 本身也要判定）；`src/main.rs` 新增 `guard` 子命令；脚本只负责跑 `cargo metadata --format-version 1 --locked --offline` 并把文档交给 guard，自身不含任何路径规则 |
| 回归名称 | `tests/isolation.rs` 8 例：`a_consumer_that_resolves_only_into_itself_and_the_artifact_passes`、`a_path_dependency_into_the_source_repository_is_refused`、`a_relative_path_escaping_into_the_repository_is_refused`、`a_transitive_path_dependency_into_the_repository_is_refused`、`a_local_package_outside_every_declared_root_is_refused`、`containment_is_decided_on_a_path_boundary_not_a_string_prefix`、`a_path_that_cannot_be_resolved_is_refused_rather_than_assumed_safe`、`a_consumer_building_inside_another_workspace_is_refused`；脚本正向 3 次 guard + 负例 `g01`…`g03` |
| 命令 | 同上两条 + `pwsh -NoProfile -File scripts/check-architecture.ps1`（新增规则：编排脚本必须用 `cargo metadata` + `guard`，出现自有路径匹配即失败） |
| 日志 | `25-consumer-identity-cargo-metadata.json` / `26-consumer-identity-guard.log`、policy 同名两份、`34-outside-consumer-identity-cargo-metadata.json` / `33-outside-consumer-identity-guard.log`、`negatives/g01-absolute-into-repo/`、`g02-relative-into-repo/`、`g03-transitive-into-repo/`、`gates/07-check-architecture.log` |
| 实际结果 | 两个仓外消费者 + 一个仓库外目录消费者的完整图判为 isolated（`check[consumer-graph-isolation] PASS`，逐包 `verdict=…`）；三种泄漏（正斜杠绝对路径、`..\..\..\..\..\crates\identity\Cargo.toml` 相对泄漏、消费者从未声明的传递性本地依赖）全部 `[consumer-graph-isolation]` 拒绝 |

## 5. F5 — 负例必须各用干净副本，且必须因被断言的原因失败

| 项 | 内容 |
|---|---|
| 缺陷 | 四个负例共用一个脏副本（第 4 例继承第 3 例的损坏）；JSON 用 `.Replace()` 字符串替换；只看非零退出，不看拒绝原因 |
| 修复位置 | `scripts/lib/plan0014-wave5a-lib.ps1::Invoke-NegativeCase`（每例从**刚 verify 通过**的候选复制一份 → 注入 → `Get-TreeHashes`/`Compare-InjectedTrees` 证明改了哪些字节 → 断言退出码**与**检查 ID → 写 `case.md`）；`Set-JsonRecord`（解析-改-序列化，且回读后与原文相同即抛错）；`Invoke-GuardNegativeCase`；脚本级：依赖步骤 `-Blocking` 失败即停、run/evidence 目录已存在即拒绝、`Remove-OwnScratch` 只能删本次 scratch；架构门禁把「脚本不得自有路径规则」变成长期规则 |
| 回归名称 | `scripts/test-plan0014-wave5a.ps1` 的 9 条（含三条**自测**：期望错检查 ID 必须判 `WRONG REASON`；注入没改字节必须抛错；干净副本基线 verify 不通过必须判 `BROKEN`） |
| 命令 | `pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1 -EvidenceDir docs/reviews/2026-09-29-wave5a-repair1-gates/script-regression`；编排命令同上 |
| 日志 | `script-regression/script-regression-log.md`（9 条全 PASS）；`negatives/*/case.md` 共 15 份 + 27 份原始日志；`assembly-log.md` 的 negative 表 |
| 实际结果 | 15 个负例全部 OK（期望检查 ID == 实际检查 ID，无一例 `WRONG REASON` / `NOT REFUSED` / `INJECTION DID NOT LAND`）；自测证明这三类误判会被脚本抓到并让运行失败 |

## 6. 现在跑的检查（顺序即 execution 顺序）

`artifact-record-format` → `artifact-integrity-digest` → `contract-surface-file-digest` →
`release-identity-consistency` → `record-consistency` → `packed-crate-bindings` →
`migration-mapping-matches-formal-catalog` → `source-revision-exists` →
`source-inputs-match-revision` → `toolchain-declaration`。

只在失败时出现、不进入通过索引的内部 id：`artifact-inventory`、`bundle-identity`、
`compare-payload-integrity`、`consumer-graph-isolation`、`repeat-build-consistency`、
`compiled-package-present`、`migration-set-digest`。

`toolchain-declaration` 的明细自己写明是 **DECLARATION CHECK ONLY**：它比对身份声明与
该提交里的 `rust-toolchain.toml`，不证明产出这些字节的进程用的是哪个工具链——后者由
`00-toolchain.txt` 里本次运行的 `rustc -vV` 作为外部证据，两者不互相冒充。
