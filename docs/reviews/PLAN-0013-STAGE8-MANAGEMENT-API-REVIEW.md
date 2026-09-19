# PLAN-0013 Stage 8 — Management API Review Disposition

- Review target: commit `b6082ec` (`feat(api): IAM management REST surface
  with bounded OpenAPI contract`) — 23 admin operations,
  `public-api-contracts::iam`, OpenAPI + client mirror, 21+ API tests.
- Reviewer: independent read-only acceptance reviewer (out-of-band pass).
- Verdict: **PASS** (no BLOCKER, no MAJOR).

## Findings and disposition

1. **[MINOR] OpenAPI response-code coverage incomplete** — 403 was
   documented only on some list GETs and explain although every one of
   the 23 operations default-denies with 403, and the
   create-membership POST's reachable 404 (`user_id` unknown) was
   undocumented.
   **Fixed.** All IAM management operations now document 403, and the
   membership POST documents 404. The gate now enforces the 403 pin
   (finding 3), so this cannot silently regress.

2. **[MINOR] Membership creation was gated by `identity.user.manage`**
   while the versioned catalog describes `identity.membership.update`
   as governing "create, suspend, and reactivate tenant memberships"
   (`crates/policy/src/catalog.rs`). Granting per the catalog
   description would 403; granting user-manage would silently carry
   membership-creation authority.
   **Fixed.** `POST /api/v1/admin/tenant-memberships` is now gated by
   `identity.membership.update`. `identity.user.manage` gates no route
   (user-status mutations arrive with their own follow-up surface; the
   constant comment records this). Grant matrix updated:
   `identity.membership.update` creates memberships; a dedicated test
   (`user_manage_grant_cannot_create_memberships`) pins that a
   user-manage grant is denied.

3. **[NIT] `check-openapi.ps1` pinned only 3 of 17 new paths.**
   **Fixed.** All 17 IAM management paths are now required paths, plus
   two new fitness pins: every IAM management operation must document
   403, and `RoleBindingView` must keep `created_at`/`updated_at` as
   required fields.

4. **[NIT] `create_role` with initial permissions is two sequential
   durable commands** (role create + permission-set) and a failed grant
   leaves the role created behind a 400.
   **Accepted, recorded as a known limitation.** The replay with the
   same idempotency key converges (the failure is retried as a pair),
   the role is tenant-owned with zero permissions (no authority leak),
   and the non-atomicity is documented in the DTO. A single-command
   composition is a policy-context change and out of Stage 8 scope.

## Also verified in this round (coordinator)

- The reviewer PASS covers `b6082ec`; the coordinator additionally
  re-ran after the fixes: `cargo clippy -p business-api --all-targets
  --all-features -- -D warnings` (0), `cargo test -p business-api
  --all-features` (all suites green), and
  `scripts/check-openapi.ps1` (PASS with the new pins).
- Reviewer NOT-VERIFIED items (PostgreSQL-backed management surface,
  OIDC production principal path, workspace-wide gates) are the
  explicit subject of Stage 10 (E2E through the real composition on
  SQLite locally and PostgreSQL in CI) and the final full-gate pass;
  they remain open evidence gaps until then and are listed in the
  completion audit.
