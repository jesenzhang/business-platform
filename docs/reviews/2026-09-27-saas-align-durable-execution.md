# PLAN-0014 Wave 3 — Durable Task Execution Reference Conformance Review

> Wave: PLAN-0014 Wave 3 (Durable Task Execution / Audit / Messaging)
> Status: Round 1 independent verdict **FAIL / NEEDS REPAIR**; Round 2 final candidate `2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a` — PostgreSQL lanes EXECUTED on PostgreSQL 18.4 (22 passed / 0 failed / 0 ignored). Round 2c repaired an audit timestamp regression that those 22 lanes did not cover; lanes re-EXECUTED on a fresh database (23 passed / 0 failed / 0 ignored). NOT INTEGRATED; independent re-review pending. See §14, §15
> Review date: 2026-09-27 (actual start date of Wave 3; no date back-filling)
> Branch: `feat/PLAN-0014-wave3-durable-audit-messaging-conformance`
> Base: `443a11cabdce1dd34006feaf08aa8be3382150c1` (Wave 2 closeout commit on the Wave 2 branch; Wave 2 itself remains NOT INTEGRATED)
> Fix commits: `217e9500471f84e2b597b4315bfda3873f365519` (durable/audit fixes + tests + CI wiring), `4b4ee71e610546449b33a84b034c18a17da65308` (architecture-check dev-edge exemption)
> Maturity level claimed: **R1 (foundation conformance)**. R2 admission is explicitly NOT claimed by this review; the verdict vocabulary and the independent-reviewer procedure are unchanged from Wave 1/2 (PASS / PASS WITH C2-C3 / FAIL, recorded only by the independent reviewer).

## 1. Review question and method

Review question: does the Wave 3 scope — Durable Task Execution as instantiated
by PLAN-0004 (`ProcessingJob`/steps/AI tasks, claim/lease/fence, retry,
cancellation, reclaim, checkpoint, fenced AI completion) — conform to the
platform baseline (`WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE` §4–§13, §18,
§23–§25; AGENTS.md §8; ADR-0010/0011/0012 semantics) when measured against
fixed external references, and are the proven C0/C1 defects repaired with
failing-first tests?

Method (PLAN-0014 §4): baseline → fixed references (tag→SHA pinned, source
evidence separated from marketing claims) → three-layer implementation facts →
gap registry (C0–C3 / ADOPT/ADAPT/KEEP/DEFER/REJECT) → bounded repair with
failing-first tests → failure matrix rows bound to test path + name + command +
environment + verdict. Investigation scope was read-only; only proven C0/C1
items were repaired in code. A reference having a feature we lack is NOT
automatically a defect: every gap below names the violated *baseline or
contract*, not merely the missing reference feature. No external runtime
(Temporal/Restate/Kafka/NATS/…) was introduced; Wave 4/Wave 5, PLAN-0015 and
PLAN-0006 were not touched; no push, merge to main, or release happened on
this branch.

## 2. Baseline and candidate

| Item | Value |
| --- | --- |
| Baseline documents | `WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE` (Baseline, §25 PLAN-0004 profile), `DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE`, AGENTS.md §7/§8/§9, ADR-0002/0010/0011/0012/0013 |
| Wave dependency | Wave 2 branch `feat/PLAN-0014-wave2-business-module-packaging-conformance` @ `443a11c` (NOT INTEGRATED). Wave 1's two patch-equivalent chains were NOT re-merged. |
| Wave 3 fix code | `217e950` + `4b4ee71` on top of base; worktree otherwise clean |
| Candidate for independent review | final HEAD of this branch (recorded in the Wave 3 final report; the code candidate reviewed by tests is `4b4ee71`) |
| Runnable environments | SQLite (in-process, single local file) executed locally; PostgreSQL lanes require a reachable server — see §9 (BLOCKED locally, wired into CI) |

## 3. Fixed references (review date 2026-09-27)

Pinned via `git ls-remote` + in-clone `rev-parse` (annotated tags record both
tag object and commit). Clones retained under `%TEMP%\plan0014-w3-refs\`.
License constraints are respected: BSL/proprietary sources were read for
semantics only; no code was copied.

| Reference | Pinned revision | License | Role in this review |
| --- | --- | --- | --- |
| Temporal `temporalio/temporal` | tag `v1.32.0` → commit `d94e34a1ebba5410a2e7d07119a76896909591aa` | MIT | lease/timeout modeling (`mutable_state_impl.go:1440-1476`, `transfer_queue_active_task_executor.go:322-358`), retry classification (`service/history/workflow/retry.go`, `common/retrypolicy/retry_policy.go:27`), terminal-state write guard (`mutable_state_impl.go:97-98` `ErrWorkflowFinished`), attempt counting (`:6810`), opaque version markers (`historybuilder/event_factory.go:803`) |
| Restate `restatedev/restate` | tag `v1.7.12` → commit `577e032d8910318a1d2293b8ab2d78df6f998555` | **BSL 1.1** (semantics-only) | journal replay that skips completed steps (`service_protocol_runner_v4.rs:352-499`), command dedup by producer/epoch (`processor/dedup.rs`), leader fencing tokens (`leadership/fencing.rs`), transient-vs-permanent classification incl. "not-yet-started doesn't consume attempts" (`invoker-impl/src/error.rs:250`) |
| Trigger.dev `triggerdotdev/trigger.dev` | `trigger.dev@3.3.17` → commit `5730faea00e85f24eec2aacb8717855bb681e752` **and** `v4.6.4` → commit `51e29f4b13c04ca4f2ac161bfeb839f10b4e81b7` | Apache-2.0 | attempt/checkpoint schema (`TaskRunAttempt`/`TaskRunCheckpoint` in v4 `schema.prisma`), durable HITL wait (`Waitpoint`, `wait.forToken`, callback-hash routes). NOTE: the v3 rev is legacy-run-engine era; its batchedWrites/DE-engine semantics do NOT exist there (BLOCKED/PARTIAL, recorded honestly rather than inferred from README). |
| jarvis-rs durable runtime `jesenzhang/jarvis-rs` | commit `af9fbe754ef20c7de9a96d5f030e8044e97414ad` (vendored snapshot baseline) | private repo; **no license set** (read-only comparison, no reuse) | floor-level transactional outbox (`crates/jarvis-runtime/src/outbox.rs`: single-transaction events+effects, claim-tick-backoff, terminal attempt cap 8). Its durable model is weaker than PLAN-0004 (no lease/fence/step checkpoint) — used as a floor, not a target. |
| "AgentOS" | `agent0ai/agent-os` **NOT FOUND** (404 via API/WebFetch/ls-remote); nearest named substitute Agno `agno-agi/agno` tag `v3.0.11` → commit `8063a7ec3ac6ab4902bdc7966870d3b5c5f51ffc` | Apache-2.0 | REJECTED as durable-execution evidence (HITL = session persistence concept, no journal/lease/fencing); retained only as product-UX reference for review-wait semantics. |

Reading conclusions that matter below: Temporal enforces terminal-state
protection as a second write gate beyond fencing; Restate's fencing token and
epoch bump are the same shape as our `fence_version`; neither Temporal nor
Restate claims cross-system exactly-once — exactly-once *facts* (events/audit)
are our own contract to keep, which is precisely where the Wave 3 defect sat.

## 4. Current implementation facts (three layers)

4.1 Defined model (source-verified): `ProcessingJob` aggregate with fixed
six-step pipeline (`crates/document-processing/src/domain/job.rs:107-149`,
no runtime reordering), execution statuses separate from business/review
state, `lease: Option<…>`, `fence_version`, `aggregate_version`;
`ExecutionFence(worker_id, lease_token, fence_version)`; adapter-owned
`ProcessingExecutionUnitOfWork` port with the Revision 1 commit boundaries
(claim/start_step/complete_step/retry_or_fail/enqueue_ai/complete_ai/
save_candidate/finalize_review/cancel/release/reclaim).

4.2 Executed this round (full logs in `docs/reviews/2026-09-27-wave3-gates/`):
SQLite adapter suites (`document-processing-sqlite`, `document-sqlite`,
`audit*`, domain unit suites) — 735 passed / 50 ignored / 169 suites
workspace-wide; the new Wave 3 files `cancel_factuality.rs` (5 tests) and
`lease_fence.rs` (4 tests) execute against the real adapters.

4.3 Absent by design or by evidence: no pipeline-version column on jobs; no
`Attempt` entity (attempt counts live on job/step rows); no broker; the
messaging crate has zero production dependents; the legacy claim-port seam
(`request_cancel`/`release`/`reclaim_expired`) writes without audit/outbox and
has no production callers (worker uses the UoW). PostgreSQL multi-worker
behavior is CI/historical evidence only in this session (§9).

4.4 Owners: Document Intelligence owns execution state; Document Management
owns content revisions/storage refs; candidate+review state stays business.
"Job Completed ≠ business completion" is structural: `AwaitReview` ends the
execution while review facts belong to `CandidateReview` finalization.

## 5. Cancellation factuality model (fixed this round)

Baseline: AGENTS.md §9 (events express facts that happened) and
WORKFLOW baseline §13 (cooperative cancel; the cancellation process must be
audited). Canonical rules now enforced by tests on both adapters:

| Path | Intent evidence | Fact evidence (exactly at transition, exactly once) |
| --- | --- | --- |
| `cancel_processing` while Running (cooperative) | audit `processing_cancel_requested` | none yet (job still executing) |
| `cancel_processing` while Queued/WaitingForReview (immediate) | audit `processing_cancel_requested` | event + audit `processing_cancelled`, exactly once |
| `cancel_processing` while WaitingForAi (immediate) | `processing_cancel_requested` | event `document.processing.cancelled.v1` + audit `processing_cancelled` |
| `complete_step` sees `cancel_requested` | — | event + audit `processing_cancelled` (was: event only, no audit) |
| `retry_or_fail_step` with Cancelled disposition | — | event + audit `processing_cancelled` alongside `step_failed` |
| `release_job` cancel-priority | — | event + audit (already correct; kept) |
| `reclaim_expired_jobs` cancel-priority | — | event + audit `processing_cancelled` alongside `job_reclaimed` (SQLite and, new, PG — PG previously delegated to the audit-less claim-port seam and wrote nothing) |
| AI-task terminal 'cancelled' | — | event + audit `processing_cancelled` alongside `ai_task_failed` |

## 6. Failure matrix (D rows)

Environment legend: `local-SQLite` = executed on this workstation (real
adapters, in-process SQLite); `local-domain` = pure domain unit tests;
`PG-ignored` = exists and compiles, requires PostgreSQL, **NOT RUN locally**
(BLOCKED cause + rerun commands in `2026-09-27-wave3-gates/07-pg-lane-blocked.md`),
wired into `.github/workflows/ci.yml` integration job.

| Row | Semantics (baseline anchor) | Evidence (file :: test) | Environment | Verdict |
| --- | --- | --- | --- | --- |
| D01 | stale worker rejected (lease/fence fail-closed, §10) | `document-processing/src/domain/job.rs::stale_fence_is_rejected_after_expiry_reclaim`; `document-processing-sqlite/tests/lease_fence.rs::stale_fence_is_rejected_everywhere_without_evidence` (5 fenced mutations incl. heartbeat/release, zero evidence rows, version unchanged); messaging outbox lane `stale_worker_cannot_publish_or_fail` | local-domain, local-SQLite, PostgreSQL 18.4 | PASS. PG lane executed 2026-09-27 on the final candidate (`outbox_reliability` 7/0); the SQLite/domain evidence is unchanged |
| D02 | lease/expiry/reclaim/heartbeat/fence consistency | `lease_fence.rs::active_lease_is_neither_reclaimed_nor_double_dispatched`, `::heartbeat_after_lease_expiry_fails_closed`, `::successor_can_progress_after_reclaim`, `cancel_factuality.rs::reclaim_of_cancelled_expired_job_publishes_the_fact`; `job.rs::cancellation_wins_release_and_expiry_reclaim`; PG `document-processing-postgres/tests/contracts.rs::postgres_processing_contract_reclaims_expired_ai_task` | local-SQLite, local-domain, PostgreSQL 18.4 | PASS. PG mirror executed on the final candidate (`contracts` 3/0) |
| D03 | retryable vs terminal classification; storage failure ≠ LeaseLost | `job.rs::transient_retry_respects_limit_and_cancellation`; UoW retry event `document.processing.retry-scheduled.v1` in `revision1.rs::revision_one_uow_commits_fixed_pipeline_and_review_atomically`; mapping separation is source-level: `map_sql_error` (Failed/Conflict/TenantMismatch) vs explicit `LeaseLost` only from fence/lease checks (`crates/document-processing-sqlite/src/lib.rs` retry/complete/reclaim arms) | local-domain, local-SQLite + source | PASS |
| D04 | crash recovery at UoW commit boundaries | Round 2 `cancel_factuality.rs::cancellation_boundaries_rollback_and_survive_file_reopen`: request/complete_step/reclaim transaction failure and reopen. Historical `revision1.rs` is in-memory Review rollback/replay, NOT restart evidence. | local-SQLite | PARTIAL; process kill, other boundaries and PG NOT RUN |
| D05 | duplicate dispatch cannot double-run | `lease_fence.rs::active_lease_is_neither_reclaimed_nor_double_dispatched`; outbox lane `concurrent_claim_no_duplicates`, `concurrent_claim_100_has_unique_ownership` | local-SQLite, PostgreSQL 18.4 | PASS (SQLite and PostgreSQL). PG lane executed on the final candidate, 7/0, including the 100-way concurrent claim. Concurrency is intra-process against a real server, not separate worker processes |
| D06 | cancellation facts and terminal idempotency | `cancel_factuality.rs` covers request, immediate AI/Queued/WaitingForReview, step/failure/reclaim; Round 2 file recovery coverage; PG mirror `cancel_factuality_pg.rs` (5 tests) | local-SQLite; PostgreSQL 18.4 | PASS for listed sequential transitions on SQLite and PostgreSQL (`cancel_factuality_pg` 5/0 on the final candidate; the first real run failed on a fixture-only VARCHAR/UUID bind). Concurrent cancel/completion NOT RUN |
| D07 | checkpoints persist and are reused after interruption | `revision1.rs` proves artifact-before-AI ordering; `successor_can_progress_after_reclaim` proves successor progress, not checkpoint reuse after process crash | local-SQLite/source | PARTIAL; checkpoint skip/reuse after crash needs a dedicated fixture |
| D08 | no blind replay after unknown side effects | fenced completion (`complete_ai_and_resume`) + durable text artifact read (source); durable AI lane re-dispatch keeps single active task via claim (`claim_next` filters `next_attempt_at`); Restate journal-style command dedup is NOT replicated — recorded as gap D-G04 rather than claimed | source + local-SQLite | PARTIAL (C2 gap registered) |
| D09 | pipeline version pinned per task | field/fixture absent; [ADR-0027](../adr/ADR-0027-processing-pipeline-version-pinning.md) Proposed | — | OPEN C1 / NOT IMPLEMENTED; fixed pipeline is not a Baseline exemption |
| D10 | PostgreSQL multi-worker real evidence | Round 2 final candidate executed the PostgreSQL lanes on PostgreSQL 18.4 in a dedicated database: `contracts` 3, `cancel_factuality_pg` 5, `chain_pg` 3, `outbox_reliability` 7 (incl. `concurrent_claim_100_has_unique_ownership`), `inbox_idempotency` 4 — 22 passed / 0 failed / 0 ignored, all migrations via the project migration CLI. See §14 and [the gate bundle](2026-09-27-wave3-round2-final-candidate-gates/README.md) | PostgreSQL 18.4, dedicated DB, single process, `--test-threads=1` | PARTIAL: real-server evidence now exists, which the Round 1 "historical CI only" line did not provide. Still one OS process against one server: no separate worker processes, no rolling upgrade, no multi-node |
| D11 | `AwaitReview` ≠ business completion | review finalization is a separate UoW command recorded with candidate facts (`revision1.rs` review phase; `candidate.rs::candidate_evidence_is_revision_and_line_bounded`); job `Succeeded` never mutates business state — no such path exists in the adapters | local-SQLite, local-domain | PASS |

## 7. Gap registry

Fields per PLAN-0014 §5: ID / current behavior / violated contract / repro
entry / impact / severity / decision / fix files / closing evidence /
residual limits.

### Closed in this wave (C1)

**D-G01 — cancellation fact event published at intent; fact audit missing on transition paths.**
Current behavior (pre-fix): `cancel_processing` published
`document.processing.cancelled.v1` + `processing_cancelled` audit while the
job was still Running (PG and SQLite); `complete_step`'s cancel branch emitted
the event without any audit; the cancelled disposition audited only
`step_failed`; SQLite reclaim and BOTH adapters' AI-cancel transition emitted
the fact without the job-level audit; PG's UoW `reclaim_expired_jobs` delegated
to the audit-less claim-port seam (silent reclaims).
Violated contract: AGENTS.md §9 (fact events), WORKFLOW baseline §13/§18
(cancellation audited; recovery visible). Repro entry:
`cargo test -p document-processing-sqlite --test cancel_factuality` observed
RED (premature event; event count 2 for one fact) before the fix.
Impact: consumers of the outbox would treat cancelled as happened while
AI/steps keep running; audit trail could not answer "when was cancellation
actually realized". Severity C1. Decision ADOPT(baseline)/FIXED.
Fix files: `crates/document-processing-sqlite/src/lib.rs`,
`crates/document-processing-postgres/src/lib.rs`.
Closing evidence: `cancel_factuality.rs` (5 PASS local-SQLite),
`cancel_factuality_pg.rs` (5 tests, PG-ignored, CI-wired).
Residual: PG execution NOT RUN in this session (§9).

**D-G02 — adapter-level fencing/reclaim evidence gap (D01/D02/D05).**
Pre-fix: fencing was pinned at domain level and via the shared contract, but
no SQLite adapter-level negative tests existed (stale fence × every fenced
mutation; live-lease reclaim/double-dispatch; expired heartbeat). Repro entry:
gap identified by test inventory, closed test-first-adjacent. Severity C1
(architecture fitness requires adapter contract evidence).
Decision FIX. Fix files: `crates/document-processing-sqlite/tests/lease_fence.rs`
(4 PASS), CI wiring unchanged (already runs the suite).
Residual: PG fenced-negative lane remains only via the two `#[ignore]`d PG
files; adding PG-specific SKIP LOCKED negatives is C2 (existing historical
evidence kept until re-run).

### Registered, not repaired in this wave

**D-G03 — pipeline version is not persisted per task (baseline §8 "Pipeline 版本必须写入任务").**
Current behavior: the pipeline is compile-time fixed (`job.rs:107-149`), so
today no ambiguity exists, but the baseline requires the version on the Job.
Violated contract: WORKFLOW baseline §5/§8 (literal deviation, latent).
Impact: an upgrade that reorders steps could not be interpreted against
historical jobs. Severity C1 (open Baseline conformance gap). Decision PROPOSED →
separate design proposal (new migration column + write-at-queue + interpretation
rule; references: Temporal opaque markers, Trigger v4 attempt→runtimeEnvironment
binding). Repro: source absence (`document_processing_jobs` has no
`pipeline_version` in any migration).

**D-G04 — no `Attempt` record and no provider-request correlation for AI
side effects (baseline §5/§12; D08 partial).** Current behavior: attempt
counters on job/step rows; no per-attempt row (worker id, error class,
external request ref); AI provider requests are not persisted for status
re-query after timeout. Decision DEFER (C2). Restate's command-dedup and
Temporal's attempt records are the calibration; current provider idempotency has not been established by this review; unknown side-effect outcome remains an unverified constraint.

**D-G05 — legacy claim-port seam (`ProcessingJobClaimPort::request_cancel/release/reclaim_expired`) writes durable state with no audit/outbox.** No production callers (worker path is the UoW; verified by grep of `apps/*`). Decision DEFER (C2) with direction: delete the seam or route it through the UoW once `data-repair`/tests migrate. Repro: `crates/document-processing-{postgres,sqlite}/src/lib.rs` claim-port impls.

**D-G06 — processing outbox events carry no envelope correlation/causation/producer fields** (API/event standard §16; AGENTS §9 "事件包含唯一 ID、版本、租户、关联和因果信息"). Outbox rows carry event_id/type/schema_version/tenant/aggregate/occurred_at but no correlation/causation columns (job/task correlation context is propagated into unified audit trace_id, but does not establish a complete outbox envelope). Severity C1-by-standard, but closing it changes the published event shape = public contract change → separate design proposal (cross-ref A-G07 in the audit/messaging review). Decision DEFER with explicit blocker noted; review continues.

**D-G07 — no external anti-truncation checkpoint for durable facts** is covered in the audit review (A-G03); cross-referenced only.

### REJECTED reference patterns (with reasons)

- Temporal task-queue/matching, sticky workers, timer queues: platform-runtime architecture, not baseline requirements; adopting means introducing an execution platform (prohibited scope).
- Restate log-structured journal + partition leader epochs: semantics (replay-skip, fencing) are KEEP-equivalents of our lease/fence design; the log architecture itself is out of scope for a modular-monolith adapter.
- Kafka transactional.id EOS / NATS JetStream dedup windows: we have no broker (zero-dependency messaging crate); inventing one violates Wave 3 boundaries.
- Generic DAG engine / workflow designer / scheduler / registry / new deployment unit: prohibited by PLAN-0014 Wave 3 scope and baseline §8/§23 (first phase stays on the code-defined fixed pipeline).

## 8. Decision register (references → dispositions)

| Reference mechanism | Disposition | Why |
| --- | --- | --- |
| claim/lease/fence_version + terminal-state second gate | KEEP (ours already matches Restate fencing tokens + Temporal `ErrWorkflowFinished` at aggregate level) | `require_lease` + `is_terminal` guards + new reclaim audit close the loop |
| cooperative cancel with intent audit + fact event at transition | ADOPT (fixed this wave, D-G01) | baseline §13 verbatim |
| cancel-priority reclaim publishes the fact | ADOPT (fixed, SQLite + PG) | baseline §18 |
| per-attempt records + provider request refs | DEFER (D-G04) | calibration from Temporal attempts / Restate journal |
| pipeline version on job | DEFER → design proposal (D-G03) | baseline letter vs compile-time-fixed reality |
| event envelope fields | DEFER → design proposal (D-G06) | public contract change |
| durable HITL awakeables (Restate) / wait-for-token (Trigger v4) | KEEP our `WaitingForAi`/`AwaitReview` + review-finalize command (equivalent semantics, no new runtime) | scope |
| AgentOS concept | REJECT | no durable semantics found |

## 9. Validation matrix (executed 2026-09-27, 本地 Windows workstation)

Full logs + exit codes retained in `docs/reviews/2026-09-27-wave3-gates/`.

| Gate | Command | Exit | Result |
| --- | --- | --- | --- |
| fmt | `cargo fmt --all -- --check` (after `cargo fmt --all`) | 0 | clean |
| check | `cargo check --workspace --all-targets --all-features` | 0 | pass |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | pass |
| tests | `cargo test --workspace --all-features` | 0 | 735 passed, 50 ignored, 169 suites |
| architecture fitness | `pwsh scripts/check-architecture.ps1` (wraps `cargo run -p architecture-check -- check`) | 0 | `Cargo metadata architecture fitness: PASS` |
| whitespace | `git diff --check` | 0 | clean |
| PG lanes | see `07-pg-lane-blocked.md` | — | BLOCKED locally (no usable local PG credentials; service left untouched); CI integration job wired: `chain_pg`, `contracts`, `cancel_factuality_pg`, `outbox_reliability`, `inbox_idempotency` with `--include-ignored --test-threads=1` |

Ignored-test accounting: the 50 ignored include the 8 NEW PG-pinned Wave 3
tests (`audit-postgres chain_pg` ×3, `document-processing-postgres
cancel_factuality_pg` ×5) — they have NEVER executed against a real PostgreSQL
in this session and must not be reported as passing; historical CI evidence
covers only pre-existing lanes.

> **Superseded on 2026-09-27 by §14.** Both the PG row above and this
> accounting paragraph describe the Round 1 session and are kept verbatim. The
> five lanes were later executed against a real PostgreSQL 18.4 on an
> operator-authorized dedicated database.

## 14. Round 2 final candidate — real PostgreSQL execution (2026-09-27)

Candidates, in order:

| Stage | SHA | Note |
| --- | --- | --- |
| Round 1 reviewed HEAD | `6b324ea76986640a3f1baef7ecebdb2c53fbd2c6` | independent verdict **FAIL / NEEDS REPAIR** |
| Round 2 repair candidate | `81640e79147128925d10efb0f6ff30a0c568ec33` | the two Round 1 regressions; [gates](2026-09-27-wave3-round2-gates/README.md) |
| **Final candidate** | `2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a` | adds the PostgreSQL-found repairs; [gates](2026-09-27-wave3-round2-final-candidate-gates/README.md) |

The operator authorized creating a dedicated database on the local server. The
legacy `TEST_POSTGRESQL_URL` / `TEST_POSTGRESQL_RESTORED_URL` variables were not
used as targets: per `docs/reports/PLAN-0012-COMPLETION-AUDIT.md` they belong to
another project's contract-test harness (`contract_management_test`,
`contract_management_restore_test`), so no migration or cleanup was run against
them. No shared service or authentication configuration was modified and no
password was guessed.

PostgreSQL **18.4**, dedicated database `plan0014_wave3_r2`, created by
`cargo run -p migration -- --backend postgres up` (exit 0, migrations 1 to 19).
Lanes run with `--include-ignored --test-threads=1`:

| Lane | Result |
| --- | --- |
| `document-processing-postgres --test contracts` | 3 passed, 0 failed, 0 ignored |
| `document-processing-postgres --test cancel_factuality_pg` | 5 passed, 0 failed, 0 ignored |
| `audit-postgres --test chain_pg` | 3 passed, 0 failed, 0 ignored |
| `messaging --test outbox_reliability` | 7 passed, 0 failed, 0 ignored |
| `messaging --test inbox_idempotency` | 4 passed, 0 failed, 0 ignored |
| total | **22 passed, 0 failed, 0 ignored** |

Two defects appeared only here, and neither was one of the two Round 1
regressions:

1. **Product, new C1.** The `audit` crate hashed `occurred_at` / `recorded_at`
   at nanosecond precision while PostgreSQL `TIMESTAMPTZ` stores microseconds,
   so every PostgreSQL audit row failed re-verification. That is the
   tamper-evidence authority failing on the production backend while 741
   SQLite/unit tests stayed green, and it is the strongest available evidence
   that `#[ignore]` plus CI wiring is not evidence. It lands in the audit
   review as A-G09 and is recorded here because `D05`/`D07`/`D08` cite the same
   audit rows written in the same transaction as the durable state change.
2. **Fixture only.** `cancel_factuality_pg` bound a `Uuid` to the legacy
   `outbox_events.tenant_id VARCHAR(100)`; PostgreSQL rejected the comparison
   (SQLSTATE 42883) where SQLite accepted it silently. Production binds that
   column as text, so this was the test lying, not the product.

Both were fixed in the final candidate and re-verified: six local gates exit 0
with **742 passed / 0 failed / 50 ignored**, and all five lanes pass on a fresh
dedicated database.

Limits this run does not remove. The lanes need a database that starts empty —
`claim_next` and `claim_batch` are global queues, so a second run over rows left
by an aborted run fails `contracts` and `outbox_reliability`; that is a fixture
isolation limitation, recorded rather than hidden. A file close-and-reopen is
still not a process kill, so **D04 and D07 stay PARTIAL**: there is no
kill-between-commit-boundaries execution and no checkpoint reuse after a real
crash. No real provider side effect or
external source pin ran. **D-G03** (pipeline version pinning, ADR-0027 Proposed)
and **D-G06/A-G07** (event envelope, ADR-0028 Proposed) remain open C1. Durable
audit ordering is not message reordering, so **D04** does not gain ordering
proof from this run.

Determination for this document: **READY FOR INDEPENDENT RE-REVIEW / NOT
INTEGRATED**, PostgreSQL executed. Independent re-review not yet performed; no
self-filled verdict. R1 claim stands only for a re-reviewed candidate;
**R2 BLOCKED**. Wave 4/5 not started.

## 10. Wave 3 exit determination (this doc)

Round 1 fixes have local SQLite evidence; PG execution remains unverified. D-G03 and D-G06 remain open C1. Round 1 independent review found two additional regressions and acceptance overstatement; Round 2 repairs and corrected evidence are recorded separately. PostgreSQL execution is
BLOCKED locally with exact rerun commands and CI wiring. Therefore: **READY
FOR INDEPENDENT REVIEW / NOT INTEGRATED**. This review does not claim R2,
does not self-assign any independent-reviewer verdict, and does not open
Wave 4/5.

## 11. Reviewer questions (for the independent reviewer)

1. D-G01 fix: is splitting intent (`processing_cancel_requested`) from fact
   (`document.processing.cancelled.v1` + `processing_cancelled`) the right
   baseline reading of §13 + AGENTS §9, or should an explicit
   `cancel-requested.v1` event also exist (it currently does not — the intent
   is audit-only)?
2. Is the architecture-check dev-dependency exemption (`4b4ee71`) acceptable
   governance, or should test fixtures instead move to an app-level
   composition test?
3. PG `reclaim_expired_jobs` now mirrors the SQLite UoW audit/event behavior
   instead of delegating to the legacy claim port (source:
   `document-processing-postgres/src/lib.rs`). Should the legacy claim-port
   seam (D-G05) be deleted outright in Wave 4?
4. D-G03 (pipeline_version) — accept as C2-defer given the compile-time fixed
   pipeline, or is the baseline §8 letter a R2 blocker for the processing
   capability?

## 12. Independent reviewer record

<pending — to be filled by the independent reviewer; this record stays empty
until then. Verdict vocabulary: PASS / PASS WITH C2-C3 / FAIL. R2 admission is
not on the table for Wave 3.</pending>

## 13. Round 2 correction

[Repair record](2026-09-27-wave3-round2-repair.md) supersedes Round 1 acceptance claims where they conflict. Historical gate logs remain historical. D-G03 is open C1 ([ADR-0027](../adr/ADR-0027-processing-pipeline-version-pinning.md)); D-G06 is open C1 ([ADR-0028](../adr/ADR-0028-processing-event-envelope-evolution.md)). Both proposals are NOT ACCEPTED. No independent PASS for the repair.

## 15. Round 2c - messaging lanes re-run after an audit-side repair (2026-09-28)

Round 2c changed no messaging code. It is recorded here because the same five
lanes were the acceptance gate, and because the Round 2c defect is a limit on what
§14's result can be read to prove.

Round 2b's 22 PostgreSQL passes did not cover recovery of historically signed audit
rows. `audit_events.occurred_at` and `recorded_at` are `timestamp with time zone`
with `datetime_precision = 6` - re-confirmed by probe in this round - so a
PostgreSQL row can never hold the sub-microsecond tail that Round 2b's reader
truncated. The defect was only reachable through the `SQLite` adapter, which stores
RFC 3339 text, and no test read such a row back. Messaging passed for the right
reasons and simply never exercised the broken path. See A-G10 in the
[audit review](2026-09-27-saas-align-audit-messaging.md) and the
[Round 2c repair record](2026-09-27-wave3-round2c-repair.md).

Re-run on the final tree against the fresh dedicated database
`plan0014_wave3_r2final` (created empty by the migration CLI, 43 public tables,
migration ledger 1-19, 0 audit rows before the lanes): `outbox_reliability` 7 passed,
`inbox_idempotency` 4 passed, plus `contracts` 3, `cancel_factuality_pg` 5 and
`chain_pg` 4 — **23 passed / 0 failed / 0 ignored**, all exit code 0. The +1 test is
the new audit precision round-trip.

State unchanged: **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**, ADR-0027 and
ADR-0028 Proposed / NOT ACCEPTED, Wave 4 and Wave 5 not started.
