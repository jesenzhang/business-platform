# PLAN-0013 Stage 5–6 Review — Persistence + Bootstrap (independent read-only)

- Date: 2026-09-18
- Reviewer scope: identity/organization/policy adapters (PG+SQLite), migration 019 +
  three SQLite catalogs, `apps/migration` chain, bootstrap use case, contract suites,
  architecture-check additions, perf harnesses. `apps/business-api` excluded
  (concurrently modified).
- Verdict returned: **FAIL** (1 BLOCKER, 3 MINOR, 5 NIT).

## BLOCKER 1 — non-UUID bootstrap audit actor deadlocks cold-start bootstrap

`BootstrapAdministrator` audited with `actor_id: "bootstrap-service"`, but both real
adapters parse `MutationContext.actor_id` as a UUID and fail closed (`Failed`), so a
cold-start bootstrap rolled back and re-failed forever on PostgreSQL and SQLite.
The fake accepted any string, so unit tests hid it.

Disposition — **fixed, verified**:
- `crates/identity/src/application/bootstrap.rs`: canonical deterministic actor
  `bootstrap_service_actor()` = UUIDv5(`EXTERNAL_IDENTITY_NAMESPACE`, `bootstrap-service`),
  exported at `identity::application::bootstrap_service_actor`.
- Contract pinned end-to-end: `MutationContext.actor_id` documented in
  `crates/identity/src/ports.rs` as canonical-UUID-required; the fake now rejects
  non-UUID actors identically to the adapters (`check_audit_actor` in all four
  mutation ports); new suite scenario `verify_actor_validation` asserts
  fail-closed (`Failed`) with zero persisted state on resolve / create_membership /
  change_user_status for non-UUID actors (runs against fake and both dialect adapters).
- `apps/business-api` composition carries the same accessor (fixed by its owning worker
  on instruction).
- Evidence: `cargo test -p identity -p identity-authorization-contracts -p identity-sqlite`
  green; PG code path uses the same parser (CI `--include-ignored` covers).

## MINOR 1 — locked hot-path indexes missing → fixed

`role_bindings(tenant_id,user_id,status)` now replaces `(tenant_id,user_id)`;
added `role_permissions(permission_key,role_id)` and
`tenant_memberships(tenant_id,status)` in 019 and the mirrored SQLite catalogs;
MANIFESTs re-hashed for exactly the changed files. Fresh SQLite `up`+`status`
verified; architecture gate PASS.

## MINOR 2 — PG collation vs SQLite BINARY parity → fixed

`COLLATE "C"` pinned on PG string `ORDER BY`s (`role_permissions.permission_key`,
`permission_definitions.stable_key`, `roles.stable_key`, `external_identities.
issuer/subject`) so row order matches the SQLite (BINARY) adapter and Rust byte
order used by convergence checks. Preflight §8 records the deviation note.

## MINOR 3 — preflight table name drift → fixed

Preflight §4/§8 reconciled to the implemented name `organization_members`
(with revision note). Preflight §8 additionally records the pinned audit-actor
UUID rule and `COLLATE "C"`.

## NIT dispositions

1. identity advisory locks lack a context prefix → **fixed** (`identity:` prefix on
   both guard formats; `org:`/`policy:` already existed).
2. Ledger-write failure masked the original bootstrap cause → **fixed** (failure path
   now returns the execution error; ledger write is best-effort, missing row is
   alertable).
3. `list_tenant_users` N+1 `external_subjects_for` (bounded by page cap ≤200) →
   **accepted**, recorded as a known limitation for a later join optimization.
4. `SigningKey|jsonwebtoken::encode` source scan is bypassable via renamed imports →
   **accepted**; the crate-dependency fitness rule covers domain/application layers;
   delivery-layer scan is defense-in-depth. Recorded in the completion audit.
5. `perf_authorize` measures Authorize with stub subject/org bridges → **accepted**
   and documented; the identity read hot path is measured separately by
   `perf_resolve`.

## Reviewer-verified positives (no action)

Dialect parity of ordering/fingerprints/replay/conflict/caps/keyset; transaction
discipline (advisory locks / BEGIN IMMEDIATE; audit+idempotency in-transaction;
conditional UPDATE + rows_affected); tenant isolation on every statement; seed
catalog identical across 019 and `policy-sqlite` 001 (uuid5 role ids recomputed);
bootstrap ledger terminality and fail-closed staleness; `PLATFORM_AUDIT_TENANT`
confined and documented; no secrets/keys/URLs in audit details or errors;
migration chain order + fail-closed status; contract assertions check concrete
store-returned values.
