# PLAN-0008 CI Evidence and C-Project Migration Rehearsal

Date: 2026-08-10  
Branch: `codex/plan-0008-document-lifecycle-revision-and-evidence-foundation`  
Base: `35d1d01`  
Scope: Accepted Candidate closeout evidence and read-only migration design. No
PLAN-0008 product capability was added by this report.

## 1. PostgreSQL + MinIO CI evidence

The repository already had a GitHub Actions integration job. It provisions
PostgreSQL 16.4 and MinIO, creates a private test bucket, applies migrations,
runs the ignored infrastructure contracts, runs the multi-process E2E script,
and then runs Architecture Fitness and OpenAPI checks:

- Workflow: `.github/workflows/ci.yml:124-198`.
- PLAN-0008 database/object contract:
  `apps/business-api/tests/plan_0008_postgres_minio.rs`.
- Multi-process API/worker contract:
  `scripts/test-postgres-minio-multiprocess.sh`.

Run `31350822824` was green for commit
`414c88a4a77e0594358c98f95c70651362f37c3f`:
[GitHub Actions run 31350822824](https://github.com/jesenzhang/business-platform/actions/runs/31350822824).
The run passed the PostgreSQL + MinIO integration, Architecture Fitness,
Format, Check, Clippy, Unit tests, Frontend checks, Frontend Playwright smoke,
and CLI/MCP contracts jobs. The additive real multipart upload assertion in
this candidate must be confirmed by the follow-up CI run for the final push.

Local PostgreSQL and MinIO are **NOT RUN**. The local executables are not used
as acceptance evidence; only the GitHub-hosted service containers count.

### Contract matrix

| Invariant | Real CI assertion | Result / boundary |
| --- | --- | --- |
| Stable idempotent upload | Multipart HTTP upload uses the same idempotency key twice; the application contract also checks the same immutable object/revision identity. | The new multipart contract must pass in the follow-up CI run. Existing green run covered the lower-level idempotent create/object path. |
| Revision creation and current switch | R1 is created, R2 is created from the aggregate, and current revision changes to R2. | PASS in run `31350822824`. |
| Historical revision immutability | Direct PostgreSQL update of R1 is rejected by the database integrity fence. | PASS. |
| Exact processing binding | The processing job stores the selected `document_revision_id` and `content_revision`; `ProcessingRun` is started for the same revision. | PASS. |
| Artifact/evidence binding | Artifact points to the run; evidence points to revision, run, and artifact; a cross-revision evidence insert is rejected. | PASS. This is a database identity/FK replay assertion, not a claim that a full review use-case replay was exercised through every application port. |
| Object bytes and metadata/hash | MinIO `HEAD` and `GET` are compared with content length, content type, SHA-256 metadata, and PostgreSQL revision/artifact checksum fields. | PASS. |
| Replay cardinality | Replayed document creation, processing job, run, artifact, and evidence identities remain single rows. | PASS for the exercised persistence identities; artifact/evidence replay uses explicit idempotent SQL conflict handling in the test fixture. |
| Concurrent/stale version rejection | Two writers with one aggregate version race; exactly one save succeeds and the loser receives `RepositoryError::Conflict`. | PASS. |
| Retry convergence | A transient step failure returns the job to `queued`, increments the attempt, and permits a new worker claim. | PASS. |
| Crash recovery and process convergence | Business Worker and AI Worker are killed while leased work is active; fresh processes reclaim the work. Review is replayed and 20 jobs converge to terminal reviewable/succeeded states with one candidate, one review, and one successful AI task for the crash-recovery job. | PASS in run `31350822824`. |

### PostgreSQL/MinIO consistency boundary

PostgreSQL and MinIO do not participate in one distributed transaction. The
contract explicitly demonstrates the failure shape: a database transaction
can roll back while an already-written MinIO object remains present and has no
database revision row. The current proven handling is immediate, best-effort
caller compensation (`delete`), plus the same compensation for the losing
stale writer's object candidate. The test does **not** claim PostgreSQL+MinIO
atomicity and does **not** claim that an automatic object-store scan, GC
worker, or retrying reconciliation loop was exercised. Those remain an
operational reconciliation obligation, outside the product scope of this
closeout.

Direct evidence: `apps/business-api/tests/plan_0008_postgres_minio.rs:221-271`
and `:559-618`; process evidence:
`scripts/test-postgres-minio-multiprocess.sh:135-237`.

## 2. C project read-only inventory

The C project at `F:\Workspace\git_repo\contract_management` and its source
data were treated as read-only. No source file, original SQLite database,
original physical file, or destructive migration was touched. All analyzer
outputs are under `.scratch/c-project-migration-rehearsal/`.

### Database and storage

- Database: SQLite, Alembic revision `0057`, 45 tables.
- Database file: `D:\contract_data_test\db\contract_management.db`.
- Managed root: `D:\contract_data_test\datasets`.
- Configured external root: `D:\contract_data_test\2026年合同1`; no current DB
  row used `storage_location = external`.
- Path rules: `contracts/{id}/versions/v{N}/source|parsed/...`,
  `tasks/{yyyy}/{mm}/{uuid}/...`, `_upload_inbox/{batch_id}/...`,
  `_quarantine/...`, and `derived/...`.

### Schema and lineage

The authoritative legacy chain is:

```text
contracts -> contract_versions -> contract_artifacts
       |             |
       |             +-> contract_parse_jobs -> extraction_results
       |
       +-> contract_ingestion_tasks -> task_files -> task_results
       +-> contract_attachments -> optional ingestion_task_file
```

Relevant schema facts are recorded in
`.scratch/c-project-migration-rehearsal/schema-storage-lineage-appendix.md`:

- `contracts` owns contract identity and legacy pointers.
- `contract_versions` is the revision candidate and owns version metadata.
- `contract_artifacts` owns source/raw/parsed/OCR/preview/extracted rows and
  optional parse-job breadcrumbs.
- `contract_attachments` owns related files, replacement chains, current flags,
  and optional task-file links.
- `contract_parse_jobs` is the strongest legacy processing-run candidate.
- `contract_ingestion_tasks` and task files/results represent staged and batch
  execution paths.
- `extraction_results` points to contract/version and source/raw result file
  IDs; 32 rows have dangling file pointers.
- `operation_logs` is audit association, not authoritative relational lineage.

There is no dedicated OCR table or dedicated LLM result table. OCR is encoded
by parser identity (`ocr_parser`), `OCR_JSON`/`RAW_JSON` artifacts, parsed JSON,
and metadata. LLM extraction is encoded by parse-job/enrichment state and
`metadata_json` fields such as `llm_fields`, source artifact IDs, text/window
metadata, confidence, and source locations. JSON breadcrumbs such as
`task_storage_key` and `migrated_from_task_id` are supporting evidence, not
first-class foreign keys.

Three import paths were identified:

1. Single-file or archive ingestion: task workspace, task-file source, worker,
   then contract/version/artifacts.
2. Multi-file batch: byte-only `_upload_inbox` landing, manifest and grouping,
   explicit confirmation, then one or more ingestion tasks.
3. Direct contract attachment: attachment file plus `ContractAttachment`; an
   optional task can parse it, but its candidates remain non-authoritative.

### Inventory counts and definitions

| Measure | Count | Definition |
| --- | ---: | --- |
| Contracts | 1,492 | `contracts` rows |
| Contract versions | 960 | `contract_versions` rows |
| File records | 7,230 | 3,687 contract artifacts + 111 attachments + 3,418 contract-task files + 14 acceptance-task files |
| Physical files | 15,976 | 12,407 managed + 3,569 external files, SHA-256 scanned |
| Attachments | 111 | `contract_attachments` rows |
| OCR-marked file records | 206 | 69 contract artifacts + 133 contract-task files + 4 acceptance-task files; file-level, not execution-level |
| LLM parse jobs/results | 505 / 505 | `contract_parse_jobs` / `extraction_results`; no dedicated LLM table |
| Missing DB-referenced files | 0 | all 7,230 DB path references resolved physically |
| Managed live orphans | 2,063 | physical managed files outside DB references, excluding transient prefixes |
| Managed transient orphans | 3,129 | `_upload_inbox`, `_quarantine`, and `derived` physical files outside active references |
| External orphans | 3,569 | external-root physical files with no current DB reference |
| Duplicate-content groups | 5,307 | SHA-256 groups involving 12,539 physical files |
| Multi-reference path groups | 15 | 30 DB rows point to shared physical paths |
| Broken relations | 32 | all in `extraction_results`, dangling source/raw file IDs |
| OCR without resolvable revision | 16 | OCR lineage cannot resolve to a revision |
| LLM results without processing lineage | 0 | all 505 sampled LLM results had resolvable lineage under current analyzer rules |
| Contracts without versions | 664 | metadata-only contract rows |
| Versions without artifacts | 37 | revision metadata has no resolvable artifact |

The representative manifest contains 120 contracts and covers ordinary
single-file cases, multiple versions, scanned pages, multiple attachments,
OCR/LLM results, metadata-only contracts, and known anomaly cases. It is a
sample manifest, not a migration instruction or target write set.

The likely mechanism behind the known bad associations is legacy pointer drift
or cleanup of artifact rows without a matching extraction-result reconciliation:
the evidence is dangling `source_file_id`/`raw_result_file_id`, task/payload
breadcrumbs, polymorphic operation-log IDs, and attachment replacement links.
This is a reasoned failure-mode hypothesis, not a proven historical root cause.

## 3. Legacy -> PLAN-0008 canonical mapping

| Legacy source / condition | Classification | Target |
| --- | --- | --- |
| `contracts` | Exact | `Document` |
| `contract_versions` with resolvable parent/source | Exact | `DocumentRevision` |
| `contract_artifacts` source/raw/parsed/OCR/preview/extracted | Probable, checksum-first | `ProcessingArtifact`; selected source may support `DocumentRevision` |
| `contract_attachments` with resolvable bytes | Probable | `DocumentLink` + `Evidence`; do not silently promote to business facts |
| `contract_parse_jobs` / ingestion tasks | Probable | `ProcessingRun` bound to the resolved revision |
| `extraction_results` / task results | Probable only with complete revision/run/artifact lineage | `Evidence` as candidate/evidence, never formal fact |
| Physical file with no DB reference | Orphan | Quarantine for disposition; no automatic target row |
| DB reference with no physical file | Missing | Hold out of authoritative import until recovered or explicitly classified |
| Multiple plausible checksum/path/source matches | Ambiguous | Manual disposition; never auto-repair |
| FK, checksum, tenant, or revision mismatch | Conflict | Reject/repair review; never auto-repair |
| Unsafe/unresolvable row after review | Rejected | Preserve provenance and exclusion reason |
| Contract with no version | Orphan / Missing | Metadata-only `Document` disposition requires separate policy; do not invent a revision |
| Version with no artifact | Missing | Recover source or keep as explicitly flagged metadata-only candidate |

No blocking PLAN-0008 model gap was found for the successful lineage. The
rehearsal does require migration policy for tenant assignment, 664 metadata-only
contracts, checksum-first deduplication, duplicate physical references,
transient/quarantine/external files, and JSON breadcrumb validation. These are
migration gates, not new PLAN-0008 product features.

## 4. Exact next Migration Rehearsal phase

1. Freeze a read-only schema/filesystem snapshot and a deterministic manifest
   containing legacy IDs, normalized path, size, MIME, SHA-256, and root class.
2. Generate deterministic target UUID maps for `Document` and resolvable
   `DocumentRevision` without writing target business data.
3. Resolve every artifact/task/result edge and emit one classification record:
   `Exact`, `Probable`, `Ambiguous`, `Conflict`, `Orphan`, `Missing`, or
   `Rejected`; stop automation for ambiguous/conflict edges.
4. Materialize only the 120-contract sample into an isolated SQLite target and
   isolated local object root under `.scratch/c-project-migration-rehearsal/`.
   This is the first target write, never a write to C project paths.
5. Run replay/idempotency, SHA-256/object metadata, multi-reference,
   revision/run binding, stale-version, missing/orphan, and reconciliation
   assertions against the isolated target.
6. Review the sample report, explicitly disposition all 664 no-version
   contracts and all 32 broken extraction relations, then obtain approval
   before expanding beyond the sample.

Do not begin production migration, do not modify the C project, and do not
activate PLAN-0006 as part of this rehearsal.

## 5. Detailed local evidence

- `.scratch/c-project-migration-rehearsal/inventory-and-migration-design.md`
- `.scratch/c-project-migration-rehearsal/inventory-and-migration-design.json`
- `.scratch/c-project-migration-rehearsal/schema-storage-lineage-appendix.md`
- `.scratch/c-project-migration-rehearsal/sample-contract-manifest.json`

These scratch artifacts remain outside the C project and are intentionally not
used as production migration input.
