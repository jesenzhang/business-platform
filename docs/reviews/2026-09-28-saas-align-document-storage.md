# PLAN-0014 Wave 4 — Document / Object Storage reference conformance review

> Wave: PLAN-0014 Wave 4 · Review date: 2026-09-28 (actual date)
> Branch `codex/plan-0014-wave4-reference-conformance` · Base `a4c2f74b5f0fbff5a9b0abde6319be33831e9890`
> Code commit `6fb9487d045108c70efd41c88ccc95f0861ba603` · Evidence `2026-09-28-wave4-gates/`, `2026-09-28-wave4-storage-lane/`
> Maturity claimed: **R1 (foundation conformance)**. Verdict column intentionally empty — the independent reviewer records PASS / PASS WITH C2-C3 / FAIL.
> Baseline and fixed references: `2026-09-28-wave4-baseline.md` (not repeated here).

## 1. Scope

Review question: does the Document Management slice (metadata aggregate, content
revisions, idempotent creation, links, retention) plus the Object Storage
adapter conform to the platform baseline — DATA_OWNERSHIP_AND_CONSISTENCY §file
state machine/checksum/compensation/consistency scan, SECURITY_ARCHITECTURE and
AGENTS §10 (no storage reference in responses, logs or traces),
API_AND_EVENT_CONTRACT_STANDARD, AGENTS §15 PLAN-0003 persistence gates — when
measured against the fixed references of baseline §3?

Method: read-only measurement first; only evidenced C0/C1 repaired, each with a
test that failed before the change; no schema migration, no public contract
change and no new capability was introduced in this wave.

Not in scope: Wave 5, PLAN-0006, ADR-0027/0028 (both still `Proposed`, **NOT
ACCEPTED**), the notification platform, and the preserved end-of-line-only
modification of `crates/document-sqlite/tests/operational.rs` (untouched, still
unstaged).

## 3. Current implementation facts

### Authority and data ownership
Document Management owns `documents`, `document_revisions`, `document_links`,
`document_purge_holds` and the content reference string. Object Storage is a
platform capability adapter, not an owner of business facts: the port
`ObjectStorageClient` (`crates/object-storage/src/client.rs:56-74`) exposes
`put_stream/open_stream/head/delete/exists/presign` and nothing else. Verified:
no other context writes those tables.

### Domain/Application model
Document ≠ content Revision ≠ physical object, and the separation is real:
`DocumentContentReference::new_revision(...)` **replaces** whatever logical path
the caller supplied with `tenants/{tenant}/documents/{doc}/revisions/{rev}/source`
(`crates/document/src/domain/entity.rs:486-529`). This wave pinned that
statement with `crates/document-sqlite/tests/content_reference.rs`, which reads
the persisted row back and asserts the exact key, `revision_no = 1`,
`sha256` persistence and `change_reason = 'initial upload'`.
(An earlier inventory note claiming new writes produce a legacy `…/v1/` key
labelled "legacy content reference backfill" was reproduced false and is not
published as a finding.)

### Tenant/Principal/AuthZ
Tenant scoping is enforced in every repository/query predicate. Authorization is
not: `apps/business-api/src/routes/documents.rs` contains no policy call, while
`admin.rs` uses `authorize_governance(...)` for every management route. So for
document resources the platform has tenant isolation but no resource-level
authorization path (gap D-W4-06).

### Public contracts
No HTTP surface exposes `object_key`, bucket, storage path or a presigned URL
(PLAN-0003 gate satisfied), and there is no download/presign route at all —
`grep 'presign|download' apps/business-api/src/routes/*.rs` matches nothing.
Consequence: presign is an internal capability with no consumer (ST-W4-07).

### Persistence/migrations
* `document_revisions` is append-only on both backends: PG migration
  `migrations/017_document_revision_evidence_integrity.sql:33-41`
  (`document_revisions_immutable`), SQLite
  `crates/document-sqlite/migrations/005_document_revision_evidence_integrity.sql:16-25`.
* SQLite additionally enforces processing-evidence → revision and → artifact
  binding (`005:28-77`, four triggers). The PostgreSQL tree contains **only**
  `document_revisions_immutable` — a full trigger inventory of
  `migrations/*.sql` yields exactly one trigger. That asymmetry is gap D-W4-09.
* `document_links` (`migrations/016_…:65-77`) keys on
  `(tenant_id, document_id, resource_kind, resource_id, role)` with **no revision
  column**; `role` includes `'evidence'`, so an evidence link follows the current
  revision implicitly (gap D-W4-11).
* `document_idempotency.document_id` is a foreign key onto `documents(id)` only.
  PG already has the composite pattern elsewhere (`documents_tenant_id_id_unique`
  from `010_document_processing_mvp.sql:2`; `document_revisions` uses
  `FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)` in
  `016:26`), so the fence is feasible without new unique infrastructure (D-W4-02).
* There is no `lifecycle_state` column: `documents` stores `status`,
  `deletion_state` and `pre_trash_lifecycle` only
  (`migrations/016_document_revision_evidence_foundation.sql:7,56-57`,
  SQLite `004_…:6`). Both adapters therefore parse `pre_trash_lifecycle` once
  and assign it to the aggregate's two lifecycle fields
  (`document-postgres/src/unit_of_work.rs:127-129,145-147` and `:260,281`;
  `document-sqlite/src/unit_of_work.rs:291,316` and `:441,468`). Under the
  current transitions the two aggregate fields are provably always equal —
  `trash()` copies `lifecycle_state` into `pre_trash_lifecycle` without changing
  it, and `restore_from_trash()` copies it back
  (`crates/document/src/domain/entity.rs:560-590`) — so the collapse is
  reversible today, but the column carries no independent information and **no
  test drives a trash→restore round trip** (D-W4-07).

### Failure/retry/idempotency
Idempotent create is atomic with its audit and outbox side effects, and a
fingerprint mismatch returns `IdempotencyConflict` (contract suite). The replay
lookup was the weak point this wave (D-W4-01, fixed). `DocumentRepository::save`
writes no audit and no outbox row and has **no production caller** — only the
revision-history test uses it (D-W4-03), so R2 cannot claim "revision writes are
formally audited".

### Recovery/concurrency
Upload compensation exists at the composition root: if the metadata transaction
fails after the object write, the object is deleted. Until this wave the
compensation failure itself was discarded with `let _ =`; it now emits a
`warn` with `compensation = "upload_compensation_delete"`
(`apps/business-api/src/routes/upload.rs`). There is still no scan that would
find an orphan or a missing object (D-W4-08 / ST-W4-05).
Metadata-exists-but-object-missing is classified at the real worker boundary:
`StorageSource::read_source` maps `StorageError::NotFound` to
`ExtractionError::SourceNotFound` (code `source_not_found`), now pinned by a
unit test with a fake repository and a fake storage client.

### Security
`#[instrument]` recorded every non-skipped argument, so `S3Client::put_stream`
emitted `key = ObjectKey("tenants/…")` at debug level; `document-sqlite` printed
the raw driver error with `eprintln!`. Both are fixed and pinned by
`crates/object-storage/tests/log_hygiene.rs`, whose hand-written subscriber
captures every record whose target starts with `object_storage` and fails on
`tenants/`, `revisions/`, the bucket name or `source`. Residual: `StorageError`
is `thiserror`-free of a machine-readable kind and
`StorageError::NotFound(String)`'s `Display` embeds the key (ST-W4-08).

### Observability/audit
Covered by `2026-09-28-saas-align-observability.md`; cross-referenced only where
storage is the subject (ST-W4-02, ST-W4-08).

### Version/release model
`schema_version = 'v1'` is stamped on outbox rows, but the payload differs per
backend (PG includes `object_key`, SQLite does not) and the SQLite outbox has no
relay columns (`published_at`/attempts), while PG does (D-W4-10).

### Current test evidence
See §6.

## 4. Capability matrix

"核心声明?" = does the port/aggregate express it; "适配器实现?" = do S3 and/or
local adapters implement it; "真实环境验证?" = proven in this wave's lanes.

| Capability | 核心声明? | 适配器实现? | 真实环境验证? | Unsupported behaviour |
|---|---|---|---|---|
| single-shot put / stream put with length + content type + metadata | yes | S3 + local | yes (RustFS round trip) | n/a |
| head / exists / delete (idempotent) | yes | S3 + local | yes (delete of absent object is a no-op on the provider) | n/a |
| streaming read | yes (`open_stream`) | S3 + local | yes | `NotFound` → typed error |
| presigned GET with key scope + expiry | yes (`presign(key, secs)`) | S3 only | yes (200 valid / 403-404 tampered / 403-404 expired) | local adapter returns `Presign(...)`, never a path (ST-W4-03 fixed) |
| presign expiry bounds | yes (`MIN/MAX_PRESIGN_EXPIRES_SECS`) | enforced centrally before the provider call | yes (unit tests; out-of-range rejected before any request) | `StorageError::Presign` |
| multipart upload (create/upload_part/complete/abort) | **no** | no | provider probed: create+abort work, a legal 5 MiB `UploadPart` → `HTTP 500 InternalError` on RustFS | nothing to reject: the port has no such operation (ST-W4-01) |
| checksum on write / verify on read | no | no | provider accepts SDK-managed CRC32 and echoes it via `HeadObject` | not declarable (ST-W4-01) |
| conditional write/read (`If-Match`, `If-None-Match`) | no | no | provider honours `If-Match` and rejects a stale ETag | not declarable (ST-W4-01) |
| object versioning | no | no | provider reports no versioning status | not declarable (ST-W4-01) |
| listing beyond one page (`ListObjectsV2` continuation) | **no** (the port exposes no listing) | no | lane cleanup follows continuation tokens with `WAVE4_S3_CLEANUP_PAGE_SIZE=3`; verified on RustFS | cleanup-only concern; the port stays listing-free (ST-W4-01) |
| incomplete multipart upload discovery (`ListMultipartUploads` + abort) | **no** | no | provider lists an interrupted upload and accepts the abort (`uploads_aborted=1`, `remaining_uploads=0`); cleanup therefore reaches zero residue — **Round 3 calibration: this row is one upload on one page, so it never exercised multipart pagination; same-key multi-page multipart is verified in Round 3 (7 uploads / page size 3 / 3 pages), and the discovery judgement was tightened from "listing non-empty" to "the exact created key + upload-id is listed"** | if a provider did *not* list them, cleanup would print `PROVIDER-LIMITATION` instead of claiming an abort — **since Round 3 that branch fails the scenario instead of exiting green, after a scope-limited compensating abort recorded separately** |
| revision-scoped content reference | yes | both backends | yes on SQLite (row read back); PG by migration only | n/a |
| append-only revisions | enforced by DB triggers | both backends | SQLite: UPDATE/DELETE rejected by trigger; PG: definition only | trigger raises (SQLite `ABORT`, PG `55000`) |
| evidence/artifact binding to a revision | partially (processing job carries `document_revision_id`) | SQLite triggers; PG none | SQLite triggers fired in the lane-equivalent unit tests | application-level check exists for jobs (PG `lib.rs:526-527`) |

No row is generalised from one provider to "all S3-compatible providers": the
three provider columns are statements about RustFS `1.0.1-preview.11` only.

## 5. Findings

Severity vocabulary is PLAN-0014's: C0 blocker, C1 R2-required, C2 should,
C3 defer. Each finding lists 编号 / 严重度 / 代码位置 / 违反的契约 / 复现方式 /
影响 / 修复范围 / 测试 / 证据 / 剩余限制.

### C0 BLOCKER

None found. No path was identified that exposes another tenant's data or a
storage reference through a public contract in the current composition.

### C1 R2 REQUIRED — repaired in this wave

**D-W4-01 · C1 · 已修复**
* 代码位置：`crates/document-sqlite/src/unit_of_work.rs` replay lookup；`crates/document-postgres/src/unit_of_work.rs` 同位置
* 违反的契约：AGENTS §10「Repository … 均保持租户隔离」；PLAN-0003 幂等重放契约（重放只能返回本租户原对象）
* 复现方式：为租户 A、B 各建一份文档后执行 `UPDATE document_idempotency SET document_id = <A 的文档> WHERE tenant_id = <B> AND idempotency_key = 'shared-key'`（外键只指向 `documents(id)`，因此语句合法），再以 B 的身份重放同一 key
* 影响：幂等重放可把另一租户的文档元数据返回给请求者；这是数据泄露级别的多租户失效，但需要既有行被外部破坏才能触发（应用自身不会产生该形状），因此定级 C1 而非 C0
* 修复范围：JOIN 增加 `AND d.tenant_id = i.tenant_id`；rehydrate 之后再比较 `document.tenant_id() != command.document.tenant_id()` 并返回 `ApplicationPortError::Failed`（fail-closed，两个后端一致）
* 测试：`crates/document-sqlite/tests/tenant_fence.rs`（2 个用例：越界必须失败 / 同租户重放仍须成功）、`crates/document-postgres/tests/tenant_fence_pg.rs`
* 证据：修复前 `tenant_fence` 失败信息 “replay crossed the tenant boundary and returned tenant A document”；修复后 SQLite 2/2、PG `replay_never_returns_a_foreign_tenants_document` ok（`2026-09-28-wave4-gates/06-postgres-lane.log`）
* 剩余限制：**schema 层仍可写入越界行**（见 D-W4-02）。本轮只做读取侧 fail-closed，未新增迁移

**ST-W4-02 · C1 · 已修复**
* 代码位置：`crates/object-storage/src/client.rs`（`put_stream` 的 `#[instrument]`）；`crates/document-sqlite/src/unit_of_work.rs`（原 `eprintln!("document idempotency read failed: {error:?}")`）
* 违反的契约：AGENTS §10「错误响应、日志和 trace 不得泄漏底层错误…」；PLAN-0004「storage keys … never enter public DTOs or logs」
* 复现方式：`cargo test -p object-storage --test log_hygiene`（修复前失败并打印 `object_storage::client::put_stream key=ObjectKey("tenants/…")`）
* 影响：debug 级别日志（以及任何采集该日志的系统）获得完整对象键与桶名，等价于把内部存储拓扑复制到观测后端
* 修复范围：`#[instrument(skip(self, body, key, metadata))]` + 只记录 `content_length`；驱动错误文本改为 crate 内 `tracing::warn!` 固定消息
* 测试：`crates/object-storage/tests/log_hygiene.rs`（自建 subscriber，断言 `object_storage*` 目标不含 `tenants/`、`revisions/`、桶名、`source`，并断言该捕获确实看到第三方记录，避免空断言）；Round 2 补 `crates/object-storage/tests/sdk_log_hygiene.rs`：真实 SDK 调用经 `observability::init_tracing`，对桶名/对象键/凭证标识/签名 URL 作**负向断言**，并保留一个「无边界对照组」——对照组必须泄露，否则其他用例就是在什么也没测
* 证据：`2026-09-28-wave4-gates/03-test-workspace.log`；Round 2 见 `2026-09-28-wave4-round2-gates/`（含 `13-sdk-log-hygiene-run.log`）；测试文件即回归门禁
* 剩余限制：本仓断言覆盖「DEBUG/TRACE 记录不进入日志与 trace」。第三方在 `WARN`/`ERROR` 自由文本里输出桶/键仍未关闭 → 归 OBS-W4-07，不得据此声称日志脱敏已完成

**ST-W4-03 · C1 · 已修复**
* 代码位置：`crates/object-storage/src/client.rs` `LocalStorageClient::presign`
* 违反的契约：同一端口不得有两种语义不同的实现；PLAN-0003 不得对外暴露内部路径
* 复现方式：`cargo test -p object-storage --lib local_storage_refuses_presign_instead_of_exposing_a_path`（修复前返回 `Ok("file:///…")`）
* 影响：本地适配器返回的“presigned URL”绕过键作用域与过期语义，任何调用方把它交给客户端即等于泄露文件系统路径并授予无期限访问
* 修复范围：改为显式 `Err(StorageError::Presign("presigned URLs are not supported by the local filesystem adapter"))`
* 测试：上述单元测试 + 真实通道 presign 用例（键作用域 / 篡改 / 过期三种结果）
* 证据：`2026-09-28-wave4-storage-lane/03-s3-contract-lane-final.log`
* 剩余限制：无消费方（ST-W4-07），因此真正的端到端下载鉴权路径仍无法验证

**ST-W4-04 · C1 · 已修复**
* 代码位置：`crates/object-storage/src/client.rs` `S3Client::presign`
* 违反的契约：过期时间属于平台契约而非 provider 参数；SECURITY_ARCHITECTURE 对签名 URL 时限要求
* 复现方式：修复前任一 `u64` 都被原样转发（`presign(key, 999999999999)` 仍成功）；现在 `cargo test -p object-storage --lib presign_rejects_expiry_outside_the_platform_bounds`
* 影响：调用方（未来的下载路由）可签发事实上永不过期的 URL
* 修复范围：新增 `MIN_PRESIGN_EXPIRES_SECS = 1` / `MAX_PRESIGN_EXPIRES_SECS = 604_800` 与 `presign_expiry()` 校验，在构造请求前失败；常量由 crate 根导出
* 测试：3 个新单元测试（下界、上界、范围内成功构造）
* 证据：workspace 测试日志
* 剩余限制：上界取 7 天是 AWS SigV4 的硬上限；若未来改用自定义凭证轮换策略需重新评估

### C1 R2 REQUIRED — registered, NOT repaired (needs schema / contract / ADR decision)

**D-W4-02 · C1**：`document_idempotency` 缺少复合外键。位置：`crates/document-sqlite/migrations/001_document_persistence.sql`、PG 对应迁移。违反：AGENTS §7「正式写入必须具有租户…策略」+ 纵深防御。复现：D-W4-01 的 UPDATE 在两个后端都成功。影响：读取侧修复后仍允许损坏存在。修复范围：新迁移 + 复合外键 `FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)`（PG 需要 `documents_tenant_id_id_unique`，已存在；SQLite 需 `PRAGMA foreign_keys=ON` 且旧库需表重建）。测试：新增迁移测试 + 越界 INSERT 必须失败。证据：D-W4-01 通道。剩余限制：**属 schema 变更，须先出设计提案并按 ADR/manifest 规则登记**，本轮停在此边界。

**D-W4-06 · C1**：文档资源无授权调用路径。位置：`apps/business-api/src/routes/documents.rs`（对比 `admin.rs:182+`）。违反：AGENTS §10「身份、租户和权限在调用 Application 前建立」；SECURITY_ARCHITECTURE 默认拒绝。复现：静态检索（路由内无 `authorize`/policy 调用）+ 任一同租户主体可读他人文档（缺 ACL 概念）。影响：租户内无资源级授权，`document_links` 的 `role` 语义无法用于授权。修复范围：需要先定义 document 资源的授权契约（Action/Resource 命名）→ 属公开契约 + 权威所有者问题，须设计提案。测试：策略矩阵 + 跨主体读写拒绝。剩余限制：本轮未修改任何路由。

**D-W4-08 · C1**（含 ST-W4-05）：无元数据↔对象一致性扫描。位置：`crates/data-integrity`（无对象存储扫描器）、`apps/business-api/src/routes/upload.rs`（补偿失败仅告警）。违反：DATA_OWNERSHIP「文件与业务元数据通过状态机、checksum、补偿和**一致性扫描**协调」（AGENTS §7 同）。复现：制造孤儿对象（上传成功→元数据事务失败→补偿删除失败）后无任何手段发现。影响：孤立对象无限累积（成本+数据残留）；对象丢失只在读取时以 `source_not_found` 暴露。修复范围：新增扫描（list prefix vs `documents`/`document_revisions`，含分页、限速、只报告不删除），属新能力 → 设计提案。测试：注入两类不一致 + 扫描断言。剩余限制：provider `ListObjectsV2` 能力已在真实通道确认可用。

**D-W4-09 · C1**：PG 缺少 SQLite 已有的 processing-evidence → revision / artifact 绑定触发器。位置：`migrations/017_document_revision_evidence_integrity.sql`（仅 `document_revisions_immutable`）对比 `crates/document-sqlite/migrations/005_…:28-77`（4 个绑定触发器）。违反：同一不变量在开发后端强、生产后端弱；「evidence 必须绑定到确切 revision」。复现：静态迁移清单枚举（`SELECT tgname FROM pg_trigger` 等价检索）+ 若绕过应用直接写 evidence 行则无数据库拒绝。影响：生产库可产生指向错误 revision 的 evidence 行（应用层对 job 有校验：`crates/document-processing-postgres/src/lib.rs:526-527`）。修复范围：新增 PG 迁移镜像 SQLite 触发器 + 预检现存行。测试：PG 通道断言直接写入被拒。剩余限制：需先确认现存数据不违反（预检查询），属 schema 变更 → 设计提案。

**D-W4-11 · C1**：`document_links` 无 revision 绑定。位置：`migrations/016_document_revision_evidence_foundation.sql:65-77`（列仅到 `resource_id/role`）、`crates/document/src/domain/entity.rs` 的 `DocumentLink`。违反：证据/材料版本绑定（本轮 §四 明确要求）。复现：为文档挂 `role='evidence'` 链接后新增 revision，链接指向的实质内容已变而链接不变。影响：法律/合规语义下的“证据”可被静默换版。修复范围：新增可空 `document_revision_id` + 复合外键（PG 已有 `UNIQUE (tenant_id, document_id, id)` 可引用）+ 核心实体与 API 变更 → 公开契约变更，设计提案。测试：链接固定 revision 后跨版本读取断言。剩余限制：本轮只登记，不改契约。

### C2 SHOULD

* **D-W4-03**：`DocumentRepository::save` 不写 audit/outbox 且**无生产调用方**（仅 `revision_history.rs`）。位置：`crates/document/src/domain/repository.rs`、两个 `save` 实现。违反：API/事件标准「正式写入必须产生审计」；影响：R2 不能声称 revision 写入有正式事实。修复范围：与 create 路径对齐（审计 + outbox + 期望版本），并需要真正的调用方（更新用例）——属能力缺口而非缺陷，建议独立小波。测试：save 后审计链可验证 + 版本冲突拒绝。证据：`grep` 显示仅测试调用。剩余限制：无。
* **D-W4-04**：PG 的 revision 不可变只由迁移定义，未在真实通道断言（SQLite 已由 `content_reference.rs` 断言）。修复范围：把同样的 UPDATE/DELETE 拒绝断言加入 PG lane。测试位置建议：`tenant_fence_pg.rs` 或新 `content_reference_pg.rs`。剩余限制：本波 PG 通道已建立，成本很低，建议 R2 前完成。
* **D-W4-05 · 已修复**：`DocumentListQuery::count` 此前**零适配器覆盖**。位置：`crates/document-persistence-contracts/src/lib.rs`（新增 `verify_count`）。断言：count 必须等于同一 filter 分页取回的行数、无匹配必须为 0、跨租户必须为 0；两个后端均通过。
* **D-W4-07**：`pre_trash_lifecycle` 列在当前转换下与 `lifecycle_state` 恒等（见 §3），不携带独立信息，且 trash→restore 往返无测试。位置：`document-postgres/src/unit_of_work.rs:147,281`、`document-sqlite/src/unit_of_work.rs:316,468`。修复范围：补往返断言；若要让两列真正承载不同语义，需要先确定 lifecycle 的权威编码（设计问题）。
* **D-W4-10**：双后端 outbox 载荷与中继列不一致（PG 载荷含 `object_key`，SQLite 仅 `document_id`+`original_filename`；SQLite 无 `published_at`/attempts）但同标 `schema_version='v1'`。位置：`crates/document-sqlite/src/unit_of_work.rs:161` 与 PG 对应处。违反：API/事件标准「事件包含…版本」+ 同一版本必须同一载荷形状。修复范围：统一载荷 v1 形状（并移除载荷中的存储引用）。
* **D-W4-12 · C3**：SQLite 时间戳为 TEXT，`'…Z'` 与 `'…+00:00'` 混用会按字典序影响游标。位置：SQLite 迁移与写入格式化处。修复范围：统一写入格式或在读写边界规范化。
* **ST-W4-01 · DEFER**：端口未声明 multipart / checksum / 条件请求 / 版本化。见 §4；本轮明确**不实现**（无业务用例，PLAN-0004/0003 也未要求），但真实通道已记录 provider 行为，未来新增能力必须同时补 provider 矩阵行。
* **ST-W4-07 · DEFER**：presign 无 HTTP 消费者。与 D-W4-06 绑定：任何下载路由都必须先解决授权。
* **ST-W4-08**：`StorageError::NotFound(String)` 的 `Display` 内嵌对象键（`crates/object-storage/src/error.rs`），且无 `kind()`。位置同上。修复范围：错误改为结构体变体 + 机器可读 kind，`Display` 不含键。风险：错误类型是 crate 公开面 → 需要变更评审。
* **ST-W4-06 · BLOCKED**：`tests/minio_contract.rs` 从未执行——MinIO 开源二进制已停发（`dl.min.io` → HTTP 410，末版无 Windows 资产）。该 lane 属**既有**测试，本轮不得删除或改写；已在证据目录记录不可执行原因。Fake 未替代真实 provider 验证。

## 6. Test matrix

| # | 测试路径 | 用例 | 命令 | 环境 | Verdict |
|---|---|---|---|---|---|
| T1 | `crates/document-sqlite/tests/tenant_fence.rs` | `replay_never_returns_a_foreign_tenants_document`、`same_tenant_replay_still_returns_the_original_document` | `cargo test -p document-sqlite --all-features` | 临时 SQLite 文件 | pass（修复前 red） |
| T2 | `crates/document-postgres/tests/tenant_fence_pg.rs` | 同上（PG） | `cargo test -p document-postgres --all-features -- --include-ignored --test-threads=1` | 专用库 `plan0014_wave4`，PG 18.4，`DATABASE_URL` 指向该库 | pass（`06-postgres-lane.log`） |
| T3 | `crates/document-sqlite/tests/content_reference.rs` | 键形状/`change_reason`/`sha256`；revision UPDATE+DELETE 被触发器拒绝 | `cargo test -p document-sqlite --all-features` | SQLite | pass |
| T4 | `crates/document-persistence-contracts/src/lib.rs` `verify_count` | count==分页行数 / 无匹配=0 / 跨租户=0 | `cargo test -p document-sqlite --all-features`；PG：T2 命令 | SQLite + PG | pass 双后端 |
| T5 | `crates/object-storage/tests/log_hygiene.rs` | `storage_references_never_enter_the_log_stream`：`object_storage*` 目标日志不含键/桶/路径片段 | `cargo test -p object-storage --test log_hygiene --all-features` | 无外部依赖 | pass（修复前 red） |
| T5b | `crates/object-storage/tests/sdk_log_hygiene.rs` | 真实 SDK 经正式观测初始化的负向断言（桶/键/凭证/签名 URL）+ 无边界对照组必须泄露 | `cargo test -p object-storage --test sdk_log_hygiene --all-features` | 6 个隔离子进程；端点不可路由，不触碰主机信任库 | pass（Round 2 新增；首版曾因 harness 自身输出含 `signed_url` 误报，见 `2026-09-28-wave4-round2-gates/12-object-storage-unit-and-sdk-log-tests.log`） |
| T6 | `crates/object-storage` 单元测试 | presign 上下界（含「未调用 provider」的调用计数断言）、离线 transport 自检、transport 失败不得当作 NotFound、本地 presign 拒绝、键校验等 17 例 | `cargo test -p object-storage --lib --all-features` | 无外部依赖（参数拒绝类用例走无网络 transport，不读主机根证书库） | pass |
| T7 | `crates/object-storage/tests/s3_contract_lane.rs` | **契约步骤**：`lane_round_trips_object_metadata_and_payload`、`lane_reports_missing_object_as_not_found`、`lane_delete_of_absent_object_is_idempotent`、`lane_presigned_url_is_key_scoped_and_expires`、`lane_records_provider_capabilities`、`contract_step_reports_its_scope`（6 例；Round 1 的 `lane_leaves_no_objects_under_the_run_prefix` 与 `lane_deletes_the_dedicated_bucket_when_requested` 已移出本步骤） | `cargo test -p object-storage --test s3_contract_lane --all-features -- --ignored --test-threads=1 --nocapture` | RustFS 1.0.1-preview.11，专用桶 `plan0014-wave4-bucket` + 专用 prefix，RUN_ID `r2-20260928-065142` | **契约通过**：6 passed / 0 failed，exit 0（`2026-09-28-wave4-round2-storage-lane/01-contract-step.log`） |
| T7b | `crates/object-storage/tests/s3_lane_cleanup.rs` | **清理步骤**：分页删除 + 分页 abort 未完成 multipart upload + 「0 对象且 0 upload」终态断言 + RUN_ID 缺失/空白/带 `/` 拒绝 + 清理失败必须单独失败 + 拒删受保护桶 + prefix 外对象不受影响（7 例） | `cargo test -p object-storage --test s3_lane_cleanup --all-features -- --ignored --test-threads=1 --nocapture` | 同上，`WAVE4_S3_CLEANUP_PAGE_SIZE=3` | **清理通过**：7 passed / 0 failed，exit 0；重复执行仍 7 passed（`03-`/`04-final-cleanup*.log`）。**Round 3 校准**：本 target 的 multipart abort 计数在 Round 2 真实运行中始终为 1，故**不构成 multipart 分页证据**；对象侧 6 对象 / 2 页的多页清理成立。Round 3 另加 `listing_incomplete` 判定：列举未完成时清理不得报告零残留 |
| T7c | `crates/object-storage/tests/s3_lane_fault_injection.rs` | 故障场景 (a) 契约失败后仍清理且不改写契约失败、(b) 中断的 multipart upload 被发现并 abort、(c) prefix 外对象不受影响（4 例，含 no-op 的 `child_entry_point`） | `cargo test -p object-storage --test s3_lane_fault_injection --all-features -- --ignored --test-threads=1 --nocapture` | 同上；故障由子进程注入 | pass：4 passed / 0 failed，exit 0；`contract step exit=101` 与 `cleanup step exit=0` 同时记录（`02-cleanup-fault-scenarios.log`）。**Round 3 更新**：新增场景 (d) 同一 key 7 个未完成 upload / page_size 3 → **3 页**全部 abort（RUN_ID `r3-20260928-155500`，5 passed / 0 failed，`../2026-09-28-wave4-round3-storage-lane/02-cleanup-fault-scenarios.log`）；(b) 的发现判定改为精确配对 |
| T7d | `crates/object-storage/tests/s3_lane_pagination.rs`（Round 3 新增） | 分页与证据判定的确定性回归 19 例：同 key 7 upload 跨 3 页且 key marker 不变、多 key 跨页不重不漏、重复/循环游标、缺游标（含半对）、页数上限、objects 仅 `IsTruncated=false` 结束、缺/空/重复/循环 continuation token、未回答 `IsTruncated`、空列表与「列出别的 upload」不得算发现、列表失败不得报告为干净范围 | `cargo test -p object-storage --all-features --test s3_lane_pagination` | 无外部依赖：SDK HTTP connector 换成脚本化 transport（`http_client_fn`），断言实发 `key-marker` / `upload-id-marker` / `continuation-token` | **Round 3**：修复前 12 passed / **5 failed**（`../2026-09-28-wave4-round3-gates/attempts/00-red-pagination-before-fix.log`），修复后 **19 passed / 0 failed**，exit 0 |
| T8 | `apps/business-worker/src/main.rs` `missing_object_boundary` | 元数据存在但对象缺失 → `source_not_found`，文本不含键 | `cargo test -p business-worker --all-features` | fakes only | pass |
| T9 | `apps/business-api/src/metrics.rs` | duration 渲染为 histogram bucket，无 `{quantile=` | `cargo test -p business-api --all-features` | 进程内 | pass |
| T10 | 全量门禁 | fmt/check/clippy/test/architecture/diff | 见 `2026-09-28-wave4-gates/README.md` | 本机 | 全部 exit 0；workspace 759 passed / 0 failed / 59 ignored |
| T11 | `crates/object-storage/tests/minio_contract.rs` | 既有 MinIO lane | `-- --include-ignored` | **不可用**（MinIO 停发） | **BLOCKED / NOT RUN** |
| T12 | PG revision 不可变断言 | 建议新增（D-W4-04） | — | — | **NOT RUN（尚不存在）** |

## 7. R2 readiness

Wave 4 已满足：真实 PostgreSQL 与真实 S3-compatible 通道均已执行且自带清理；租户
隔离在读取侧 fail-closed；日志与错误面首次拥有可执行门禁。

R2 进入前必须关闭：D-W4-02、D-W4-06、D-W4-08/ST-W4-05、D-W4-09、D-W4-11（全部
C1），以及 D-W4-03（写入无审计）。其中 4 项需要 schema 或公开契约变更 → 需要设计
提案，不能在本波顺手实现。C1=0 目前不成立，因此 **R2 不成立**。

## 8. Reviewer questions

1. D-W4-01 的读取侧纵深防御是否足以作为 D-W4-02 迁移落地前的临时接受条件？
2. D-W4-06（文档资源无授权）是否应与 Identity/Policy 波合并，而不是留在 Document 波？
3. D-W4-11（`document_links` 无 revision 绑定）在法律证据语义下是否应升为 C0？
4. 用 RustFS 作为唯一真实 provider 是否可接受，还是要求 CI 增加 MinIO 容器镜像（官方停发后需镜像仓库）？

## 9. Verdict

**Self-verification only: READY FOR INDEPENDENT REVIEW / NOT INTEGRATED.** The
verdict row is left for the independent reviewer (PASS / PASS WITH C2-C3 / FAIL);
this wave does not self-sign a PASS.

### Blocking actions
D-W4-02, D-W4-06, D-W4-08/ST-W4-05, D-W4-09, D-W4-11（均需设计提案）；D-W4-03。

### Deferred actions
ST-W4-01（multipart/checksum/条件请求/版本化）、ST-W4-07（下载路由）、
D-W4-12、ST-W4-08。

### Accepted differences from references
* Paperless-ngx 的“消费目录即权威”不采纳：本仓权威是数据库 + 内容引用，目录只是入口。
* Documenso 的签署/投递状态机不复现：本仓无签署上下文，只采纳“业务记录与文件对象分离”。
* Mayan EDMS 与 Novu 本轮未能固定 revision（见 baseline §3），因此不引入任何行为结论；这不是拒绝，而是证据不可得。
* AWS S3 的 multipart/checksum/versioning 能力只在 provider 矩阵中登记，不进入端口，直到存在真实业务用例。
