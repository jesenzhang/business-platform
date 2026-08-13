# PLAN-0011 Accepted Candidate Evidence

Date: 2026-08-13

This document freezes the exact PLAN-0011 candidate scope and records the
evidence available before independent REVIEW-C. It is evidence, not a runtime
registry or a second business-data authority.

## Immutable checkpoints

| Evidence | SHA |
| --- | --- |
| Architecture foundation integrated on `main` | `be75249` |
| PLAN-0011 activation | `31b24c6` |
| Stable identities | `bd58f9c` |
| Typed contributions | `6c84094` |
| Published extension points | `22b274f` |
| Package compiler | `1324cdf` |
| Deterministic dry-plan | `e61edc5` |
| Synthetic module isolation | `467a2f0` |
| Architecture fitness hardening | `308e745` |
| Bounded compiler/dry-plan repair | `f08ac7dfede130471e4838a20a577aade9475026` |
| Canonical compiled-manifest deserialization repair | `474285719a066c9810ac1cfed4886cf8bb455d2f` |
| Public capability dependency catalog repair | `014d3a5fbc97e041c557900d7fde825a36d7b754` |
| Candidate head | `PENDING_FINAL_EVIDENCE_ALIGNMENT` |

`PLAN_0011_IMPLEMENTATION_BASE` is `31b24c6993dbff1f3e88b2476e0c87460400ec31`.
The exact implementation range is:

```text
31b24c6993dbff1f3e88b2476e0c87460400ec31..014d3a5fbc97e041c557900d7fde825a36d7b754
```

The repairs close five independently identified blockers: compiled-manifest
serde round-trip reconstructs canonical bytes; desired installation state can
produce `DisableModule` without conflating data retention; removing an owner
module checks active extension consumers; legacy/typed contribution IDs are
validated in one collision domain; and public capability dependencies are
published into the compiler dependency catalog before resolution.

## Review and repair history

The prior candidate alignment at `96aed3f6a2b4313f08658bbcf4d96c5652782b31`
was independently reviewed by fresh Sol reviewer
`019ffb87-1e40-7592-a54a-77c7fcd363ef`. REVIEW-C found one HIGH blocker in
`crates/business-application-compiler/src/lib.rs`: the compiler accepted the
`PublicCapability` dependency contract but did not emit corresponding catalog
entries from provider agent-tool contributions, so valid capability
dependencies were rejected as unknown. That candidate was invalidated.

Fresh Luna repair commit `014d3a5fbc97e041c557900d7fde825a36d7b754` adds the
catalog publication and deterministic capability-dependency coverage. A new
independent REVIEW-C is required for the exact final range after the evidence
alignment commit.

## Scope proof

The range contains only generic `business-module-contracts` and
`business-application-compiler` Rust contracts/compiler, their focused tests,
the architecture fitness script, and the architecture fitness standard.
It contains no database migration, application runtime, worker, object storage,
network/provider integration, dynamic plugin, WASM/Node/Python runtime,
Marketplace, concrete business module, PLAN-0006 implementation, PLAN-0009
runtime/migration change, or arbitrary SQL capability.

Synthetic business validation uses only `module-a`, `module-b`, and
`module-extension`, and those names occur only in tests. The packaging compiler
does not define semantic Dataset/Metric/Dimension/Relationship/Lineage
authority; semantic compilation remains owned by `semantic-contract` under the
existing semantic contract decisions.

## Evidence matrix

| Invariant | Evidence |
| --- | --- |
| Stable namespaced module/contribution/extension identities | `business-module-contracts` stable identity and typed contract tests |
| Typed UI, policy/capability and agent contributions | typed contribution tests; ownership/catalog validation |
| Published extension points only | extension point tests; private/unknown/wrong-owner/version/classification rejection |
| SemVer and dependency safety | compiler tests for invalid versions, unknown/incompatible dependencies, cycles and downgrade |
| Deterministic compiler/canonical JSON/SHA-256 | compiler permutation tests |
| Deterministic dry-plan | planning permutation tests and canonical-input integrity checks |
| Live dependency removal | planning and synthetic fixture blocked-removal tests |
| Active extension consumer removal | planning and synthetic fixture blocked-removal tests, including owner-module removal |
| Uninstalled is not data purge | retained-data plan tests; no purge/delete/drop change type |
| Platform Core neutrality | architecture fitness source/dependency scan |
| Synthetic module isolation | `synthetic_fixtures.rs` |

## Validation

Historical local gates on implementation checkpoint `308e745`:

- `cargo fmt --all -- --check` — PASS
- `cargo check --workspace --all-targets --all-features` — PASS
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
- `cargo test --workspace --all-features` — PASS (217 passed, 34 ignored)
- `pwsh ./scripts/check-architecture.ps1` — PASS
- `pwsh ./scripts/check-openapi.ps1` — PASS
- `git diff --check` — PASS

Remote implementation CI run `31688120911` for HEAD
`308e7452338c608f9017ac146e6c4d3a8eeb08df` — PASS. Remote candidate CI for
the final evidence head `7567e590137a3417ecd78c003f9a3f13843c3f85` is tracked
separately and must pass before REVIEW-C.

Current repair validation supersedes the historical checkpoint details above:

- Previous repair implementation commit: `21c3420a9791d2d8a236ed01cc06885423a185f0`
- Previous repair Feature CI `31692592164`: PASS, including PostgreSQL / MinIO / E2E contracts
- Current implementation repair commit: `014d3a5fbc97e041c557900d7fde825a36d7b754`
- Current focused compiler tests: PASS (13 tests)
- Current local format, check, clippy, workspace tests, architecture fitness,
  OpenAPI, and diff checks: PASS
- Feature CI for the current implementation repair: `31711682449` for
  `014d3a5fbc97e041c557900d7fde825a36d7b754` — PASS, including PostgreSQL /
  MinIO / E2E contracts and Architecture Fitness
- Canonical deserialization repair commit:
  `474285719a066c9810ac1cfed4886cf8bb455d2f`
- Feature CI for the canonical deserialization repair: `31709350955` — PASS,
  including PostgreSQL / MinIO / E2E contracts
- The evidence commit will be followed by one final head-alignment commit; the
  latter is the exact candidate head for REVIEW-C.

External scanners:

- `cargo-audit`, `cargo-deny`, `gitleaks`, `trivy`, `syft`, `grype`, and
  `osv-scanner` — NOT RUN: no repository-provided entrypoint and no installed
  executable was available in this environment.

## Accepted limitations and deferred work

This candidate provides declarations, pure validation/compilation, deterministic
manifest evidence, synthetic planning, and fitness checks only. It does not
provide a production registry, persistence, installer/updater, runtime loader,
worker, Marketplace, dynamic plugin execution, or business-module implementation.
Cross-module collaboration runtime remains deferred to PLAN-0012. Contract/C
migration and PLAN-0006 remain outside this candidate.
