# PLAN-0010 Candidate Reconstruction Evidence

> Review date: 2026-08-11
> Status: Accepted Candidate
> Branch: `codex/business-module-isolation-semantic-contract`
> Base SHA: `654fe83d82107d899079d20e5fef8aaf4d5431b8`
> Candidate SHA: `7997a501528bf12ae7846a9dc278fe4fce65a467`
> Final evidence HEAD: docs-only close-out commit following the Candidate implementation (exact SHA in task handoff)

## Baseline correction

The previous Candidate was invalidated. PR #7's actual GitHub base is
`origin/main@654fe83d82107d899079d20e5fef8aaf4d5431b8`, but the previous
candidate declared `f09d2a5012627ab2219f309a2d9c1c4eacfe11f4` as its base.
Previous invalid Candidate SHA: `d4033e5b31dd3ec96b951d76b6e221a6862929b6`.
That local-only commit is the PLAN-0009 rehearsal close-out and is not the
GitHub `main` tip. Consequently, the old `Base..HEAD` included PLAN-0009
runtime and rehearsal files even though its two PLAN-0010 commits were pure.
The old CI PASS does not make that scope invalidity acceptable.

The clean candidate was reconstructed from the actual GitHub base by applying
only the proven PLAN-0010 implementation/evidence file differences. The old PR
branch was updated only after its remote ref was confirmed as the expected
`d4033e5b31dd3ec96b951d76b6e221a6862929b6`; the update used
`--force-with-lease`. No main merge, PLAN-0009 activation, or PLAN-0006
activation occurred.

## Scope decision

ADR-0020's architecture range is unchanged. PLAN-0010 adds only Business
Module Manifest contracts, the Semantic Contract compiler, deterministic
validation, reference documentation, and architecture Fitness Functions. It
adds no Business Module business capability, WrenAI runtime, Text-to-SQL,
dynamic plugin loading, persistence/runtime storage boundary, API, Worker,
migration, or deployment unit.

PLAN-0010 has no persistence/runtime storage change: no database migration or
write path and no object-storage boundary changed. Therefore no dedicated
local PostgreSQL/MinIO E2E is manufactured; workspace CI remains the relevant
runtime-environment evidence.

## Changed files

The clean `Base..HEAD` contains only the following 28 PLAN-0010 files:

- `AGENTS.md`
- `Cargo.lock`
- `Cargo.toml`
- `crates/business-module-contracts/Cargo.toml`
- `crates/business-module-contracts/src/lib.rs`
- `crates/business-module-contracts/src/manifest.rs`
- `crates/business-module-contracts/tests/platform_core_isolation.rs`
- `crates/semantic-contract/Cargo.toml`
- `crates/semantic-contract/src/compiler.rs`
- `crates/semantic-contract/src/lib.rs`
- `crates/semantic-contract/src/model.rs`
- `docs/README.md`
- `docs/adr/ADR-0020-business-module-isolation-and-semantic-contract.md`
- `docs/adr/README.md`
- `docs/architecture/ARCHITECTURE_STATUS.md`
- `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`
- `docs/architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`
- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`
- `docs/architecture/ENTERPRISE_BUSINESS_DOMAIN_ARCHITECTURE.md`
- `docs/architecture/LEGACY_MIGRATION_ARCHITECTURE.md`
- `docs/plans/README.md`
- `docs/plans/current/PLAN-0010-business-module-isolation-and-semantic-contract-foundation.md`
- `docs/reference/README.md`
- `docs/reference/WRENAI_REFERENCE_ANALYSIS.md`
- `docs/reviews/2026-08-11-plan-0010-accepted-candidate.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
- `scripts/check-architecture.ps1`

## Reference and architecture decision

WrenAI is pinned as a reference project at Canner/WrenAI commit
`ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902`. Adopted ideas are declarative
semantic objects, explicit source/derived boundaries, validation before
planning, canonical rebuildable output, and dry-plan-style separation. WrenAI
runtime, Python, LanceDB, DataFusion, SQLGlot, ClickHouse, raw schema access,
credentials, Text-to-SQL, and generic OLAP remain outside this candidate.

ADR-0020 keeps Business Module Isolation and Semantic Contract as complementary
platform seams. Platform Core owns generic contract/compiler behavior only;
Business Modules own business facts and published semantic contributions; the
Analytics boundary owns only compiled, rebuildable registry input and summaries.

## Dependency isolation evidence

- `business-module-contracts` depends only on `serde` and `thiserror`.
- `semantic-contract` depends only on `business-module-contracts`, `serde`,
  `serde_json`, `sha2`, and `thiserror`.
- Cargo tree, Cargo metadata Fitness Functions, source scans, and the explicit
  Platform Core isolation test reject Axum, SQLx, reqwest, AWS, object storage,
  Wren, Python, LanceDB, provider/runtime, and concrete business-crate edges.
- No Platform Core -> Business Module reverse dependency is present.

## Semantic compiler evidence

The compiler is pure, side-effect free, and fail-closed for:

- duplicate module IDs and namespaced semantic IDs;
- duplicate semantic IDs and duplicate metric ownership;
- unknown module dependencies, incompatible versions, and dependency cycles;
- unknown semantic/relationship references and cross-module private references;
- unknown relationship endpoints, descriptor mismatches, and invalid versions.

Tests prove that registration order changes produce byte-identical canonical JSON
and identical SHA-256 digests. The module-removal fixture constructs
`fixture-sales` and `fixture-finance`, removes the latter, and proves the
remaining compiled manifest and canonical output are unchanged. The
`business-module-contracts` integration test proves Platform Core source and
manifests do not reference either fixture module.

## C-specific-code scan

The architecture Fitness Function scans Platform Core and the changed runtime
surface for C-specific names, migrations, ACLs, and production write paths.
The clean branch contains no PLAN-0009 rehearsal runtime files and no
C-specific runtime code. Guard/document mentions remain negative checks or
scope statements only. Result: PASS.

## Verification matrix

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace --all-features` | PASS - 145 passed, 34 ignored |
| `cargo test --workspace --all-features -- --ignored` | NOT RUN / environment unavailable - `DATABASE_URL` is absent; fail-closed PostgreSQL E2E precondition |
| `pwsh ./scripts/check-architecture.ps1` | PASS |
| `pwsh ./scripts/check-openapi.ps1` | PASS |
| `git diff --check` | PASS |
| cargo-deny / cargo-audit / secret scan | NOT RUN - no formal repository configuration or entrypoint exists |
| exact clean Base..HEAD scope audit | PASS - 28 files; no PLAN-0009 runtime paths |
| remote CI on exact Candidate implementation SHA | PASS - run [31478430670](https://github.com/jesenzhang/business-platform/actions/runs/31478430670) |

## Final disposition

PLAN-0010 is `Accepted Candidate` at implementation SHA
`7997a501528bf12ae7846a9dc278fe4fce65a467`, with the final docs close-out
commit and its exact-head CI recorded when publication completes. PLAN-0009
remains `Proposed / NOT ACTIVE`; PLAN-0006 remains `Proposed / NOT ACTIVE`.
No archive and no main integration.
