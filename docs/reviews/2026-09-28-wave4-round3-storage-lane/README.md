# PLAN-0014 Wave 4 Round 3 — S3 lane：清理分页判定与「已创建但不可见」

本目录是 Wave 4 Round 3 对**真实 S3-compatible provider lane** 的执行证据。
它不替换 Round 1 的 `../2026-09-28-wave4-storage-lane/`，也不替换 Round 2 的
`../2026-09-28-wave4-round2-storage-lane/`；三份日志并存，中间失败记录一律保留。

本轮只动测试侧（`crates/object-storage/tests/support/mod.rs`、
`tests/s3_lane_cleanup.rs`、`tests/s3_lane_fault_injection.rs`、新增
`tests/s3_lane_pagination.rs`）。生产存储能力、`ProviderSafetyFilter` 语义、
通用日志脱敏平台均未扩展，也不在本轮范围。

## 1. 本轮关闭的三个缺陷

| # | 缺陷 | 失败方向 | 修复 |
|---|---|---|---|
| R3-1 | `list_all_incomplete_uploads` 只比较 **key** 判断游标是否推进 | 同一 key 的第 2 页必然被判「游标未推进」而报错 | 游标是 `(key-marker, upload-id-marker)` **整对**；整对变化即合法推进。opaque upload id 不参与本地排序 |
| R3-2 | `list_all_keys` 在 `IsTruncated=true` 且无 continuation token 时 `return Ok` | **最危险方向**：删掉第一页后相信列表完整，报告「prefix 为空」 | 只有 `IsTruncated=false` 正常结束；`true` 必须有非空且未出现过的 token；`IsTruncated` 缺失也判失败 |
| R3-3 | 故障场景 (b) 只断言 `ListMultipartUploads` **非空**，空列表时打印 PROVIDER-LIMITATION 后仍以 0 退出 | provider 接受 `CreateMultipartUpload` 却不列举时，lane 仍然全绿 | 子进程以结构化 `PLAN0014-CREATED-UPLOAD {json}` 记录回传真实创建的 key/upload-id；父进程断言**确切配对**被列出；未列出则补偿 abort + 单独记录 + **本场景失败** |

分页错误全部进入 `CleanupReport.errors`；`CleanupReport` 另加
`listing_incomplete` 字段并在 `render()` 打印
`CLEANUP LISTING INCOMPLETE`，`ok()` 直接拒绝——列表没跑完时
`remaining_objects=0` 的含义是「没看完」，不是「没有残留」。

## 2. Provider 身份与专用范围

| 项 | 值 |
|---|---|
| 实现 | RustFS `1.0.1-preview.11`（build 2026-09-24 10:16:46 +00:00, release） |
| provider git | commit `0913686b12845eefc69a2f4ed4ee6a8161e25742`，tag `1.0.1-preview.11`（`00-provider-identity.log`） |
| 归档 | `rustfs-windows-x86_64-v1.0.1-preview.11.zip`，105,742,053 bytes |
| 归档 SHA-256 | `cae2628159a8759118d8a68cd043c389a05ee9a6a13ceb9e188e9414578453d5`（本轮重新计算，与上游 `SHA256SUMS` 及 Round 1/2 记录一致：同一构建） |
| 启动 | `rustfs.exe server <volume> --address 127.0.0.1:19100`，仅回环监听；凭据为一次性本地值，未进入仓库任何文件 |
| 健康探测 | `GET /minio/health/live` → 200 |
| 专用桶 | `plan0014-wave4-bucket`（**未删除**，未清空整桶） |
| 专用范围 | `plan0014-wave4/r3-20260928-155500/` |
| 列举页大小 | `WAVE4_S3_CLEANUP_PAGE_SIZE=3` |
| RUN_ID | `r3-20260928-155500`（与失败的首轮 `r3-20260928-155258` 不同） |

## 3. 步骤退出码（`run-summary.txt`）

| 步骤 | 目标 | 退出码 | 用例 | 日志 |
|---|---|---|---|---|
| 2 契约执行 | `s3_contract_lane` | **0** | 6 passed | `01-contract-step.log` |
| 3 故障场景 | `s3_lane_fault_injection` | **0** | 5 passed（Round 2 为 4，本轮新增 (d)） | `02-cleanup-fault-scenarios.log` |
| 4 最终清理 | `s3_lane_cleanup` | **0** | 7 passed | `03-final-cleanup.log` |
| 4b 再次清理 | `s3_lane_cleanup` | **0** | 7 passed（幂等） | `04-final-cleanup-again.log` |

契约失败与清理失败各自退出码：场景 (a) 内层契约子进程 `exit=Some(101)` 保持不变，
清理步骤独立 `exit=0`；`WAVE4_S3_CLEANUP_SIMULATE_FAILURE` 用例仍单独失败。

## 4. 本轮新增的真实多页证据（同 key 7 个未完成 upload）

```
FAULT (d) 7 upload(s) on one key listed across 3 page(s) at page size 3
FAULT (d) 7 upload(s) on one key: listed across 3 page(s), aborted 7, none left
FAULT (d) post-cleanup scope empty (checked over 1 upload page(s) and 1 object page(s))
```

这条记录支撑什么、不支撑什么，划清：

* 支撑：该 provider 在同一 key 上按 `(key-marker, upload-id-marker)` 分页返回；
  本仓修复后的游标逻辑在真实 provider 上走完 3 页；7 个 upload 全部被 abort，
  清理后 `ListMultipartUploads` 为空；prefix 外哨兵内容未变、哨兵 scope 仍只有 1 个对象。
* 场景内部另有两条硬断言，防止「其实是单页」冒充多页：
  `before.pages > 1`（发现列举跨页）与 `report.upload_pages > 1`（**清理自身**的列举也跨页）。
* 不支撑：provider 的 multipart 完成链路（`UploadPart` 仍 `service error`，见 §5）；
  不支撑跨 provider 通用能力；不支撑 CI 已执行。

对比 Round 2：其真实 multipart 场景 `uploads_aborted=1` 是**一个 upload、一页、
没有游标**，因此 Round 2 从未执行 multipart 分页路径（见 §7 校准）。

## 5. 能力观察（provider 行为记录，不参与通过判定）

| 能力 | 本次观察 |
|---|---|
| `CreateMultipartUpload` / `AbortMultipartUpload` | 可用（abort=true） |
| 合法 5 MiB `UploadPart` | 失败：`service error`（provider 侧 InternalError） |
| `CompleteMultipartUpload` | 未执行（上一步失败） |
| `ListMultipartUploads` | 可用，**且支持同 key 多 upload 的游标分页**（§4） |
| `ListObjectsV2` | 可用；`IsTruncated` 有应答；对象多页清理跨 2 页（6 对象 / page_size 3） |
| 条件读 `If-Match` | 可用；过期 ETag 被拒绝 |
| 对象校验和 | SDK CRC32 被接受并回显；`HeadObject` 返回校验和 |
| 版本化 | `unconfigured` |

这些行只描述本次 RustFS 构建，禁止外推。

## 6. 保留的失败记录：首轮真实运行（`attempts/01-…`）

首轮（RUN_ID `r3-20260928-155258`）故障步骤 **exit=101**，2 个场景失败：

```
(b): the child case creates one upload; it reported []
(d): the child case creates 7 uploads; it reported 6
```

根因是本轮新加通道自身的缺陷，且被真实运行才暴露：libtest 打印
`test <name> ... ` 后子进程第一条 `println!` **续在同一行**，
`parse_created_uploads` 原按「行首前缀」匹配，于是**恰好丢掉第一条**记录
（(b) 只有 1 条 → 全丢；(d) 7 条 → 丢 1 条）。
断言选择器按 `contains` 找 `FAULT_ARMED` 仍命中，所以只体现为记录数不足。

修复：标记在行内任意位置定位；回归
`s3_lane_pagination::a_record_sharing_a_line_with_the_harness_still_parses`。

这一条值得单独说明立场：判定失败**没有**被放宽成「列表非空即可」，而是先修通道再复跑；
首轮的桶内状态没有残留——该 RUN_ID 的清理步骤当时已 `exit=0`，
诊断用的 `plan0014-wave4/r3-diag-1/` 也由其自己的 scoped cleanup 清空
（`objects_deleted=0 uploads_aborted=0 remaining_objects=0 remaining_uploads=0`）。

## 7. 对 Round 2 结论的校准（不得混读）

| 说法 | 事实 |
|---|---|
| 「Round 2 已验证对象多页清理」 | **成立**：6 个对象 / page_size 3，`object_pages=2`，Round 3 再次复现 |
| 「Round 2 已验证 multipart 分页清理」 | **不成立**。Round 2 真实 multipart 场景只有 1 个 upload → 1 页、无游标；其 `uploads_aborted=1` 只证明 abort 可用 |
| 「Round 3 验证了同 key 的 multipart 多页清理」 | **成立**（§4，同一 provider 构建，RUN_ID `r3-20260928-155500`） |
| 「Round 2 证明了清理能发现被中断的 upload」 | **弱化**：Round 2 的判定是「列举结果非空」，不是「列出了自己创建的那个 upload」。Round 3 改为精确配对判定后，本轮才成立 |

## 8. 未执行 / 范围外

* `tests/minio_contract.rs`：仍 **BLOCKED**（MinIO 开源二进制停发），未删除、未弱化、未以 Fake 冒充。
* CI 上的 lane 步骤：需 PR 触发，本轮未执行；CI YAML 未在本轮改动。
* provider multipart 完成链路：`UploadPart` 失败仍是 provider 观察，本轮未修，也未因此扩展端口。
* 共享桶未清空、清理 prefix 未扩大、受保护桶拒删逻辑未放宽。
* 生产 `S3Client` 未为受限环境重构。

## 9. 绑定

| 项 | 值 |
|---|---|
| 起点 HEAD | `7fb9f735a7e0a787666d7b62592f08ac5161245a` |
| 代码候选 commit | 见交付报告与本目录所在提交链（代码提交不含本 README） |
| 确定性分页/判定测试证据 | [`../2026-09-28-wave4-round3-gates/`](../2026-09-28-wave4-round3-gates/) |
| Round 2 记录（保留，不覆盖） | [`../2026-09-28-wave4-round2-storage-lane/`](../2026-09-28-wave4-round2-storage-lane/) |
| provider 凭据 | 仅回环一次性值，未出现在本目录任何日志或仓库文件中 |
| 终态 | READY FOR INDEPENDENT REVIEW / NOT INTEGRATED |
