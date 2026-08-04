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
occurred time, and tamper-evidence links (`previous_hash`, `record_hash`).
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

## Queries and verification

Audit lists use `(occurred_at DESC, id DESC)` keyset pagination and filters for
tenant, actor, action, resource, operation, trace, time range, and result.
Chain verification checks canonical payload hashes in tenant stream order and
reports the first broken link without claiming absolute WORM immutability.
