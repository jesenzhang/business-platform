# ADR-0008: CQRS Query Model and Read Projections

> Status: Accepted
> Date: 2026-08-03

## Context

Aggregate repositories were beginning to serve HTTP detail/list reads. That
couples read shape, pagination and database optimisation to the write model and
cannot scale to cross-context dashboards without leaking ownership.

## Decision

Commands and queries use separate interfaces. Each query use case owns a Query
Object and Read DTO; projections are non-authoritative, idempotent and
rebuildable. Same-database cross-context joins are permitted only in dedicated
read-only Query Adapters with explicit field ownership and tenant/permission
predicates. They never write source tables.

Cross-service reads use API Composition for low-frequency, strongly current,
small aggregation; event-driven Projections for frequent list/dashboard/search;
and a Reporting Database/Warehouse for historical analysis. OLTP and reporting
separate when analytical scans threaten transaction SLOs, retention/schema needs
diverge, or independent scaling/recovery is required.

## Consequences

Callers learn small capability-specific interfaces and database-specific
optimisation stays local. More DTOs/mappers and projection operations are
required. Aggregate repositories cannot be used as generic query interfaces.
Cursor/API compatibility must be versioned and tested.

## Alternatives rejected

A universal repository is shallow and erases ownership. Returning Aggregate
graphs couples reads to write invariants. Direct private-table reads across
services bypass published contracts and are rejected.
