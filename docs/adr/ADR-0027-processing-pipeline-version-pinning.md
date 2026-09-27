# ADR-0027: Processing pipeline version pinning

> Status: Proposed / NOT ACCEPTED
> Date: 2026-09-27
> Owner: Document Intelligence / Durable Task Execution
> Related: PLAN-0014 Wave 3 D-G03; ADR-0010/0011/0012

## Context

The workflow Baseline requires the pipeline version to be stored on the job.
The current six-step pipeline is compiled into the worker but jobs have no
version field. A fixed current pipeline does not discharge the requirement.
D-G03 remains an open C1 conformance gap; this proposal does not close it.

## Proposed decision

- Document Intelligence owns the version alongside its ProcessingJob. No new
  scheduler, pipeline registry or DAG is introduced.
- Queue captures a validated immutable version such as `document-processing.v1`.
  A retry, claim, checkpoint, cancel and review must preserve it.
- Add a database column through additive PostgreSQL and SQLite migrations.
  Backfill existing jobs to v1 only after proving their stored step vocabulary
  is the known fixed pipeline. Unknown step/state combinations fail migration
  validation rather than being silently relabeled.
- Every worker advertises a supported version set in code. Claim must filter
  supported versions; rehydration and completion reject an unsupported version.
  Unsupported jobs remain durable and observable, not auto-failed or replayed.
- During rolling upgrade, deploy readers that understand the additive field
  before new writers emit another version. Old workers must be drained before
  enabling a version they cannot interpret. Do not rely on a new column alone
  to protect old binaries whose claim SQL ignores it.
- Checkpoints remain tied to job, step and pinned version. A version change
  cannot reinterpret an existing checkpoint or repeat an unknown external effect.

## Implementation and acceptance after approval

Scope: job value model, queue/rehydration, both persistence adapters, worker
claim/completion, migrations and shared contract fixtures. No public HTTP change
is required merely to pin the internal version; any new exposed field needs an
explicit compatibility review.

Tests: queue/restart retains v1; retry/reclaim preserves version; unknown version
is not claimed; mixed-version workers do not steal unsupported work; old-schema
upgrade preserves all facts; checkpoint interpretation is version-specific;
PostgreSQL multi-worker rolling-upgrade fixture is mandatory.

Rollback: retain the additive column. Disable new-version writes, drain or
quarantine their jobs, then restore a compatible binary. Never rewrite a job's
version to make a rollback appear compatible.

## Current disposition

Design only, no version-column migration or runtime change in this repair.
Acceptance of this proposal and execution evidence remain outstanding. The
processing capability remains R1 with explicit conformance/R2 blocking items.
