# Architecture Foundation Convergence — Inventory and Gap Matrix

> 日期：2026-08-12
> REAL_BASE：`c93a1e67547ebe2f3d59bea02ed0928bd84aa722`
> OLD_DOC_BRANCH：`a5069a0ca9fe746f0c9f2bb0467e6e899f0fa3dc`
> NEW_BRANCH：`docs/business-application-platform-architecture-convergence`
> 旧文档分支直接合并：REJECTED

## 1. Stage 0 reconciliation evidence

执行时 `origin/main` 与本地 `main` 均解析为 `c93a1e67547ebe2f3d59bea02ed0928bd84aa722`，工作树在建分支前干净。旧远端文档分支 `origin/docs/twenty-tier1-reference-and-app-packaging-plan` 的 HEAD 为 `a5069a0ca9fe746f0c9f2bb0467e6e899f0fa3dc`，与 `origin/main` 的 merge-base 为 `ad35c3c172cf19c97366c38ae8340852f3b6365c`，存在 ahead/behind/diverged 历史。

旧分支相对真实 base 的文件差异包含有效的 Twenty reference、ADR-0021 和 PLAN-0011，也包含过期的 PLAN-0009 current 状态/前置 gate。因此未直接 merge、未整体 cherry-pick；只选择性搬运有效文件并修复其中的历史状态链接。

```text
OLD_BRANCH_DIRECT_MERGE = REJECTED
PLAN_0009_RESURRECTION = NONE
PLAN_0010_STATUS = INTEGRATED
```

PLAN-0009 仍是 `docs/plans/archive/2026/` 下的 `Completed / Rehearsal Closed / Archived`；PLAN-0010 在本变更中从 `current/` 迁移到同一 archive 年份；二者的 recovery/closeout evidence 不回退。

## 2. Concept inventory

| Concept | Existing evidence | Convergence disposition |
|---|---|---|
| Business Module | ADR-0020、module contracts、isolation architecture | Reuse; add package/contribution boundary |
| Bounded Context | ADR-0003、Context Map、ownership architecture | Reuse as domain boundary; module is not table/page wrapper |
| Platform Capability | `business-module-contracts` manifest/compiler | Reuse; Core remains business-neutral |
| Resource Kind / ResourceRef | module contracts、semantic contract、data architecture | Reuse; formalize cross-module reference rules in ADR-0022 |
| Published Contract | API/event standard、module manifest | Reuse; only Application/Public Contract crosses ownership |
| Application Port | layered backend、CQRS、existing crates | Reuse; adapters cannot leak private implementation |
| Command / Query | API/event standard、ADR-0008 | Reuse; owner transaction and query timeout/error mapping added |
| Domain Event / Integration Event | API/event standard、Outbox/ownership architecture | Reuse; explicit owner→Outbox→versioned event chain |
| Projection | ADR-0008/0017、semantic contract | Reuse; owner/freshness/version/rebuildability/non-authority required |
| Reference + Snapshot | ADR-0019、data architecture | Reuse; historical interpretation rules clarified |
| Semantic Contribution | ADR-0017/0020、semantic compiler | Reuse; one semantic authority only |
| UI Contribution | manifest descriptor and Twenty research | Partially defined; typed host-controlled contract proposed |
| Agent Contribution | manifest descriptor、ADR-0018 | Partially defined; declaration is not authorization |
| Module Dependency | module compiler | Reuse; dependency graph, ranges, cycle/unknown checks |
| Compatibility | manifest descriptor | Partially defined; SemVer schema/module/platform/contribution ranges proposed |
| Lifecycle | manifest descriptor and isolation baseline | Partially defined; install/enable/disable/uninstall separate from retained/purged data |
| Published Extension Point | ADR-0021 proposal only | Missing from accepted baseline; owner-published, versioned, blocked removal proposed |
| Process Manager / Saga | data/workflow baseline names Process Manager | Partially defined; ADR-0022 makes it the sole cross-module business-process model |
| Business Application Package / Dry Plan | old Twenty branch ADR/PLAN proposal | Missing from integrated runtime; pure declaration→compile→digest→plan design only |

## 3. Gap matrix

| Area | Existing | Partially Defined | Missing | Conflicting / duplicated | Minimum convergence action |
|---|---|---|---|---|---|
| Core/module boundary | ADR-0020 + pure Rust crates | runtime packaging not present | explicit neutrality tests for future package plan | none authoritative | add Business Application architecture and fitness requirements |
| A↔B query/command | API/Query/ownership baseline | timeout/error mapping across modules | one standard | private access risk in future code | ADR-0022 + communication standard |
| Events | Outbox and event envelope baseline | module consumer lifecycle | cross-module replay/ordering fixture | Domain vs Integration vs Execution can be confused | owner/outbox/version/idempotency rules |
| References | Resource kinds and Reference+Snapshot exist | lifecycle/authorization/stale behavior | single cross-module reference profile | private FK temptation | ResourceRef + Snapshot profile |
| Projections | CQRS/Analytics baseline | cross-module freshness and rebuild evidence | synthetic projection acceptance | projection could be mistaken for authority | explicit non-authority contract |
| Saga | named in workflow/data architecture | state ownership and durable execution seam | consistent cross-module process model | risk of second workflow runtime | Proposed ADR-0022 |
| Extensions | ADR-0021 Proposed | typed descriptors not implemented | owner/consumer/removal semantics | Twenty/Frappe permit shared mutable extension | PublishedExtensionPoint + BlockedRemoval |
| Packaging | module manifest/compiler exists | dependency/compatibility descriptors | package digest and dry-plan | old branch gated PLAN-0009 incorrectly | keep PLAN-0011 Proposed/NOT ACTIVE |
| UI/Agent/Semantic | manifest fields, ADR-0017/0018 | typed UI/Agent details | unified contribution validation | metadata vs DDD vs semantic confusion | three-layer boundary and same module identity |
| Lifecycle/removal | Installed/Enabled/Disabled/Uninstalled vocabulary | execution and data retention split | removal dependency plan | old Twenty uninstall semantics too destructive | `Uninstalled != Data Purged` |
| Synthetic proof | compiler tests exist for semantic isolation | generic fixture design | module-a/b/extension acceptance | no production fixture this round | PLAN-0011 future acceptance only |
| Governance | PLAN-0009 archived; PLAN-0010 integrated in content | indexes stale for PLAN-0010 | archive path/index consistency | old branch resurrects PLAN-0009 | migrate PLAN-0010 and update all indexes |

## 4. Architecture quality gate answers

At the document-foundation stage, the target answers are:

| Question | Decision |
|---|---|
| Q1–Q3 Core neutrality and module add/remove isolation | YES by boundary; runtime proof deferred to synthetic fixture |
| Q4–Q5 private access prohibition and allowed seams | YES by ADR-0020/0022 and fitness requirements |
| Q6 cross-module consistency | YES: owner-local transaction + Outbox/Event + idempotent consumer/Saga + compensation |
| Q7 one Saga model | YES: Process Manager/Saga; Durable Task remains execution state only |
| Q8 extension without private schema pollution | YES: owner-published Extension Point; live removal blocked |
| Q9 UI/Agent/Semantic boundary | YES: same module identity, independent typed contributions |
| Q10 semantic single authority | YES: ADR-0017/0020 only |
| Q11 metadata does not replace DDD | YES |
| Q12 removal does not purge data | YES |
| Q13 deterministic package/dry-plan | Proposed contract; runtime/compiler acceptance fixture still future |
| Q14 three-module proof | Designed, not implemented in this task |

The foundation is not declared runtime-converged by this inventory alone; acceptance still depends on the full validation matrix and independent reviewer verdict.

## 5. Validation record

Executed on 2026-08-12 from `REAL_BASE` without Rust/runtime/database changes:

| Gate | Result | Evidence / note |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | no formatting changes required |
| `cargo check --workspace --all-targets --all-features` | PASS | 0 errors; existing incremental hard-link warnings only |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | 0 errors; existing 127 warnings reported by wrapper, no denied lint failure |
| `cargo test --workspace --all-features` | PASS | 169 passed, 34 ignored, 118 suites |
| `pwsh ./scripts/check-architecture.ps1` | PASS | Cargo metadata, OpenAPI and Architecture Fitness passed |
| `pwsh ./scripts/check-openapi.ps1` | PASS | OpenAPI contract passed |
| `git diff --check` | PASS | trailing Markdown line-end whitespace fixed |
| `cargo-audit`, `cargo-deny`, `gitleaks`, `trivy`, `syft`, `grype`, `osv-scanner` | NOT RUN | no repository-provided or installed entrypoint found |

No database migration, API, worker, object-store, messaging or business-data write was introduced by this convergence candidate; persistence/provider E2E evidence is therefore not manufactured.

## 6. Independent reviewer verdict

**PASS** — 2026-08-12 read-only independent review found no issues. The reviewer specifically checked Platform Core neutrality, module isolation, six-way inter-module communication, transaction consistency, Outbox/idempotency/ordering/evolution, Saga versus Durable Task separation, Extension Point governance, DDD/Metadata/Semantic boundaries, semantic single authority, `Uninstalled != Data Purged`, deterministic package/dry-plan, synthetic fixture adequacy, pinned reference facts/license boundaries, and old-branch reconciliation.

No repair loop was required.
