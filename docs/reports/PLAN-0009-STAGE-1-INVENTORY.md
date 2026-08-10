# PLAN-0009 Stage 1 — read-only inventory and frozen manifest

Status: **candidate — independent Reviewer pending**.

This report records the real C-project rehearsal only. It does not authorize a
production migration and it does not activate PLAN-0006.

## Source boundary

The run used only the user-provided local-test tuple:

- env: `F:\Workspace\git_repo\contract_management\backend\.env.local-test`
- `DATA_ROOT`: `D:\contract_data_test`
- authoritative SQLite: `$DATA_ROOT/db/contract_management.db`
- physical roots: `$DATA_ROOT/datasets`, `$DATA_ROOT/2026年合同1`, `$DATA_ROOT/2026年合同`
- isolated output: `F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v8`

The SQLite adapter uses one read-only connection. The application rejects a
production argument and rejects targets outside the fixed isolated root. No C
application, upload, import, migration, or source-database write was invoked.

The source snapshot observed by the run was Alembic `0057`, journal mode
`wal`, SQLite `integrity_check=ok`, and 32 retained `foreign_key_check`
violations. The database and physical-root byte counts are recorded in the
manifest; the retained foreign-key anomaly is not repaired by rehearsal code.
There is no independent cryptographic pre/post source snapshot in this stage,
so this report does not claim one.

## Deterministic implementation

The adapter reads fixed, ordered queries for contracts, versions, attachments,
ingestions, ingestion tasks/files/results, artifacts, parse jobs, and extraction
results. The application owns the coverage-first selector; the shared
`legacy-migration-rehearsal` crate owns the production rejection/isolation
boundary and classification vocabulary. A focused selector test proves that
coverage features are selected before ordinary ID fill.

The selected set is fixed at 120. The selection order is classification and
lineage coverage, then the positive source flag, then `contracts.id ASC` as a
tie-break. Legacy non-SHA-256 fingerprints remain classified as legacy data;
they are never treated as SHA-256.

Manifest records preserve `source_table` and `source_record_id` for the owning
contract, and every physical `evidence` entry preserves a deterministic source
table/row reference plus only safe path metadata (root label, relative-path
SHA-256, depth, extension, size, and optional content fingerprints). Raw source
names, text, absolute paths, URLs, credentials, and signed URLs are not written
to the manifest. Orphan records have no physical evidence by definition, but
remain traceable to their exact `contracts` row through record-level provenance.

## Real source evidence

The complete source census is:

| Classification | Count |
| --- | ---: |
| Exact | 0 |
| Probable | 6 |
| Ambiguous | 644 |
| Conflict | 0 |
| Orphan | 208 |
| Missing | 0 |
| Rejected | 634 |

The frozen 120-contract sample is:

| Classification | Count |
| --- | ---: |
| Exact | 0 |
| Probable | 1 |
| Ambiguous | 89 |
| Conflict | 0 |
| Orphan | 29 |
| Missing | 0 |
| Rejected | 1 |

The sample contains multi-version, multi-attachment, parse/extraction, task
file, OCR, structured-artifact, and multiple-physical-match lineages. The
source tuple has no `Missing` or `Conflict` records; those cases are not
fabricated. The source data therefore yields no auto-materialization-eligible
`Exact` record in this rehearsal.

## Frozen artifact and replay

Authoritative frozen artifact:

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v8\manifest-v1.json`

- schema: `plan-0009.stage-1.inventory.v8`
- canonical manifest SHA-256: `00d9c3a3b74fbf27b3ba97e82a5331d62c73bd9b5fff7b7f8a280ac6c4a7d5d6`
- written manifest file-bytes SHA-256: `e76477660c29cca2e53c7a6a1fd2a3302153c7fd73e20193176c06e4b018a6af`
- evidence references: `22,121`; missing source-table/row provenance: `0`
- digest sidecar: `manifest-v1-digests.json`
- replay audit: `replay-audit-v1.json`

First real run:

```text
stage=1 status=frozen selected=120 replayed=false
canonical_manifest_sha256=00d9c3a3b74fbf27b3ba97e82a5331d62c73bd9b5fff7b7f8a280ac6c4a7d5d6 file_bytes_sha256=e76477660c29cca2e53c7a6a1fd2a3302153c7fd73e20193176c06e4b018a6af
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

Second run against the same source tuple and frozen target:

```text
stage=1 status=replayed selected=120 replayed=true
canonical_manifest_sha256=00d9c3a3b74fbf27b3ba97e82a5331d62c73bd9b5fff7b7f8a280ac6c4a7d5d6 file_bytes_sha256=e76477660c29cca2e53c7a6a1fd2a3302153c7fd73e20193176c06e4b018a6af
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

The audit records `selected_contracts=120`, `replay_count=1`, and
`last_status=replayed`. Manifest, sidecar, and audit mismatches fail closed.

## Focused verification

Executed:

```text
rtk cargo fmt --all -- --check
rtk cargo check -p plan-0009-rehearsal --all-targets --all-features
rtk cargo test -p plan-0009-rehearsal --all-features   # 9 passed
```

The full workspace gates were not run in this focused Stage 1 loop (`NOT RUN`);
they remain required before final Goal closeout. Stage 2 must be regenerated
from this v8 manifest rather than reusing the superseded v7 artifact.

## Review ledger

- Initial independent review: `FAIL`; it identified missing source-table/row
  provenance in physical evidence and a report mismatch about selector ownership.
- Repair: upgraded the manifest to v8, added record-level and evidence-level
  provenance, corrected selector ownership documentation, added the focused
  selector test, and reran the real freeze/replay.
- Current independent review: pending for the complete Stage 1 repair
  candidate. No PASS is claimed here.

## Exit decision

Coordinator evidence is complete for the v8 candidate. Stage 1 remains open
until an independent Reviewer verifies the implementation, report, and real
frozen artifacts and returns `PASS`.
