# PLAN-0014 Wave 4 Round 2 — S3 provider lane：契约执行与最终清理拆分证据

本目录是 Wave 4 Round 2 对**真实 S3-compatible provider lane** 的清理闭环证据。
它不替换 Round 1 的 `../2026-09-28-wave4-storage-lane/`，只补充 Round 2 把 lane 拆成
「契约执行」与「最终清理」两步之后的执行记录。中间失败记录一律保留，不覆盖。

## 1. 本轮要关闭的缺陷

Round 1 的 lane 把契约用例和清理放在同一个测试目标里，产生三个可验证的问题：

1. `lane_leaves_no_objects_under_the_run_prefix` 只调用一次 `ListObjectsV2`。
   单次调用最多返回 1000 个键，超过一页时它只删第一页，然后仍然报告「prefix 为空」；
   未完成的 multipart upload 对 `ListObjectsV2` 完全不可见，Round 1 从未查看。
2. 清理与契约共享退出码：清理失败要么不可见，要么被算成契约失败；
   清理成功也无法区分「契约本身没跑完」。
3. `lane_deletes_the_dedicated_bucket_when_requested` 会在 `WAVE4_S3_DELETE_BUCKET=true`
   时删除 `WAVE4_S3_BUCKET`，而 CI 用的是跨 job 复用的 `contract-test-bucket`。

Round 2 的拆分：

| 步骤 | 测试目标 | CI 条件 |
|---|---|---|
| 契约执行 | `crates/object-storage/tests/s3_contract_lane.rs` | 正常 |
| 故障场景 | `crates/object-storage/tests/s3_lane_fault_injection.rs` | `if: always()` |
| 最终清理 | `crates/object-storage/tests/s3_lane_cleanup.rs` | `if: always()` |

三步共享 job 级 `WAVE4_S3_RUN_ID`（`.github/workflows/ci.yml`）：清理范围由 RUN_ID
推导，两步使用不同 RUN_ID 会让清理去清一个从未写入过的 prefix，把真正的残留留在桶里。

## 2. Provider 身份与专用范围

| 项 | 值 |
|---|---|
| 实现 | RustFS `1.0.1-preview.11`（build 2026-09-24 10:16:46 +00:00, release） |
| 归档 | `rustfs-windows-x86_64-v1.0.1-preview.11.zip`，105,742,053 bytes |
| 归档 SHA-256 | `cae2628159a8759118d8a68cd043c389a05ee9a6a13ceb9e188e9414578453d5` |
| 校验 | 与上游 release `SHA256SUMS` 一致，且与 Wave 4 Round 1 记录的摘要一致（同一构建） |
| 启动 | `rustfs.exe server <volume> --address 127.0.0.1:19100`（凭据经环境变量注入，仅回环监听，不提交） |
| 健康探测 | `GET /minio/health/live` → 200 |
| 专用桶 | `plan0014-wave4-bucket` |
| 专用范围 | `plan0014-wave4/r2-20260928-065142/` |
| 列举页大小 | `WAVE4_S3_CLEANUP_PAGE_SIZE=3`（故意小于对象数，确保走分页分支） |
| RUN_ID | `r2-20260928-065142` |

`page_size=3` 是本轮的关键设置：默认 1000 会让分页路径在真实运行中永远不被执行，
Round 1 的缺陷正是这样被掩盖的。

## 3. 步骤退出码与结果

见 `run-summary.txt`（原始汇总）与逐步日志。

| 步骤 | 目标 | 退出码 | 用例 | 日志 |
|---|---|---|---|---|
| 2 契约执行 | `s3_contract_lane` | **0** | 6 passed / 0 failed | `01-contract-step.log` |
| 3 故障场景 | `s3_lane_fault_injection` | **0** | 4 passed / 0 failed | `02-cleanup-fault-scenarios.log` |
| 4 最终清理 | `s3_lane_cleanup` | **0** | 7 passed / 0 failed | `03-final-cleanup.log` |
| 4b 再次清理 | `s3_lane_cleanup` | **0** | 7 passed / 0 failed | `04-final-cleanup-again.log` |

## 4. 结论分类（不得混读）

四类结论含义不同，本目录逐条归类；任何一行都不得当成另一类使用。

### 4.1 契约通过（provider-neutral port 在真实 provider 上的行为）

| 断言 | 结果 |
|---|---|
| 流式写 + head 元数据 + caller metadata + ETag + exists + 全量读 + delete | pass |
| 缺失对象 → `StorageError::NotFound`，`exists` = false | pass |
| delete 幂等（对不存在的对象重复删除） | pass |
| presigned URL 键作用域（改写路径 → 403/404）与过期（1s 后失效） | pass |

### 4.2 能力观察（provider 行为记录，不是本仓断言，也不构成通过）

RustFS 1.0.1-preview.11 在本次运行中的答复（`01-contract-step.log` 的 `CAPABILITY` 行）：

| 能力 | 观察 |
|---|---|
| `CreateMultipartUpload` / `AbortMultipartUpload` | 可用（abort=true） |
| 合法 5 MiB `UploadPart` | 失败：`service error`（provider 侧 InternalError） |
| `CompleteMultipartUpload` | 未执行（上一步失败） |
| `ListMultipartUploads` | **可用**：故障场景 (b) 中确实列出了未完成的 upload |
| 条件读 `If-Match` | 可用；过期 ETag 被拒绝 |
| 对象校验和 | SDK 计算的 CRC32 被接受并回显；`HeadObject` 返回校验和 |
| 版本化 | 报告 `unconfigured` |

这些行的作用是记录 provider 差异，禁止把单一 provider 的能力推广为通用能力。
它们不参与「lane 通过」的判定。

### 4.3 清理通过（本轮新增的判定）

> **Round 3 校准（原文保留，不追改）**：下表第 2 行「分页列举未完成 multipart
> upload 并 abort」的 `uploads_aborted=1` 是**一个 upload、一页、没有游标**的场景，
> 因此它证明的是 abort 可用，**不是 multipart 分页已验证**。Round 2 真实运行里
> `ListMultipartUploads` 只有一页可走，分页分支根本没被执行。同 key 多 upload 的
> 多页验证在 Round 3 才成立（7 upload / page_size 3 / 3 页，见
> [`../2026-09-28-wave4-round3-storage-lane/`](../2026-09-28-wave4-round3-storage-lane/) §4）。
> 第 1 行的对象多页清理不受此校准影响：6 对象 / page_size 3 → 2 页，确为多页。
> 另：本表「provider 列出该 upload」当时的判定是**结果非空**，而非「列出了自己创建的
> 那一个」；Round 3 已改为精确配对判定（`PLAN0014-CREATED-UPLOAD` 结构化回传）。

| 断言 | 结果 | 记录 |
|---|---|---|
| 分页列举并删除本 RUN_ID prefix 下对象 | pass | `objects_deleted=6`，`remaining_objects=0` |
| 分页列举未完成 multipart upload 并 abort | pass | `uploads_aborted=1`，`remaining_uploads=0` |
| 最终断言：对象为 0 **且** 未完成 upload 为 0 | pass | 两处清理日志均成立 |
| RUN_ID 缺失/空/纯空白/带 `/` 时拒绝清理 | pass | 子进程退出码非 0，输出含 `WAVE4_S3_RUN_ID` |
| 清理失败必须单独失败（注入 `WAVE4_S3_CLEANUP_SIMULATE_FAILURE`） | pass | 注入时退出非 0 且输出 `INJECTED FAULT`；不注入时同一子进程退出 0 |
| 拒绝删除受保护桶 `contract-test-bucket` | pass | 子进程在**任何删除动作之前**被拒绝，桶存在性前后一致 |
| prefix 之外的对象不受影响 | pass | sentinel 内容/数量不变，兄弟 prefix 完整 |

### 4.4 未执行 / BLOCKED

| 项 | 状态 | 原因 |
|---|---|---|
| `crates/object-storage/tests/minio_contract.rs` | **BLOCKED / NOT RUN** | MinIO 开源二进制停发（`dl.min.io` → HTTP 410，末版无 Windows 资产）。既有 lane，未删除、未改写、未用 Fake 替代 |
| CI 上的同一 lane（MinIO 服务 + `if: always()` 步骤） | **未执行** | 本轮在本地对 RustFS 执行；GitHub Actions 上的两步要等 PR 触发。CI YAML 已通过 `python yaml.safe_load` 校验步骤顺序与 `if: always()` |
| 生产侧 multipart 能力 | **未实现，明确不做** | 端口未声明 multipart（ST-W4-01 DEFER），且本 provider 的 `UploadPart` 本身不完整（4.2）。不为测试或清理便利新增生产 multipart 功能 |

## 5. 故障场景证据（拆分之所以成立的部分）

三条场景都通过**子进程**注入故障，父进程断言退出码与残留，再执行清理：

* **(a) 契约失败后仍清理**：子进程在 prefix 下写入 6 个对象（> 1 页）后 `panic!`，
  退出码 `Some(101)`；父进程确认残留 6 个对象，随后清理删除 6 个并断言范围为空。
  日志同时保留两个判定：`contract step exit=Some(101) (still a failure)` 与
  `cleanup step exit=0`——清理成功不改写契约失败。
* **(b) 上传中断留下未完成的 multipart upload**：子进程创建 multipart upload 后中断，
  父进程先用 `ListMultipartUploads` 确认该 upload 可见（`FAULT (b) ... 1 entry(s)`），
  清理报告 `uploads_aborted=1` 且 `remaining_uploads=0`。
  若 provider 不列举未完成 upload，测试会打印 `PROVIDER-LIMITATION` 而不是把
  「清理成功」伪装成「中止成功」——本次运行中 RustFS 支持列举，因此走的是断言分支。
  > **Round 3 校准（原文保留）**：这一条当时的判定强度是**列表非空**，不是「列出了
  > 子进程实际创建的那个 key + upload-id」；而 `PROVIDER-LIMITATION` 分支只打印说明、
  > **仍以退出码 0 结束**，也就是 provider「接受了创建却什么都不返回」时该场景会全绿。
  > Round 3 已改为：子进程以 `PLAN0014-CREATED-UPLOAD {json}` 结构化回传真实创建对象，
  > 父进程断言确切配对出现在列举中，否则先做受限补偿 abort、单独记录补偿结果，
  > 并**让本场景失败**——补偿成功也不得把「未验证发现能力」改成 pass。
  > 见 [`../2026-09-28-wave4-round3-storage-lane/`](../2026-09-28-wave4-round3-storage-lane/) §1 R3-3。

* **(c) prefix 之外的对象不变**：在 `plan0014-wave4-sentinel/<RUN_ID>/` 与
  `plan0014-wave4-sibling/<RUN_ID>` 各放一个对象，经历「契约失败 + 清理」后内容与
  数量均不变。

## 6. 尝试记录（不覆盖）

| 目录 | 内容 |
|---|---|
| `attempts/10-fault-target-child-cases-ran-directly/` | 首次真实运行：契约步骤 exit=0、清理 exit=0，但故障目标 exit=101。原因是 `child_contract_that_fails` / `child_upload_then_interrupt` 自身是 `#[ignore]` 测试，`--ignored` 会把这两个「故意 panic」的用例当作用例执行。三个真实场景本身全部通过（该目录 `02-cleanup-fault-scenarios.log` 可见 `FAULT (a)`/`FAULT (b)` 与 `uploads_aborted=1`）。修复方式：故障注入改为单一 `child_entry_point`，由 `PLAN0014_LANE_CHILD_CASE` 选择进入的分支，未设置时为 no-op |

## 7. 复现

```powershell
# 1. 启动 provider（仅回环，凭据不提交）
$env:RUSTFS_ACCESS_KEY = '<dedicated>'
$env:RUSTFS_SECRET_KEY = '<dedicated>'
rustfs.exe server <volume> --address 127.0.0.1:19100

# 2. 三步（CI 中三步共享 job 级 RUN_ID）
$env:WAVE4_S3_ENDPOINT = 'http://127.0.0.1:19100'
$env:WAVE4_S3_ACCESS_KEY = '<dedicated>'
$env:WAVE4_S3_SECRET_KEY = '<dedicated>'
$env:WAVE4_S3_BUCKET = 'plan0014-wave4-bucket'
$env:WAVE4_S3_REGION = 'us-east-1'
$env:WAVE4_S3_RUN_ID = "r2-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
$env:WAVE4_S3_CLEANUP_PAGE_SIZE = '3'

cargo test -p object-storage --test s3_contract_lane        --all-features -- --ignored --test-threads=1 --nocapture
cargo test -p object-storage --test s3_lane_fault_injection --all-features -- --ignored --test-threads=1 --nocapture
cargo test -p object-storage --test s3_lane_cleanup         --all-features -- --ignored --test-threads=1 --nocapture
```

## 8. 本目录不证明的事

* 不证明 provider 的 multipart 完成链路可用（4.2 记录了相反的观察）；
* 不证明 CI 环境（MinIO + `if: always()`）已执行——CI YAML 已改并做语法校验，
  真实执行要等 PR；
* 不证明 `minio_contract.rs` 通过（BLOCKED）；
* 不证明任何跨 provider 的通用能力：矩阵行只描述本次 RustFS 构建。

## 9. 绑定

| 项 | 值 |
|---|---|
| 代码候选 commit（本目录证据所绑定的实现） | `8be1d1b152bbc726fc1cb3002e3f462d2ebad328` |
| provider 身份探测原始输出 | `attempts/10-fault-target-child-cases-ran-directly/00-provider-identity.log`（同一构建，含 `git commit 0913686b…` / `git tag 1.0.1-preview.11`；branch 为空是因为 provider 源码以 tag  detached 检出） |
| 换行符 | 本目录日志由 PowerShell 重定向产生 CRLF，入库前统一为 LF（与 Round 1 日志一致）；只改行结束符与行尾空白，日志文本内容未改动 |
| provider 凭据 | 仅回环监听的一次性本地值，未出现在本目录任何日志或仓库文件中 |
| 门禁与日志安全修复证据 | [`../2026-09-28-wave4-round2-gates/`](../2026-09-28-wave4-round2-gates/) |
| Round 1 记录（保留，不覆盖） | [`../2026-09-28-wave4-storage-lane/`](../2026-09-28-wave4-storage-lane/) |
| 终态 | READY FOR INDEPENDENT REVIEW / NOT INTEGRATED |
