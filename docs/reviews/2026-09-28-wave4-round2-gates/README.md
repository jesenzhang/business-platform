# PLAN-0014 Wave 4 Round 2 — 日志安全边界复关闭与门禁证据

本目录是 Wave 4 **Round 2** 的门禁与修复证据，配合
[`../2026-09-28-wave4-round2-storage-lane/`](../2026-09-28-wave4-round2-storage-lane/)
使用。Round 1 的 `../2026-09-28-wave4-gates/` 与 `../2026-09-28-wave4-storage-lane/`
保持原样（已加 Round 2 更正指针），本目录不覆盖其中的任何中间记录。

## 1. 本轮性质：安全修复复关闭，不是「provider 噪声优化」

Round 1 把 OBS-W4-02 记为 C2「provider 噪声压制 · 已修复」。复核结论不同：
provider 在 `DEBUG`/`TRACE` 输出的是**桶名、对象键、完整请求 URI 与凭证标识**，
违反 AGENTS §10，因此属安全面缺陷；而且 Round 1 的修复方式没有关闭它。

Round 1 的做法是在 `EnvFilter` 指令串尾部追加 `<target>=warn`。两条绕过路径：

1. `RUST_LOG` 一旦设置就**整体替换**指令串 → 追加的 `=warn` 消失；
2. `EnvFilter` 取最具体匹配 → `aws_sigv4::http_request::sign=trace` 优先于父目标的 `=warn`。

还有第三种泄漏面：fmt 会把**外层 span 的字段**当作前缀打印在每条嵌套记录前，
因此只过滤事件或只过滤落盘点都不够。RED 日志里的实例：

```
DEBUG signing{bucket="synth-bucket-plan0014" access_key_id="AKIASYNTHETICPLAN0014"}: plan0014_app: PLAN0014-APP-DEBUG inside provider spans
```

Round 2 的修复是独立于 `EnvFilter` 的边界层 `ProviderSafetyFilter`
（`crates/observability/src/lib.rs`）：

* `register_callsite` 对被压制目标返回 `Interest::never()`；
* `enabled`（span 面）与 `event_enabled`（事件面）**都**实现——`Layer::event_enabled`
  是独立 hook 且默认 `true`，只覆盖 `enabled` 会放过所有事件（这是本轮实测到的
  `tracing-subscriber` 语义，已在代码注释中固定）；
* `Layered` 对多层是合取且对 `never` 短路，因此任何 `EnvFilter` 都无法重开该 callsite；
* 结果：记录不被构造，fmt 与 OTLP span 两条出口同时生效；
* 目标判定按**模块边界**匹配（`aws_sdk_s3` 命中 `aws_sdk_s3::client`，不误伤 `aws_sdk_s3x`）；
* 级别只关 `DEBUG`/`TRACE`。`Level` 的 `Ord` 按严重度排序（`ERROR < … < DEBUG`），
  写成 `level <= Level::DEBUG` 会连带吞掉 `WARN`/`ERROR`——`only_debug_and_trace_are_within_the_boundary`
  专门钉住这一点。

应用自身的 `DEBUG` 保留，provider 的 `WARN`/`ERROR` 保留。**不存在**「设置 `RUST_LOG`
即可放开」的例外。

## 2. 修复前后（原始日志）

| 阶段 | 日志 | 结果 |
|---|---|---|
| 修复前 RED | `10-red-provider-log-safety-before-fix.log` | `5 passed; 7 failed`；输出中出现 76 行 `PLAN0014-PROVIDER-DEBUG` 泄漏（含 `access_key_id="AKIASYNTHETICPLAN0014"`、span 前缀泄漏） |
| 修复后 GREEN（observability） | `30-test-observability.log`、`11-green-observability-after-fix.log` | lib 12 passed + 子进程套件 12 passed；泄漏标记 0 行 |
| 修复后 GREEN（SDK 真实调用） | `13-sdk-log-hygiene-run.log`、`31-test-object-storage.log` | 4 passed；6 个隔离子进程，其中「无边界对照组」必须泄露 |
| 首版 harness 误报记录 | `12-object-storage-unit-and-sdk-log-tests.log` | 保留：负向断言把 harness 自己打印的 `presigned_url` 当成 `signed_url` 泄漏；同时该次运行暴露 `aws_smithy_http_client` 未纳入压制清单（其 `DEBUG new connector created in …` 实测出现），已补入 |

测试形态（要求「必须驱动真实 `init_tracing`」）：

* `crates/observability/tests/provider_log_safety.rs`：11 个用例，每个都在
  **独立子进程**中调用真实 `observability::init_tracing`（全局 subscriber 与
  `RUST_LOG` 无法在并行测试里安全共享）。覆盖 RUST_LOG 未设置 / `debug` / `trace` /
  更具体的 provider 子目标 `=trace` / span 指令 / 应用自身 DEBUG 仍在 /
  provider `WARN` 仍在 / text 与 json / OTLP 分支。
* `crates/object-storage/tests/sdk_log_hygiene.rs`：真实 `aws-sdk-s3` 调用经正式
  观测初始化，对桶名、对象键、access key id、secret、签名 URL 词汇
  （`X-Amz-Signature`/`X-Amz-Credential`/`authorization`/`access_key_id`/…）作
  **负向断言**；全部为合成值；失败信息只打印输出，不打印真实凭证（本仓无真实凭证）。
  对照组 `control-no-boundary-trace` 断言**必须**泄露，防止「什么都没测」也算通过。

## 3. 本轮明确没有解决的事

* **OBS-W4-07 仍然 open**：`WARN`/`ERROR` 自由文本仍可能携带桶/键，一般性脱敏
  （字段黑名单、值截断、审计口径统一）未实现。本轮只关闭已知 provider
  `DEBUG`/`TRACE` 泄漏路径，任何「日志脱敏已完成」的表述都是错的。
* 已登记的 R2 阻塞项与 Proposed ADR 状态**未改动**：OBS-W4-05（依赖 ADR-0028，
  仍为 Proposed/未接受）、OBS-W4-06（无 Collector，BLOCKED）、D-W4-02/06/08、
  D-W4-09、D-W4-11 等保持原状。
* 未扩展为通用脱敏平台、未实现 Notification、未做 Document ACL。

## 4. 执行环境限制复核（`TrustStore … no valid root certificates parsed!`）

见 `40-truststore-environment-probe.log`。结论：**受限执行环境差异，不是仓库缺陷**。

* 同一个二进制在受限文件访问下于 `presign_rejects_out_of_range_expiry_without_calling_the_provider`
  panic，在正常访问下通过；panic 来自 AWS SDK 默认 HTTPS connector 读取 OS 根证书库
  得到 0 个根之后的 `debug_assert!`。
* 处理方式：参数拒绝类测试改为安装**无网络 transport**（`src/client.rs` 的
  `tests::offline_client`，通过模块私有字段这一既有 seam，不新增生产构造器），
  因此根本不再触碰主机信任库；并新增「transport 确实被使用」的自检用例，
  使「未调用 provider」成为可观测事实（调用计数）。
* 生产 TLS 校验未做任何削弱：工作区内
  `danger_accept_invalid_certs` / `accept_invalid_certs` / `NoCertVerification` /
  `CertVerifier` / `ssl_verify` / `tls_verify=false` 命中数全部为 0（已写入证据日志）。
* 真实 SDK 日志集成测试与真实 provider lane 仍**实际执行**（本目录 + 存储 lane 目录），
  未以 fake 替代。

## 5. 门禁（本轮全部重跑）

原始汇总：`gate-summary.txt`；主机 Windows 10.0.19045 / rustc 1.94.1。

| 门禁 | 命令 | 退出码 | 记录 |
|---|---|---|---|
| fmt | `cargo fmt --all -- --check` | 0 | `20-fmt-check.log`（空） |
| check | `cargo check --workspace --all-targets --all-features` | 0 | `21-check-workspace.log` |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | `22-clippy-workspace.log` |
| workspace tests | `cargo test --workspace --all-features` | 0 | `23-test-workspace.log`：179 suites，**784 passed / 0 failed / 69 ignored** |
| observability | `cargo test -p observability --all-features` | 0 | `30-test-observability.log`：12 + 12 passed |
| object-storage | `cargo test -p object-storage --all-features` | 0 | `31-test-object-storage.log`：17 lib + 1 `log_hygiene` + 4 `sdk_log_hygiene`；真实 provider lane 用例保持 `ignored`（`minio_contract` 10、契约 6、清理 7、故障 4），并在真实 provider 上另行执行 |
| document | `cargo test -p document-sqlite -p document-persistence-contracts --all-features` | 0 | `24-test-document-sqlite-contracts.log` |
| 架构门禁 | `pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | `25-check-architecture.log`：Cargo metadata PASS · OpenAPI PASS · Architecture fitness PASS |
| diff | `git diff --check` | 0 | `26-git-diff-check.log`（仅 `core.autocrlf=true` 的 LF→CRLF 提示，无空白错误） |

真实 provider lane（契约 / 故障 / 清理 / 再次清理）四个步骤的退出码与残留计数在
[`../2026-09-28-wave4-round2-storage-lane/README.md`](../2026-09-28-wave4-round2-storage-lane/README.md)。

## 6. 结论分类口径

* **契约通过**：provider-neutral port 在 RustFS 上的 round trip / NotFound / delete 幂等 / presign 作用域与过期。
* **能力观察**：`CAPABILITY …` 行（multipart 完成失败、条件读可用、校验和回显、版本化未配置、`ListMultipartUploads` 可用）。不参与通过判定。
* **清理通过**：分页删除 + 分页 abort + 「0 对象且 0 未完成 upload」终态 + RUN_ID 守卫 + 清理失败单独失败 + 拒删受保护桶 + prefix 外对象不受影响。
  > **Round 3 校准（原文保留）**：此处「分页 abort」应读作「abort 可用」。Round 2 真实运行的
  > multipart abort 计数为 1，列举只有一页、没有游标，multipart **分页**路径未被执行；
  > 对象侧 6 对象 / 2 页的多页删除成立。同 key 多 upload 的多页验证见
  > [`../2026-09-28-wave4-round3-storage-lane/README.md`](../2026-09-28-wave4-round3-storage-lane/README.md) §4。
* **未执行 / BLOCKED**：`tests/minio_contract.rs`（MinIO 停发）；CI 上的 lane 两步（需 PR 触发）；OTLP exporter 隔离（OBS-W4-06）。

## 7. 本轮未做的事

* 未 rebase、未改 main、未 push、未 merge、未启动 Wave 5；
* 未为测试引入生产构造器或放宽 TLS；
* 未因 RustFS multipart 不完整而新增生产 multipart 功能；
* `crates/document-sqlite/tests/operational.rs` 的工作区差异仍只是换行符
  （`core.autocrlf=true`），内容未改，blob 保持 `8dc7c7ac49e08ef8eec8ea79a53a352aec48b5c9`。

## 8. Binding

| 项 | 值 |
|---|---|
| 基线 HEAD（Round 1 终态） | `6335ff39cacd43d8ab241b2bda5b6ae49226619f` |
| **代码候选 commit** | `8be1d1b152bbc726fc1cb3002e3f462d2ebad328`（仅代码 + CI + `Cargo.lock`，不含本目录） |
| 文档收尾 commit | 即包含本文件的那次提交（在代码候选之后，证据绑定上面的显式候选） |
| 门禁运行时间 | 2026-09-28T07:02:58Z → 07:06:27Z（`gate-summary.txt`） |
| provider lane 运行 | RUN_ID `r2-20260928-065142`，见 `../2026-09-28-wave4-round2-storage-lane/run-summary.txt` |
| 分支 | `codex/plan-0014-wave4-reference-conformance`，未 push、未 merge、未改 main |
| 换行符 | 本目录日志由 PowerShell 重定向产生 CRLF，入库前统一为 LF（与 Round 1 日志一致）；只改行结束符与行尾空白，日志文本内容未改动 |
| 终态 | READY FOR INDEPENDENT REVIEW / NOT INTEGRATED |
