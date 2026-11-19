# PLAN-0014 Wave 4 Round 3 — 门禁与确定性证据（清理分页判定 / 证据判定边界）

起点 HEAD `7fb9f735a7e0a787666d7b62592f08ac5161245a`，分支
`codex/plan-0014-wave4-reference-conformance`。本轮改动全部在测试侧：
`crates/object-storage/tests/support/mod.rs`、`tests/s3_lane_fault_injection.rs`、
新增 `tests/s3_lane_pagination.rs`；`tests/s3_lane_cleanup.rs` 仅改注释归属。

## 1. 失败先行：修复前的回归（`attempts/00-red-pagination-before-fix.log`）

在**只加入测试、未改分页判定**的状态下运行
`cargo test -p object-storage --all-features --test s3_lane_pagination`：

```
test result: FAILED. 12 passed; 5 failed
```

失败的 5 例正是三个缺陷的方向，不是环境噪声：

| 失败用例 | 修复前的实际行为 |
|---|---|
| `seven_uploads_on_one_key_are_all_listed_across_three_pages` | 第二页 key marker 未变 → 报「游标未推进」，同 key 多 upload 无法翻页 |
| `a_truncated_upload_page_without_a_cursor_is_an_error` | 回退到本页最后一条的 marker 继续猜，或直接跑偏 |
| `objects_truncated_without_a_token_is_an_error_not_a_full_listing` | `IsTruncated=true` 且无 token 时**返回 Ok**（残缺列表被当作完整） |
| `an_objects_page_that_does_not_answer_is_truncated_is_an_error` | `IsTruncated` 缺省按 `false` 处理 → 静默视为最后一页 |
| `an_upload_listing_that_never_terminates_fails_on_the_page_bound` | 页上限检查未生效（先撞上游耗尽） |

`10-green-pagination-after-fix.log`：修复后同一命令 `17 passed; 0 failed`。
后续加入第 18、19 例（见 §3），当前该 target **19 passed / 0 failed**。

## 2. 这些测试驱动的是真函数，不是副本

`tests/support/mod.rs` 的 `ScriptedProvider` 用
`aws_smithy_runtime_api::client::http::http_client_fn` 替换 SDK 的 HTTPS connector：
测试给出确定性的 S3 XML 响应，被测对象仍是
`list_all_incomplete_uploads_with_pages` / `list_all_keys_with_pages`
（即清理步骤实际调用的同一实现），断言同时检查**实际发出的线参数**
`key-marker` / `upload-id-marker` / `continuation-token`。
不建 socket、不读主机根证书库、不依赖外部 provider。

判定函数 `next_upload_cursor` / `next_object_token` 是从列表循环里抽出的**纯函数**，
列表循环调用它们——不存在「测试里另抄一份算法」。

## 3. 用例覆盖映射（`s3_lane_pagination`，19 例，全部非 `#[ignore]`）

| 要求 | 用例 |
|---|---|
| 同一 key 7 个未完成 upload / page_size=3 / 前两页 truncated 且同 key 不同 upload-id | `seven_uploads_on_one_key_are_all_listed_across_three_pages`（并断言第 2 页 marker 为 `upload-002`、第 3 页为 `upload-005`，key marker 始终不变） |
| 多 key 跨页且不重不漏 | `uploads_spread_over_several_keys_are_listed_exactly_once` |
| 重复游标对 → 明确错误 | `a_repeated_cursor_pair_is_an_error_not_a_complete_listing` |
| 循环游标 → 明确错误 | `a_cycling_cursor_pair_is_an_error_not_a_complete_listing` |
| 缺必要游标（全无 / 只给一半）→ 明确错误 | `a_truncated_upload_page_without_a_cursor_is_an_error`、`a_truncated_upload_page_with_only_half_a_cursor_is_an_error` |
| 页数上限 → 明确错误 | `an_upload_listing_that_never_terminates_fails_on_the_page_bound`（`MAX_LIST_PAGES`） |
| 正常最后一页（`IsTruncated=false`）结束 | `an_objects_listing_ends_on_is_truncated_false_after_every_page` |
| objects truncated 但缺 token / 空 token | `objects_truncated_without_a_token_is_an_error_not_a_full_listing`、`objects_truncated_with_an_empty_token_is_an_error` |
| token 重复 / 循环 | `objects_with_a_repeated_or_cycling_token_are_an_error` |
| 未回答 `IsTruncated` | `an_objects_page_that_does_not_answer_is_truncated_is_an_error` |
| provider 返回空列表不得算「发现」 | `a_provider_that_lists_nothing_does_not_verify_upload_discovery` |
| 列出的是别的 upload 不算发现 | `a_different_upload_in_the_listing_is_not_discovery_of_the_created_one` |
| 配对必须两半都匹配 | `the_pair_is_matched_on_both_halves` |
| 子进程无结构化上报不能产出 pass | `a_child_that_reported_nothing_cannot_produce_a_pass` |
| 列表失败绝不报告为干净范围 | `a_failed_listing_is_never_reported_as_a_clean_scope`（含正向对照 `report_and_fail_control_passes`，证明失败来自判定而非脚手架） |
| 结构化记录可解析、损坏记录上报而非丢弃 | `created_upload_records_are_structured_and_garbage_is_reported` |
| 与 libtest 同行输出仍解析（真实 lane 抓出的缺陷） | `a_record_sharing_a_line_with_the_harness_still_parses` |

分页错误进入 `CleanupReport.errors` 并置 `listing_incomplete=true`，`ok()` 拒绝，
最终清理步骤非零退出——不把未完成的列举当作「零残留」证据。

## 4. 门禁结果

| 门禁 | 命令 | 退出码 | 日志 |
|---|---|---|---|
| 分页/判定测试（修复后） | `cargo test -p object-storage --all-features --test s3_lane_pagination` | 0 | `10-green-pagination-after-fix.log` |
| object-storage | `cargo test -p object-storage --all-features` | **0** | `31-test-object-storage.log`：17 lib + 1 `log_hygiene` + 19 `s3_lane_pagination` + 4 `sdk_log_hygiene`；lane 目标保持 ignored（10/6/7/5） |
| observability | `cargo test -p observability --all-features` | **0** | `30-test-observability.log`：12 + 12 |
| fmt | `cargo fmt --all -- --check` | **0** | `20-fmt-check.log` |
| check | `cargo check --workspace --all-targets --all-features` | **0** | `21-check-workspace.log` |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** | `22-clippy-workspace.log` |
| test | `cargo test --workspace --all-features` | **0** | `23-test-workspace.log`：**803 passed / 0 failed / 70 ignored**，180 suites |
| 架构门禁 | `pwsh -NoProfile -File scripts/check-architecture.ps1` | **0** | `24-check-architecture.log`：Cargo metadata / OpenAPI / Architecture fitness 全 PASS |
| 空白 | `git diff --check` | **0** | `25-git-diff-check.log`（仅 `operational.rs` 的既有 EOL 提示，见 §6） |

Round 2 为 784 passed / 69 ignored / 179 suites；本轮 +19 用例 +1 suite，无删除、无弱化。

## 5. SDK 日志集成的证书环境限制（复测结论）

见 `40-truststore-environment-probe.log`。要点：受限文件访问下
`log_hygiene` / `sdk_log_hygiene` 会以
`TrustStore configured to enable native roots but no valid root certificates parsed!`
中止，根因是 `failed to open current user certificate store`（OS error 5 拒绝访问）；
把本轮改动 stash 掉（＝HEAD 代码）复现相同 panic，可排除本轮引入；
在可访问证书库的会话中同两条命令 **1 passed / 4 passed，exit 0**。
**未关闭 TLS 校验**，未安装自定义 verifier，未改生产 `S3Client`；
lane 本身走回环明文 HTTP，与该限制无关。

## 6. 未执行 / 边界

* 真实 provider 四步 lane 结果在
  [`../2026-09-28-wave4-round3-storage-lane/`](../2026-09-28-wave4-round3-storage-lane/)，
  本目录只含确定性与门禁证据。
* `tests/minio_contract.rs` 仍 **BLOCKED / NOT RUN**（MinIO 停发）；未删除、未弱化。
* CI 上的 lane 两步需 PR 触发，本轮 **NOT RUN**；CI 配置未改。
* provider 的 `UploadPart` 仍返回 `service error`：属 provider 观察，本轮未修，
  未因此新增生产存储能力，也未把 multipart 完成链路写成已验证。
* `crates/document-sqlite/tests/operational.rs` 为进入本轮前已有的 EOL-only 脏改动：
  未 staged、未 reset/checkout/clean、未修改内容，按要求原样保留。
* 日志卫生：本目录与 lane 目录下的捕获日志只做过行尾符统一与逐行行尾空白/EOF 空行清理（与 Round 2 一致，`git diff --check` 干净），命令输出内容未作改动。
