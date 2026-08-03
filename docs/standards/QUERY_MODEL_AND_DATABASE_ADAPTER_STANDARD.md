# Query Model and Database Adapter Standard

> Status: Baseline
> Date: 2026-08-03

## Write model

Aggregate Repositories only load/save Aggregates. A transaction modifies data
owned by one context. Cross-context writes use Application Services or events.
Business state, Audit, Outbox and Idempotency commit atomically. Optimistic
versions are mandatory for updates; automatic cascades may not cross Aggregates.

## Query model

Each use case has a dedicated Query Port and Query Object. It returns a Read DTO,
not a complete Aggregate graph. Detail, list, search, export and dashboard use
different objects/interfaces. Native SQL is allowed. Universal repositories,
lazy loading and N+1 access are prohibited.

Offset pagination is acceptable for small back-office lists. High-frequency or
large lists use keyset/cursor pagination. Sorting is stable and includes a unique
tie-breaker. A critical query records index rationale, scale assumption, EXPLAIN
evidence, maximum rows, timeout, tenant predicate and permission predicate.

Document Search is Deferred until a complete capability is designed. The next
implementation should use PostgreSQL full-text/`pg_trgm` or a dedicated search
index behind a versioned port; adapters must not expose a partial search port.
Document filename filters are literal LIKE filters: escape `\\`, `%`, and `_`
and specify `ESCAPE '\\'`. SQLite/PostgreSQL matching is ASCII
case-insensitive only; full Unicode case equivalence is not part of this
contract.

## SQL and ORM

Default: SQLx + explicit SQL + Data Mapper. ORM/SQLx Row types are not Domain
Models and cannot cross the Adapter seam. An Adapter may use an ORM/Query Builder
for simple CRUD; core transactions and complex queries prefer reviewable SQL.
Active Record, lazy loading, ORM Entity leakage and automatic cross-Aggregate
cascade writes are prohibited.

SeaORM, Diesel or another builder requires an ADR proving no Domain pollution,
no lazy loading/Active Record, preserved SQL reviewability, explicit complex SQL
and unchanged PostgreSQL/SQLite Contract Tests.

## Cross-context query

A same-database, read-only Query Adapter may join multiple context tables. Its
independent DTO documents each field's authoritative owner. It cannot write and
must apply tenant and permission filtering in the database. Cross-service choice:
API Composition for low-frequency current reads; Projection for frequent
lists/dashboard/search; Reporting Database for historical analytics.

## Projection and hierarchy

Projection consumers use Transactional Inbox and persist projection name, event
ID/schema, last applied time, idempotency, lag and failure. Projections are
deletable/rebuildable and non-authoritative.

Hierarchy selection: Adjacency List + Recursive CTE by default; Closure Table for
frequent ancestor/descendant permission queries; Materialized Path for read-heavy
rarely moved paths; Nested Set only by ADR. Approval/task execution uses state
machines and Process Managers, not hierarchy tables.

## Durable processing query profile

Processing job and candidate reads remain dedicated Query ports and safe Read
DTOs; an Aggregate Repository only loads/saves a job aggregate and never grows
list/search/report/export methods. Job execution status is authoritative in
the processing tables, while candidate and review records are tenant-scoped
read models of the Document Intelligence context. The SQLite processing
adapter is local single-process and uses its own migration catalog; the
PostgreSQL adapter uses the runtime catalog and production locking semantics.
