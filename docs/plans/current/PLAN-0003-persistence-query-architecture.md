# PLAN-0003: Persistence, Query and Multi-Database Architecture

> Status: Active — Revision 1
> Revision: 1
> Date: 2026-08-03
> Owner: Platform Foundation / Document Management
> Base: `3f953c10c18ecd1666e35550c50f7cca6dc3ff93`

## Goal and architecture preflight

Establish reusable persistence and query seams using Document Management as the
reference slice. Document Management owns document metadata. Aggregate state is
authoritative business state; query DTOs and projections are non-authoritative.
Command ports cover aggregate persistence and atomic Audit/Outbox/Idempotency;
Detail/List/Search are separate query ports. PostgreSQL is production authority;
SQLite is a local, single-process adapter and is rejected in production.

No bounded context, data owner, public event, or deployment unit is added. The
API contract changes list reads to stable cursor pagination. Query filters always
carry tenant scope. Cross-context writes remain Application Service or
Process Manager/Saga coordination with Outbox/Inbox and compensation.

## Work packages and acceptance

| ID | Scope | Evidence | Rollback / done |
|---|---|---|---|
| WP-01 | Architecture, query standard, ADR-0008/0009 | Link and architecture checks | Revert docs; done when accepted assets agree. |
| WP-02 | Database-neutral Detail/List/Search ports and Read DTOs | Core unit/check | Revert query module; no SQLx types in core. |
| WP-03 | PostgreSQL query adapters with tenant-scoped keyset pagination | Shared contract on PostgreSQL | Revert adapters; aggregate repository stays unchanged. |
| WP-04 | SQLite adapter, independent migration, WAL/busy timeout | SQLite contracts and file E2E | Delete local DB and revert adapter; never production. |
| WP-05 | Backend-aware migration CLI and composition root | CLI up/status and production rejection tests | Select PostgreSQL; API never auto-migrates. |
| WP-06 | Shared behavior contracts and database-specific tests | Workspace and CI | Test crate is dev-only. |
| WP-07 | Cargo/source fitness functions and CI | `check-architecture.ps1` | Revert rules only with ADR-compatible replacement. |

## Quality, security, consistency, and completion

- Performance: list limit 1..200; stable `(created_at,id)` keyset; tenant index;
  key queries require EXPLAIN evidence before production scale claims.
- Availability: SQLite is local fault containment, not HA. PostgreSQL behavior
  and concurrency tests remain authoritative.
- Recovery: SQLite WAL/busy timeout and file restart E2E; projections remain
  rebuildable. Migration histories are forward-only and independent.
- Security: tenant is a mandatory query parameter and SQL predicate; production
  SQLite is fail-closed; DTOs do not expose object storage keys.
- Observability: backend is logged without URL; projection lag/failure metrics
  are required when projections are introduced.
- Compatibility: cursor list response is an explicit query contract evolution;
  no event schema changes.

Done requires fmt/check/clippy/workspace tests, both adapter contracts, SQLite
file E2E, PostgreSQL remote contracts, architecture fitness, regression MinIO and
Document E2E, updated docs, green feature CI, and recorded Candidate evidence.
Rollback is code revert plus local SQLite file removal; published migrations are
corrected only by new forward migrations.

## Local verification (2026-08-03)

- `cargo fmt --all -- --check`: PASS.
- Workspace check and Clippy `-D warnings`: PASS.
- `cargo test --workspace --all-features`: PASS, 56 passed, 26 existing
  infrastructure tests ignored, 0 failed.
- SQLite shared contracts, rollback, WAL, busy timeout, unknown-state rejection
  and file reopen tests: PASS.
- SQLite migration CLI `up` and `status`: PASS.
- File-backed API E2E: PASS for create, idempotent replay, conflict, detail,
  cursor list, tenant isolation and persistence after process restart.
- Cargo metadata/source Architecture Fitness: PASS.
- Windows PostgreSQL/MinIO: NOT RUN because Docker is unavailable. GitHub Linux
  remains the required authority for PostgreSQL, MinIO and Document E2E.

## Prior candidate evidence (2026-08-03)

- Candidate code SHA: `79bc57e642fbeab1500dc449a50cd7c7b1893d29`.
- Feature CI run `30786233808`: PASS, 6/6 jobs.
- Format, workspace check, Clippy `-D warnings`, Unit and Architecture Fitness:
  PASS.
- PostgreSQL migration 008, shared persistence/query contracts, keyset EXPLAIN,
  migration compatibility, Inbox/Outbox, and Document PostgreSQL E2E: PASS.
- SQLite shared contracts, rollback, WAL/busy timeout, local file recovery and
  migration CLI: PASS locally and in Linux CI where applicable.
- MinIO metadata/Content-Type, real presigned GET, streaming and LocalStorage
  regression contracts: PASS.
- Completion checklist: all 15 PLAN-0003 items PASS. PostgreSQL remains the
  production authority; SQLite remains local/single-process and non-equivalent
  for distributed concurrency.
- The final Candidate SHA is the subsequent evidence commit and is intentionally
  not self-referenced here.
