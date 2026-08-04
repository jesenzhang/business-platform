# Data Integrity and Controlled Repair Architecture

> Status: Baseline profile for PLAN-0005

Runtime Governance owns scan execution, findings, repair plans/runs/steps and
the append-only repair ledger. It does not own Document Intelligence business
state. Integrity rules read owner-defined `IntegrityQuery` and processing
query ports. A repair handler is implemented by the data-owning context and
exposes a typed operation, never a table name, SQL string, JSON Patch, or
generic repository.

## Finding lifecycle

Rules have stable IDs, explicit versions, bounded-context ownership, severity,
and an allow-list flag. A finding is keyed by tenant, rule/version, resource,
and fingerprint. Repeated scans update detection time and occurrence count;
they do not create duplicates. Lifecycle transitions are `open`,
`repair_planned`, `repairing`, `repaired`, `ignored`, `false_positive`,
`stale`, and `needs_manual_review`, each with an optimistic version.
Transient dependency failures produce an unknown/failed scan result, not a
false finding.

## Repair protocol

`DryRun -> Approve -> Execute -> Verify` is mandatory for medium/high risk;
low-risk handlers may be configured for automatic execution. Every step
re-runs the rule, verifies the target version, claims a lease/fence, executes
one typed owner operation, appends Audit and Repair Ledger evidence, updates
the Finding, and commits. Resource batches use one transaction per resource.
The durable run supports idempotency, retry, cancellation, checkpoint,
resume, and stale-worker rejection.

## Initial processing rules and handlers

PROC-INT-001..008 cover missing active AI task, missing candidate, invalid
review/job terminal state, succeeded AI without candidate, terminal lease,
current-step mismatch, candidate revision mismatch, and missing text artifact.
The only handlers are `reconcile_processing_job.v1`,
`requeue_missing_ai_task.v1`, `clear_terminal_job_lease.v1`,
`rebuild_processing_step_projection.v1`, and `reconcile_ai_completion.v1`.
