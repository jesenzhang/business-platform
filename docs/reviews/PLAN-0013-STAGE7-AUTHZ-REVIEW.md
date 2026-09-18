# PLAN-0013 Stage 7 Review — Governance Authorization Migration (independent read-only)

- Date: 2026-09-18
- Reviewed: committed state `49dbdb4` (`git diff 488f011..49dbdb4`, apps/business-api only).
- Verdict returned: **FAIL** (1 BLOCKER, 2 MINOR, 3 NIT). All findings fixed in the
  follow-up commit (this document's companion).

## BLOCKER 1 — dev-auth startup always crashed: shared https validation applied to the locked urn issuer

`run_dev_auth` called `BootstrapAdministratorConfig::validate()`, which requires an
`https://` issuer, but the locked dev resolver issuer is the fixed constant
`urn:business-api:dev-auth` (preflight §5). Every dev/demo/e2e boot would abort at
`dev-auth bootstrap failed`. No test covered `run_dev_auth`.

Disposition — **fixed + pinned**:
- `run_dev_auth` now validates dev-scoped: issuer must equal the `DEV_AUTH_ISSUER`
  constant (with length cap), tenant non-nil, subject non-empty/NUL-free/length-capped,
  version ≥ 1 — mirroring the production `validate()` except the operator-facing
  https rule, with an explicit code comment on why the shared entry point cannot apply.
- New test file `apps/business-api/tests/dev_auth_bootstrap.rs`:
  runs Executed → NoOp convergence on the `urn` issuer; tampered stored digest at the
  same version → `ConfigStale` fail-closed; version bump → documented re-execution;
  claimed dev user id conflicting with the external link → `PrincipalMismatch`;
  disabled config inert; wrong issuer and nil user rejected before any port call.

## MINOR 1 — dev/prod bootstrap metric parity → fixed

`run_dev_auth` is now a thin wrapper that records `bootstrap_outcome_total` exactly
once for every outcome class (including early Config/ConfigStale exits), matching the
production wrapper; the step runner no longer records inline (double-count hazard
removed).

## MINOR 2 — contract-suite crate in the runtime dependency graph → fixed

`identity-authorization-contracts` moved from `[dependencies]` to
`[dev-dependencies]` of `business-api` (referenced nowhere in `src/`; the runtime
graph must not depend on the test-only contract suite — the deliberate local
`system_role_id` copy stays pinned by the contract suites).

## NIT dispositions

1. `AppState.access` doc said missing services yield a retryable 503; behavior and
   directive are 403 → **doc corrected**.
2. Dev bootstrap version pinned at 1 means post-`Executed` dev config changes fail
   closed as `ConfigStale` → **documented** at the call site with the throwaway-database
   recovery procedure (delete the ledger row / fresh db; never reuse dev ledgers).
3. Dev `record()` error mapping diverged from production (PrincipalMismatch vs
   collapse-to-Failed) → **aligned** (non-`Unavailable` ledger errors → `Failed`).

## Reviewer-verified positives (no action)

Middleware ordering proof (auth outermost; CORS preflight cannot reach handlers;
`x-management-permissions` stripped on every path); compat fidelity (closed 7-variant
enum, byte-identical keys, flag-off empty grants, evaluator order deny-before-bridge,
IAM/reserved keys unreachable); BEFORE=AFTER one-for-one across all 15 governance
handlers with identical keys and preserved messages; fail-closed status mapping
(401/403/503) with generic client bodies and debug-level-only error detail;
bootstrap production path verbatim through the use case with the canonical service
actor; org bridge walk bounded (64) and cycle-safe; metrics labels bounded enums, no
double counting; security.rs tests discriminate in both directions.
