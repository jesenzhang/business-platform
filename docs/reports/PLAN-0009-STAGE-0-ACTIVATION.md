# PLAN-0009 Stage 0 — Activation and Isolation Evidence

Status: **Activated — rehearsal only**  
Recorded: 2026-08-10 (Asia/Shanghai)  
Real business-platform base: `654fe83d82107d899079d20e5fef8aaf4d5431b8`  
Feature branch: `codex/plan-0009-c-migration-rehearsal`

## Decision

PLAN-0009 is activated only for a deterministic, read-only rehearsal. This is not
production migration and does not authorize a write to the C project. PLAN-0008 is
already Integrated; PLAN-0006 remains Proposed / NOT ACTIVE. No Workspace, Agent
Runtime, Generated App, general Workflow, arbitrary SQL/Shell/HTTP tool, or new
business bounded context is activated by this record.

## Source boundary

The legacy source boundary is the complete tree:

`F:\Workspace\git_repo\contract_management`

The source repository was observed at C `master` HEAD
`b864fad6376ade03a5a4584a5716fa44dc2f91fb`. Its working tree already contained
user-owned uncommitted changes before this rehearsal (backend, frontend and
deliverables). That dirty state is a baseline fact, not a migration change. The
rehearsal must never clean, checkout, commit, migrate, upload, delete, rename, or
rewrite anything below this path.

The source database and storage observations were read-only:

| Source | Observation |
|---|---|
| `contract_management.db` | 4 KiB SQLite file with no user tables |
| `backend/contract_management.db` | SQLite schema at Alembic `0044`; 674 contracts and no contract versions/artifacts |
| `contract_management1.db` | SQLite schema at Alembic `0044`; 1,364 contracts, 755 versions, 4,459 artifacts, 87 attachments, 300 parse jobs and 300 extraction results |
| `backend/contract_datasets` | Existing physical source tree; 414 files, about 2.89 GB by read-only enumeration |
| `backend/contract_data` | Existing auxiliary source tree; 43 files, about 60 KiB by read-only enumeration |
| `contract_importer/output`, `output` | Existing manifest/import evidence; treated as read-only source material |
| `downloaded_yongyou_exports` and `backend/downloaded_yongyou_exports` | Existing export inputs; treated as read-only source material |

The full source database candidate passed SQLite `integrity_check` and
`foreign_key_check` during observation. The row/object count difference is an
unresolved data-quality signal, not evidence that missing objects may be guessed.
Stage 1 must classify every reference using physical SHA-256 and preserve the
original source record evidence.

## Three legacy import paths

The analyzer must cover all three observed ways data entered the C system:

1. The standalone `contract_importer` path: Excel parse → file discovery →
   `contracts.json`/`manifest.json`, followed by either the server-side
   `backend/utils/import_from_contract_importer.py` consumer or the upload flow.
   Its `--in-place` mode records external relative paths; upload mode records
   legacy uploaded paths.
2. The backend directory-import path:
   `backend/utils/import_contracts_from_directory.py`, which combines the legacy
   template parser and directory discovery before calling the importer. The
   wrapper is exposed by `scripts/import_contracts.ps1` and
   `scripts/import_contracts.sh`.
3. The application ingestion path: upload-created
   `contract_ingestion_tasks`/`contract_ingestion_task_files` with
   `UPLOAD_CONTRACT`, `UPLOAD_SCAN`, and `UPLOAD_ATTACHMENT` records, followed by
   `contract_ingestions`, `contract_versions`, artifacts and parse results.

The analyzer must not invoke any of these import paths. It reads their database
records, manifests and physical objects directly through a read-only adapter.

## OCR and LLM lineage

Lineage is available across `contract_ingestion_task_files`,
`contract_ingestion_task_results`, `extraction_results`, `contract_parse_jobs`,
and `contract_artifacts`. The source schema carries source/raw/parsed/extracted
file IDs, parser names, `metadata_json`, `sha256`, `object_key`, and relative
paths. OCR-related artifacts include `ocr.json`/`ocr.raw.json` and OCR parser
metadata; extraction metadata can include `parser_mode` and `llm_fields`.
The target mapping must bind OCR to an exact DocumentRevision and LLM-derived
artifacts to an exact ProcessingRun and revision. Missing or conflicting lineage
is quarantine, never an automatic upgrade.

## Isolated target and safety guard

All generated target state must be below:

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810`

The target consists only of an isolated SQLite database, isolated LocalStorage
objects, manifests, quarantine evidence and reports. The safety boundary rejects
production mode, source/target overlap, isolation roots under the source, and
targets outside the configured isolation root. Source file handles are read-only
and cannot create or open a write path. The guard is covered by focused unit
tests and must be called before inventory or mapping work.

## Frozen validation rules

- Physical identity is SHA-256 of bytes plus size; legacy ID/path is only evidence.
- Deterministic IDs, object keys, classification and lineage must derive from the
  frozen manifest, not wall-clock time, traversal order or database row order.
- Only `Exact` may enter automatic isolated rehearsal. `Probable` remains in the
  manual-review set; `Ambiguous`, `Conflict`, `Orphan`, `Missing` and `Rejected`
  are quarantined.
- Metadata-only contracts do not receive a fabricated revision. Orphan objects do
  not become business facts. Historical revisions remain immutable.
- OCR must reference one exact revision. LLM artifacts must reference one exact
  processing run and revision. Every quarantine item preserves source table,
  record ID, path evidence, checksum evidence and the reason.
- A clean second run with the same frozen manifest must produce equivalent target
  identity, object keys, classifications and quarantine output.
- Raw text, document bodies, credentials, signed URLs, internal storage keys and
  database URLs are excluded from public DTOs and logs.

## Rollback and cleanup

There is no source rollback because no source write is permitted. A rehearsal
rollback means stopping the process, retaining the manifest/evidence hash, and
removing or moving only the isolated target directory after evidence is archived.
Cleanup must use the exact target path above and a fresh clean target is required
for replay. If a target or source boundary check fails, the process exits before
opening any source file. Production migration remains a separate future plan and
is not implied by a rehearsal PASS.

## Stage 0 exit criteria

- Real main base and the source dirty baseline are recorded.
- Source locations, schema version, three import paths, OCR/LLM lineage and known
  data-quality signals are recorded.
- The source/target guard tests pass.
- PLAN-0009 is Active / Rehearsal Only; PLAN-0008 remains Integrated and PLAN-0006
  remains Proposed / NOT ACTIVE.
- No file under the C source tree is changed by this Goal.

Stage 1 may start only after an independent reviewer returns `PASS` for the exact
Stage 0 candidate.
