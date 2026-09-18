# PLAN-0013 Preflight / Design Lock

> Status: Stage 0 (Design Lock)
> Date: 2026-09-18
> Branch: `feat/PLAN-0013-identity-authorization-foundation`
> BASE_SHA: `a72ec6d2ac832e5bdb10bf5547a5e99fbbcb39bb`
> START_TIME: 2026-09-18T05:19:46Z
> Authority: ADR-0024 → IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md → PLAN-0013
> This document records verified current-code facts and locks the design before
> implementation. Facts were re-audited against the code at BASE_SHA, not
> assumed from the plan.
> Stage 0 independent review: PASS (no blockers); the three MAJOR amendments
> requested by the reviewer are incorporated in §§5, 11, and 15.

## 1. Current authentication (verified facts)

- OIDC validation lives only in `apps/business-api/src/oidc.rs`: ES256/RS256
  allow-list before key lookup, mandatory `kid`, JWKS discovery/override with
  HTTPS enforcement in production, 600 s cache with unknown-kid refresh, 60 s
  leeway, issuer always enforced, audience enforced when configured (production
  config requires it), every failure (JWKS outage, bad signature, unknown kid)
  is a 401 fail-closed. This path is **not modified** by PLAN-0013.
- `AuthenticatedPrincipal` (`auth.rs`): private fields `tenant_id: Uuid`,
  `user_id: Uuid`, `subject: String`, `roles: BTreeSet<String>`,
  `permissions: BTreeSet<ManagementPermission>`, `authentication_type`.
  `new()` rejects nil ids/blank subject/blank roles. Claim mapping requires
  `sub`, UUID `tenant_id`; `user_id` falls back to UUID-parseable `sub`;
  unknown `management_permissions` strings are dropped (fail closed).
- `ManagementPermission` stable strings (must be preserved exactly):
  `audit.read, integrity.read, integrity.scan, repair.dry-run, repair.execute,
  repair.approve, repair.cancel`.
- Dev auth: server-config principal only (`dev_secret` exact match,
  `dev_tenant_id/dev_user_id/dev_subject/dev_roles/dev_permissions`);
  `x-tenant-id`/`x-user-id`/`x-management-permissions` request headers are
  ignored/stripped. Production config validation forbids every dev-auth field,
  requires https issuer + audience + json logs + no sqlite backend.
- Middleware order (protected router only): request-id → trace → CORS → body
  limit → timeout → compression → metrics → `auth_middleware` → handlers.
  `TenantContext` is constructed from the authenticated principal inside
  `auth_middleware`. Public routes: `/health/live`, `/health/ready`,
  `/metrics`.
- Current authorization: `require_permission(&principal, ManagementPermission)`
  in `routes/admin.rs` → 403. No role strings are consulted for authorization
  anywhere (`roles()` is carried but never used to grant).

## 2. Current management API inventory (governance)

| Route | Permission | Application call | Audit |
|---|---|---|---|
| POST `/admin/integrity/scans` | integrity.scan | IntegrityScanPort::run | scan-run row only |
| GET `/admin/integrity/scans[/{id}]` | integrity.read | IntegrityQueryPort | read |
| GET `/admin/integrity/findings[/{id}]` | integrity.read | IntegrityQuery/Persistence | read |
| POST `/admin/repairs/dry-run` | repair.dry-run | dry_run_repair | read |
| POST `/admin/repairs` | repair.execute | RepairRun + create_repair_execution | `repair.created` |
| GET `/admin/repairs/{id}` | repair.execute | load_run | read |
| POST `/admin/repairs/{id}/approve` | repair.approve | approve_repair (ledger) | ledger |
| POST `/admin/repairs/{id}/execute` | repair.execute | execute_repair (ledger) | ledger |
| POST `/admin/repairs/{id}/cancel` | repair.cancel | cancel_repair (ledger) | ledger |
| POST `/admin/repairs/{id}/resume` | repair.execute | resume_repair (ledger) | ledger |
| GET `/admin/audit-events[/{id}]`, POST `/admin/audit/verify-chain` | audit.read | AuditQuery | read |

Negative tests to preserve: `apps/business-api/tests/security.rs`,
`documents.rs`, `oidc_auth.rs` (full OIDC fail-closed matrix + header
inertness), `plan_0008_postgres_minio.rs`.

## 3. Current persistence patterns (verified)

- Aggregate pattern: all-private fields + validated `rehydrate` (document);
  strong-typed `AggregateVersion(i64>0)`; repository `load/save(expected_version)`
  with `RepositoryError::{Unavailable,Conflict,Failed}`; application UoW ports
  persist aggregate + audit + outbox + idempotency atomically.
- PostgreSQL: root `migrations/NNN_name.sql` (next number **019**), embedded via
  `runtime_migration::MIGRATOR`; per-tenant `pg_advisory_xact_lock` for audit
  chain + idempotency; optimistic `WHERE tenant_id=$ AND id=$ AND version=$`;
  keyset `(created_at,id) < ($,$) ORDER BY ... DESC LIMIT n+1`; `ILIKE ...
  ESCAPE '\'` with `escape_like_literal`.
- SQLite: per-crate `migrations/` + `sqlx::migrate!`, pool ≤ 4,
  `BEGIN IMMEDIATE` via single acquired connection, manual COMMIT/ROLLBACK.
  Runner `apps/migration` chains each crate migrator.
- Audit: `crates/audit` zero-IO `AuditEvent` (hash-chained per tenant,
  `stream_sequence`, redaction list, `sanitize_details_for_read`);
  `audit_postgres::append_postgres_in_transaction(conn, event)` /
  `audit_sqlite::append_sqlite_in_transaction(conn, event)` are the shared
  in-transaction writers used by business UoWs. Outbox via
  `messaging::ReliableOutbox::append_in_tx`.
- Adapter contract tests run one suite against both backends
  (`document-persistence-contracts` pattern).
- Fitness: `architecture-check` (cargo-metadata layer deps) + string scans in
  `scripts/check-architecture.ps1`; migration filename + MANIFEST.sha256 gates.
- Console: React 19 + Vite + TanStack Query/Table + zustand; routes in
  `App.tsx`, pages in `pages.tsx`, `api.ts` envelope unwrap; scripts
  `dev/build/lint/typecheck/test/test:e2e`.

## 4. Target model (owners locked)

| Bounded Context | Crate | Owns tables |
|---|---|---|
| identity-management | `crates/identity` (+`identity-{postgres,sqlite}`) | `platform_users`, `external_identities`, `tenant_memberships`, `platform_bootstrap_executions`* |
| organization | `crates/organization` (+`organization-{postgres,sqlite}`) | `organization_units`, `organization_memberships` |
| policy | `crates/policy` (+`policy-{postgres,sqlite}`) | `permission_definitions`, `role_definitions`, `role_permissions`, `role_bindings` |

\* bootstrap ledger is written only by the bootstrap application service.

### Identity

- `PlatformUser { user_id (stable), lifecycle_status Active|Disabled,
  created_at, updated_at, version }`. Identity is **never** decided by
  display name/email. Disabled user ⇒ no authorization anywhere; historical
  references are never deleted (soft status only).
- `ExternalIdentity { external_identity_id, issuer, subject, user_id,
  linked_at, version }` with uniqueness **(issuer, subject) → exactly one
  user_id** enforced by DB unique index and domain conflict error. A `user_id`
  already bound to a different (issuer,subject) fails closed — no silent
  rebind.
- `TenantMembership { tenant_id, user_id, status Active|Suspended,
  joined_at, suspended_at?, source (bootstrap|admin|migration), version }`.
  Tenant boundary is (tenant_id, user_id). No active membership ⇒ no tenant
  business authorization (enforced in resolver, evaluator, and API).
- Statuses stay `Active|Suspended` (user: `Active|Disabled`). No HR lifecycle.

### Organization (authorization-only minimum)

- `OrganizationUnit { org_unit_id, tenant_id, parent_org_unit_id?, type
  (company|department|team), name, status Active|Disabled, version }`.
  Invariants: parent must exist in **same tenant**, no self-parent, no cycles
  (path walk bounded), disabled unit ⇒ no new members/bound scope use.
- `OrganizationMembership { tenant_id, user_id, org_unit_id, membership_type
  (member|leader), status Active|Inactive }`. Uniqueness
  (tenant,user,org_unit,type).
- No Position/Job/Salary/Performance/Attendance.

### Policy

- `PermissionDefinition { stable_key UNIQUE, description, active }`.
  Built-in seed (migration, ON CONFLICT DO NOTHING): the 7 governance keys
  (active) + reserved `document.read, document.review, contract.read,
  contract.create, contract.update, contract.review, contract.archive`
  (defined-but-reserved: no Application use case consumes them yet, so an
  allow can never produce a business operation) + IAM keys required to operate
  this plan's management plane: `identity.read, identity.membership.update,
  organization.read, organization.manage, policy.role.read, policy.role.manage,
  policy.binding.read, policy.binding.manage, policy.explain`.
  Stable-key collision at seed ⇒ fail closed (unique constraint, startup
  seed validation rejects duplicates). Unknown permission at decision time ⇒
  DENY.
- `RoleDefinition { role_id, scope system|tenant, tenant_id (NULL for system),
  stable_key, display_name, status Active|Disabled, version }`. System roles
  are **immutable** (created only by migration seed): `system.bootstrap-admin`
  (7 governance + 9 IAM permissions) and `system.platform-admin` (same set,
  bindable to others). Uniqueness: global for system stable keys,
  (tenant_id, stable_key) for tenant roles. No cross-tenant shared mutable
  role.
- `RolePermission { role_id, permission_key }` — plain set, replaced
  transactionally (SetRolePermissions), only for tenant roles.
- `RoleBinding { binding_id, tenant_id, user_id, role_id (same tenant or
  system), scope, status Active|Revoked, effective_at, expires_at?, version }`.
  Cross-tenant role reference rejected at write and read.
- `ResourceScope` variants (bounded enum, no DSL):
  `Tenant` | `OrganizationUnit { org_unit_id, include_subtree: bool }` |
  `ResourceType { kind }` | `Resource { kind, resource_id }`. A scope only
  ever narrows (binding with org scope never grants sibling orgs; subtree flag
  is the only expansion and is resolved from trusted organization data).
  Consumer justification (review MINOR): `Tenant` and `OrganizationUnit` are
  consumed by the governance/IAM path and org-scoped fixtures; `ResourceType`
  and `Resource` are consumed by the Stage 13 contract-authorization fixtures
  (task §20 requires proving tenant/org/resource-scoped distinction) — they
  exist as evaluator + fixture semantics only and no governance endpoint
  widens through them.
- `PolicyDecision { allowed, reason: DecisionReason (bounded enum),
  matched_permission?, matched_binding?, scope?, policy_reference }` — no
  internal state leak; raw detail only via admin ExplainDecision.
  DecisionReason: `AllowRoleBinding, AllowCompatManagementClaim,
  DenyNoUser, DenyUserDisabled, DenyNoMembership, DenyMembershipSuspended,
  DenyNoBinding, DenyBindingExpired, DenyBindingNotYetEffective,
  DenyBindingRevoked, DenyRoleDisabled, DenyUnknownPermission,
  DenyCrossTenant, DenyScopeMismatch, DenyInternal` (default DENY;
  `DenyInternal` is used for store failures — an unavailable policy store is
  **never** an allow).

## 5. Authorization flow (locked)

```text
auth_middleware (unchanged: OIDC/dev → AuthenticatedPrincipal + TenantContext)
  ↓ (inside protected router, one new middleware)
PlatformAuthorization middleware
  → identity.ResolveAuthenticatedUser(issuer, subject, claimed user/tenant)
     - provision-or-match PlatformUser by (issuer, subject)
     - require Active PlatformUser + Active TenantMembership  (else 403, fail
       closed; identity store unavailable ⇒ 503)
  → build AuthorizationContext { platform_user_id, tenant_id,
     compat_management_grants: BTreeSet<ManagementPermission> }
  → attach to request extensions
  ↓
Handler: requires only Authentication + AuthorizationContext
  → Application use case calls policy.Authorize(ctx, permission, resource)
```

- Resolver issuer for OIDC = configured `auth.issuer_url`; for dev-auth =
  fixed `urn:business-api:dev-auth` (server-side constant).
- Provisioned `user_id` = claimed `user_id` when present (trusted UUID);
  otherwise `uuid_v5(namespace, issuer|subject)`. Conflict with an existing
  different binding fails closed.
- Resolve-provision is a write-on-read path: concurrent first requests for one
  (issuer,subject) race on the unique index; the adapter must treat a
  **same-binding unique violation as match-and-continue** (re-read and
  proceed), and only a *different* existing binding fails closed. Every
  IdP-authenticated subject creates one permanent row (bounded by IdP trust —
  accepted).
- Token `roles` (arbitrary strings) grant **nothing** — unchanged, now
  additionally structurally impossible (evaluator input has no role-string
  path).
- Revocation semantics: every authorization re-reads membership status +
  bindings from the store; revoked/suspended state denies on the next request
  even with an unexpired JWT.

### Compatibility bridge (locked, deliberate decision)

`AuthenticatedPrincipal.permissions` sources are **server-trusted claim
injection** (`management_permissions` claim per ADR-0024 §4.2, or dev-auth
server config). They are kept as an explicit **compat grant**: the evaluator
allows a governance permission when compat grant contains it AND user Active
AND TenantMembership Active, with `reason = AllowCompatManagementClaim`
(metricked and explainable separately). Constraints: the set is bounded to the
7 built-in governance keys (already fail-closed); it never grants IAM or
reserved permissions; membership suspension or user disable overrides it; the
bridge is a config flag (`auth.management_permission_compat_enabled`,
default true) so a deployment that has moved to RoleBindings can turn it off.
Primary path for all new permissions (IAM + reserved) is RoleBinding.
`x-*` headers remain inert. This preserves "BEFORE = AFTER" for the governance
API without letting token `roles` become a general platform authority.
Known limitation (documented, accepted for compat phase): revoking a
*claim-injected* governance grant requires IdP-side change or membership
suspension, unlike platform bindings which revoke instantly.

**Sunset commitment (review MAJOR #2).** The baseline (§4) forbids a permanent
second authorization model. The bridge is therefore time-boxed: it exists only
while PLAN §12.3's "逐步改用 Authorize" cutover is in progress. Sunset
conditions, recorded here and tracked in `PLAN-0013-COMPLETION-AUDIT.md` as an
explicit follow-up: (a) the next (Contract) vertical slice consumes platform
RoleBindings as its only authority path; (b) deployment runbook guidance for
"IdP claim injection → explicit RoleBinding migration" ships; at that point the
flag default flips to **false** in a follow-up plan and the bridge is deleted
after one release of `AllowCompatManagementClaim`-zero observability. Within
PLAN-0013 the default stays `true` because hard cutover would break the
ADR-0024 §4.2 claim contract and existing deployments' behavior parity.

## 6. Management operations and safety rules

- Application contracts (names lock the semantics; Rust shapes may adapt):
  Identity: `ResolveAuthenticatedUser`, `GetUser`, `ListUsers`,
  `CreateTenantMembership`, `ChangeTenantMembershipStatus` (optimistic version,
  idempotency keys).
  Organization: `CreateOrganizationUnit`, `UpdateOrganizationUnit`,
  `MoveOrganizationUnit`, `AddOrganizationMember`, `RemoveOrganizationMember`,
  `ListOrganizationTree`.
  Policy: `CreateRole`, `UpdateRole`, `SetRolePermissions`, `BindRole`,
  `RevokeRoleBinding`, `Authorize`, `ExplainDecision`.
- Self-escalation invariants (domain, not handler):
  - `BindRole` rejects binding a role to the acting user when the role
    contains any IAM-management permission, unless the actor's access is
    already bootstrap-sourced (single-admin cold start); re-binding to self is
    still a no-op against an existing active binding (idempotent convergence).
  - `SetRolePermissions` rejects adding IAM-management permissions to a role
    that is currently bound to the acting user.
  - System roles are immutable (domain rejects any mutation).
  - `ChangeTenantMembershipStatus` cannot reactivate one's own suspended
    membership. "bootstrap-sourced" in the BindRole carve-out is evaluated as
    **an active binding to an immutable system role at decision time**, never
    a historical flag.
  - Self-DoS (admin suspends own membership / revokes own only binding) is
    allowed but audited (recovery via `[auth.bootstrap]` version bump);
    covered by an explicit test; runbook note in Stage 12 docs.
- All mutations: Idempotency-Key + expected_version + unified audit
  (`identity.*`, `organization.*`, `policy.*` action prefixes, chain-writer
  in-transaction) + outbox only where a domain event is contractually needed
  (none required by PLAN-0013; audit is mandatory, no fake events).

## 7. Bootstrap admin (locked)

No "first user becomes admin" anywhere. Two server-controlled modes only:

1. **Production explicit config bootstrap**: `[auth.bootstrap]` requires
   `enabled=true`, `tenant_id`, `issuer`, `subject` (all explicit). Runs once
   at startup in the composition root via `BootstrapAdministrator` use case:
   provision PlatformUser by (issuer,subject) → Active TenantMembership
   (source=bootstrap) → bind `system.bootstrap-admin` (Tenant scope) → unified
   audit → persist ledger row `platform_bootstrap_executions`.
   The use case lives in Identity and composes the binding **through the
   policy crate's `BindRole` application port** (never a direct write to
   `role_bindings`), keeping table ownership single-writer; the ledger row is
   identity-owned and written by the bootstrap service only.
   (tenant,issuer,subject,role,config_digest,version,outcome). Re-run with the
   same digest is a no-op **even after the binding was revoked**; repeating a
   bootstrap requires a deliberate config `version` bump (audited). HTTP never
   triggers bootstrap; there is no request surface that can enable it.
   Production config validation requires issuer https + explicit values when
   enabled.
2. **Dev-auth bootstrap** (dev/test only, dev_auth already production-forbidden):
   binds the dev principal (server-config tenant/user/subject) with
   `system.bootstrap-admin` using the same ledger + audit. This keeps
   `run-local-*.ps1`, `demo-*` scripts and existing SQLite flows working.

## 8. Persistence design (PostgreSQL authority; SQLite decision)

**SQLite: IN SCOPE — required, not optional.** The current local stack
(`run-local-document-processing.ps1`, `demo-*`, business-api non-PG tests,
`test-local-governance-repair.ps1`) runs business-api on SQLite. With
fail-closed identity resolution, omitting SQLite adapters would break every
existing local/demo flow. SQLite adapters implement the same contract suite.

- PG: one migration `migrations/019_identity_authorization_foundation.sql`
  (+ MANIFEST.sha256 update): 10 tables, FKs, unique constraints:
  `external_identities(issuer,subject)`, `tenant_memberships(tenant_id,user_id)`,
  `role_definitions` partial unique (system: stable_key WHERE tenant_id IS NULL;
  tenant: (tenant_id,stable_key)), `role_permissions(role_id,permission_key)`,
  `organization_units` parent FK same-tenant enforced in domain (FK on id +
  trigger-free domain check + cross-tenant read filter),
  `organization_memberships(tenant_id,user_id,org_unit_id,membership_type)`,
  `role_bindings(tenant_id,user_id,role_id) where status='active'` partial
  unique not required (multiple bindings allowed; hot-path indexes:
  `role_bindings(tenant_id,user_id,status)` incl. effective/expiry,
  `role_permissions(permission_key,role_id)`, memberships by (tenant_id,status),
  users list keyset (created_at,id)). version BIGINT default 1, updated_at.
  Seed inserts (permissions + 2 system roles + their RolePermissions) with
  ON CONFLICT DO NOTHING.
- SQLite: `crates/{identity,organization,policy}-sqlite/migrations/001_*.sql`
  each with `MANIFEST.sha256`; `apps/migration` chains the three new migrators
  (up+status); `scripts/check-architecture.ps1` MANIFEST/filename gates extended
  to the new directories; existing gate expectations (numeric prefix, strictly
  increasing) honored.
- Optimistic concurrency: every mutation `UPDATE ... WHERE tenant_id=$ AND
  id=$ AND version=$expected` → `Conflict`; same-key idempotent replay
  converges (document idempotency pattern with per-key lock; BEGIN IMMEDIATE
  on SQLite).
- Adapter contract suite crate:
  `crates/identity-authorization-contracts` (pure async test functions per
  port: uniqueness, tenant isolation, version conflict, expiry/effective,
  revoked, scope lookups, keyset pagination, LIKE escaping, replay convergence)
  executed by both `-postgres` and `-sqlite` integration tests (PG under
  `#[ignore]` + CI `--include-ignored`, matching repo convention).

## 9. REST surface (Stage 8) — all under existing `/api/v1` conventions

Read: `GET /admin/users`, `GET /admin/users/{userId}`,
`GET /admin/tenant-memberships`, `GET /admin/permissions`,
`GET /admin/roles`, `GET /admin/roles/{id}`,
`GET /admin/role-bindings`, `GET /admin/organization-units` (flat + tree),
`GET /admin/organization-units/{id}/members`.
Write (Idempotency-Key + expected_version where update + audit):
`POST /admin/tenant-memberships`,
`POST /admin/tenant-memberships/{userId}/suspend|reactivate`,
`POST/PATCH /admin/organization-units(+/{id}, POST /{id}/move,
POST|DELETE /admin/organization-units/{id}/members/{userId})`,
`POST/PATCH /admin/roles`, `PUT /admin/roles/{id}/permissions`,
`POST /admin/role-bindings`, `POST /admin/role-bindings/{id}/revoke`,
`POST /admin/authorization/explain` (needs `policy.explain`).
Envelope/`ApiResponse`/`ErrorResponse`/cursor-pagination/opaque versioned
cursors/stable error codes as existing. DTOs in `public-api-contracts`;
`business-api-client` methods per endpoint; root `openapi.json` updated
(+ check-openapi.ps1 required paths for `admin/users`, `admin/roles`,
`admin/role-bindings`). Handlers contain no business rules.

## 10. Observability

- Counter `authorization_decisions_total{decision,reason}` (reason = bounded
  DecisionReason enum only) + histogram `authorization_duration_seconds`;
  counters `platform_mutation_total{kind,outcome}`, `bootstrap_outcome_total`
  (bounded outcome enum). No user_id/tenant_id/resource/arbitrary-string
  labels. Audit details never contain JWT/raw token/claims payload (existing
  redaction list extended as needed).
- Existing `auth_failures_total` untouched.

## 11. Testing & gates plan

- Domain: pure unit tests per crate (invariants above).
- Application: in-memory fake ports (each crate ships `testing` fakes gated to
  cfg(test) or a `fakes` feature used by business-api tests).
- Adapter contract suite on both backends (CI includes ignored PG).
- API: extend security.rs matrix — cross-tenant (role/binding/org parent/IDOR),
  self-escalation (bind-to-self, role-edit-escalate, self-reactivate), header
  spoof (`x-role/x-permission/x-user-id/x-tenant-id/x-management-permissions`
  inert), unknown permission deny, expired/revoked binding, suspended
  membership, disabled user, JWT-issued-before-revoke, compat bridge parity
  BEFORE=AFTER on every governance route (existing 403/200 tests kept).
- E2E (`#[ignore]`, CI PG): OIDC (in-test ES256 JWKS, existing pattern) →
  provision → bootstrap-admin via config → governance read 200 → suspend →
  403 → reactivate+binding → revoke → 403; plus local SQLite equivalent smoke.
- Perf evidence (Stage 12): criterion-less focused timing in an ignored test or
  bench harness recording Authorize P50/P95/P99 and list P95 on local PG;
  recorded in Completion Audit. **No cache** unless evidence demands (none
  expected at this scale).
- Fitness (automated, in `architecture-check` + `check-architecture.ps1`):
  1. `identity/organization/policy` domain+application layers declare
     `layer=domain-and-application` → existing forbidden-dependency rule blocks
     sqlx/axum; **new work**: extend the `architecture-check` forbidden list
     for `domain-and-application`/`domain` layers with `jsonwebtoken` and
     `openidconnect` (verified gap: the current list in
     `crates/architecture-check/src/lib.rs` does not include them), so
     "Domain does not depend on JWT/OIDC" is enforced, not incidental;
  2. identity/organization/policy must not depend on contract/finance/approval/
     workflow/agent-integration;
  3. string scan: `crates/contract|customer|finance|project|approval` and
     `apps/business-api/src/routes/*` must not reference `role_bindings` /
     parse role strings (`roles().contains` scan with allowlist = none);
  4. scan: no Authorization-Server markers (token/JWK signing endpoints) —
     scan for `sign(` / `SigningKey` outside oidc tests + vendor;
  5. no generic policy DSL: scan policy crate for `parser|eval(uate)?_expr|dsl`
     markers (bounded enum design passes trivially);
  6. new SQLite migration dirs covered by existing manifest gates;
  7. business modules read RoleBinding only through policy ports (scan).

## 12. Overwritten/updated surfaces & doc sync (same change)

- New crates (10): 3 domain + 6 adapters + 1 contract-suite.
- `apps/business-api`: config (bootstrap + compat flag), authz middleware,
  state.rs `AccessServices`, governance route authorization calls, new admin
  routes, main.rs wiring both backends, metrics.
- `apps/migration`: chain new SQLite migrators.
- `crates/public-api-contracts` + `crates/business-api-client` + root
  `openapi.json`.
- `apps/business-console`: Users, Roles(+detail/permissions), Role Bindings,
  Organization, Explain panel (existing design system, `api.ts` methods).
- Docs: PLAN-0013 status → Active (branch only; Completion Definition
  untouched); completion audit at the end; `ARCHITECTURE_STATUS.md` + data
  ownership doc rows for new tables/contexts; SECURITY_ARCHITECTURE authZ
  section pointer; no new ADR needed (implementation inside ADR-0024 +
  IDENTITY baseline; the compat bridge is explicitly permitted by PLAN §12.3)
  — if final review disagrees, an ADR amendment is filed instead of silently
  proceeding.

## 13. Explicit non-goals confirmed (code will not contain)

OAuth/OIDC Authorization Server (no signing/credential issuance — ADR-0024),
passwords/MFA/refresh/session server, SCIM/LDAP/HR, Zanzibar/ReBAC graph,
ABAC/policy DSL, Contract/Finance/Approval/Workspace/AgentRun/Capability/
RAG/Workflow/Marketplace/plugin/GeneratedApp code. The `contract.read`-style
permissions exist only as catalog rows + Stage 13 authorization fixtures.

## 14. Risks accepted for Stage 0 review

1. Compat bridge keeps claim-based governance grants alive (bounded, flagged,
   config-disableable, membership-overridden). Rationale: PLAN §12.3 +
   behavior-parity requirement; alternative (hard cutover) breaks the ADR-0024
   claim contract and existing deployments.
2. IAM management permissions added to the first-batch catalog (not literally
   in PLAN §6) — required to operate the management plane without a
   bootstrap-superrole backdoor.
3. Resolver auto-provisions PlatformUser on first authenticated resolve —
   provisioned users have **no** membership and therefore zero authority;
   avoids an admin pre-provisioning chicken-and-egg. Admins create
   memberships explicitly.

## 15. Upgrade cutover for existing deployments (review MAJOR #1)

PLAN-0013 resolution is deliberately stricter than pre-PLAN-0013 behavior:
today a valid OIDC token with `management_permissions` reaches governance
endpoints with no platform state at all; after PLAN-0013 it additionally needs
an Active PlatformUser + Active TenantMembership (PLAN §5 invariant 2 —
legitimate tightening, not a bridge defect). Therefore upgrading an existing
production OIDC installation requires a cutover sequence, shipped as a Runbook
section (`docs/operations/PLAN-0013-UPGRADE-RUNBOOK.md`, Stage 12) and echoed
in the Completion Audit:

1. Before deploying the PLAN-0013 build, set `[auth.bootstrap]` with the
   intended initial admin's explicit `tenant_id`/`issuer`/`subject` (the
   subject must match their IdP `sub`), **or** pre-seed memberships + bindings
   via the documented migration path.
2. Deploy. Startup executes the idempotent bootstrap (audited;
   digest-stable re-runs are no-ops).
3. Verify `authorization_decisions_total{reason="AllowCompatManagementClaim"}`
   and `DenyMembership*` metrics for one observation window; grant remaining
   operators explicit RoleBindings.
4. Only existing governance access without membership flips 200→403 on
   upgrade — this is the intended fail-closed tightening; the runbook calls it
   out as the breaking operational change.
Dev/demo flows (SQLite, dev-auth) need no operator action: dev-auth bootstrap
runs automatically (production config forbids dev-auth).
