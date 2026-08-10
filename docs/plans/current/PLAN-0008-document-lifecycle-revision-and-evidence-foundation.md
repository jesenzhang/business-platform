# PLAN-0008：Document Lifecycle、Revision 与 Evidence Foundation

文档 ID：PLAN-0008  
版本：0.1  
状态：Accepted Candidate / local solo fast-forward  
日期：2026-08-08  
基线：`35d1d01`  
目标 Candidate：`Accepted Candidate`（不自动合并到 `main`）

## 1. 目标和边界

本计划把 ADR-0019 之后各企业业务领域共同依赖的文档事实基础落到运行时代码：

- Document Management 拥有 Document、DocumentRevision、生命周期和 typed DocumentLink；
- Document Intelligence 拥有 ProcessingRun、ProcessingArtifact、Evidence 以及候选抽取结果；
- 现有 durable ProcessingJob 以兼容方式升级为精确绑定 DocumentRevision；
- PostgreSQL 是生产多 Worker 权威，SQLite 保持本地单进程适配器并与 PostgreSQL 做契约 parity；
- REST、CLI、MCP 继续使用 Application/query ports，公开 DTO 不泄漏 storage key、bucket、路径或原始内容。

本轮不实现 Party、Customer、Contract、Legal、Approval、Finance、Business Assurance、People、Analytics 的完整业务垂直切片，不新增空壳 crate 或微服务，也不把 ProcessingArtifact、Runtime Audit、Analytics 或 Agent 变成业务事实所有者。

## 2. 权威依据与依赖

适用基线和决策：

- ADR-0019：`docs/adr/ADR-0019-enterprise-business-domain-portfolio-and-cross-functional-assurance.md`
- `docs/architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md`
- `BACKEND_ARCHITECTURE_MANIFEST.md`、`SERVER_BACKEND_ARCHITECTURE.md`、`BOUNDED_CONTEXT_MAP.md`
- `DATA_OWNERSHIP_AND_CONSISTENCY.md`、`DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`
- `DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`
- `PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`、`SECURITY_ARCHITECTURE.md`
- `API_AND_EVENT_CONTRACT_STANDARD.md`、`ARCHITECTURE_FITNESS_FUNCTIONS.md`
- ADR-0010、ADR-0011、ADR-0012、ADR-0013

PLAN-0006 仍为 Proposed / NOT ACTIVE。本计划只更新其 dependency/precondition：Workspace page context 与 `document.processing_status.get` 的正式输入必须支持 `document_revision_id`；旧客户端的 `document_id`/aggregate version 只能作为 current revision 的兼容入口。PLAN-0006 不在本轮激活。

## 3. Architecture preflight

| 项目 | 决策 |
|---|---|
| Bounded Context | Document Management；Document Intelligence |
| 统一语言 | Document 是逻辑身份；DocumentRevision 是不可变内容版本；ProcessingRun 是一次处理尝试；Artifact 是处理产物；Evidence 是可追溯来源定位 |
| 权威数据所有者 | Document Management 拥有 Document/Revision/Link；Document Intelligence 拥有 Job/Run/Artifact/Evidence/Candidate；Runtime Governance 拥有 Audit/Finding/Repair |
| 状态归属 | Document lifecycle/deletion 属 Document；Job/Run/step/lease/retry 属 Document Intelligence；对象删除通过 Outbox/GC 执行，不以消息作为权威状态 |
| Application commands/queries | CreateDocument、CreateRevision、Archive/Restore、Trash、Purge request、Link/RemoveLink；Get current/history；Start ProcessingRun、Get artifact/evidence |
| 协作 | 同上下文本地事务；跨上下文通过版本化 integration event、幂等、outbox、worker 和 reconciliation |
| 幂等/并发 | Idempotency-Key；Document aggregate version 和 expected current revision；唯一 `(document_id, revision_no)`；过期或错误 fence fail closed |
| 身份/租户/授权 | 所有 command 从可信 TenantContext 建立 tenant/principal；resource reference 为受控 enum；Repository 和 object key 复核租户归属 |
| 质量场景 | 重复上传不覆盖历史；stale write 返回 conflict；旧 run 不能写入新 revision；purge 可重试且不与 SQL 伪分布式事务耦合；跨租户不可见 |
| 验证 | Domain、Fake ports、SQLite、PostgreSQL concurrency、migration/backfill、MinIO/E2E、OpenAPI/CLI/MCP compatibility、architecture fitness |

## 4. Gap Matrix（ADR-0019 集成后的实际切片）

| Context | Current | Target / Gap | 状态 |
|---|---|---|---|
| Party & Counterparty | crate 仅骨架，无正式 aggregate/persistence/API | Party identity、counterparty relationship、tenant-owned facts | Missing（后续计划） |
| Customer | crate 仅骨架 | Customer account 与 Party 的 ACL/引用 | Missing（后续计划） |
| Contract | crate/契约仅骨架；不拥有文件内容 | Contract aggregate 通过 DocumentLink 引用 revision | Missing；本轮只提供 Link seam |
| Document | `DocumentMetadata`、aggregate version、整数 `content_revision`、PG/SQLite、REST 已有 | Document 身份、current revision、独立 deletion state、purge request/outbox | Partial |
| Document Revision | 无一等持久化实体；对象引用可被当前字段表达 | immutable UUID revision、parent、checksum、revision-specific key、history/current unique | Missing |
| Document Intelligence | fixed pipeline、durable job/step/AI task/candidate/review 已有 | exact revision binding、独立 ProcessingRun、immutable Artifact/Evidence | Partial |
| Legal | 无正式事实模型 | legal matter/review/hold 通过 Document/Approval 应用协作 | Missing（后续计划） |
| Approval | 无正式 aggregate | approval decision/authority/versioned command | Missing（后续计划） |
| Finance | 无正式事实模型 | payment/settlement/reconciliation facts | Missing（后续计划） |
| Business Assurance & Reconciliation | Integrity Finding、Controlled Repair、Ledger 已有，但偏 Runtime/Processing | assurance case 独立拥有业务结论；复用 finding/repair 基础设施 | Partial；不在本轮扩展 |
| People & Performance | 无正式领域实现 | organization/employee/performance facts | Missing（后续计划） |
| Analytics / Report | 架构 baseline 和 query seam；不拥有业务事实 | semantic metrics、projection、report/export | Partial（只消费 revision/evidence events） |
| Runtime Audit | append-only/hash-chain Runtime Audit 已有 | 审计执行事实与 Business Assurance 分离 | Complete for current foundation |

## 5. 目标模型和不变量

```text
Document(id, tenant_id, lifecycle_state, deletion_state, current_revision_id, aggregate_version)
DocumentRevision(id, document_id, revision_no, parent_revision_id, source_object_ref,
                 sha256, content_type, size_bytes, original_filename, created_by,
                 created_at, change_reason)
ProcessingRun(document_revision_id, pipeline/parser/model metadata, status, timestamps)
ProcessingArtifact(processing_run_id, kind, storage_ref, checksum, schema_version)
Evidence(document_revision_id, processing_run_id, artifact_id, location, source_checksum)
```

DocumentRevision 内容不可更新；新内容只产生新 revision。`aggregate_version` 只表示 Document 状态/意图变更，不与 `revision_no` 混用。每个 Document 只能有一个 current revision。对象 key 固定为 `tenants/{tenant}/documents/{document}/revisions/{revision}/source`；MinIO/S3 VersionId 不是业务 revision ID。

生命周期拆为：`Lifecycle = Active | Archived` 与 `Deletion = Present | Trashed | PendingPurge | Purged`。`RemoveLink` 只删关系，`TrashDocument` 保留 revisions/runs/evidence/audit，`PurgeDocument` 需 retention、reference、legal/audit hold 和授权均通过，先 DB 标记并写 outbox，再由 Storage GC worker 删除、验证、重试和恢复。

## 6. API、事件和访问面

兼容 v1：现有 `document_id` 与 `content_revision` 保留为 deprecated/read compatibility 字段；新增 `revision_id`、`revision_no`、`is_current`。只有 `document_id` 的旧读取默认 current revision；替换、处理、恢复和 purge command 支持 `expected_document_version` 与 `expected_revision_id`。

新增/稳定化的业务意图：

- `POST /api/v1/documents/{id}/revisions`；`GET /api/v1/documents/{id}/revisions`
- `POST /api/v1/documents/{id}:archive`、`:restore`、`:trash`、`:purge`
- `POST/DELETE /api/v1/documents/{id}/links/{resource_kind}/{resource_id}`（仅受控 ResourceKind）
- Processing create 支持 `revision_id`，旧 `content_revision` 仅兼容解析 current revision。

版本化 outbox/integration facts：`document.revision.created.v1`、`document.lifecycle.changed.v1`、`document.purge.requested.v1`、`document.purge.completed.v1`、`document.processing_run.completed.v1`。事件只包含租户、业务 ID、版本、关联和 checksum，不含 raw text、secret、signed URL 或内部 storage detail。

## 7. Persistence、迁移与一致性

新增 migration 不修改旧 migration：

- PostgreSQL runtime catalog：`016_document_revision_evidence_foundation.sql`；
- SQLite document catalog：`004_document_revision_evidence_foundation.sql`；
- SQLite processing catalog：`007_document_revision_binding.sql`。

迁移建立 revision、link、run、artifact、evidence 表和 tenant-aware FK/unique constraints；为现有 Document 建 R1、回填 current revision，并将旧 processing job dual-write/backfill 到 revision。迁移和 reconciliation 检查 DB revision/object、current ownership、重复 revision number、job missing revision、artifact/evidence tenant mismatch、stuck purge 与 orphan object；发现写入现有 Integrity Finding/Controlled Repair，不创建第二套修复系统。

SQLite 使用显式 single-writer/`BEGIN IMMEDIATE`、单进程限制和连接池上限 4；PostgreSQL 使用本地事务、行锁/乐观版本。对象存储删除绝不在 SQL transaction 内同步完成。

## 8. 安全、质量属性与回滚

- 性能/容量：revision/history 使用 `(tenant_id, document_id, revision_no DESC)` keyset 索引；每次新增 revision O(1) 写入，历史查询按页；purge/GC 异步限速。
- 可用性/恢复：DB commit 与 outbox 原子；GC 至少一次、幂等，missing object 视为可验证状态；reconciliation 可重放；PG 为生产多 Worker authority。
- 安全/多租户：所有查询带 tenant predicate 和复合 FK；公开 DTO 不返回 object key；hold/reference/purge 授权 fail-closed。
- 可维护性/替换性：Domain 不依赖 SQLx/S3；Application 只依赖 ports；PG/SQLite contract parity；Run/Artifact/Evidence 不写正式业务 facts。
- 可观测性：记录稳定 operation/event IDs、revision/run/artifact IDs 和安全 failure class；不记录原始正文或 lease secret。
- 兼容性：REST/CLI/MCP 先 dual-read/dual-write；legacy `content_revision` 只在兼容窗口使用，后续计划再移除。

回滚：应用可回滚到只读 legacy contract；迁移采用 additive schema，不删除旧字段；未完成 purge 由 pending 状态和 outbox 恢复。完成回填和兼容窗口前不得删除 `content_revision`。

## 9. RED contract 与完成定义

先提交失败测试，再实现：Document R1/R2 immutable/current/stale conflict；archive/trash/restore 不改变内容 revision；purge 的 retention/reference/hold 拒绝；exact revision processing、multi-run、immutable artifact/evidence、stale result rejection；tenant isolation；PG concurrency；SQLite parity；migration backfill/reconciliation；REST/CLI/MCP DTO 不泄漏 storage details。

完成定义：领域和 adapter contract 全绿；PG/SQLite migration 可在现有数据执行且 manifest 更新；公开 API 兼容测试、OpenAPI、CLI、MCP 和架构 Fitness 全绿；完整门禁真实执行并记录；Candidate 有 commit SHA、状态为 `Accepted Candidate`，不自动 fast-forward 到 main。

## 10. Local Candidate evidence

- GitHub Actions real PostgreSQL + MinIO contract and multi-process E2E：final run `31352005264` at commit `70469be26cb009c23f1a77c1553947522ba82aed` PASS；Architecture Fitness、Format、Check、Clippy、Unit、Frontend、Playwright、CLI/MCP contracts 同 run PASS。详细证据见 `docs/reports/PLAN-0008-CI-EVIDENCE-AND-C-MIGRATION-REHEARSAL.md`。
- 本机 PostgreSQL/MinIO 仍保持 NOT RUN；本地安装的工具不作为验收证据。

- `cargo fmt --all -- --check`：PASS。
- `cargo check --workspace --all-targets --all-features`：PASS（仓库既有/环境 warning，无 error）。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：PASS（0 error）。
- `cargo test --workspace --all-features`：126 passed，32 ignored。
- `pwsh ./scripts/check-architecture.ps1`：PASS；`scripts/check-openapi.ps1`：PASS；`git diff --check`：PASS。
- PostgreSQL、MinIO、真实迁移升级和跨进程 E2E：NOT RUN；对应测试因缺少真实设施保持 `#[ignore]`，不得推断为 PASS。

## 11. 后续建议（本轮不执行）

Party & Counterparty → Customer → Contract + DocumentLink → Legal/Approval → Finance/Reconciliation → Business Assurance → Formal Report/Analytics → People & Performance。
