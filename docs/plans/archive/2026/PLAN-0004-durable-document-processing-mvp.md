# PLAN-0004: Durable Document Processing MVP

> Status: Integrated — Archived
> Date: 2026-08-04
> Owner: Platform Foundation / Document Intelligence
> Base: `97f6a41608aa136ac05176f37c6e7d3bda0e25a7`
> Integration Mode: local solo fast-forward
> Pull Request: not used
> Revision Goal: Execution Correctness Hardening
> Previous Candidate: `27cd250`

## Goal and architecture preflight

Deliver the first durable Document Intelligence flow without turning the
platform into a general workflow engine. A persisted Processing Job owns
workflow execution state; Document Management remains the owner of document
identity, content revisions, metadata, and storage references. Document
Intelligence owns candidates and evidence. Durable Task Execution owns claims,
leases, fencing, retries, cancellation, and recovery mechanics. PostgreSQL is
the production concurrency authority; SQLite is a local single-process mode.

The fixed MVP pipeline is:

```text
ValidateSource → DetectType → ExtractText → ExtractFields
→ ValidateCandidate → AwaitReview
```

Only text/plain, text/markdown, and application/json-as-text are supported.
PDF, image, Office, and archive inputs fail explicitly as unsupported; no fake
OCR or model capability is introduced. DeterministicLocalExtractor is the
offline and test implementation. A future AI provider port is defined without
adding a general model gateway.

## Bounded contexts and ownership

- Document Management owns `Document`, content revision, metadata, lifecycle,
  and `DocumentContentReference`.
- Document Intelligence owns `ProcessingJob`, fixed pipeline semantics,
  `ExtractionCandidate`, evidence, and review records for candidates.
- Durable Task Execution owns execution leases, fences, retries, checkpoints,
  and worker coordination; it does not own business completion.
- Audit and Messaging own audit persistence and Outbox delivery mechanics.
- Object and Artifact Storage owns bytes and artifacts; database rows retain
  tenant-scoped references, checksums, revisions, and state.

## Work packages and gates

| ID | Scope | Completion evidence |
|---|---|---|
| WP-00 | Aggregate/content revision split, storage-key ownership, legacy query removal, SQLite cross-adapter locking, aggregate serialization removal, migrations 009/002 | Gate 0 tests, migrations, and Architecture Fitness pass before job tables/workers |
| WP-01 | Processing domain model and fixed state machines | Domain transition/property tests and stable error codes |
| WP-02 | PostgreSQL/SQLite job, step, AI task, candidate, and review persistence | Shared adapter contracts and migration tests |
| WP-03 | Claim, lease, heartbeat, reclaim, retry, cancellation, fencing | PostgreSQL concurrency and stale-writer rejection tests |
| WP-04 | Fixed source/text/extraction/validation pipeline | Supported/unsupported type and evidence tests |
| WP-05 | Business Worker runtime | Graceful shutdown, checkpoint, SQLite inline mode, PostgreSQL concurrency |
| WP-06 | AI Worker runtime | Deterministic extractor, retries, invalid result and lease-loss tests |
| WP-07 | Candidate and human review | Accept/edit/reject and optimistic review concurrency |
| WP-08 | Versioned HTTP API | Tenant, auth, idempotency, cancellation, candidate, review, and redaction tests |
| WP-09 | Observability and operational controls | Safe structured fields, metrics port, readiness/config guards |
| WP-10 | Contracts and local/remote E2E | SQLite process E2E, PostgreSQL/MinIO recovery and regression suites |
| WP-11 | Candidate evidence | Green feature CI and documented accepted risks |

## Public commands, queries, API, and events

Application commands include CreateProcessingJob, RequestCancel,
ClaimNextJob, AdvanceStep, RetryJob, and ReviewCandidate. Queries include Job
Detail, Candidate Detail, and Jobs-for-Document. HTTP endpoints are versioned
under `/api/v1`; all writes require tenant context and Idempotency-Key where
retryable. Lease tokens, worker ownership, storage keys, checkpoints, raw text,
provider responses, and secrets never cross the public DTO boundary.

Versioned events include `document.processing.requested.v1`, `started.v1`,
`step-completed.v1`, `waiting-for-ai.v1`, `waiting-for-review.v1`,
`retry-scheduled.v1`, `succeeded.v1`, `failed.v1`, and `cancelled.v1`. Events carry event, tenant, job/document, correlation,
causation, trace, schema-version, and occurred-at fields, but no raw content,
object key, signed URL, or provider secret. Job/step/outbox changes commit in
one local transaction; consumers remain at-least-once and idempotent.

## Consistency, security, and quality attributes

- Aggregate and job versions use optimistic concurrency; leases use owner,
  token, expiry, and fence predicates on every worker write.
- PostgreSQL claims use `FOR UPDATE SKIP LOCKED`; SQLite never claims to be
  distributed and forces one inline worker with a shared database write lock.
- Content revision is fixed into a job and source artifact; later uploads do
  not silently change an existing job's input.
- Tenant and authorization checks precede Application commands and every query.
  Errors expose stable safe codes only. Logs use a hash prefix for lease tokens
  and never contain full text, prompts, URLs, credentials, or database URLs.
- Lease heartbeat is configurable but must be less than half the lease
  duration. Retries are bounded, classified, and recover from checkpoints.
- Capacity targets are measurable: list/claim queries are indexed, SQLite pool
  is bounded, PostgreSQL CI claims at least four workers, and pipeline content
  is bounded by configured maximum bytes.
- Availability and recovery are demonstrated by restart, lease expiry, stale
  fence rejection, and object/database consistency tests. Rollback is code
  revert plus forward-only migration correction; published PostgreSQL migrations
  001–010 and published SQLite migrations remain immutable.

## Fitness functions and documentation

Architecture metadata must identify document-processing core and adapters;
core crates cannot depend on SQLx, Axum, storage SDKs, or apps. The checker
must enforce aggregate non-serialization, repository command-only methods,
metadata-driven dependency rules, immutable migration manifests, and the
single migration catalog. Update the durable-processing, workflow, deployment,
observability, API/event, query, ADR, AGENTS, and status documents in the same
change as behavior.

## Accepted Candidate definition

PLAN-0004 may become `Accepted Candidate` only after every checklist item in
the execution request is PASS, including Gate 0, domain and adapter contracts,
lease/fencing and crash recovery, fixed pipeline, both workers, review API,
local SQLite process E2E, GitHub PostgreSQL/MinIO E2E, existing regressions,
Architecture Fitness, and documentation. Windows PostgreSQL/MinIO may be
`NOT RUN` only when GitHub Linux evidence is green. The feature branch is
pushed for CI evidence; no PR, main merge, branch deletion, PLAN-0005, or
published-migration edits are allowed in this plan.

## PLAN-0004 Revision 1: Execution Correctness Hardening

Revision 1 reopens this candidate to close correctness gaps in the Separate AI
production path. The bounded context and data ownership remain unchanged:
Document Management owns content revisions and storage references; Document
Intelligence owns ProcessingJob, steps, AI tasks, candidates, and reviews; the
Durable Task Execution concern owns leases, fencing, retries, cancellation, and
recovery state.

The revision is limited to the fixed pipeline and does not introduce a DAG,
scheduler, OCR/Office support, real model provider, or general model gateway.
Every worker write is lease-, fence-, tenant-, and expiry-checked. Business
transitions that touch Job, Step, AI Task, Candidate, Review, Audit, or Outbox
must use one adapter-owned database transaction. SQLite remains local,
single-process, inline-AI only; PostgreSQL remains the multi-worker authority.

### Revision 1 work packages

| ID | Scope | Completion evidence |
|---|---|---|
| R1-01 | Atomic processing transaction ports | Worker depends on business-level execution UoW, not composed write stores |
| R1-02 | Step-aware pipeline execution and recovery | current_step dispatch, durable text artifact, checkpoint-aware restart |
| R1-03 | Lease/fence/heartbeat enforcement | database expiry predicates, heartbeat guard, stale-writer tests |
| R1-04 | Separate AI lifecycle, retry, and reclaim | AI task uniqueness, reclaim, bounded retry/backoff, atomic completion |
| R1-05 | Review and cancellation atomicity | review idempotency/rollback and cancellation propagation tests |
| R1-06 | Worker concurrency and graceful shutdown | PostgreSQL slots, SQLite restriction, cancellation/drain tests |
| R1-07 | Real PostgreSQL/MinIO multi-process E2E | API + Business Worker + AI Worker + PostgreSQL + MinIO process evidence |
| R1-08 | Candidate evidence | final code SHA, feature CI, E2E and accepted-risk record |

Revision 1 must return to `Accepted Candidate` only after all listed evidence,
the full existing regression suite, Architecture Fitness, and a green CI run
for the evidence commit. Windows PostgreSQL/MinIO remains `NOT RUN` when the
GitHub Linux infrastructure evidence is green.

## Verification record

Gate 0 and later verification entries are appended here with exact commands,
CI run IDs, candidate SHA, and explicit `PASS`, `PARTIAL`, `NOT RUN`, or
`BLOCKED` status. A failure blocks progression; under the requested
`blockers-only` stop strategy, non-blocking polish is deferred.

### Historical Revision 0 candidate

The previous candidate (`dddb50cd7851e479e309a3b0d0ef5f34a465dadf`, Feature CI
`30811814533`, evidence CI `30812079019`) is retained as historical evidence.
Revision 1 supersedes that candidate and must establish fresh evidence before
the plan returns to `Accepted Candidate`.

### Revision 1 local verification — 2026-08-03

- Local code gates: `PASS` — `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --all-features`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo test -p migration --test migration_test --all-features`, `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-architecture.ps1`, and `git diff --check`.
- Domain, SQLite UoW, API, and migration contracts: `PASS`.
- SQLite process E2E including a killed running-step worker and restart: `PASS` — `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-local-document-processing.ps1`.
- Windows PostgreSQL/MinIO multi-process E2E: `NOT RUN` — no local PostgreSQL/MinIO runtime is installed; GitHub Linux is the required environment-dependent gate.
- Feature CI for the Revision 1 candidate: `PASS` — GitHub Actions run
  `30833527820` at implementation SHA `a0bb0ad9374e87b1225e26ccbfbd44f4d616ebf2`.
  Format, Check, Clippy, Unit tests, Architecture Fitness, and PostgreSQL +
  MinIO + E2E contracts all passed.

### Revision 1 accepted-candidate evidence

The Revision 1 implementation candidate is `a0bb0ad9374e87b1225e26ccbfbd44f4d616ebf2`.
Its Feature CI run is `30833527820` and is green across all six jobs.

- GitHub Linux PostgreSQL/MinIO multi-process E2E: `PASS` — API, Business
  Worker, AI Worker, real PostgreSQL and MinIO covered source upload, durable
  text artifacts, business-worker crash recovery, AI-worker reclaim, review
  replay, and twenty same-document jobs.
- Local SQLite process E2E: `PASS` — the killed running-step worker was
  restarted and recovered to review.
- Review atomic rollback and running-job cancellation contracts: `PASS` —
  test-only PostgreSQL/SQLite fault injection rolls back the Review and Job
  together; a running Job cancellation is observed by the next fenced step.
- Architecture Fitness: `PASS` — local and CI checks passed.
- Windows PostgreSQL/MinIO: `NOT RUN` — the runtime is not installed locally;
  GitHub Linux is the environment-dependent evidence.
- Accepted MVP risks: deterministic extraction remains the only provider
  implementation; PDF/image/Office/archive inputs remain explicitly
  unsupported; SQLite remains local single-process and Separate AI is
  PostgreSQL-only. These are bounded MVP constraints, not deferred correctness
  work.

Revision 1 is therefore an `Accepted Candidate`. The feature branch was then
integrated to `main` by local solo fast-forward; no PR, merge commit, or force
push was used.

## Integration closeout — 2026-08-04

- Candidate SHA: `12454709a88fde16f7769af27a75e79c4bc0981a`.
- Main integration SHA: `12454709a88fde16f7769af27a75e79c4bc0981a`.
- Feature CI: `30833916455` — `PASS` across Format, Check, Clippy, Unit
  tests, Architecture Fitness, and PostgreSQL + MinIO + E2E contracts.
- Main CI: `30868701290` — `PASS` across the same six jobs after the
  fast-forward integration.
- Local SQLite process E2E: `PASS`, including killed running-step recovery.
- GitHub Linux PostgreSQL/MinIO E2E: `PASS`.
- Windows PostgreSQL/MinIO E2E: `NOT RUN`; the required runtime is not
  installed locally and Linux CI is the environment-dependent evidence.
- Accepted MVP boundaries remain unchanged: deterministic local extraction is
  the only provider; PDF/image/Office/archive inputs are explicitly
  unsupported; SQLite is local single-process only; Separate AI is
  PostgreSQL-only. These are bounded MVP constraints, not correctness gaps.

The plan is now `Integrated / Archived`. The PLAN-0004 delivery branch may be
removed only after the post-integration Closeout CI is green.
