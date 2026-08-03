# Persistence, Query and Multi-Database Architecture

> Document type: Baseline
> Status: Baseline
> Date: 2026-08-03

## Command side

The Domain Model is not a database Row Model. A DDD Aggregate owns invariants;
its Repository loads and saves only that Aggregate. Application use cases define
Unit of Work and optimistic-concurrency intent. A successful business write
atomically commits the Aggregate, Audit, Outbox and Idempotency record in the
owner context. A transaction does not modify another context's private tables.

Cross-context writes use an Application Service for short synchronous
coordination or a Process Manager/Saga for durable work. Outbox/Inbox provides
at-least-once delivery and deduplication. Compensation is explicit; exhausted
or ambiguous cases enter a visible failure state and may require authorised
human intervention.

## Query side

CQRS here means separate command and query interfaces, not mandatory separate
deployment. Each use case owns a Query Object and returns a purpose-built Read
DTO. A Read DTO is not an Aggregate and never becomes a write authority. Data
Mappers translate database rows. Native SQL, database Views, Materialized Views
and event-driven Projections are allowed in Infrastructure Adapters.

Detail, list, search, export and dashboard are distinct query interfaces. An
Aggregate Repository never carries reporting or dashboard queries. SQLx/ORM
types and SQL stay inside Infrastructure Adapters.

## Cross-context reads and writes

Inside one modular monolith/database, a dedicated read-only Query Adapter may
join multiple contexts. Its Read DTO identifies each field's authoritative
source, it applies tenant/permission filters in SQL, and it cannot write any
source table. Examples should be named by capability (`contract-query-postgres`,
`project-query-postgres`, `reporting-postgres`), never GlobalRepository,
GenericBusinessRepository or UniversalQueryService.

- Low-frequency, strongly current, small aggregation: API Composition.
- High-frequency list/dashboard/search: Event-driven Projection.
- Historical analysis, annual statistics and BI: Reporting Database/Warehouse.
- Stable in-process consumers may use a Published Query Contract; cross-service
  consumers never query private tables.

Cross-context writes always use an Application Service or Process Manager/Saga,
Outbox/Inbox, compensation, and an operational path for human intervention.

## Projection lifecycle

`Integration Event -> Projection Consumer -> Transactional Inbox -> update Read
Model -> commit`. Every projection records `projection_name`, `event_id`,
`schema_version`, `last_applied_at`, idempotency state, lag metric and failure
state. It can be deleted and rebuilt from event replay or authoritative business
tables. Projection data is never authoritative business data. PLAN-0003 defines
fixtures/rules only and does not invent a cross-business projection.

## Multiple databases

Domain/Application expose shared ports. PostgreSQL and SQLite are independent
Adapters with independent migrations and shared behavior Contract Tests.
PostgreSQL is the production authority. SQLite supports local development and
single-process tests only: no distributed claim, `SKIP LOCKED` equivalence,
production HA, or identical concurrency guarantee is claimed. Production
configuration rejects SQLite before connecting.

Shared tests cover semantic behavior. PostgreSQL-specific tests preserve
SKIP LOCKED, lease/fencing, concurrent Inbox/Outbox and migration compatibility.
SQLite-specific tests cover WAL, busy timeout, single writer/transaction
rollback, process-lock behavior and local file recovery. Different concurrency
semantics do not weaken PostgreSQL tests.

PostgreSQL migrations remain in `/migrations`; SQLite migrations live with
`document-sqlite/migrations`. The migration CLI selects a backend explicitly.
The formal API process does not automatically migrate PostgreSQL or SQLite;
development database creation remains an explicit CLI operation.

## Hierarchy decision

- Default tree: Adjacency List + Recursive CTE.
- Frequent ancestor/descendant permission reads: Closure Table.
- Read-heavy, rarely moved directory paths: Materialized Path.
- Highly static, almost never moved: Nested Set only through an ADR.

Organization/directory trees and permission inheritance are hierarchy data.
Approval flows and task execution are state machines and Process Managers, not
tree tables.

## Quality and operations

Critical queries document indexes, scale assumptions, EXPLAIN evidence, maximum
rows, timeout, tenant filter and permission filter. Keyset pagination uses a
stable unique tie-breaker. Read models expose lag/failure metrics. Adapter errors
are sanitised at core seams; URLs and sensitive row data are not logged.
