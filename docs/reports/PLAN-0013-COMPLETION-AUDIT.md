# PLAN-0013 Completion Audit

Document ID: REPORT-PLAN-0013-COMPLETION-AUDIT
Status: Accepted Candidate (independent review and explicit trunk-integration
instruction pending; `main` untouched, plan not archived)
Date: 2026-09-18
Scope: `docs/plans/current/PLAN-0013-identity-and-authorization-foundation.md`
completion verification on branch
`feat/PLAN-0013-identity-authorization-foundation` (33 commits over
`origin/main` `a72ec6d`). This report records what was executed, what passed,
and — explicitly — what could NOT be run in the available environment and
under what condition it must be re-run. No item below is marked complete on
partial evidence.

## Identity

| Item | Value |
| --- | --- |
| Base SHA | `a72ec6d2ac832e5bdb10bf5547a5e99fbbcb39bb` |
| Branch | `feat/PLAN-0013-identity-authorization-foundation` |
| Candidate SHA (code head) | `f3a7544` (32 commits over base; this audit lands as the 33rd, docs-only) |
| Branch CI (authoritative for infra-backed gates) | 35370573249 at `f3a7544` — green, all 10 jobs |
| Prior branch runs (failure→fix loop) | `35366546137`, `35367327662`, `35367995098` (see CI loop table) |

## Result

The full chain `External OIDC subject → PlatformUser → TenantMembership →
OrganizationMembership → RoleBinding → Permission + ResourceScope → Policy
Decision → Application Use Case → Audit/Outbox` is implemented and exercised
end-to-end through the real composition root on both backends. The
Governance API migrated from the fixed `ManagementPermission` enum to the
unified `Authorize` decision path with the versioned compat bridge pinned by
tests (claim-grant parity while the bridge flag is on, claim-only denial with
it off, and membership/user/permission overrides on top). Every automated
gate passes locally and green on run `35370573249` in CI.

The first two CI attempts of this branch died inside the real-PostgreSQL job
and both were genuine latent defects that only a real `PostgreSQL` run could
expose — a legacy-schema collision in the migration chain, and an `int4`/`i64`
decode divergence that made duplicate-key handling return a database error
instead of `AlreadyExists` on `PostgreSQL` while the identical SQLite
contract suite stayed green. Both were fixed at the root cause, and run
`35370573249` proves the whole chain green.

## Completion Definition (plan §16) item by item

| # | Condition | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | PlatformUser / TenantMembership / Organization / Role / Permission / Binding / Scope implemented | PASS | `crates/identity`, `crates/organization`, `crates/policy` domain+application (commits `89db921..04bce84`), shared contract suites in `crates/identity-authorization-contracts`, adapters on PostgreSQL and SQLite (`81dbef3`) |
| 2 | Unified `Authorize` replaces the fixed Governance permission path with no behavior regression | PASS | `49dbdb4`: `authorize_governance(ManagementPermission)` → `authorize_permission(perm.as_str())`; BEFORE=AFTER pinned by `apps/business-api/tests/security.rs` (`compat_claim_grant_passes_guard_when_bridge_enabled`, `compat_flag_off_denies_claim_only_grant`, `role_binding_grant_works_with_compat_flag_off`, `suspended_membership_overrides_claim_grant`, `disabled_user_overrides_claim_grant`, `retired_permission_denies_claim_grant`) and the governance route tests in `documents.rs`/`security.rs` |
| 3 | Minimal management API and Console usable | PASS | `b6082ec`/`cae96d8` (`/api/v1/admin/*` IAM surface, OpenAPI-pinned); `14936f2` Console views (users, memberships, roles, bindings, explain); CI Frontend checks + Playwright smoke green on run `35370573249` |
| 4 | OIDC → PlatformUser → Policy E2E passes | PASS | `e2e_admin_bootstrap_sqlite` (local) and `e2e_admin_bootstrap_postgres` (CI `--include-ignored`) — full grant lifecycle: bootstrap admin → membership → role → binding → allow → explain → suspend→403 → stale-version→409 → reactivate → revoke→403, through `composition::build_app` (`e9eda5e`, `a4b3bb5`) |
| 5 | Negative cases green (revoke / suspend / cross-tenant / unknown permission …) | PASS | `iam_adversarial_sqlite.rs` (header spoofing inert, claim widening bounded, `roles` claim inert, contradicted `user_id` claim → 401, self-escalation trio, cross-tenant probes fail closed); unknown/retired permission tests (`retired_permission_denies_claim_grant`, policy decision matrix `5e2e0c4`) |
| 6 | Real PostgreSQL CI passes | PASS | Run `35370573249` integration job (`--include-ignored`): `apply_migrations_from_empty ... ok`; `postgres_identity_contract_suite_passes`, `postgres_organization_contract_suite_passes`, `postgres_policy_contract_suite_passes` all `ok`; full-chain E2E `bootstrap_admin_grant_suspend_reactivate_revoke_chain_postgres ... ok`; perf harnesses emitted their `PERF-EVIDENCE` lines |
| 7 | SQLite contract tests pass where support is declared | PASS | `identity-sqlite` / `organization-sqlite` / `policy-sqlite` contract suites run unignored in CI and locally (fresh temp DBs); SQLite declared local-single-process only |
| 8 | Audit / Outbox consistent with existing rules | PASS | All IAM writes land audit + outbox in the same fenced transaction (adapter unit-of-work); `audit-events` admin view reuses the existing audit store; no new outbox consumer semantics; contract suites assert audit/outbox rows per command |
| 9 | OpenAPI, Architecture Fitness, fmt / check / clippy / test / security gates | PASS | Local: `fmt` PASS, `clippy --workspace --all-targets --all-features -D warnings` clean, `check-architecture.ps1` PASS, `check-openapi.ps1` PASS, workspace `cargo test --all-features`: 163 suites, 692 passed, 0 failed, 42 ignored (PG/MinIO-gated). CI: Format, Check, Clippy, Architecture Fitness, Unit tests, CLI and MCP contracts, Security scanning, Frontend checks, Frontend Playwright smoke all green (run `35370573249`) |
| 10 | Completion Audit records PASS / NOT RUN and environment limits | PASS | This document; nothing is recorded as passed without executed evidence |
| 11 | No Contract domain / Workspace / Agent Capability / OAuth server implemented | PASS | No `contract` bounded context created; `contract_authorization_fixture.rs` is a pure in-memory authorization fixture (WP-11, `28297e4`) with no Contract domain, storage, or API; no OIDC Authorization Server, no session/refresh server; fitness scans (`3f49d9d`) keep it that way |

## CI failure→fix loop (branch)

| Run | HEAD | Failure | Fix |
| --- | --- | --- | --- |
| `35366546137` | `14017fe` | PostgreSQL job: migration 19 aborted — `relation "roles" already exists, skipping` then `column "stable_key" does not exist`: `001_initial.sql` (PLAN-0001 demo schema) reserved `roles`/`role_permissions`/`permissions`/`user_roles` with incompatible shapes and `CREATE TABLE IF NOT EXISTS` silently kept the legacy set | `9259898`: rename legacy set to `v0_demo_*` in 019 (recoverable, no drop; verified no shipped code reads it) + MANIFEST regen |
| `35367327662` | `7d33338` | same (fix not yet pushed) | — |
| `35367995098` | `9259898` | PostgreSQL job: shared policy contract `duplicate tenant stable key must be AlreadyExists` failed — the first-ever real PG run of the IAM suites exposed that the PG existence pre-checks used `SELECT 1` decoded as `i64`: PostgreSQL describes the bare literal as `int4`, so sqlx refused the decode exactly when a row was found (first create: no row → passed; duplicate: decode error instead of `AlreadyExists`). SQLite's dynamic typing hid the divergence | `f3a7544`: `SELECT 1::BIGINT` at all three probe sites + constraint comment; swept the new PG adapters for the same class (`COUNT(*)`/`EXISTS` are safe) |
| `35370573249` | `f3a7544` | — | **all 10 jobs success** |

All other nine jobs were green on every one of these runs.

## Performance evidence (plan §14)

Harnesses: `crates/identity-postgres/tests/perf_resolve.rs`,
`crates/policy-postgres/tests/perf_authorize.rs` (`e974f3d`), executed by CI
with `--include-ignored --nocapture` (`ci.yml` lines 226–227):

```text
PERF-EVIDENCE resolve_existing       samples=2000 p50_us=1215 p95_us=1282 p99_us=1427
PERF-EVIDENCE resolve_provision      samples=200  p50_us=3369 p95_us=3701
PERF-EVIDENCE authorize              samples=2000 bindings_per_user=100 p50_us=3558 p95_us=3778 p99_us=5048
PERF-EVIDENCE list_bindings_for_user p50_us=2044 p95_us=2733
```

These are evidence lines, not SLO claims: they characterize
identity resolution and authorize decisions under the plan's bounded data
shapes on the CI runner class.

## Stage reviews

| Stage(s) | Reviewer record | Verdict disposition |
| --- | --- | --- |
| 1 (identity), 2 (organization), 3+4 (policy core) | Findings fixed and cited in-code by `d00e508`, `704120a`, `e12ef95`, `741f9f9` during the first working sessions; no standalone verdict document was persisted for these stages (recorded as a process gap — the task's per-stage fixed-format review records were only archived from Stage 5 onward) | findings resolved |
| 5+6 (persistence) | `docs/reviews/PLAN-0013-STAGE5-6-PERSISTENCE-REVIEW.md` | FAIL (1 BLOCKER, 3 MINOR, 5 NIT) → all fixed in `488f011` |
| 7 (unified Authorize + bootstrap) | `docs/reviews/PLAN-0013-STAGE7-AUTHZ-REVIEW.md` | FAIL (1 BLOCKER, 2 MINOR, 3 NIT) → all fixed (`13ac9de`, `49dbdb4`) |
| 8 (management API) | `docs/reviews/PLAN-0013-STAGE8-MANAGEMENT-API-REVIEW.md` | PASS (2 MINOR fixed in `cae96d8`; create-role non-atomicity NIT accepted and documented) |
| 10+11 (E2E + adversarial) | `docs/reviews/PLAN-0013-STAGE10-11-E2E-REVIEW.md` | PASS (3 MINOR fixed in `7d33338`; JWKS-task-leak and prod-transport-through-`build_app` NITs accepted) |

## Security evidence

- Identity is the verified JWT only: spoofed `x-user-id`, `x-tenant-id`,
  `x-management-permissions`, `x-permissions`, `x-role` headers are inert
  under OIDC (`header_spoofing_is_inert_under_oidc`).
- `roles` claim never grants (inert entirely); `management_permissions`
  claims grant only the seven governance keys while the versioned compat
  bridge flag is on, never IAM/reserved keys, and the bridge denies when the
  flag is off (`claim_widening_stops_at_the_compat_bridge_boundary`,
  `roles_claim_alone_unlocks_nothing`, security.rs compat tests,
  `reserved_contract_key_is_unreachable_through_compat_claims`).
- Valid-signature token with contradicted `user_id` claim → 401
  (`foreign_user_id_claim_fails_closed`); wrong audience and anonymous → 401.
- Tenant isolation: cross-tenant reads/mutations → 404, foreign-tenant access
  → 403, tenant-scoped grants never follow the subject across tenants,
  explain answers caller-tenant truth only (`cross_tenant_probes_fail_closed`).
- Self-reaction guard: self-suspend legal, self-reactivation impossible;
  a suspended bootstrap admin is denied at the policy gate before the use
  case (defense in depth) (`self_escalation_trio_denied`).
- Bootstrap admin is server-controlled and explicit: forbidden
  “first user is admin” logic verified absent (`no_bootstrap_means_no_admin_and_no_http_bootstrap_surface`:
  without `[auth.bootstrap]` there is no admin and no HTTP bootstrap route at
  all); idempotent replay 200, key-reuse-conflict 409; production requires
  explicit opt-in (`config.validate()` fail-closed).
- Unknown/retired permissions deny by default (default-DENY decision matrix,
  `5e2e0c4`); revoked binding stops authorization on the next decision
  (fixture `revocation_stops_authorization_on_the_next_decision`).
- Architecture fitness scans (`3f49d9d`, §15 source scans) keep client-side
  role/permission injection, contract-key reachability, and the migration
  manifests pinned; the security job (cargo-audit + gitleaks + trivy) is
  green.
- No JWT, secret, lease token, raw subject, or connection string enters audit
  payloads, API error bodies, or logs (adapter-level payload tests;
  gitleaks green).

## NOT RUN / environment limits (honest record)

| Item | Status | Condition to run |
| --- | --- | --- |
| PostgreSQL tests locally (`--include-ignored`) | NOT RUN locally — no PostgreSQL or Docker on this machine | Proven in CI instead: every PG-gated test (identity/org/policy contracts, IAM E2E, perf harnesses, migration tests, document/governance suites) executes in the `integration` job with `--include-ignored` |
| MinIO-backed suites locally | NOT RUN locally | Same as above (CI) |
| Load/capacity beyond the two perf harnesses | NOT RUN | Pre-production environment with production data shapes |
| Stage 1–4 fixed-format reviewer verdict documents | NOT PERSISTED as standalone records (findings themselves were fixed in the cited commits) | Reconstruct only if the independent acceptance review requires it |

## Known limitations (accepted for the candidate)

1. `list_roles` and `list_tenant_users` use bounded N+1 permission/catalog
   lookups (page size capped); acceptable at management-plane volumes.
2. `create_role` composes create + set-permissions as two commands; a crash
   between them converges on idempotent replay (documented in the review doc).
3. Compat bridge: verified-claim `management_permissions` still grant the
   seven governance keys until the sunset flag flips — by design, versioned,
   default-on, single-flag off-switch; the flag off path is test-pinned.
4. Dev-mode auth and dev-pinned bootstrap version (`version=1`) are
   development-profile only; production config fails closed.
5. Architecture §15 source scans are lexical; renamed-import evasion remains
   theoretically possible (accepted risk, per plan).
6. Self-suspend lockout of a sole bootstrap admin is legal by design
   (baseline §10); recovery is a re-deploy with an explicit bootstrap config.

## Stop condition

PLAN-0013 stops here as **Accepted Candidate**: `main` was not touched, the
plan is not archived, no Contract / PLAN-0006 / Workspace / Agent Capability
work was started, and the branch is preserved for independent review and an
explicit integration decision.
