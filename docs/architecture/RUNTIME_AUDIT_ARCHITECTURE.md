# Runtime Audit Architecture

> Status: Baseline profile for PLAN-0005

Audit is a supporting Bounded Context. It owns validated, tenant-scoped,
append-only `AuditEvent` records and query/verification semantics. Audit is
not a technical log, domain event store, finding store, or repair ledger.
Business Application use cases create an audit intent; the owner adapter
persists it in the same local transaction as the business state and Outbox.

## Model

An event contains a stable id, tenant, actor, action, resource, operation and
trace correlation, outcome, bounded reason, redacted details, schema version,
business occurrence time, and database recording time. Tamper evidence uses
the immutable tenant-local append tuple `chain_version + stream_sequence`
with (`previous_hash`, `record_hash`); `occurred_at` never determines chain
order.
`AuditActor` distinguishes User, Service, Worker, RepairJob, and System.
`AuditResult` is Succeeded, Failed, Denied, or Cancelled. Failed/Denied events
must carry a stable failure code; successful events must not carry one.

## Boundaries

The domain has no SQLx, Axum, database, storage, provider, or business-adapter
dependency. `AuditAppendPort` is an application seam; PostgreSQL and SQLite
adapters map rows without exposing row types. Query DTOs are purpose-built and
tenant filtered. Audit details are redacted before persistence and before API
serialization.

## Atomicity

An owner transaction appends business state, an audit row, and Outbox records
before commit. An audit insertion failure rolls the business transaction back.
There is no post-commit asynchronous audit compensator for a high-risk write.

## Append sequence and history boundary

Revision 1 migration `013_runtime_governance_revision1.sql` assigns a
deterministic tenant-local `stream_sequence` to pre-existing rows and marks
them `chain_version=0`. Those rows are an explicit legacy/unverified
boundary; their historical payloads are not retroactively claimed to be a
complete hash chain. New rows start a `chain_version=1` chain at the next
sequence with a null previous hash (the documented genesis anchor), and
PostgreSQL serializes concurrent appends with a tenant advisory transaction
lock. SQLite uses `BEGIN IMMEDIATE` for its single-process authority.

## Queries and verification

Audit lists use `(occurred_at DESC, id DESC)` keyset pagination and filters for
tenant, actor, action, resource, operation, trace, time range, and result.
Chain verification checks canonical payload hashes in tenant stream order,
always verifies the links before a requested business-time range, and only
counts rows inside `from/to`. A time range therefore cannot manufacture a new
Genesis or hide a broken predecessor. It reports the first broken link
without claiming absolute WORM immutability.
