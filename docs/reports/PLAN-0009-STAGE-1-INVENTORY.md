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
- isolated output: `F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v9`

The SQLite adapter uses one read-only connection. The application rejects a
production argument and rejects targets outside the fixed isolated root. No C
application, upload, import, migration, or source-database write was invoked.

The source snapshot observed by the run was Alembic `0057`, journal mode
`wal`, SQLite `integrity_check=ok`, and 32 retained `foreign_key_check`
violations. The database and physical-root byte counts are recorded in the
manifest; the retained foreign-key anomaly is not repaired by rehearsal code.
There is no independent cryptographic pre/post source snapshot in this stage,
so this report does not claim one.

## Deterministic inventory

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

Each manifest record preserves the owning `contracts` table and row ID. Each
physical `evidence` entry preserves a deterministic primary source table/row
and the complete ordered `source_records` list of every contributing source
table/row pair. The primary pair is retained for simple consumers; the list is
the authoritative complete provenance. Orphan records have no physical
evidence by definition, but remain traceable to their exact `contracts` row.

Manifest evidence contains only safe metadata: root label, relative-path
SHA-256, depth, extension, size, content fingerprints, and source provenance.
Raw source names, text, absolute paths, URLs, credentials, and signed URLs are
not written.

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

`F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v9\manifest-v1.json`

- schema: `plan-0009.stage-1.inventory.v9`
- canonical manifest SHA-256: `8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43`
- written manifest file-bytes SHA-256: `759e8f96b9555b697a83798ace0d3a888fb8010bb816113bcddd05d517ab13aa`
- evidence references: `22,121`
- evidence entries with complete source provenance: `22,121`
- evidence entries with multiple source rows: `21,570`
- digest sidecar: `manifest-v1-digests.json`
- replay audit: `replay-audit-v1.json`

First real run:

```text
stage=1 status=frozen selected=120 replayed=false
canonical_manifest_sha256=8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43 file_bytes_sha256=759e8f96b9555b697a83798ace0d3a888fb8010bb816113bcddd05d517ab13aa
classifications=Exact=0,Probable=1,Ambiguous=89,Conflict=0,Orphan=29,Missing=0,Rejected=1
```

Second run against the same source tuple and frozen target:

```text
stage=1 status=replayed selected=120 replayed=true
canonical_manifest_sha256=8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43 file_bytes_sha256=759e8f96b9555b697a83798ace0d3a888fb8010bb816113bcddd05d517ab13aa
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

The full workspace gates were not run in this focused loop (`NOT RUN`);
they remain required before final Goal closeout. Stage 2 must consume this v9
manifest and must not reuse the superseded v8 artifact.

## Review ledger

- Initial independent review: `FAIL`; it identified missing source-table/row
  provenance and a report mismatch about selector ownership.
- v8 repair review: `FAIL`; the reviewer found that the first source pair was
  retained while the real multi-source evidence lost the remaining pairs.
- v9 repair: upgraded the manifest schema, preserved the complete ordered
  provenance list per evidence entry, retained record-level orphan provenance,
  and reran the real freeze/replay.
- Current independent review: pending for the complete v9 repair candidate.

## Exit decision

Coordinator evidence is complete for the v9 candidate. Stage 1 remains open
until an independent Reviewer verifies the implementation, report, and real
frozen artifacts and returns `PASS`.
