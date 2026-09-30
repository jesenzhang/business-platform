# Repair 2 — RED：修复前失败的 F1-A / F1-B 回归

- 代码基线：Repair 1 候选 `77a4fbd2ae89b5cec8ac2e6f482eba8722295387`（`tools/module-release-tool/src` 未改动）。
- 新增回归文件：`tools/module-release-tool/tests/source_derivation.rs`。
- 命令与退出码见本目录两份日志；`10` 为整体运行（`test result: FAILED. 0 passed; 9 failed`，exit 101），
  `11` 为修正测试自身 `git add --quiet` 参数错误后的单例复跑（该例在修复前即应通过，属兼容性护栏）。
- 每个用例都先验证候选可 `verify`，注入一处损坏，再用工具自身公开的
  `plan-0014.artifact-integrity.v1` 算法重算 inventory 与 integrity digest，
  并断言物理摘要仍然自洽——保证失败原因只能来自“来源派生”，不能来自摘要。

| # | 用例 | 注入 | Repair 2 期望 | Repair 1 实际观察 | 结论 |
| --- | --- | --- | --- | --- | --- |
| 1 | `a_packed_manifest_the_pinned_commit_cannot_produce_is_refused` | 打包 `Cargo.toml` 依赖版本 `=0.1.92`→`=99.0.0`（另含 features、path 两类） | `derived-members-match-revision` 拒绝 | `verify` 全部 10 项 PASS，重算后 integrity `03cae52234b9df82016d787feafbdfaee187f70905b85612f7a7dae5a2f9217f` | 未拒绝（= 评审 §3 同一摘要） |
| 2 | `a_packed_dependency_bump_is_refused_for_every_pilot_module` | 程序化改写第一个钉住版本 `<v>`→`99.0.0` | 同上，且对 identity/policy 通用 | identity 通过，integrity `88f5c3bcf1e2f034d2e932caea4ec31c01ba18e7acb2b2ff1d4ea5fc97bff1fc` | 未拒绝 |
| 3 | `a_manifest_compiled_from_a_different_spec_is_refused_even_when_self_consistent` | 用改动后的 spec 重新编译 `module.manifest.json`+surface，并同步 release/bundle/record 摘要 | 同上 | 通过；digest 变为 `fa663a02…`/`a4d3f789…`，与提交产生的 `3fed976b…`/`b9dfa42c…` 完全不同 | 未拒绝（自洽即被当作已证明） |
| 4 | `generating_from_a_checkout_that_dropped_a_committed_source_file_is_refused` | 隔离检出删除 tracked `crates/identity/src/testing.rs` | `generate` 以 `source-member-set-matches-revision` 拒绝且不产出目录 | `generate` 成功，`member_count: 20`，integrity `350da720a25c56f3e294a58da3d9046a91bae5143493df24936bff0d78c92dc9` | 未拒绝（= 评审 §4 同一摘要） |
| 5 | `generating_from_a_checkout_with_an_uncommitted_source_file_is_refused` | 隔离检出新增未跟踪 `crates/identity/src/review_untracked.rs` | 同一成员集合标识拒绝 | 已拒绝，但标识是 `source-inputs-match-revision`（走 blob 缺失路径） | 原因不符 |
| 6 | `a_candidate_missing_a_committed_source_member_is_refused_after_rehashing` | 完整候选删除成员 + 重算 inventory/integrity | 成员集合标识拒绝 | 已拒绝，标识 `source-inputs-match-revision`（期望集合取自磁盘） | 原因不符 |
| 7 | `a_candidate_with_an_extra_source_member_is_refused_after_rehashing` | 完整候选新增成员 + 重算 | 成员集合标识拒绝 | 已拒绝，标识 `source-inputs-match-revision` | 原因不符 |
| 8 | `a_pinned_candidate_still_verifies_after_the_working_tree_inputs_move` | 隔离检出修改 `crates/identity/Cargo.toml`、增删源码 | 固定候选仍须按提交验证通过 | 被拒绝：`source-inputs-match-revision`（验证绑定了当前输入） | 过度绑定 |
| 9 | `a_candidate_whose_commit_object_is_absent_is_refused_without_a_disk_fallback` | 另一仓库（自有提交）+ 同样磁盘文件 | `source-revision-exists` 拒绝，不回退磁盘 | `11` 日志：PASS | 护栏（修复前已成立） |

1–4 是评审 §5.2 要求“必须先失败”的核心反例；5–7 说明拒绝标识与磁盘集合来源不符；8 说明验证仍绑定当前输入；
9 是 §5.5 要求的缺失提交对象护栏，修复前已成立，保留以防止修复引入磁盘回退。