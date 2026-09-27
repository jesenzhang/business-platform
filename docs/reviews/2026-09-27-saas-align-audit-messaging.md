# PLAN-0014 Wave 3 — Audit Chain & Messaging Reliability Reference Conformance Review

> Wave: PLAN-0014 Wave 3 (Durable Task Execution / Audit / Messaging)
> Status: Wave 3 implementation reviewed / self-verified / **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**
> Review date: 2026-09-27 (actual date; no back-filling)
> Branch: `feat/PLAN-0014-wave3-durable-audit-messaging-conformance`
> Base: `443a11cabdce1dd34006feaf08aa8be3382150c1` (Wave 2 closeout; Wave 2 NOT INTEGRATED)
> Fix commits: `217e9500471f84e2b597b4315bfda3873f365519` (audit-chain writer fix + chain test suites + CI wiring), `4b4ee71e610546449b33a84b034c18a17da65308` (architecture-check dev-edge exemption; shared with the durable review)
> Maturity level claimed: **R1 (foundation conformance)**. R2 admission is explicitly NOT claimed; verdicts are recorded only by the independent reviewer (PASS / PASS WITH C2-C3 / FAIL).

Companion document: `2026-09-27-saas-align-durable-execution.md` (same wave,
same base/candidate; cross-references D-Gxx).

## 1. Review question and method

Review question: do the Audit subsystem (append-only audit events, per-tenant
hash chain, redaction, replay/verification) and the Messaging reliability
substrate (outbox claim/publish, inbox idempotency) conform to the platform
baseline (SECURITY_ARCHITECTURE audit requirements, API/event standard §16,
DATA_OWNERSHIP §audit rules, AGENTS §9/§10) when measured against fixed
external references — and are tamper-evidence claims in docs and code honest
(what the chain does and does not detect)?

Method: identical to the durable-execution review (PLAN-0014 §4): baseline →
tag→SHA-pinned references, source evidence separated from vendor marketing →
three-layer facts → gap registry → bounded C1 repair with failing-first tests
→ failure matrix rows bound to test path + name + command + environment +
verdict. Read-only investigation; only proven C0/C1 repaired; no broker, no
external chain service, no new deployment unit introduced; no push / merge /
release / Wave 4-5 / PLAN-0015 / PLAN-0006.

## 2. Baseline and candidate

| Item | Value |
| --- | --- |
| Baseline documents | `SECURITY_ARCHITECTURE` (audit integrity, redaction), `API_AND_EVENT_CONTRACT_STANDARD` §16, `DATA_OWNERSHIP_AND_CONSISTENCY` (audit+business same transaction), AGENTS §9/§10, ADR-0013 |
| Wave dependency | same as durable review: Wave 2 branch @ `443a11c` |
| Wave 3 fix code | `217e950`, `4b4ee71` (shared with durable review) |
| Runnable environments | local-SQLite (in-process) executed; PostgreSQL lanes BLOCKED locally this session (cause + rerun commands + CI wiring in `2026-09-27-wave3-gates/07-pg-lane-blocked.md`) |

## 3. Fixed references (review date 2026-09-27)

Pinned via `git ls-remote` + in-clone `rev-parse`; clones under
`%TEMP%\plan0014-w3-refs\`. License constraints honored: BSL / AGPL / EE
sources read for semantics only; no code copied; proprietary parts recorded
as such rather than assumed.

| Reference | Pinned revision | License | Role / honest reading |
| --- | --- | --- | --- |
| ZITADEL `zitadel/zitadel` | tag `v4.18.0` → commit `6d7878a2e4128517684f43ba7774dd4a9f64ba36` | AGPL-3.0 (semantics-only reading) | production audit log: sequence-numbered rows per instance, unique `(instance, sequence)` as the concurrency mechanism, **no hash chain**; "append-only" is a write-path convention with retention deletion existing (`AuditLogRetention`) — calibration that a hash chain is above, not below, an accepted industry baseline |
| immudb `codenotary/immudb` | tag `v1.11.2` → commit `bfdce03649f52d575be46f74425fd18eaf4fa69c` | **BSL 1.1** (semantics-only) | Merkle inclusion proofs, root history, dual proofs; anti-truncation **requires a client-pinned prior root** (`root.go`, `dual_prover.go`) — README "tamper-proof / immutable" is the marketing-overclaim sample our own wording must avoid |
| Infisical `Infisical/infisical` | tag `0.165.16` → commit `0b52a6043719869d8a8c1d234918253799b5ba86` | dual MIT / Enterprise (audit-log service is **EE** — not FOSS behavior) | outbox: `SELECT … FOR UPDATE SKIP LOCKED` claim + `sweepStaleClaims` (10-min stale reclaim) + `MAX_ATTEMPTS=5` drop-with-labeled-metric; dedup quote: "unique (streamId, auditLogId) constraint is the hard dedup guarantee"; no tamper evidence on its audit rows |
| Debezium `debezium/debezium` | tag `v3.6.3.Final` (tag object `6d46a77ba7a4448d27fcd8fc99be1db2b738f1d8`) → commit `8ee69a6972ae1ab50b90550a0b5bef082ff8bdd3` | Apache-2.0 | outbox core contract: no sequence column in the core router — ordering is by `aggregateid` as the Kafka key; the `__seqfield`/aggregation columns belong to the Quarkus outbox extension repo (**NOT FOUND in core pin** — recorded, not inferred) |
| NATS `nats-io/nats-server` | tag `v2.15.0` (tag object `4165d0d62508a8abe99feed39bb9e8ca4216aede`) → commit `eb763679aa3c24a40dcd3012aa046ad1996d851c` | Apache-2.0 | JetStream dedup window is time-bounded (2 min default idempotency window); poison handling = `MaxDeliver` advisory then drop to advisory subject — **no native DLQ**; calibration for our terminal-`failed` design |
| Kafka `apache/kafka` | tag `4.3.1` → commit `a07059eb9b5bac1bfdbb1e74313f2fae4ca20fd9` → commit-object `26b251a451ce941d3d7a55e6487bcb7f16b5ad48` | Apache-2.0 | producer idempotence is scoped to a single producer session; cross-restart fencing needs `transactional.id`; DLQ is a Connect/ecosystem feature, not broker. Literal "epoch" column in our sense **NOT FOUND** — recorded honestly |

Key conclusion carried into the matrix: our per-tenant hash chain +
trusted-checkpoint wording (baseline docs) is at or above the pinned open
baselines (ZITADEL/Infisical have no chain at all; immudb's stronger proofs
still require client-held roots). Our inbox unique-key + fencing is the same
class as the Infisical "hard dedup" mechanism and stronger than JetStream's
bounded window. No reference in this set offers unconditional server-side
anti-truncation, which is exactly the limitation our docs must (and do) state.

## 4. Current implementation facts (three layers)

4.1 Defined model (source-verified): `AuditEvent` (`crates/audit/src/lib.rs`)
— deterministic record hash over canonicalized fields with redaction applied
**before** hashing; per-tenant chain: `chain_version` 0 = legacy/unverified
(no hashes, sequence 0, readable but not chain-verifiable), 1 = v1 chain
(stream_sequence = MAX+1, genesis anchor `audit_chain_genesis(tenant)`,
`previous_hash` linkage, `with_chain_metadata` validation, rehydrate
invariants at `audit/src/lib.rs:252-265`). Append path: audit insert +
business write + outbox insert share one transaction in both document UoWs;
PG appends serialize per tenant via `pg_advisory_xact_lock(hashtextextended(tenant))`.
Messaging: `outbox_events` + `inbox_events` (`crates/messaging/`) with claim
lease (`claimable_at`), fencing (`lease_token`), attempt accounting,
deterministic claim order; inbox keyed `(consumer, message_key)` unique.

4.2 Executed this round: `document-sqlite/tests/audit_chain.rs` 3 tests,
`audit` unit tests 4 (incl. `hash_chain_detects_payload_change`,
`append_sequence_is_independent_of_business_time`), full workspace 735
passed / 50 ignored (log `04-test.log`). `audit-postgres/tests/chain_pg.rs`
3 tests compile, PG-ignored.

4.3 Absent / honest limits: tail truncation is **not** detectable by
re-verification alone (a shorter valid prefix verifies) — detected only
against an externally pinned checkpoint, which does not exist (A-G03);
redaction is a key blacklist, not a structural schema (A-G08); the messaging
crate has no production consumers yet (A-G04); there is no DLQ (A-G05);
repair-ledger rows carry NULL chain columns (A-G02).

## 5. Tamper-evidence model — what we claim vs what the code proves

Claimed (docs/code): per-tenant append-only hash chain detects modification,
replacement and interior deletion of verified (chain_version=1) rows; legacy
rows are counted separately and never presented as chain-verified.
Proven by tests: payload change, interior deletion, and row replacement
detected (SQLite + unit; PG mirrors compiled, NOT RUN this session).
NOT claimed anywhere in this repo: anti-truncation without a trusted external
checkpoint (test `audit_chain.rs::chain_detects_mutation_and_interior_deletion_but_not_tail_truncation`
asserts the limitation stays true — the name itself encodes it; immudb's own
anti-truncation requires client-pinned roots — same class of limitation,
stated less loudly by vendors).

## 6. Failure matrix (A rows)

Environment legend identical to the durable review
(`local-SQLite` / `local-unit` / `PG-ignored` = exists + compiles, NOT RUN
locally, CI-wired).

| Row | Semantics (baseline anchor) | Evidence (file :: test) | Environment | Verdict |
| --- | --- | --- | --- | --- |
| A01 | audit + business write atomically (DATA_OWNERSHIP; AGENTS §7) | `document-sqlite/tests/contracts.rs::sqlite_rolls_back_the_document_when_atomic_side_effect_fails`; processing side: `document-processing-sqlite/tests/revision1.rs::revision_one_uow_commits_fixed_pipeline_and_review_atomically`; PG mirrors `contracts.rs::postgres_processing_revision_one_uow_is_atomic_and_replayable` (audit-postgres PG audit+outbox lane historical-CI only) | local-SQLite, PG-ignored | PASS (SQLite); PG NOT RUN this session |
| A02 | tamper evidence is honest: detect payload change / interior deletion, do NOT claim anti-truncation | `audit/src/lib.rs::hash_chain_detects_payload_change`; `document-sqlite/tests/audit_chain.rs::chain_detects_mutation_and_interior_deletion_but_not_tail_truncation` (mutation + interior deletion detected; tail truncation asserted NOT detected); `audit-postgres/tests/chain_pg.rs::chain_detects_mutation_and_interior_deletion_but_not_tail_truncation` mirror | local-unit, local-SQLite, PG-ignored | PASS (SQLite/unit); PG NOT RUN |
| A03 | duplicate / replay converges to one side effect (AGENTS §9 consumers handle duplicates) | `messaging/tests/inbox_idempotency.rs::sequential_duplicate_commits_one_side_effect`, `::concurrent_duplicate_commits_one_side_effect`, `::rollback_releases_marker_and_side_effect`, `::consumers_have_independent_idempotency_scopes`; replay determinism: `audit_chain.rs::document_created_audit_row_is_chain_verifiable_and_readable` (chain re-verified after reopen/replay of the same rows) | local-SQLite (chain replay), PG-ignored (inbox suite) | PASS (chain replay); inbox suite NOT RUN (needs PG) |
| A04 | out-of-order delivery rules (sequence ≠ wall clock; independent of business time) | `audit/src/lib.rs::append_sequence_is_independent_of_business_time`; chain validation uses stored `previous_hash` linkage, never timestamps; `chain_pg.rs::concurrent_appends_form_one_verified_tenant_chain` (advisory-lock serialization, sequences 1..=10 contiguous) | local-unit, PG-ignored | PASS (unit); PG NOT RUN |
| A05 | publisher crash after claim is recoverable; republish is idempotent | `messaging/tests/outbox_reliability.rs::expired_lease_recovery` (reclaim by new worker), `::concurrent_claim_no_duplicates` / `::concurrent_claim_100_has_unique_ownership` (no double claim), `::stale_worker_cannot_publish_or_fail` (fenced old lease rejected); event_id stable across republish (source: publish path mutates only lease/attempt columns) | PG-ignored | NOT RUN locally (all 7 outbox tests require PostgreSQL; cause + rerun commands in gates/07) |
| A06 | retry exhaustion → explicit terminal state, poison message cannot loop | `outbox_reliability.rs::failure_schedules_retry` (backoff), `::max_attempts_reached` (terminal `failed`, row retained, no re-claim); calibration: Infisical drops after 5 with labeled metric, JetStream drops after MaxDeliver — our terminal-without-delete is the stricter choice (A-G05 KEEP) | PG-ignored | NOT RUN locally |
| A07 | schema/version evolution: readers survive legacy rows, writers stay on v1 | `document-sqlite/tests/audit_chain.rs::legacy_null_chain_rows_stay_readable_and_are_counted_as_legacy` (NULL hash columns from pre-004 migrations) + `chain_pg.rs::legacy_boundary_rows_stay_readable_and_new_chain_continues_after_them` (PG mirror); `crates/audit-sqlite/src/lib.rs` `into_event` normalizes `chain_version NULL→0` matching PG migrations 013 (backfill 0) / 015 (reset); `schema_version` carried on every row | local-SQLite + source | PASS |
| A08 | tenant/actor/resource/outcome/correlation present; no secrets/lease tokens/raw text/storage paths in audit or events | `audit/src/lib.rs::validates_and_redacts_sensitive_details`, `::legacy_details_are_redacted_for_readers`; cancel/lease suites assert evidence rows contain no lease tokens or raw text (`lease_fence.rs::stale_fence_is_rejected_everywhere_without_evidence`; `cancel_factuality.rs` payload assertions); envelope correlation/causation columns missing → gap D-G06/A-G07 | local-unit, local-SQLite | PASS WITH registered C1 contract-change gap (D-G06) |

## 7. Gap registry

### Closed in this wave (C1)

**A-G01 — the document-processing SQLite writer never produced a verifiable chain.**
Current behavior (pre-fix, `git show 443a11c`): `document-sqlite`'s
`insert_audit` used a 17-column INSERT that omitted
`recorded_at`/`stream_sequence`/`chain_version` entirely and bound
`previous_hash`/`record_hash` to NULL — every document-processing audit row
landed as legacy/unverified even after the 004 chain migration; replay
verification of the same rows returned mismatch (the rehearsal's own
`verify_rehearsal_events` compared rebuilt events field-by-field and failed).
Violated contract: SECURITY_ARCHITECTURE audit-integrity (new writes must be
chain-verifiable), DATA_OWNERSHIP (audit rows must be trustworthy evidence).
Repro entry: `cargo test -p document-sqlite --test audit_chain` observed RED
before the fix (first append produced no `record_hash`; continuation failed).
Impact: the chain existed only in schema and in the audit-postgres writer; any
tenant relying on document-processing audit would silently have zero
verifiable history. Severity C1. Decision FIX.
Fix files: `crates/document-sqlite/src/unit_of_work.rs` (writer:
MAX+1 sequence, previous-hash lookup with genesis fallback,
`with_chain_metadata`, full 20-column v1 insert), `crates/audit-sqlite/src/lib.rs`
(reader normalization NULL→0 documented against PG 013/015),
`crates/document-sqlite/tests/audit_chain.rs` (new suite, 3 tests).
Closing evidence: 3 PASS local-SQLite; existing suites that write audit
(document-sqlite contracts/concurrency/revision_history,
plan-0009-rehearsal) green after fixtures apply both migration catalogs.
Residual: chain coverage for other writers (user, org, platform audit-sqlite
adapters) was already v1 via `audit-sqlite`; the PG document lane is
executed only in CI (PG-ignored mirrors).

### Registered, not repaired

**A-G02 — repair-ledger audit rows always have NULL hash columns.** By
design of the repair ledger table (operator evidence, not tenant chain), but
today nothing says so in the schema comment — doc-honesty C2. Decision:
DEFER (comment + read-model labeling in next doc touch).
**A-G03 — no external anti-truncation checkpoint.** Chain re-verification
accepts a truncated tail; immudb-grade safety needs pinned roots or a witness.
C2 DEFER (design proposal: periodic signed checkpoint export + verification
CLI). Docs already state the limitation honestly (A02 row); no overclaim to
fix.
**A-G04 — messaging crate has zero production dependents** (verified: no
`crates/*/Cargo.toml` outside `apps/` dev-deps references it). It is
substrate, not a shipped capability. C2 — honest status; wiring waits for a
capability that needs cross-context events (PLAN-0015 candidates, not
activated here).
**A-G05 — no DLQ; terminal `failed` rows are retained in place.** JetStream
and Infisical drop/emit-advisory; Kafka DLQ is Connect-level. Retention +
terminal state + later replay tooling is the strictest of the three. C2 KEEP.
**A-G06 — chain append logic is duplicated** (document-sqlite writer vs
audit-sqlite writer vs audit-postgres writer each compute the previous-hash/
sequence logic). Drift risk (this very bug was that drift). C2 DEFER →
extraction into `audit` crate helper proposed alongside D-G04 work.
**A-G07 — event/audit envelope lacks correlation/causation/producer columns**
(API/event standard §16 verbatim requirement). Same root as D-G06; closing it
changes published DTOs = public contract change → separate design proposal
with explicit blocker; C1-by-standard, DEFER to proposal (not silently fixed).
**A-G08 — redaction is a key blacklist** (`SENSITIVE_KEYS`), so a new
sensitive field name added elsewhere can leak until registered. Infisical's
EE logger redacts structurally at the DTO boundary. C2 DEFER (field-registry
lint or DTO-level redaction).

## 8. Decision register (references → dispositions)

| Reference mechanism | Disposition | Why |
| --- | --- | --- |
| unique-key hard dedup (Infisical quote; Kafka idempotent producer) | KEEP — our `inbox_events` unique `(consumer, message_key)` + transactional side effect is the same guarantee, scoped per consumer | already implemented + tested (PG lane) |
| SKIP LOCKED claim + stale-claim sweep (Infisical, Debezium) | KEEP — ours adds a lease token + fence version + attempt accounting (stricter than 10-min sweep alone) | `outbox.rs` + tests |
| sequence-based concurrency control without hash chain (ZITADEL) | KEEP ours above baseline: tenant advisory-lock sequence + hash chain; ZITADEL proves a chain is optional for an accepted audit baseline, so our chain claims must stay scoped to what it proves (A02) | docs wording |
| immudb root/dual proofs | DEFER (A-G03) — anti-truncation requires external checkpoint discipline; adopt the honest-wording lesson now | license BSL, semantics only |
| JetStream bounded dedup window / no native DLQ | KEEP terminal-`failed` retention instead | A-G05 |
| Kafka transactional EOS | REJECT for this wave — no broker; would invent a deployment unit | scope prohibition |
| Debezium "core has no seq column" | informational — validates that ordering-by-key is a legitimate design; our outbox orders deterministically by (occurred_at, event_id) instead | no change |

## 9. Validation matrix (executed 2026-09-27, 本地 Windows workstation)

Same six gates as the durable review (fmt / check / clippy / test /
architecture fitness / git diff --check, all exit 0; 735 passed / 50 ignored
/ 169 suites; `Cargo metadata architecture fitness: PASS`). Logs retained in
`docs/reviews/2026-09-27-wave3-gates/` (`01-fmt.log` … `06-git-diff-check.log`).
PG audit/messaging lanes (`audit-postgres chain_pg`, `messaging
outbox_reliability`, `messaging inbox_idempotency`,
`document-processing-postgres cancel_factuality_pg`) are **NOT RUN / BLOCKED**
locally — cause: a local `postgres.exe` owns :5432 but the only discoverable
credential set fails authentication (non-UTF8, likely GBK, server messages;
no psql/docker available to enumerate); the service and its configuration were
NOT touched (shared-environment confirmation rule). Exact rerun commands
(dedicated DB `plan0014_wave3_test`, credentials injected via environment,
never printed) are recorded in `07-pg-lane-blocked.md` and wired into
`.github/workflows/ci.yml` with `--include-ignored --test-threads=1`.
Evidence classes are kept separate: historical CI (pre-existing lanes) ≠
current local (this session, SQLite/unit only) ≠ current integration (none in
this session).

## 10. Wave 3 exit determination (this doc)

The one proven C1 defect (A-G01: chain writer silent break) is closed
failing-first with tests on the real adapter; tamper-evidence wording was
checked against all six references and stays honest (no overclaim found, the
truncation limitation is explicitly asserted by test); remaining items are
registered C2 with decisions, and the one C1-by-standard item (A-G07/D-G06
envelope) is explicitly routed to a separate design proposal because it
changes public contracts. **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED.**
No R2 claim; no self-filled independent verdict.

## 11. Reviewer questions

1. A-G01 fix choice: the SQLite writer now mirrors the PG writer's chain
   rules. Is the genesis-anchor fallback (first row per tenant links to
   `audit_chain_genesis(tenant)`) acceptable, or should genesis be a random
   per-tenant salt persisted somewhere?
2. A-G07/D-G06 envelope columns (correlation/causation/producer) are the only
   open C1-by-standard item and are blocked behind a public-contract proposal.
   Is that routing acceptable for Wave 3 exit, or should the proposal land
   inside Wave 3 scope before review?
3. A-G08: is key-blacklist redaction acceptable at R1 with the registered C2,
   given every reference in scope (incl. EE Infisical) also lacks structural
   redaction?
4. The legacy/unverified `chain_version=0` boundary (PG 013 backfill + 015
   reset; SQLite NULL→0) leaves old rows permanently unverifiable. Confirm
   acceptance vs a backfill-into-v1 proposal (which would weaken the chain's
   historical credibility).

## 12. Independent reviewer record

<pending — to be filled by the independent reviewer only. Verdict vocabulary:
PASS / PASS WITH C2-C3 / FAIL. R2 admission is not on the table for Wave 3.</pending>
