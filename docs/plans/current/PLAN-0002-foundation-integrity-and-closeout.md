# PLAN-0002: Foundation Integrity and PLAN-0001 Closeout

> Status: Active
> Date: 2026-07-31
> Owner: Platform Foundation
> Predecessor: PLAN-0001-foundation-hardening
> Base: 64dbf8b4157c1bfde2f2b319f4657321b2f52f6b

## Goal

Close the integrity gaps found after PLAN-0001 by enforcing process-local
configuration, document/database invariants, transactional message handling,
complete object-storage contracts, and workspace-wide architecture gates.

## Work packages

| ID | Goal | Non-goal | Boundary and code scope | Migration impact | Evidence | Rollback | Done when |
|---|---|---|---|---|---|---|---|
| WP-01 | Make configuration process-specific. | A new configuration service. | Composition roots and runtime support only; remove topology configuration from `shared-kernel`. | None. | Unit tests for each process loader. | Revert the process config commit. | Each process loads only fields it uses. |
| WP-02 | Redact database and messaging connection strings. | Secret-manager integration. | Runtime/configuration types only; explicit `expose()` at connection boundaries. | None. | Debug, Display, validation, and connection-failure tests. | Revert secret wrapper adoption. | No supported rendering path reveals credentials. |
| WP-03 | Enforce Document domain invariants and typed request fingerprinting. | A broad Document model redesign. | `document` domain/application and its PostgreSQL adapter. | Adds fingerprint version support. | Domain and repository tests. | Application rollback before migration or forward compatibility path. | Negative size is rejected, option state is unambiguous, unknown status fails closed. |
| WP-04 | Add forward-only document integrity constraints. | Altering published migrations. | `migrations/007_document_integrity_constraints.sql`. | New checks and fingerprint version column. | Empty/upgrade/repeat PostgreSQL migration tests. | Forward-only corrective migration. | Invalid rows fail fast and legal legacy rows survive unchanged. |
| WP-05 | Derive readiness requirements from one migration catalog. | Auto-migrating from the API process. | Migration app, readiness probe, tests, and compatibility documentation. | None. | Below/equal/ahead-version tests. | Revert catalog consumer while retaining migration. | CLI and readiness share the same embedded catalog. |
| WP-06 | Make Inbox marker and consumer side effect transactional. | Leaking SQLx transactions into domain/application. | Messaging infrastructure transaction helper and PostgreSQL contracts. | Test-only projection schema if required. | Duplicate, failure, concurrency, and multi-consumer tests. | Revert helper. | Exactly one committed side effect per consumer/event. |
| WP-07 | Reconcile exhausted Outbox events into `failed`. | Changing delivery semantics for published events. | Messaging adapter and PostgreSQL contracts. | Constraint adjustment only if needed. | Pending/retry/expired/concurrent reconciliation tests. | Revert behavior or forward migration. | No exhausted pending/retry record remains indefinitely. |
| WP-08 | Complete streaming object-storage contracts. | A new object-storage provider. | Object-storage port, S3 adapter, LocalStorage adapter, and MinIO contracts. | None. | Metadata, presign, large stream, error, cancellation, and atomic local-write tests. | Revert adapter changes. | Contract semantics are explicit and proven against MinIO. |
| WP-09 | Enforce workspace architecture from Cargo metadata. | Replacing compiler checks. | Architecture checker, fixtures, CI. | None. | Legal and illegal dependency graph fixtures. | Revert checker expansion. | CI rejects forbidden dependency and migration/configuration boundaries. |
| WP-10 | Archive PLAN-0001 with merge evidence. | Deleting its remote branch before PLAN-0002 merges. | Documentation and plan indexes only. | None. | Link scan and review of recorded SHA/CI evidence. | Restore plan to `current` if the archive is wrong. | PLAN-0001 is `Integrated` and PLAN-0002 owns follow-up integrity work. |

## Acceptance evidence

PLAN-0002 remains `Active` and its PR remains Draft until every listed work
package has passing local and GitHub Actions evidence. It becomes `Accepted
Candidate` only when the checklist in the task request is fully satisfied.

## WP-01 and WP-02 evidence

- WP-01 Process-specific configuration: PASS. `runtime-config` contains only
  generic runtime support; each application owns its own configuration root;
  `shared-kernel` no longer carries runtime topology; and `AppState` retains
  only services and a readiness probe.
- WP-02 Secret and connection-string protection: PASS. `SecretUrl` redacts
  credentials and sensitive query parameters in `Debug`, `Display`, and parse
  errors. Migration supports the deprecated `DATABASE_URL` input only when it
  does not conflict with `MIGRATION__DATABASE__URL`.
- Verification: `cargo fmt --all -- --check`, workspace check, workspace
  Clippy, package tests, and `scripts/check-architecture.ps1` passed on
  2026-07-31. Full workspace test was blocked by the Windows linker PDB limit
  after the first workspace test attempt.
