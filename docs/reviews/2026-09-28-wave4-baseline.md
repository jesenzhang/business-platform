# PLAN-0014 Wave 4 — Document / Object Storage / Observability / Notification baseline

> Wave: PLAN-0014 Wave 4 (Document & Object Storage / Observability / Notification reference conformance)
> Date: 2026-09-28 (actual date; no back-filling)
> Branch: `codex/plan-0014-wave4-reference-conformance` (created from the verified start point; `main` untouched; no push, no merge)
> Base commit: `a4c2f74b5f0fbff5a9b0abde6319be33831e9890`
> Code commit under review: `6fb9487d045108c70efd41c88ccc95f0861ba603`
> Maturity claimed: **R1 (foundation conformance)** for the areas measured. R2 admission is NOT claimed; the verdict column is left for the independent reviewer.
> Status: **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**
> Companion reviews: `2026-09-28-saas-align-document-storage.md`, `2026-09-28-saas-align-observability.md`, `2026-09-28-saas-align-notification.md`; evidence: `2026-09-28-wave4-gates/`, `2026-09-28-wave4-storage-lane/`.

## 0. Previous round conclusion (recorded verbatim, not expanded)

> Round 2c 修复通过；Wave 3 保持 R1 / NOT INTEGRATED；R2 阻塞项继续有效

That sentence is Wave 3 scoped. It is not a repository-wide, release or
production acceptance, and this wave does not extend it. The Wave 3 R2 blockers
(`docs/reviews/2026-09-27-wave3-round2c-repair.md`) remain open and are not
re-opened, re-litigated or silently closed here.

## 1. Scope, method and boundaries

Method (PLAN-0014 §4, unchanged): Step A reconstruct the real capability
baseline; Step B pin fixed references (release/tag/commit + concrete source);
Step C capability matrix; Step D classify gaps C0/C1/C2/C3/REJECT; Step E
bounded repair of evidenced C0/C1 with a failing test first; Step F is the
independent reviewer's, not ours.

In scope: Document Management + Object Storage (§四), Observability (§五),
Notification (§六) of the Wave 4 brief.

Out of scope and not touched: Wave 5, PLAN-0006, ADR-0027 and ADR-0028 stay
`Proposed` / **NOT ACCEPTED** (nothing in this wave depends on them being
accepted; where a gap needs ADR-0028 it is registered as a dependency and an R2
block instead of inventing a public event envelope), no new deployment unit, no
notification platform, no push/merge/release.

Pre-existing working-tree modification preserved: `crates/document-sqlite/tests/
operational.rs` was already modified (end-of-line only) when the wave started. It
was never staged, committed or reformatted; `git hash-object` = `8dc7c7ac49e08ef
8eec8ea79a53a352aec48b5c9` = the base-commit blob.

## 2. Step A — capability baseline

Maturity vocabulary (used consistently in all three Wave 4 documents):

* **未实现** — no code, no port, or a placeholder that nothing consumes.
* **仅接口** — port/DTO/schema exist; no adapter or no caller executes them.
* **仅 Fake** — behaviour only proven against an in-memory fake or a local adapter.
* **真实适配器已验证** — proven against the real adapter in this wave's lanes.
* **已实现（带限制）** — production path exists and is verified, with named limits.

### 2.1 Document Management and Object Storage

| 能力 | 权威所有者 | 核心端口 | 适配器 | 公开契约 | 现有测试（Wave 4 前） | 真实环境证据 | 缺口 | 成熟度 |
|---|---|---|---|---|---|---|---|---|
| Document 元数据聚合、生命周期、乐观版本 | Document Management (`crates/document`) | `DocumentRepository`, `CreateDocumentUnitOfWork` | `document-sqlite`, `document-postgres` | `POST/GET /documents…` (`business-api`) | `contracts.rs`, `concurrency.rs`, `audit_chain.rs`, `revision_history.rs` | SQLite 全绿；PG 专用库 `plan0014_wave4` 契约通过 | D-W4-02, D-W4-03 | 真实适配器已验证 |
| 幂等创建与重放（含指纹冲突） | Document Management | `CreateDocumentUnitOfWork` | 同上 | `Idempotency-Key` 头 | `contracts.rs`, `concurrency.rs` | SQLite + PG 均通过 | **D-W4-01（本波修复）** | 真实适配器已验证 |
| 内容 Revision 不可变 + revision 作用域存储引用 | Document Management | `DocumentMetadata::create_with_revision_id*`, `rehydrate` | 同上（迁移 005 / 017 触发器） | 无（内部不变量） | `revision_history.rs`（只覆盖 R1→R2 原子性） | **新增** `content_reference.rs`：SQLite 上 key 形状 + `change_reason` + UPDATE/DELETE 被触发器拒绝；PG 侧只有迁移 017 定义，未在通道中断言 | D-W4-04, D-W4-09 | SQLite 已验证 / PG 仅迁移定义 |
| 业务对象链接 `document_links` | Document Management | `DocumentLink` | 同上 | REST 链接端点 | 链接契约测试 | 无（本波未运行链接通道） | **D-W4-11（无 revision 绑定）** | 已实现（带限制） |
| 对象流式读写 / head / delete / exists / presign | Document Management（对象引用）+ 平台能力 object-storage | `ObjectStorageClient` | `S3Client`（aws-sdk-rust 1.x）、`LocalStorageClient` | 无 HTTP 暴露（仅内部端口） | 15 个单元测试 + `minio_contract.rs`（从未执行） | **新增** RustFS 真实通道 7/7；MinIO 官方二进制已停发（410）→ 该文件仍 BLOCKED | ST-W4-01…ST-W4-07 | 真实适配器已验证（受限 provider 能力集） |
| 上传补偿（对象写入成功但元数据失败） | Composition root (`business-api`) | 无端口（组合层直接调用） | `upload.rs` 补偿删除 | 无 | 无 | **新增**：补偿失败现在会留下 `warn` 级证据（此前被 `let _ =` 丢弃） | ST-W4-05（无孤儿对象扫描） | 已实现（带限制） |
| 元数据存在但对象缺失 | Document Processing（读取侧）+ Document Management（引用） | `ProcessingSource::read_source` | `business-worker` `StorageSource` | 稳定错误码 `source_not_found` | 无 | **新增** `apps/business-worker` 单元测试（fake repository + fake storage） | — | 真实组合边界已验证 |
| 租户 / 资源作用域 / ACL | Identity+Policy（授权事实）；Document（数据） | `Authorize`(policy)、`TenantContext` | `business-api` composition | `Authorization` 中间件 | identity/policy lanes | 文档路由只做租户隔离，**未调用 policy 授权**（仅 `admin.rs` 走 `authorize_governance`） | **D-W4-06** | 仅接口（对 document 资源） |
| 下载 / presign 对外能力 | Document Management | `ObjectStorageClient::presign` | `S3Client` | **无路由** | 本波新增 presign 单元测试 + 真实通道 presign 用例 | presign 端口与真实 provider 已验证，但无消费方 | ST-W4-07 | 仅接口（对外） |
| 保留 / 删除契约（purge hold、tombstone） | Document Management | 领域状态 + `document_purge_holds` | 双后端表 | 状态查询 | 生命周期测试 | 无本波通道；`pre_trash_lifecycle` 已持久化但在现有转换下与 `lifecycle_state` 恒等（不带独立信息），trash→restore 往返无测试 | **D-W4-07** | 已实现（带限制） |
| 元数据 ↔ 对象一致性核对 | Document Management + Data Integrity | `data-integrity` 扫描框架 | 无对象存储扫描器 | 管理端扫描端点 | integrity lanes（不含对象侧） | 未运行 | **D-W4-08** | 未实现（对象侧） |
| 公开 DTO / 错误 / 日志不泄露 object key、bucket、路径、签名 URL | 各上下文 + API 标准 | DTO 定义 | `business-api`、`object-storage` | OpenAPI | 无（本波新增） | **新增** `log_hygiene.rs`（自建 subscriber 捕获 `object_storage` 目标全部记录）+ DTO 字段审查 | ST-W4-08（`StorageError::NotFound` Display 内嵌 key） | 已实现（带限制） |

### 2.2 Observability

| 能力 | 权威所有者 | 核心端口 | 适配器 | 公开契约 | 现有测试（Wave 4 前） | 真实环境证据 | 缺口 | 成熟度 |
|---|---|---|---|---|---|---|---|---|
| 结构化日志（级别/格式/服务名） | 平台 (`crates/observability`) | `init_tracing` | `business-api`, `business-worker`, `ai-worker` | 无 | observability 单元测试 | 未运行真实 exporter | OBS-W4-06 | 已实现（带限制） |
| 同一能力在所有进程一致可用 | 各 composition root | 同上 | 同上 | 无 | 无 | **新增**：`governance-worker` 之前**完全没有** subscriber；现在初始化并 fail-closed 解析格式 | OBS-W4-01（已修复） | 已实现 |
| provider 噪声与平台信号分离 | 平台 | `default_filter_directives` | EnvFilter | 无 | 无 | **新增** observability 测试：`debug` 级别下 provider `debug` 被压到 `warn`，平台 `debug` 保留 | OBS-W4-02（已修复） | 已实现 |
| 指标：直方图 vs 摘要、标签基数 | 平台 + 各上下文 | `observability::metrics` | Prometheus 渲染 | `/metrics` | business-worker 标签有界测试 | **新增** business-api 测试：duration 渲染为 bucketed histogram，无 `{quantile=` | **OBS-W4-03（已修复）** | 已实现（带限制） |
| 请求 → 应用 → 任务 → provider 关联与 trace 传播 | 平台 + 各上下文 | 无（未定义契约） | `tracing` spans、`x-request-id` | `trace_id` 字段（`ApiError`） | 无 | 三处路由把**客户端 `x-request-id` 原样回显**为 `trace_id`；其余路由恒为 `None`；异步任务只带 `correlation_id`，无 trace 连续性 | **OBS-W4-05（依赖 ADR-0028）** | 仅接口 |
| 日志脱敏 | 平台 + Audit | `audit` 侧 `redact_details`/`sanitize_details_for_read` | 同 | 审计读端 | audit lanes（Wave 3） | 日志路径**没有**等价脱敏 | **OBS-W4-07** | 仅接口（仅审计侧） |
| exporter 故障不影响业务结果 | 平台 | `init_tracing` OTLP 分支 | OTLP exporter | 无 | 无 | **未运行**：本环境无 Collector | **OBS-W4-06** | 未验证（BLOCKED） |
| telemetry 与正式审计分离 | Audit（正式事实）+ 平台（信号） | `audit_events` 链 vs tracing | 双后端 | 审计 API | Wave 3 audit lanes | 分离成立；但 document 生命周期审计行不带 trace/correlation 列值 | OBS-W4-05 | 已实现（带限制） |

### 2.3 Notification

| 能力 | 权威所有者 | 核心端口 | 适配器 | 公开契约 | 现有测试（Wave 4 前） | 真实环境证据 | 缺口 | 成熟度 |
|---|---|---|---|---|---|---|---|---|
| NotificationIntent / 业务事实边界 | Notification（规划中） | 无 | 无 | 无 | 无 | 无 | N-G01 | **未实现** |
| 收件人解析（租户作用域）/ 偏好 / 退订 | Notification + Identity | 无 | 无 | 无 | 无 | 无 | N-G02 | 未实现 |
| 模板与版本 | Notification | 无 | 无 | 无 | 无 | 无 | N-G03 | 未实现 |
| 幂等 / 重试 / 失败分类 / 投递状态真值 | Notification | 无 | 无 | 无 | 无 | 无 | N-G04 | 未实现 |
| provider 适配（邮件/短信/Webhook/站内） | Notification | 无 | 无 | 无 | 无 | 无 | N-G04 | 未实现 |
| 可靠的出站底座（Outbox → broker） | Messaging（Wave 3） | `messaging` 端口 | `messaging` adapters | 无 | `outbox_reliability`, `inbox_idempotency`（PG ignored lanes） | 本波未运行；`config/production.toml` 的 `[messaging] enabled = true` **无任何读取方**，`otlp_endpoint` 为注释行 | **N-G05** | 仅接口（文档域未接线） |

`crates/notification` 的实际状态：7 行 crate 文档 + 5 个未使用依赖 + **零依赖方**
（无 crate/组合根引用）。crate 文档原先声称“站内信、邮件、短信、Webhook 等多渠道
通知的发送与管理”，本波已改为如实的占位说明（`34f1921a…`）。

## 3. Step B — fixed references

Every row states a fixed release, the concrete source that was actually read at
that revision, the corresponding behaviour in this repository, and the verdict.
`homepage-only` citations are not used; where a reference could not be pinned in
this environment the row says so and contributes **no** behaviour conclusion.

| Reference | Fixed revision (verified) | Concrete source read | Corresponding behaviour here | Verdict and reason |
|---|---|---|---|---|
| Repository review template | this repo @ `a4c2f74b` | `docs/templates/REFERENCE_CONFORMANCE_REVIEW_TEMPLATE.md`; PLAN-0014 C0/C1/C2/C3 definitions | all three Wave 4 reviews follow the 9 sections | **采纳**（方法基线） |
| AWS S3 — object ops | API reference pages verified live 2026-09-28: `API_PutObject.html`, `API_UploadPart.html`, `API_CreateMultipartUpload.html`, `API_CompleteMultipartUpload.html`, `API_AbortMultipartUpload.html` | request/response semantics incl. the 5 MiB minimum for non-final parts | `ObjectStorageClient` has no multipart at all; RustFS rejected a legal 5 MiB `UploadPart` with `InternalError` | **采纳为行为契约**，但 **REJECT 立即实现**：无业务用例，登记 ST-W4-01 / ST-W4-07 |
| AWS S3 — presigned URLs | `sigv4-query-string-auth.html`, `using-presigned-url.html` (live 2026-09-28) | signature is scoped to method+key+expiry and expires with the credential | `S3Client::presign` bounded by `MIN/MAX_PRESIGN_EXPIRES_SECS`; real-lane case proves key scoping and expiry; `LocalStorageClient` now refuses | **采纳**（已在 ST-W4-02/03 修复中体现） |
| AWS S3 — checksums / conditional reads / versioning | `checking-object-integrity.html`, `conditional-requests.html`, `versioning-workflows.html` (live 2026-09-28) | optional integrity checksums, `If-Match` preconditions, per-object version ids | port exposes none of the three; provider behaviour recorded per provider only | **暂缓**（DEFER，ST-W4-01）：端口未声明即不得声称支持 |
| S3-compatible implementation actually used | RustFS `1.0.1-preview.11`, tag commit `0913686b12845eefc69a2f4ed4ee6a8161e25742`, archive SHA-256 `cae26281…53d5` | server CLI + `GET /minio/health/live` + observed responses in `2026-09-28-wave4-storage-lane/` | the real provider lane's assertions and capability rows | **采纳为测试靶**（单一实现，不外推） |
| MinIO | last open-source release `RELEASE.2025-09-07T16-13-09Z`; `dl.min.io/.../windows-amd64/minio.exe` → HTTP 410 | download probe output | `tests/minio_contract.rs` stays unexecuted | **BLOCKED 记录**（ST-W4-06），不得以 Fake 代替 |
| Paperless-ngx | `v3.2.1` → commit `7575d6078227ebdb4cf443f263d53ebc7575aa37` | `src/documents/consumer.py`, `src/documents/models.py`, `src/documents/data_models.py`, `src/paperless_mail/mail.py` (HTTP 200 at that commit) | consumption pipeline: source path/label, corruption folder, document↔storage-path separation, correspondents-as-labels | **部分采纳**：确认“消费失败不得吞掉来源、消费记录与文档分离”的方向与本仓 upload 补偿一致；**REJECT** 文件系统目录即权威（本仓权威是 DB + 对象引用） |
| Documenso | `v2.18.0` → commit `389390c884949fe27c240488a3259da3cdba93e0` | `packages/prisma/schema.prisma` (HTTP 200 at that commit): document/recipient/field/model tables | separate document record vs stored file, envelope/recipient state | **部分采纳**：支持“业务文档记录与文件对象分离、投递/签署状态独立建模”；其 presign 具体实现文件在本环境无法定位（多次 404），故不据此下行为结论 |
| Open Contracts | `v3.1.1` → commit `68bede568d4073edf85ca56ec7434d5a10444c87` | `README.md` at that commit (spec/schema layout description) | contract metadata vs attachment separation | **暂缓（DEFER）**：本轮无对应新增行为需求 |
| OpenTelemetry specification | `v1.61.0` → commit `32c0b2c1d9e11f68da0e6e7b63044e57878590de`; normative text at `opentelemetry.io/docs/specs/otel/trace/api/`, `/context/`, `/logs/data-model/`, `/common/#attribute` (all live) | trace context propagation model, span/attribute guidance, "avoid unbounded attribute cardinality" | provider noise pinning, histogram-vs-summary, correlation contract absence | **采纳方向**，但跨进程 trace 连续性依赖 **ADR-0028（未接受）** → 只登记 OBS-W4-02 为 R2 阻塞，不自造事件信封 |
| OpenTelemetry Collector | `v0.161.0` → commit `0bf928af5487d3c4e0b4174eabb7ba075c322517` | `docs/observability.md` at that commit | exporter failure isolation, sampling outside the app | **BLOCKED**：本环境无 Collector 实例，OBS-W4-06 |
| Novu | **未能固定**：`api.github.com/repos/novu/novu` → HTTP 404，`novu-hq/novu` → 404，`git ls-remote --tags` → 0 refs | 无法读取 | 通知域整体未实现，本轮无行为可对照 | **BLOCKED 记录**：本轮不引用 Novu 的任何具体结论 |
| Mayan EDMS | **未能固定**：`/repos/mayan-edms/mayan-edms/tags` 只返回 `v1.0`/`v0.5`；`git ls-remote --tags` 只有 4 个 tag；`raw.githubusercontent.com/.../<4.9.x>/…` → 404 | 无法读取 | 同上 | **BLOCKED 记录**：本轮不引用 Mayan EDMS 的任何具体结论 |

**References never override Accepted ADRs or data ownership.** Where a reference
suggests a different owner (e.g. storage-directory-as-authority, or a
notification platform owning delivery facts), the repo boundary wins and the
difference is recorded in the review's "Accepted differences" section.

## 4. Binding

| Item | Value |
|---|---|
| Baseline HEAD | `a4c2f74b5f0fbff5a9b0abde6319be33831e9890` |
| Code commit | `6fb9487d045108c70efd41c88ccc95f0861ba603` (code only, no docs) |
| Docs closeout commit | recorded in `docs/reviews/2026-09-28-wave4-gates/README.md` §closeout |
| Gate results | fmt 0 · check 0 · clippy `-D warnings` 0 · workspace tests 759 passed / 0 failed / 59 ignored · architecture fitness PASS · `git diff --check` 0 |
| Real lanes | PostgreSQL (dedicated `plan0014_wave4`, dropped) 3/3 · RustFS S3-compatible lane 7/7 + cleanup + teardown |
| Independent reviewer verdict | *pending — left blank on purpose* |

## 5. Round 2 addendum (2026-09-28, same wave, not a replacement)

Round 2 re-checked two of the rows above and found both weaker than recorded.
The Round 1 rows are left untouched; this addendum is the correction.

| Round 1 statement | Round 2 finding | Evidence |
|---|---|---|
| "RustFS S3-compatible lane 7/7 + cleanup + teardown" | The cleanup case listed the prefix once, so with more than one page of residue it deleted the first page and still reported an empty prefix; incomplete multipart uploads were never looked at; bucket teardown could delete CI's shared `contract-test-bucket`. Contract execution and final cleanup are now separate test targets and separate CI steps (`if: always()`, one shared RUN_ID) | `docs/reviews/2026-09-28-wave4-round2-storage-lane/` |
| OBS-W4-02 "provider 噪声压制 · 已修复" (C2) | The fix was an `EnvFilter` directive suffix, bypassed by any `RUST_LOG` value and by more-specific subtarget directives, and fmt renders enclosing span fields — so credential identifiers and storage references still reached output. Re-closed in Round 2 as a **security fix** via an `EnvFilter`-independent layer; OBS-W4-07 (WARN/ERROR payload, general redaction) stays open | `docs/reviews/2026-09-28-wave4-round2-gates/` |

Round 2 also records one execution-environment finding: the `TrustStore configured
to enable native roots but no valid root certificates parsed!` panic reproduced
only under a restricted file-access sandbox — the same test binary passed with
normal access — so it is an environment difference, not a repository defect.
Production TLS verification was **not** changed; the parameter-rejection unit
tests now install a no-network transport instead of pointing at a dead endpoint,
which is why they no longer read the host trust store at all. The probe log, the
"no TLS-weakening configuration anywhere" grep and the passing unit suite are in
`docs/reviews/2026-09-28-wave4-round2-gates/40-truststore-environment-probe.log`.

| Round 2 item | Value |
|---|---|
| Branch | `codex/plan-0014-wave4-reference-conformance` (not integrated) |
| Round 2 code candidate | `8be1d1b152bbc726fc1cb3002e3f462d2ebad328` |
| Evidence dirs | `docs/reviews/2026-09-28-wave4-round2-gates/`, `docs/reviews/2026-09-28-wave4-round2-storage-lane/` |
| Round 2 gates | fmt 0 · check 0 · clippy `-D warnings` 0 · workspace tests 784 passed / 0 failed / 69 ignored (179 suites) · architecture fitness PASS · `git diff --check` 0 |
| Round 2 real lane | RustFS 1.0.1-preview.11 (same archive digest as Round 1), RUN_ID `r2-20260928-065142`: contract 0 · fault cases 0 · final cleanup 0 · cleanup again 0 |
| Terminal state | READY FOR INDEPENDENT REVIEW / NOT INTEGRATED |
