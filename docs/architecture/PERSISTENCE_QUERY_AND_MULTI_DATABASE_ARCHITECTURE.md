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
types and SQL stay inside Infrastructure Adapters. The Document Search
capability is explicitly Deferred in PLAN-0003 Revision 1; when it is started,
the recommended implementation is PostgreSQL full-text/`pg_trgm` or a
dedicated search index behind a versioned query port, rather than a partial
LIKE adapter.

Document aggregate state is private and can only be created through the domain
factory or `rehydrate`, which validates persisted invariants. Infrastructure
adapters never construct the aggregate with a struct literal. HTTP read DTOs
exclude `object_key` and other storage-internal locations. Filename filters
escape `\\`, `%`, and `_` and use `LIKE ... ESCAPE '\\'` in SQLite and
`ILIKE ... ESCAPE '\\'` in PostgreSQL. These filters provide ASCII
case-insensitive matching; they do not promise full Unicode case equivalence.
The HTTP cursor is an opaque versioned v1 token; the database cursor remains a
stable `(created_at, id)` keyset.

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
production HA, or identical concurrency guarantee is claimed. Its pool is
bounded to 1..4 connections and the atomic write adapter uses explicit
single-writer serialization, WAL and busy timeout. Production configuration
rejects SQLite before connecting.

Shared tests cover semantic behavior. PostgreSQL-specific tests preserve
SKIP LOCKED, lease/fencing, concurrent Inbox/Outbox and migration compatibility.
SQLite-specific tests cover WAL, busy timeout, single writer/transaction
rollback, process-lock behavior and local file recovery. Different concurrency
semantics do not weaken PostgreSQL tests.

PostgreSQL migrations remain in `/migrations` and are embedded only by the
shared `runtime-migration` catalog; SQLite migrations live with
`document-sqlite/migrations` and are embedded only by that adapter. The
migration CLI selects a backend explicitly. Status treats only a missing
`_sqlx_migrations` table as an empty catalog; permission, connectivity and
other database errors fail the command.
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

## PLAN-0014 Wave 3 SQLite audit bootstrap correction (2026-09-27)

Document's SQLite bootstrap already creates the shared audit_events table.
Migration 006 completes its Audit-owned chain format without requiring the
Processing catalog. No business or audit semantic ownership is transferred.
Document-only contract tests and composed-profile tests are separate.

Use document_sqlite::MIGRATOR.run(&pool), including through the migration CLI.
The exported adapter migrator preserves run(pool)/iter() usage; its concrete
type is DocumentMigrator. It validates SQLx historical checksums and dirty or
unknown versions. For 006 it conditionally adopts the three chain columns and
applies backfill, indexes and SQLx migration registration atomically under
BEGIN IMMEDIATE. This permits upgrading databases where historical Processing
004 already supplied those columns. Historical migration SQL is unchanged.

Existing v1 rows retain their hashes. Pre-chain all-NULL metadata is migrated
to explicit unverified legacy rows; no historical verification is manufactured.
Explicit version 0 with hashes remains invalid at read time. Back up before
upgrade; rollback retains additive columns and the audit history. Directly
executing 006 SQL bypasses its documented conditional-column precondition and
is not the supported migration entry point.

Evidence and limits: [Wave 3 Round 2 repair](../reviews/2026-09-27-wave3-round2-repair.md).
