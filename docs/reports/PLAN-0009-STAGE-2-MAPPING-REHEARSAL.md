# PLAN-0009 Stage 2 — Deterministic Mapping and Isolated Target Rehearsal

状态：Coordinator Verified；独立 Reviewer attempt 1 = FAIL；repair candidate pending independent re-review  
日期：2026-08-10  
原始实现候选：`dc4c492`；repair candidate：`4a6ef75`  
范围：Stage 1 frozen manifest → isolated target only；不执行 production migration，不激活 PLAN-0006

## 1. 结论

真实 C 项目的 120 条 frozen sample 已完成 Stage 2 首次运行与 replay。Stage 2
重新读取并校验 Stage 1 manifest、源数据库 fingerprint 和三个 physical-root
fingerprint；相同输入生成相同 mapping digest、candidate UUID 和分类。

本次真实样本的 `Exact=0`。因此没有写入任何 Document、DocumentRevision、
DocumentLink、ProcessingRun、ProcessingArtifact 或 Evidence 事实；这符合
`Exact only` 和 fail-closed 规则。`Probable=1` 进入 review，其他不确定或无效
记录进入 quarantine，未被自动修复、合并或升级。

## 2. 固定输入与边界

- 输入：Stage 1 `plan-0009.stage-1.inventory.v7` frozen manifest，120 records。
- target：`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-2-rehearsal-v1`。
- target SQLite：`db/document-management.sqlite`；target objects：`objects/`。
- C 项目、其数据库和 storage roots 通过 `RehearsalBoundary` 以 read-only handle
  访问；target 必须位于 isolation root 内且与 source disjoint。
- target 使用单连接 SQLite、`document-sqlite` migrations 和
  `document-processing-sqlite` migrations；写事务使用 `BEGIN IMMEDIATE`。
- CLI 只接受 `inventory` 或 `stage2`；任何 `production` 参数都 fail closed。

## 3. Mapping 与写入规则

每条 mapping 输出以下安全字段：source contract id、selection rank、classification、
reason code、lineage counts、root label、relative-path SHA-256、observed/expected
SHA-256、candidate target UUID 和 target object-ref digest。manifest 不写 source
absolute path、明文 relative path、raw text、database URL、secret、signed URL 或
内部 target object key。

规则如下：

- `Exact` 且只有一个经过 SHA-256 验证的 evidence 才允许自动 materialize；复制
  source bytes 前再次通过 read-only boundary 和 frozen path/size/checksum 校验。
- `Probable` 只进入 `manual_review`；当前唯一 Probable 未写入 Document 事实。
- `Ambiguous`、`Conflict`、`Orphan`、`Missing`、`Rejected` 进入 `quarantine`。
- Document 使用应用层 `CreateDocumentMetadata` 与 SQLite UoW；DocumentLink、
  ProcessingRun、ProcessingArtifact、Evidence 先经领域构造器校验，再由隔离
  adapter 以幂等写入；deterministic identity constructor 不改变普通创建路径。
- target mapping rows 以 `(manifest_sha256, source_contract_id)` 幂等；摘要文件和
  replay audit 发现 digest/分类/计数冲突时 fail closed。
- SQLite evidence 复合外键所需的 tenant-scoped unique indexes 在 target adapter
  初始化时建立，避免 `foreign_key_check` 把合法的复合父键误判为 schema mismatch。

## Repair 1 after independent review attempt 1

The first independent review identified two latent Exact-lane defects and one
evidence-wording defect. Repair candidate `4a6ef75`:

- stores an Exact object below the same canonical revision-scoped key used by
  `DocumentRevision`, and binds `ProcessingArtifact.storage_ref` to that key;
- derives document, link and processing timestamps from the frozen source
  contract identity, skips already-materialized mapping rows, and checks full
  persisted row equivalence on replay;
- records the Stage 0 source-boundary evidence limitation explicitly instead
  of treating a narrative dirty-tree baseline as an independently provable
  post-run snapshot.

The clean target was recreated only at the exact isolated Stage 2 target path,
then executed once and replayed twice with the same Stage 1 manifest. The
result below is from that repaired candidate. Independent re-review is still
required.

## 4. 真实运行证据

首次运行：

```text
stage=2 status=frozen selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119 replayed=false mapping_plan_sha256=09b43c8e1cc36b337bdc704d4d9c00ccdbd36e49ce2c0846f8eab129f3378464
```

Replay：

```text
stage=2 status=replayed selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119 replayed=true mapping_plan_sha256=09b43c8e1cc36b337bdc704d4d9c00ccdbd36e49ce2c0846f8eab129f3378464
```

Target mapping census：

| classification | disposition | rows | materialized |
|---|---|---:|---:|
| Ambiguous | quarantine | 89 | 0 |
| Orphan | quarantine | 29 | 0 |
| Probable | manual_review | 1 | 0 |
| Rejected | quarantine | 1 | 0 |

Target business entity counts：

| entity | count |
|---|---:|
| Document | 0 |
| DocumentRevision | 0 |
| DocumentLink | 0 |
| ProcessingRun | 0 |
| ProcessingArtifact | 0 |
| Evidence | 0 |

The zero counts are an observed result of `Exact=0`, not a skipped stage and not a
fabricated success claim.

## 5. Digest、回放与安全验收

- mapping plan records：120。
- canonical mapping digest recomputation：PASS。
- written-file bytes digest and sidecar：PASS。
- audit `mapping_plan_sha256` binding：PASS。
- replay audit `replay_count=2`，`last_status=replayed`。
- target SQLite `integrity_check=ok`。
- target SQLite `foreign_key_check` violations：0。
- target output path/content scan for source absolute roots, source database name,
  local-test env name and known business-name patterns：0 matches。
- The `RehearsalBoundary` source handles are read-only and the target/source
  guard passed. Stage 0 recorded a coordinator-observed dirty baseline, but it
  did not capture a cryptographic pre/post status artifact; this report makes
  no stronger independent no-write claim. A production decision must require
  an explicit pre/post source snapshot.

## 6. 测试与门禁

已运行：

- `cargo fmt --all -- --check`：PASS。
- `cargo check -p plan-0009-rehearsal --all-targets`：PASS。
- `cargo test -p plan-0009-rehearsal -p document -p document-processing --all-targets`：45 passed。
- 真实 Stage 2 frozen + replay：PASS。
- target digest、audit、SQLite integrity/FK 和敏感字段扫描：PASS。

未完成或未运行：

- full workspace `cargo clippy --workspace --all-targets --all-features -- -D warnings`：
  NOT RUN；本次用户明确允许 focused/small tests，且当前 Stage 1 app 仍有既有
  clippy baseline warnings（长函数、测试 expect 等），未将其伪称为通过。
- Stage 2 repair candidate is awaiting independent re-review; it is not yet
  approved.

## Review ledger

- Independent Reviewer attempt 1, candidate `9cbb450` / implementation
  `dc4c492`: `FAIL`.
  - HIGH: Exact object path and revision source reference diverged.
  - HIGH: Exact replay used wall-clock timestamps and only partial row checks.
  - LOW: source no-write wording exceeded the committed evidence.
- Coordinator repair candidate: `4a6ef75`; focused tests and repaired real
  Stage 2 frozen/replay rerun passed. Independent re-review is pending.

## 7. 进入 Stage 3 的条件

Stage 3 只能在独立 Reviewer 对 repair candidate `4a6ef75` 和本报告返回 PASS 后开始。当前仍禁止：

- production migration；
- 任何 C source write；
- Probable/ambiguous/conflict/orphan/missing/rejected 自动升级；
- metadata-only contract 虚构 revision；
- 激活 PLAN-0006 或扩大 PLAN-0008 产品边界。
