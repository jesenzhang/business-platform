# PLAN-0009 Stage 2 — deterministic mapping and isolated write rehearsal

Status: **candidate — independent Reviewer pending for the v9-input rerun**.

Stage 2 is rehearsal-only. It never writes the C source database or source
roots, never calls the C application, and never activates PLAN-0006.

## Inputs and boundary

The input is the frozen Stage 1 v9 manifest:

- target manifest: `F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-1-inventory-v9\manifest-v1.json`
- input canonical SHA-256: `8376eac8c5aa2447077048f3a50d68c3584e3df929d3473a865f995f5319cb43`
- source tuple: the user-provided C local-test env, database, and physical roots
- isolated Stage 2 target: `F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810\stage-2-rehearsal-v2`

The adapter verifies the source snapshot and reads it through the existing
read-only boundary. All writes are limited to the isolated target SQLite
database, mapping ledger, and object root. Target configuration is path-bound
to the fixed rehearsal directory; the reviewed `run_stage2_at` seam accepts
only another explicitly bounded isolated target for later rehearsal stages.

## Mapping policy

Only an `Exact` record with one observed SHA-256 evidence item is eligible for
automatic materialization. `Probable` records become `manual_review`; all
`Ambiguous`, `Conflict`, `Orphan`, `Missing`, and `Rejected` records become
`quarantine`. Candidate UUIDs in the mapping plan are deterministic proposal
identifiers; they are not formal facts until an eligible record is materialized.

The target mapping ledger contains one immutable-by-digest row per selected
contract. Materialized writes use the Document Management application/UoW and
the Document Intelligence processing ports, with deterministic target object
references, timestamps, audit IDs, and outbox IDs. The post-write verifier
checks object bytes, document/revision/link rows, processing rows, and audit /
outbox transitions before accepting a materialized record.

## Real 120-contract run

The first v9-input run produced:

```text
stage=2 status=frozen selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119 replayed=false mapping_plan_sha256=f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0
```

The second run against the same frozen manifest and target produced:

```text
stage=2 status=replayed selected=120 exact_eligible=0 exact_materialized=0 review=1 quarantine=119 replayed=true mapping_plan_sha256=f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0
```

The target mapping ledger has `manual_review=1` and `quarantine=119`.
Because the source sample has no `Exact` record, the target has no formal
documents, revisions, links, processing runs, processing artifacts, evidence,
audit events, or outbox events. This is a source-data result, not a fabricated
success case; the next stage must exercise the real candidate/quarantine
surface without promoting `Probable` data automatically.

## Frozen mapping artifact

- mapping schema: `plan-0009.stage-2.mapping.v1`
- canonical mapping SHA-256: `f5aae49415500877e0e9db753cbfea9567493b611c3d7eac62ba317f05f021e0`
- mapping file-bytes SHA-256: `e00a2769b53e68212a98cdeb4939a0ca668f737be909844568b73c9f7b220e59`
- sidecar: `mapping-plan-v1-digests.json`
- replay audit: `rehearsal-audit-v1.json`, `replay_count=1`, `last_status=replayed`
- target SQLite: `integrity_check=ok`, `foreign_key_check` violations: `0`
- mapping ledger rows: `120`

The mapping file, sidecar, audit, and ledger are checked for digest and replay
consistency. Any target mismatch fails closed; no duplicate formal facts were
created by the replay.

## Focused verification

Executed in the workspace:

```text
rtk cargo fmt --all -- --check
rtk cargo check -p plan-0009-rehearsal --all-targets --all-features
rtk cargo test -p plan-0009-rehearsal --all-features   # 9 passed
```

The full workspace gates remain `NOT RUN` in this focused loop and are still
required before final Goal closeout.

## Review ledger

- The earlier Stage 2 implementation review passed for the superseded v7-input
  candidate after deterministic timestamp/event and full-row verification
  repairs.
- This v9-input rerun changes the frozen input and mapping digest and includes
  the bounded target-directory seam; it therefore requires a fresh independent
  review.
- Current status: coordinator evidence complete; independent Reviewer pending.

Stage 2 remains rehearsal-only until that fresh review returns `PASS`.
