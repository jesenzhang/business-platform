# SaaS Reference Conformance Review：Identity / Organization / Policy

> Review date: 2026-09-23<br>
> Plan: PLAN-0014 Wave 0 / Wave 1<br>
> Base SHA: `8365de0211390a6a3367b536c5ceaa5256bbb5cf`<br>
> Branch: `feat/PLAN-0014-wave1-iam-policy-reference-conformance`<br>
> Current implementation baseline: [`2026-09-23-plan-0014-wave0-iam-policy-baseline.md`](2026-09-23-plan-0014-wave0-iam-policy-baseline.md)
> Scope: review and bounded evidence only; no external authorization runtime is activated

## 1. Review question and method

本 review 不比较 feature 数量，也不把 reference 的内部 Domain 当作本平台的 authority。问题是：当前 Identity、Organization、Policy 是否保留清晰的 owner、tenant boundary、default-deny、revocation/failure/idempotency 语义；哪些成熟实现证明了重要失败模式；哪些差异需要 ADOPT、ADAPT、KEEP、DEFER 或 REJECT。

源码 review 在每个固定 revision 的 core source、tests、migration/API 和 architecture/docs 路径进行。README 只用于产品边界，不作为行为证据。参考项目只做校准，不进入 Cargo workspace。

## 2. Fixed reference revisions

| Reference | Exact release/tag and commit | Review date | License | Core source inspected | Tests / docs inspected | Failure issue/PR or release evidence |
|---|---|---:|---|---|---|---|
| Logto | [`v1.43.0`](https://github.com/logto-io/logto/releases/tag/v1.43.0) / `d066df7d26d596b6ba7ad0bdfaaecfda9c612226` | 2026-09-23 | MPL-2.0 | [`packages/core/src/routes/organization`](https://github.com/logto-io/logto/tree/v1.43.0/packages/core/src/routes/organization), [`queries/organization`](https://github.com/logto-io/logto/tree/v1.43.0/packages/core/src/queries/organization), organization role/scope routes, [`oidc/grants/client-credentials.ts`](https://github.com/logto-io/logto/blob/v1.43.0/packages/core/src/oidc/grants/client-credentials.ts) | `organization/user/index.test.ts`, `role*.test.ts`, `koa-tenant-guard.test.ts`, adjacent OpenAPI files and end-user flows | Release notes include suspended-user token denial, scope revalidation and webhook retry/idempotency; [`#9410`](https://github.com/logto-io/logto/pull/9410) is a concrete retry/re-delivery reference. |
| ZITADEL | [`v4.18.0`](https://github.com/zitadel/zitadel/releases/tag/v4.18.0) / `6d7878a2e4128517684f43ba7774dd4a9f64ba36` | 2026-09-23 | AGPL-3.0 | [`internal/api/authz`](https://github.com/zitadel/zitadel/tree/v4.18.0/internal/api/authz), `internal/command`, `internal/query`, `internal/eventstore`, admin/management org and IAM APIs | authz unit tests, admin IAM integration tests, management org integration tests, service-account docs, organization/structure docs and audit-trail docs | Resource-based API migration and dynamic-client changes are tracked in [`#12313`](https://github.com/zitadel/zitadel/issues/12313); the fixed source was used for behavior, not the issue text. |
| ABP Framework | [`10.6.1`](https://github.com/abpframework/abp/releases/tag/10.6.1) / `9ca1d3ee62c8c557af34b50fc49c146405d752d4` | 2026-09-23 | LGPL-3.0 | `framework/src/Volo.Abp.Authorization*`, `framework/src/Volo.Abp.MultiTenancy*`, `modules/permission-management` | `framework/test/Volo.Abp.Authorization.Tests`, `framework/test/Volo.Abp.MultiTenancy.Tests`, Permission Management domain/application/EF/Mongo tests, authorization and multi-tenancy docs | The fixed repository test layout is the evidence. No ABP issue is promoted to a business-platform requirement; permission providers, EF/Mongo adapters and module conventions remain framework-specific. |
| OpenFGA | [`v1.21.0`](https://github.com/openfga/openfga/releases/tag/v1.21.0) / `ab557c5592670c899de35297e7aa067015f06502` | 2026-09-23 | Apache-2.0 | [`internal/check`](https://github.com/openfga/openfga/tree/v1.21.0/internal/check), [`pkg/server/commands`](https://github.com/openfga/openfga/tree/v1.21.0/pkg/server/commands), storage and authorization-model packages | `pkg/storage/test`, `pkg/server/commands/*_test.go`, [`tests/check`](https://github.com/openfga/openfga/tree/v1.21.0/tests/check), functional/list tests | [`CONTRIBUTING.md`](https://github.com/openfga/openfga/blob/v1.21.0/CONTRIBUTING.md) requires reusable storage/API/check contracts; dynamic conditions release work is [`#3313`](https://github.com/openfga/openfga/pull/3313). |
| SpiceDB | [`v1.56.2`](https://github.com/authzed/spicedb/releases/tag/v1.56.2) / `be3e1ef65427df44adf26bd01f54d4f0c5312893` | 2026-09-23 | Apache-2.0 | [`internal/dispatch`](https://github.com/authzed/spicedb/tree/v1.56.2/internal/dispatch), [`internal/caveats`](https://github.com/authzed/spicedb/tree/v1.56.2/internal/caveats), `pkg/datastore`, zedtoken and protocol packages | `pkg/datastore/test`, dispatch/caveat tests, `e2e/newenemy`, command integration tests | v1.56.2 changelog records datastore readiness timeout work [`#3305`](https://github.com/authzed/spicedb/issues/3305); the dispatch/caveat/revision source and tests establish the consistency model being compared. |
| Cerbos | [`v0.55.0`](https://github.com/cerbos/cerbos/releases/tag/v0.55.0) / `2901f6421b08bb544049bf0fae4e61ebfd52d59b` | 2026-09-23 | Apache-2.0 | [`internal/engine`](https://github.com/cerbos/cerbos/tree/v0.55.0/internal/engine), `internal/policy`, `internal/audit`, storage and public policy/engine API | engine/evaluator/policy tests, storage backend tests, public protobufs, tutorial policy tests and audit tests | Release/test history is treated as operational evidence only; Cerbos policy evaluation, derived roles, conditions and audit are reference behavior, not a mandate to add a PBAC runtime. |

### Reference reading conclusions

- Logto and ZITADEL demonstrate that external authentication, organization membership, application/service identity and organization-scoped roles are separate concerns; neither makes the business application’s tenant data owner implicit.
- OpenFGA and SpiceDB demonstrate explicit relation graphs, consistency/revision semantics, reusable adapter tests and the cost of a remote PDP/dispatch boundary. They do not justify replacing the current Policy authority before a real relation-graph consumer exists.
- Cerbos demonstrates contextual policy evaluation, derived roles/conditions and explicit decision/audit contracts. It is a policy-engine reference, not a reason to leak CEL/policy files into the Rust domain.
- ABP demonstrates module-local permission definition/provider composition and tenant-aware application boundaries. Its framework/module/database conventions are not portable Platform Core authority.

## 3. Current capability matrix — Identity

| Dimension | business-platform current fact | Reference evidence / conformance observation | Gap | Decision |
|---|---|---|---|---|
| Authority | Identity owns PlatformUser, ExternalIdentity and TenantMembership; OIDC verifies credentials only | Logto/ZITADEL separate IdP protocol from platform/organization records | No current service/workload principal owner | C3 DEFER until a consumer exists |
| Domain model | Private validated aggregates plus rehydrate; issuer+subject stable mapping | Logto users/organizations and ZITADEL users/orgs/service accounts are explicit records | One ExternalIdentity per PlatformUser; no account-linking requirement is established | C2 SHOULD, no Wave 1 change |
| Tenant | Active tenant membership gates every policy decision | ABP current-tenant and Logto/ZITADEL organization guards reinforce tenant context | No external tenant registry or cross-process PrincipalContext | KEEP current; C2 contract follow-up |
| AuthN/AuthZ | OIDC/JWKS/claims at delivery; Policy owns business authorization | ZITADEL/Logto show strong external IdP and internal management boundaries | None for current human consumer | KEEP |
| API / events | ResolveAuthenticatedUser and typed identity ports; no identity events added in this wave | ZITADEL has eventstore/commands; Logto has route/query boundaries | No published cross-module identity event contract | C1 only for future R2 release packaging; current R1 allowed |
| Lifecycle | User active/disabled; tenant active/suspended; versioned transitions | References include deleted/suspended/disabled and machine states | Tenant expiry/removal are not distinct; behavior is deny via missing/suspended | C2 SHOULD; do not invent states |
| Failure semantics | resolver/store failure is unauthorized or retryable unavailable; policy never allows on bridge failure | All references test invalid tokens, unavailable stores, or failed policy paths | Real current PostgreSQL failure lane not run locally | KEEP semantics; evidence gap is NOT RUN |
| Retry / idempotency | tenant-scoped operation/key fingerprint, replay vs conflict, advisory lock in PG | Logto retry notes and ZITADEL command/event patterns reinforce replay discipline | No externally published identity command artifact | KEEP; R2 blocker is packaging |
| Recovery | Bootstrap ledger and explicit version bump can recover suspended admin; no automatic first-user admin | ZITADEL/Logto provide explicit admin/service bootstrap paths | No independent recovery runbook/artifact for module | C1 R2 blocker |
| Version / migration | migration 019, private rehydrate, optimistic aggregate version | ABP and ZITADEL have module/migration discipline | migration mapping is workspace-level, not module release-level | ADAPT for R2 |
| Security | signed issuer/subject, headers inert, claimed user mismatch denied, no token roles grant IAM | Logto suspended-token and ZITADEL authz tests support immediate deny semantics | Service identity separation absent | KEEP current; C3 service identity |
| Observability / audit | bounded actor/tenant/operation/reason and same local transaction audit | ZITADEL audit trail and Logto event listeners show explicit audit surfaces | no independent identity audit export contract | ADAPT later; no current security gap |
| Performance | bounded resolution path; historical PG perf harness exists | References use caches/queries/events but do not transfer SLOs | current run has no fresh local PG percentile | NOT RUN; do not claim SLO |
| Release model | crate version `0.1.0` inside workspace; no module release identity | ABP modules and reference services have versioned artifacts | no independent manifest/digest/artifact/consumer build | C1 R2 BLOCKER |

## 4. Current capability matrix — Organization

| Dimension | business-platform current fact | Reference evidence / conformance observation | Gap | Decision |
|---|---|---|---|---|
| Authority | Organization owns units and membership; Identity is read through a port | Logto organizations and ZITADEL organizations/projects are first-class records | No generic relation graph owner | KEEP |
| Domain model | company/department/team, parent tree, active/disabled; member/leader membership | Logto organization roles and ZITADEL org/project grants show scoped relations | No group/explicit share/relation tuple model | C2/C3 DEFER |
| Tenant | all units/memberships tenant-scoped; cross-tenant visibility denied | ABP current tenant and Logto tenant guard provide analogous boundary | No separate tenant registry in this slice | KEEP current authority |
| AuthZ | Organization supplies unit status and subtree facts to Policy; Policy scope evaluation does not check whether the target user has an active OrganizationMembership | Cerbos/OpenFGA/SpiceDB distinguish subject, resource and contextual relation | Organization roster membership is not currently an authorization revocation input | KEEP current boundary; document G-09 and define membership-to-grant semantics with a real consumer |
| API / events | typed application commands and public admin DTOs; no organization event bus contract | ZITADEL eventstore and ABP distributed event patterns are larger frameworks | no published organization event schema | C1 only for future R2 artifact; no Wave 1 event expansion |
| Lifecycle | unit active/disabled; membership active/inactive with deactivated_at and re-add | References support disable/remove/role revocation in different layers | time-boxed membership expiry absent | C2 SHOULD |
| Failure semantics | cycle/tenant/disabled parent and scope bridge errors fail closed | OpenFGA/SpiceDB graph/dispatch failures are explicit; Cerbos errors are not allow | intermediate subtree revalidation is deliberately bounded | KEEP current documented rule; C2 review if a real resource consumer needs stronger semantics |
| Retry / idempotency | add/remove/move and list operations have tenant-scoped replay/conflict contracts | ABP application services and reference management APIs use repeatable commands | no independent contract artifact | KEEP; R2 packaging blocker |
| Recovery | inactive membership history retained and reactivated by explicit command | Logto/ZITADEL organization membership/admin changes are recoverable through management APIs | no independent organization release/recovery package | C1 R2 blocker |
| Version / migration | migration 019, versioned units/memberships and adapter parity | ABP modules and ZITADEL migrations are versioned | module migration namespace/digest not separated from workspace | ADAPT later |
| Security | user must be active in tenant; unit bound/host disabled stops scope; no private table writes | OpenFGA/SpiceDB relation checks and Cerbos resource attributes support same fail-closed direction | no service principal/org delegation semantics | C3 DEFER |
| Observability / audit | organization mutations audit in local transaction, bounded details | ZITADEL audit trail and ABP module tests provide reference patterns | no event/outbox semantics required by current synchronous writes | KEEP; do not add outbox for feature parity |
| Performance | bounded tree walk depth and list caps; adapters use tenant indexes | SpiceDB/OpenFGA optimize graph dispatch; not a direct SLO | no current real-PG measurement | NOT RUN locally; historical CI only |
| Release model | crate and adapter exist, but no independent module package | ABP module packaging is a useful packaging reference | no manifest/digest/compatibility artifact | C1 R2 BLOCKER |

## 5. Current capability matrix — Policy

| Dimension | business-platform current fact | Reference evidence / conformance observation | Gap | Decision |
|---|---|---|---|---|
| Authority | Policy catalog/role/binding/scope/decision are internal authority | Cerbos is a policy engine; OpenFGA/SpiceDB are relation PDPs | no active external PDP adapter | KEEP internal authority; REJECT runtime replacement in Wave 1 |
| Domain model | permission grammar, system/tenant roles, role permissions, bindings and four scopes | ABP permissions, Cerbos policies, OpenFGA tuples and SpiceDB schemas cover different abstractions | no relation graph/caveat language | ADAPT only when a real consumer proves need |
| Tenant | every query/write/decision has tenant; visible roles are system or current tenant | All references enforce resource/organization scope in their own model | none in current tested path | KEEP |
| AuthZ | default-deny shared Engine; subject status before grants; unknown keys deny | Cerbos contextual decisions, OpenFGA Check and SpiceDB Check/dispatch all separate decision from storage | external consistency token/remote decision not active | ADAPT as future `AuthorizationEnginePort`, no runtime now |
| API / events | `Authorize`, `ExplainDecision`, admin IAM REST/public DTO; closed decision reasons | OpenFGA API and Cerbos protobufs are explicit decision contracts; ZITADEL management API is resource based | no versioned independent policy contract artifact | C1 R2 blocker |
| Lifecycle | binding active/revoked/effective/expires; role active/disabled; immediate next-decision revocation | Logto/ZITADEL token/role removal, SpiceDB relationship expiration and revisions validate immediate/consistent deny concerns | relationship expiry is not a current policy relation feature; bindings have expiry | C2 SHOULD / ADAPT only with consumer |
| Failure semantics | store/organization bridge error -> DenyInternal -> retryable 503; never allow | OpenFGA/SpiceDB dispatch/datastore failures and Cerbos evaluation errors are explicit | no remote timeout/consistency behavior because no remote adapter | KEEP; add provider contract only when introduced |
| Retry / idempotency | role/permission/binding writes use request fingerprint, expected version and caps | OpenFGA storage tests and ABP permission management tests reinforce adapter behavior contracts | `create_role` + initial set is two commands | C2 accepted limitation; no scope expansion |
| Recovery | revoked rows retained; stale versions fail; bootstrap re-run explicit | SpiceDB revision/watch and ZITADEL event/rebuild patterns are richer runtime concerns | no policy package migration/rollback artifact | C1 R2 blocker |
| Version / migration | catalog/migration 019, policy reference `v1`, role/binding version | OpenFGA model version, SpiceDB zedtoken/revision, Cerbos policy/schema versions | no independent policy contract/version release | ADAPT for R2 |
| Security | roles claim inert; seven-key compat bounded; foreign role invisible; scope narrows; self-escalation guards | Reference engines validate subject/resource/context separately | explain request can describe resource metadata; business resource loader not yet a consumer | KEEP current; C2 API clarification |
| Observability / audit | bounded decision reason/metric; mutation audit; explain candidate evaluations | Cerbos audit, ZITADEL audit and OpenFGA/SpiceDB metrics/trace provide reference patterns | no external decision trace/revision in current local engine | ADAPT later |
| Performance | binding/user/role caps, bounded org walk, historical PG authorize perf harness | OpenFGA/SpiceDB dispatch/cache and Cerbos engine benchmarks are architecture evidence only | no fresh local PG percentile | NOT RUN / no SLO claim |
| Release model | Policy crate is workspace-internal and adapters compile together | OpenFGA/SpiceDB/Cerbos publish server/API artifacts; ABP publishes modules | no independent SemVer, manifest, digest, artifact or consumer build | C1 R2 BLOCKER |

## 6. Decision register

| ID | Decision | Subject | Rationale / boundary |
|---|---|---|---|
| D-01 | ADOPT | Stable external subject mapping, explicit tenant membership, default-deny, immediate revocation checks, bounded decision reasons, adapter behavior contracts | These are cross-reference safety properties and already fit the accepted authority model. |
| D-02 | ADAPT | Organization roles/relations from Logto/ZITADEL; contextual inputs from Cerbos; reusable contract-test discipline from OpenFGA/SpiceDB; module permission registration from ABP | Use the concepts at existing typed ports and catalogs; do not import provider types, schemas, SDKs or runtime topology. |
| D-03 | KEEP | Internal Identity/Organization/Policy ownership and `Authorize` as current authority | ADR-0025 and the Backend Baseline explicitly make authentication external and business authorization internal. |
| D-04 | KEEP | PostgreSQL production authority + SQLite local single-process mirror + shared `identity-authorization-contracts` | Current behavior is already encoded at domain/application/adapter boundaries; a second authority would create drift. |
| D-05 | DEFER | ServiceIdentity/M2M/workload identity, org membership expiry, group/ReBAC/explicit share, remote PDP consistency contract, cross-process PrincipalContext | No current consumer or accepted API requires these in Wave 1. Each needs a separate owner, lifecycle, authorization and failure contract before implementation. |
| D-06 | REJECT | OpenFGA/SpiceDB/Cerbos runtime as current Policy authority | It would change authority, deployment, consistency and failure semantics without a real consumer; future adapters remain possible only behind a typed port. |
| D-07 | REJECT | Logto/ZITADEL/ABP framework types as Platform Core domain or persistence authority | They are references/providers, not this platform’s tenant/user/role owner. |
| D-08 | REJECT | First authenticated user auto-admin, client-supplied role/permission headers, external role claims as direct IAM authority | These violate existing fail-closed and bootstrap boundaries and are covered by adversarial tests. |
| D-09 | ADAPT | R2 module identity/version/manifest/digest/migration/compatibility/release artifact | This is the only Wave 1 material blocker for an R2 claim; it belongs to the packaging/R2 pilot boundary, not a speculative IAM rewrite. |

## 7. Gap registry

| Gap | Level | Evidence | Why it matters | Disposition / close condition |
|---|---|---|---|---|
| G-01 independent IAM module release contract absent | C1 R2 REQUIRED | No independent SemVer/manifest/digest/release artifact/consumer build for identity/org/policy | R2 cannot be independently released or consumed reproducibly | **R2 BLOCKED**; close in packaging/R2 pilot with manifest, digest, migration map, compatibility fixture and artifact |
| G-02 fresh real PostgreSQL evidence for this candidate absent locally | C1 evidence blocker for R2, not a code defect | PG contract/E2E evidence is historical CI at PLAN-0013 fixed head; current local environment has no PG | Adapter parity and production claims need real database evidence | Current Wave 1 records `NOT RUN`; close with CI/staging PostgreSQL lane, not fake substitution |
| G-03 time-bounded tenant/org membership lifecycle absent | C2 SHOULD | Tenant is Active/Suspended; organization membership is Active/Inactive; role binding has validity window | Some enterprise invite/contract scenarios need expiry separate from manual removal | Keep current lifecycle; add only with a concrete consumer and owner/transition matrix |
| G-04 ServiceIdentity/M2M/workload principal absent | C3 DEFER | No domain/API/consumer; audit only maps Bootstrap/Migration to Service actor type | Adding it now would create credentials, rotation, tenant scope and impersonation semantics without use case | Defer; future plan must prove non-inheritance from human bindings |
| G-05 graph/ReBAC/group/explicit share absent | C2/C3 | Current scope is Tenant/OrganizationUnit/ResourceType/Resource | Real project/document relations may eventually need graph semantics | Defer behind a relation port; no OpenFGA/SpiceDB runtime in Wave 1 |
| G-06 explain target resource/compat semantics need explicit API wording | C2 | Authorize and Explain share Engine; admin explain builds target context without caller token compat grants | A caller could mistake stored-policy explanation for another token’s complete decision | Document as policy-binding diagnostic; add parity fixture when a real business resource explain consumer exists |
| G-07 role creation with initial permissions spans two commands | C2 accepted limitation | `apps/business-api/src/routes/iam_admin.rs` creates role then set-replaces permissions; prior Stage 8 review recorded this | Crash between commands can leave an empty role, but replay converges and no authorization widening occurs | Keep bounded behavior; repair only if a product contract requires atomic composite command |
| G-08 cross-process PrincipalContext not yet a published contract | C2 | Current `TenantContext`/`AuthenticatedPrincipal` are request-local; no worker/remote IAM consumer | Independent worker/module use needs stable subject, tenant, auth method, delegation and correlation semantics | Defer until real cross-process consumer; do not create a large context now |
| G-09 organization roster removal does not revoke a separate Policy RoleBinding | C2 SHOULD | `OrganizationMembership` removal only deactivates the organization row; Policy `scope_matches` validates unit status/tree but not the subject's membership. Existing organization tests assert roster removal/filtering, not a subsequent authorization deny | If roster removal is intended to revoke org access, a still-active org-scoped binding may remain effective while the unit stays active | Clarify whether membership and authorization are separate product semantics; if removal must cut access, define owner/transaction or revocation contract and add a remove→authorize regression before claiming it |

No C0 was found in the reviewed current paths. C1 items are R2 admission/evidence blockers, not permission bypasses; Wave 1 therefore does not claim R2 readiness.

## 8. Required adversarial scenario matrix

| # | Scenario | Current evidence | Status on this branch |
|---:|---|---|---|
| 1 | Forged tenant/principal header or claim | `iam_adversarial_sqlite::header_spoofing_is_inert_under_oidc`; `foreign_user_id_claim_fails_closed`; OIDC issuer/audience/subject checks | PASS on SQLite/fake evidence |
| 2 | Cross-tenant user read/mutation | `cross_tenant_probes_fail_closed`; tenant-scoped Identity/Organization queries | PASS on SQLite/fake evidence |
| 3 | Cross-tenant resource/scope follows the same human | cross-tenant policy fixture and IAM probes; binding query includes tenant | PASS on SQLite/fake evidence |
| 4 | Missing user or missing tenant membership | `TenantAccessReason::NoMembership`; decision matrix denies before bindings; explain foreign identity is tenant-local | PASS |
| 5 | Suspended tenant membership | full grant lifecycle and `suspended_membership_overrides_claim_grant` | PASS |
| 6 | Revoked binding | `contract_authorization_fixture::revocation_stops_authorization_on_the_next_decision`; E2E revoke → 403 | PASS |
| 7 | Expired/not-yet-effective binding | policy decision matrix and binding contract validity-window cases | PASS |
| 8 | Expired tenant/org membership | no expiry state in current membership models; organization removal is inactive and is tested | PARTIAL; expiry is C2, not an untested allow path |
| 9 | Removed organization membership | organization contract proves remove→inactive, list filtering and re-add; Policy OrganizationUnit scope checks unit status/tree but does not resolve the subject's OrganizationMembership | Roster removal PASS; post-removal authorization effect NOT VALIDATED (G-09 C2) |
| 10 | Unknown/retired/malformed permission | policy decision matrix and bounded compat parser; unknown keys never grant | PASS |
| 11 | Bad scope/resource kind or scope widening | ResourceScope validation, decision matrix, exact/type/org-subtree tests | PASS |
| 12 | Wrong-tenant/foreign role | policy contract role visibility and cross-tenant bind rejection | PASS |
| 13 | Service identity accidentally inherits human role | no ServiceIdentity/M2M runtime or consumer exists | NOT RUN / C3 DEFER; no capability is claimed |
| 14 | Bootstrap / cold-start / first-user promotion | `e2e_admin_bootstrap_sqlite`, no-bootstrap negative path, explicit versioned bootstrap | PASS on SQLite; PG historical CI |
| 15 | Stale role/binding version and revocation race | expected_version tests, stale 409, adapter CAS/idempotency contracts | PASS for current single-command writes |
| 16 | Idempotency replay vs same-key conflict | identity/org/policy shared contracts and API write-key assertions | PASS on SQLite/fake; PG historical CI |
| 17 | Explain parity and PostgreSQL/SQLite parity | shared Engine + explain tests; SQLite contracts this run; PG contract/E2E historical CI | PASS for binding path; current PG run NOT RUN; compat explanation semantics are C2 documented |

The matrix deliberately distinguishes unsupported future capabilities from failed security controls. “NOT RUN” never becomes “PASS” through a README, a fake or a historical plan statement.

## 9. Shared behavior-suite assessment

`identity-authorization-contracts` already runs common Identity, Organization and Policy persistence behavior against the fake and SQLite adapters, and the PostgreSQL test targets consume the same verification functions when `DATABASE_URL` exists. This is the correct seam for a future remote/provider adapter.

Current boundary:

- The shared suite strongly covers mutation/query/storage semantics, tenant isolation, replay/conflict, versioning, status and caps.
- Policy decision evaluation is currently covered by `policy` fake decision matrices and HTTP/fake IAM tests, not by a single remote-provider contract.
- No external authorization adapter exists, so no provider contract is missing from the current runtime.
- Before adding an `AuthorizationEnginePort` adapter, the suite must gain a decision fixture covering allow/deny, unknown permission, subject status, resource scope, explain parity, error→deny and consistency/revocation semantics, then run it against fake, PostgreSQL/SQLite composition and the remote adapter.

## 10. Validation and evidence status

| Gate / evidence | Result | Scope / qualification |
|---|---|---|
| Identity/Organization/Policy domain + shared fake tests | PASS | `cargo test -p identity -p organization -p policy -p identity-authorization-contracts --all-features` |
| SQLite adapter contracts | PASS | identity, organization and policy contract tests; each one passed in this run |
| Business API adversarial SQLite | PASS | `cargo test -p business-api --test iam_adversarial_sqlite --all-features`; 6 passed |
| Business API bootstrap / IAM admin / security | PASS | `e2e_admin_bootstrap_sqlite` 3 passed; `iam_admin_api` 22 passed; `security` 16 passed |
| PostgreSQL adapter contracts/E2E/perf | NOT RUN locally | No local PostgreSQL/Docker; historical PLAN-0013 CI evidence is cited separately and is not current-run evidence |
| `cargo fmt --all -- --check` | PASS | Run on the final docs-only tree; no Rust source changed |
| `cargo check --workspace --all-targets --all-features` | NOT RUN | No production code changed; the focused Business API command compiled the API and relevant dependency graph |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | NOT RUN | No production code changed; no workspace-wide lint claim |
| `cargo test --workspace --all-features` | NOT RUN | No production code changed; focused identity, organization, policy, adapter, API and security tests are listed above |
| `scripts/check-architecture.ps1` | PASS | `Architecture fitness: PASS`; included OpenAPI contract check passed |
| security/license/secret scan | NOT RUN locally unless the repository command is available | Do not infer from source review |

## 11. R2 readiness

| R2 admission item | Result |
|---|---|
| Authority / tenant / default-deny / C0 | Current foundation evidence; **C0 = 0 found** |
| C1 correctness gaps | No current permission bypass or tenant-crossing C1 found; R2 packaging/evidence blockers remain |
| Independent module identity and SemVer | NOT READY |
| Manifest, public contract artifact and canonical digest | NOT READY |
| Migration/retention/rollback mapping | NOT READY as an independent module artifact |
| Shared contract artifact and consumer build | Workspace contracts exist; independent release artifact NOT READY |
| PostgreSQL/remote release evidence | Current local PG NOT RUN; remote adapter not activated |
| Independent review | **PASS WITH C2/C3** on Base `8365de0211390a6a3367b536c5ceaa5256bbb5cf` → Candidate `c7dc3d142a526b47e9ba8e472b82346ffdaed221` |

**R2 verdict: BLOCKED / NOT CLAIMED.** Wave 1 can close only with a review verdict and honest C0/C1 disposition; it does not promote `identity`, `organization` or `policy` to R2. Packaging and at least two-module R2 pilot remain in the later PLAN-0014 wave.

## 12. Independent reviewer record

- Reviewer: GPT-5.6 Sol, High reasoning, read-only independent pass;
- Review date: 2026-09-24;
- Fixed range: Base `8365de0211390a6a3367b536c5ceaa5256bbb5cf` → Candidate `c7dc3d142a526b47e9ba8e472b82346ffdaed221`;
- Verdict: **PASS WITH C2/C3**; no evidence-backed C0/C1 correctness or tenant-isolation blocker found;
- Accepted C2 clarification: G-09, organization roster removal does not itself prove revocation of a separately stored Policy binding;
- Other accepted limits: membership expiry, service/workload identity, ReBAC/group sharing, cross-process PrincipalContext, and complete resource-owner loading remain deferred;
- R2 remains blocked by independent module SemVer/identity, manifest/digest, migration/rollback mapping, reproducible artifact/consumer build, and fresh PostgreSQL release evidence;
- Review coverage note: the reviewer did not independently re-fetch the six upstream repositories during the pass; their exact tags/commits, licenses and inspected source/test paths are recorded in Section 2 from the reference pinning work.

## 13. Conclusion

Current reference alignment supports **KEEP internal authority + ADAPT proven safety/contract practices + DEFER consumer-less capabilities + REJECT provider replacement**. The independent pass found no blocking C0/C1 defect. G-09 remains an explicit C2 contract question; no production Rust behavior change is justified without a product decision that organization roster removal must revoke an independent policy grant. The remaining R2 blockers must not be hidden by the green domain/SQLite evidence.
