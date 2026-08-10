# PLAN-0008 Integration Closeout

Document ID: REPORT-PLAN-0008-CLOSEOUT  
Status: Final  
Date: 2026-08-10  
Scope: PLAN-0008 integration and evidence closeout only.

## Result

PLAN-0008 is `Integrated / Archived` on `main`.

| Identity | SHA / run |
| --- | --- |
| Base SHA | `35d1d01fd49a70ee996fbb5fb72818a632989efe` |
| Implementation/runtime evidence SHA | `70469be26cb009c23f1a77c1553947522ba82aed` |
| Final Candidate HEAD | `7eb5421e492a11c0ac20b17f8fd5c3a034f7a29b` |
| Final Candidate CI | `31353149398` — PASS |
| PLAN-0008 Integration SHA | `7eb5421e492a11c0ac20b17f8fd5c3a034f7a29b` |
| Main CI | `31353409550` — PASS |

The candidate was integrated with repository local solo `git merge --ff-only`.
The candidate had the correct merge-base, was not behind `main`, and the
working tree was clean before integration.

## CI acceptance

The exact Final Candidate and main runs passed all required jobs:

- PostgreSQL + MinIO + E2E contracts, including migration, multipart upload,
  revision/evidence, concurrency, retry, crash recovery, and multi-process E2E;
- Architecture Fitness;
- Format, Check, Clippy, Unit, CLI/MCP, Frontend, and Playwright.

Local PostgreSQL and MinIO were **NOT RUN**. GitHub-hosted service containers
are the only PostgreSQL/MinIO acceptance evidence.

The object/database boundary remains explicit: the evidence proves only
best-effort immediate orphan compensation. PostgreSQL + MinIO atomicity is not
claimed. Automatic object-store reconciliation or GC is not claimed as
implemented or exercised.

## Read-only C-project research

The C project and its original database and files were not modified. No
destructive migration or production migration was run. The inventory and
mapping design found:

- 1,492 contracts; 960 versions; 7,230 database file records;
- 15,976 physical files; 111 attachments;
- 5,307 duplicate SHA-256 groups;
- 2,063 managed live orphans, 3,129 transient managed orphans, and 3,569
  external orphans;
- 32 broken relations; 16 OCR records without a resolvable revision;
- 0 unresolved LLM processing lineages.

Detailed schema, storage, lineage, inventory, anomaly and canonical mapping
evidence is in
[`PLAN-0008-CI-EVIDENCE-AND-C-MIGRATION-REHEARSAL.md`](PLAN-0008-CI-EVIDENCE-AND-C-MIGRATION-REHEARSAL.md).
Research outputs remain preserved in the local excluded `.scratch` directory
and in the external safety copy
`F:\Workspace\plan-0008-c-project-migration-rehearsal-20260810`.

## Follow-up and boundaries

PLAN-0006 remains `Proposed / NOT ACTIVE`. PLAN-0009 is created only as a
`Proposed` read-only migration rehearsal design. No formal C-project migration
has started, and no Ambiguous, Conflict, Orphan, Missing, or Rejected item may
be auto-repaired or promoted to a business fact.
