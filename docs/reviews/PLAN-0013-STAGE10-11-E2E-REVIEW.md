# PLAN-0013 Stages 10/11 — Composition + E2E/Adversarial Review Disposition

- Review targets: `e9eda5e` (composition extraction) and `a4b3bb5`
  (full-chain E2E + adversarial matrix + CI line).
- Reviewer: independent read-only acceptance reviewer (out-of-band pass).
- Verdict: **PASS** (no BLOCKER, no MAJOR).
- Reviewer's mechanical verdict evidence worth keeping: the extraction
  was diffed against the pre-refactor `main.rs` and confirmed
  "moved, not edited" (startup order, both metrics installs, bootstrap
  branch conditions, OIDC transport policy, router creation all
  byte-identical); the test-only diff contains no production code
  besides the extraction; the CI line sits in the PostgreSQL job with
  `DATABASE_URL`, `--include-ignored`, `--test-threads=1`.

## Findings and disposition

1. **[MINOR] fmt violation at the reviewed commits** (`grant(...)`
   call in `iam_admin_api.rs` was rustfmt-expanded at HEAD but the fix
   was uncommitted at review time).
   **Fixed** — `14017fe style(api): rustfmt the user-manage probe
   helper call`; `cargo fmt --all -- --check` is green at HEAD after
   that commit.

2. **[MINOR] Cross-tenant explain assertion was vacuous** (asserted
   only the status of an already-in-tenant explain and discarded the
   body, while the comment claimed a cross-tenant denial).
   **Fixed.** The suite now asserts the explain pair: before the
   foreign identity joins the caller tenant it answers the bounded
   in-tenant truth `allowed=false, reason=deny_no_membership` (no
   existence leak, no cross-tenant evaluation), and after the
   RoleBinding grant the same call answers
   `allowed=true, reason=allow_role_binding`. Note the explain use
   case is tenant-scoped by construction (`AuthorizationContext::new
   (body.user_id, authz.tenant_id)`), so `DenyCrossTenant` is an
   engine-internal defense not reachable through the API surface; the
   bounded membership answer is the observable contract.

3. **[MINOR] §11 residuals**: `roles`-only inertness not isolated on
   the real composition; the named `x-role` header never sent; the
   `user_id`-claim conflict never exercised through HTTP.
   **Fixed.** New tests: `roles_claim_alone_unlocks_nothing` (roles
   claim alone → 403 on both IAM and governance surfaces),
   `foreign_user_id_claim_fails_closed` (valid signature, contradicted
   `user_id` claim → 401 principal conflict through the middleware),
   and the header-spoof probe now sends `x-role` too.

4. **[NIT] Harness JWKS task leaks until process exit; production
   transport-policy branch not exercised through `build_app`.**
   **Accepted.** Per-process test listeners die with the test binary;
   the production transport policy has its dedicated unit coverage
   (`oidc_auth.rs` production-transport tests) and the composition
   passes the flag straight through.

## Post-fix gate evidence (coordinator)

- `cargo test -p business-api --test iam_adversarial_sqlite` => 6
  passed (was 4; +`roles_claim_alone_unlocks_nothing`,
  +`foreign_user_id_claim_fails_closed`).
- `cargo clippy -p business-api --all-targets --all-features -- -D
  warnings` => exit 0; `cargo fmt --all -- --check` => clean.
