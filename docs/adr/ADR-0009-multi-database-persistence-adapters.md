# ADR-0009: Multi-Database Persistence Adapters

> Status: Accepted
> Date: 2026-08-03

## Context

Local development needs a low-friction durable database while production needs
PostgreSQL concurrency, operations and recovery semantics. A single portable SQL
implementation would either hide PostgreSQL capabilities or falsely promise
SQLite equivalence.

## Decision

PostgreSQL is the production authoritative backend. SQLite is for local
development and single-process tests. Domain/Application do not know the selected
database. Each database has its own Adapter and migrations; shared semantic
behavior is enforced through Contract Tests and database-specific behavior stays
in dedicated tests. Production configuration rejects SQLite before connecting.

PostgreSQL migrations remain in workspace `/migrations`; SQLite migrations live
in `crates/document-sqlite/migrations`. The migration CLI requires an explicit
backend. Database-specific concurrency semantics need not be identical: SQLite
does not claim distributed claim/lease/fencing, SKIP LOCKED equivalence or HA.

## Consequences

Local file-based E2E is fast and durable, while production semantics remain
strong. Schema/mapping duplication is accepted and verified. Feature parity is
semantic, not SQL/concurrency identity. New backends require an Adapter,
independent migration catalog, contracts, operations model and ADR review.

## Alternatives rejected

SQLite in production fails availability/concurrency requirements. Making
PostgreSQL SQL portable weakens it. Selecting a database in handlers or core code
breaks dependency inversion. Introducing an ORM does not solve semantic parity
and is outside this decision.
