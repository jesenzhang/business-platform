# Durable Document Processing Architecture

> 文档 ID：ARCH-DOCUMENT-PROCESSING-001
> 版本：1.1
> 状态：Baseline
> 生效日期：2026-08-03
> 适用范围：PLAN-0004 固定文档处理 MVP

## 1. Scope and bounded contexts

Document Management owns document identity, metadata, lifecycle, content
revisions, and the tenant-scoped `DocumentContentReference`. Document
Intelligence owns processing jobs, fixed pipeline semantics, extraction
candidates, evidence, and review records. Durable Task Execution owns
execution status, steps, leases, fencing, retries, cancellation, and recovery.
Object Storage owns bytes; it does not own business state.

The MVP is deliberately a fixed pipeline:

```text
ValidateSource → DetectType → ExtractText → ExtractFields
→ ValidateCandidate → AwaitReview
```

It is not a general DAG, scheduler, workflow designer, OCR platform, or model
gateway. A job is a durable persisted process, not an in-memory task.

## 2. State and data ownership

`ProcessingJob` fixes the document content revision at creation. A later upload
cannot silently change the input of an existing job. Aggregate version is used
for optimistic writes; content revision changes only when bytes are replaced.
The job row is the authority for execution state. A candidate is a suggestion,
not a formal business fact. Only an authorized review command can accept, edit,
or reject it; review does not directly write another bounded context's private
tables.

PostgreSQL is the production concurrency authority. SQLite is a local,
single-process adapter with database-level `BEGIN IMMEDIATE` write
serialization and no distributed-worker claim guarantee.

## 3. Commands, queries, and events

Commands are create, cancel, claim, advance/retry, and review. Queries are job
detail, candidate detail, and jobs for a document. HTTP is versioned under
`/api/v1`; retryable writes require an idempotency key and all operations apply
tenant and authorization context before the application port.

Execution events use versioned envelopes:

```text
document.processing.requested.v1
document.processing.started.v1
document.processing.step-completed.v1
document.processing.retry-scheduled.v1
document.processing.waiting-for-review.v1
document.processing.succeeded.v1
document.processing.failed.v1
document.processing.cancelled.v1
```

Events contain event/tenant/job/document/correlation/causation/trace identity
and schema version. They never contain raw text, storage keys, signed URLs,
provider prompts, credentials, or database URLs. The outbox is the delivery
mechanism; it is not the job state authority.

## 4. Durable execution semantics

Claims carry owner, opaque lease token, expiry, and a monotonically increasing
fence version. Every worker mutation is fenced by all of those values. A
stale worker cannot complete a step, checkpoint, save a candidate, or mark a
job terminal after reclaim. Heartbeats must run at less than half the lease
duration. Reclaim scans expired leases, and retry classification distinguishes
transient, permanent, cancelled, and lease-lost failures.

High-cost or externally visible work stores a checkpoint containing only safe
artifact references, checksum, revision, and bounded counters. Messages may
wake workers, but a worker always claims from the durable job store. Business
completion remains in the owning application use case.

## 5. Fixed pipeline contract

The MVP accepts `text/plain`, `text/markdown`, and `application/json` as text
only. PDF, images, Office files, and archives fail with a stable
`unsupported_content_type` code. Source reads are bounded, strict UTF-8, and
checksummed; complete text is never logged or placed in a public DTO.

`DeterministicLocalExtractor` derives only a first non-empty title, content
type, line/character counts, and bounded evidence. Unknown business fields
remain null. Candidate validation checks the fixed schema, evidence line
ranges, content revision, payload size, and absence of internal locations.

## 6. Runtime roles and configuration

- `business-api`: authenticated job/review commands and redacted queries;
- `business-worker`: durable job claim, source validation, fixed pipeline,
  checkpoints, recovery scanning, and local/S3 source adapters;
- `ai-worker`: independent PostgreSQL AI-task claim/lease boundary for resource
  steps with local/S3 source adapters; SQLite cannot run separate AI mode;
- `migration`: controlled forward-only schema changes.

SQLite local mode uses one inline worker and an isolated file. Production
configuration fails closed unless PostgreSQL is selected. Worker shutdown stops
new claims and lets current leases complete, release, or expire safely.

## 7. Security and observability

Tenant predicates and ownership checks are mandatory on every port. Public
responses contain safe status, step, attempt, failure code, timestamps, and
candidate/review presence only; lease data, checkpoints, object keys, raw text,
provider responses, and secrets remain internal.

Structured logs use tenant/document/job/step/attempt/worker/fence/status and
duration/failure-code fields. Lease tokens are represented only by a short
one-way hash prefix. Metrics cover created/completed/failed/cancelled jobs,
duration, retries, lease loss, queue age, and pending AI tasks.

## 8. Verification and rollback

Required evidence includes domain transition tests, shared SQLite/PostgreSQL
adapter contracts, stale-fence rejection, lease expiry/reclaim, crash/restart,
local process E2E, and PostgreSQL/MinIO CI E2E. Historical migrations are
immutable and manifests are checksum-verified. Rollback is a code revert plus
a forward-only correction migration; published migrations 001–009 are never
edited in place.

## 9. Revision 1 execution correctness

Revision 1 makes the worker-facing application port an adapter-owned
`ProcessingExecutionUnitOfWork`. A step transition, checkpoint, candidate or
review write, audit event, and outbox event commit in one local transaction;
workers do not compose the legacy command, step, candidate, or task stores for
these transitions. PostgreSQL uses one transaction per unit of work and
`FOR UPDATE`/`SKIP LOCKED`; SQLite uses `BEGIN IMMEDIATE` and remains
single-process only.

`ExtractText` writes a tenant/job-scoped text artifact with checksum, content
revision, and bounded counters before a separate AI task is queued. The AI
worker reads that artifact, and completion or bounded retry/reclaim updates the
AI task and job atomically. Heartbeat guards own and join their heartbeat
tasks; a lost fence fails closed. Business workers dispatch exactly one
`current_step` per claim and drain in-flight work on shutdown. Migration 011
and SQLite migration 002 add tenant/state/lease constraints, AI attempt
uniqueness, and processing audit records without changing published history.
