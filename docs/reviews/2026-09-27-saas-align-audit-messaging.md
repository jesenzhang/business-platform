# PLAN-0014 Wave 3 — Audit Chain & Messaging Reliability Reference Conformance Review

> Wave: PLAN-0014 Wave 3 (Durable Task Execution / Audit / Messaging)
> Round 1 status: Wave 3 implementation reviewed / self-verified / **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**
> Review date: 2026-09-27 (actual date; no back-filling)
> Branch: `feat/PLAN-0014-wave3-durable-audit-messaging-conformance`
> Base: `443a11cabdce1dd34006feaf08aa8be3382150c1` (Wave 2 closeout; Wave 2 NOT INTEGRATED)
> Fix commits: `217e9500471f84e2b597b4315bfda3873f365519` (audit-chain writer fix + chain test suites + CI wiring), `4b4ee71e610546449b33a84b034c18a17da65308` (architecture-check dev-edge exemption; shared with the durable review)
> Maturity level claimed: **R1 (foundation conformance)**. R2 admission is explicitly NOT claimed; verdicts are recorded only by the independent reviewer (PASS / PASS WITH C2-C3 / FAIL).

Companion document: `2026-09-27-saas-align-durable-execution.md` (same wave,
same base/candidate; cross-references D-Gxx).

> Current status: Round 2 final candidate `2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a`;
> real PostgreSQL lanes EXECUTED on PostgreSQL 18.4 (22 passed / 0 failed / 0 ignored),
> which exposed and closed one further C1 (A-G09). Round 2c then found that A-G09's
> fix had regressed recovery of historically signed rows (A-G10) and that the normalization
> helper itself overflowed (A-G11); both reproduced by failing tests, then repaired, and
> the lanes re-EXECUTED on a fresh database (23 passed / 0 failed / 0 ignored).
> NOT INTEGRATED; independent
> re-review pending. See §14.

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
detected (SQLite + unit; the PostgreSQL mirror first executed on 2026-09-27 and
**failed there**, exposing a real defect in this claim on the production
backend — see §13 and
[pg-discovery](2026-09-27-wave3-round2-gates/pg-discovery/README.md); repaired
and now passing on the final candidate).
Not established by these tests: anti-truncation without a trusted external
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
| A01 | audit + business write atomically (DATA_OWNERSHIP; AGENTS §7) | `document-sqlite/tests/contracts.rs::sqlite_rolls_back_the_document_when_atomic_side_effect_fails`; processing side: `document-processing-sqlite/tests/revision1.rs::revision_one_uow_commits_fixed_pipeline_and_review_atomically`; PG side executed: `document-processing-postgres/tests/contracts.rs::postgres_processing_revision_one_uow_is_atomic_and_replayable` | local-SQLite, PostgreSQL 18.4 (dedicated DB) | PASS (SQLite and PostgreSQL); PostgreSQL first executed 2026-09-27, 3 passed / 0 failed |
| A02 | tamper evidence is honest: detect payload change / interior deletion, do NOT claim anti-truncation | `audit/src/lib.rs::hash_chain_detects_payload_change`; `document-sqlite/tests/audit_chain.rs::chain_detects_mutation_and_interior_deletion_but_not_tail_truncation` (mutation + interior deletion detected; tail truncation asserted NOT detected); `audit-postgres/tests/chain_pg.rs::chain_detects_mutation_and_interior_deletion_but_not_tail_truncation` mirror | local-unit, local-SQLite, PostgreSQL 18.4 (dedicated DB) | PASS on the final candidate (SQLite/unit/PostgreSQL, 3 passed / 0 failed). **First real PostgreSQL run FAILED all 3 rows**: `record_hash` was taken over nanosecond timestamps that `TIMESTAMPTZ` truncates to microseconds, so no freshly written PG row verified. New C1, see A-G08 and §13. |
| A03 | duplicate / replay converges to one side effect | Existing PostgreSQL inbox tests are the relevant evidence; audit-chain rereading proves no inbox semantics | PostgreSQL 18.4 (dedicated DB) | PASS on `messaging --test inbox_idempotency` (4 passed / 0 failed, 2026-09-27); audit-chain replay still not accepted as substitute evidence |
| A04 | out-of-order messages cannot regress consumer state | Audit append sequence and business timestamps are unrelated to consumer version handling; no production messaging consumer fixture establishes this requirement | — | NOT PROVEN / gap retained; no new consumer introduced. Real PostgreSQL execution does not change this |
| A05 | publisher crash after claim is recoverable; republish is idempotent | `messaging/tests/outbox_reliability.rs::expired_lease_recovery` (reclaim by new worker), `::concurrent_claim_no_duplicates` / `::concurrent_claim_100_has_unique_ownership` (no double claim), `::stale_worker_cannot_publish_or_fail` (fenced old lease rejected); event_id stable across republish (source: publish path mutates only lease/attempt columns) | PostgreSQL 18.4 (dedicated DB) | PASS on the final candidate: 7 passed / 0 failed. Still no real broker and no process kill; the claim is lease/commit-boundary recovery only |
| A06 | retry exhaustion → explicit terminal state, poison message cannot loop | `outbox_reliability.rs::failure_schedules_retry` (backoff), `::max_attempts_reached` (terminal `failed`, row retained, no re-claim); calibration: Infisical drops after 5 with labeled metric, JetStream drops after MaxDeliver — our terminal-without-delete is the stricter choice (A-G05 KEEP) | PostgreSQL 18.4 (dedicated DB) | PASS on the final candidate (both rows inside the 7 passed / 0 failed) |
| A07 | event schema/version evolution | Legacy audit-row compatibility is tested separately by `audit_chain.rs`; no event consumer schema-evolution fixture executed | local-SQLite audit only | PARTIAL for audit storage compatibility; event evolution NOT PROVEN |
| A08 | envelope context and redaction | `audit` unit tests cover their listed redaction keys. Cancel/fence tests count evidence rows and do not establish payload redaction. Unified Audit already has correlation/causation/trace fields; missing event propagation is scoped by ADR-0028 | local-unit/source | PARTIAL; D-G06/A-G07 OPEN C1; no whole-envelope PASS |

## 7. Gap registry

### Closed in this wave (C1)

**A-G01 — the Document Management SQLite writer did not produce a v1 chain.**
Current behavior (pre-fix, `git show 443a11c`): `document-sqlite`'s
`insert_audit` used a 17-column INSERT that omitted
`recorded_at`/`stream_sequence`/`chain_version` entirely and bound
`previous_hash`/`record_hash` to NULL — every Document Management audit row from this writer
landed as legacy/unverified even after the 004 chain migration; replay
verification of the same rows returned mismatch (the rehearsal's own
`verify_rehearsal_events` compared rebuilt events field-by-field and failed).
Violated contract: SECURITY_ARCHITECTURE audit-integrity (new writes must be
chain-verifiable), DATA_OWNERSHIP (audit rows must be trustworthy evidence).
Repro entry: `cargo test -p document-sqlite --test audit_chain` observed RED
before the fix (first append produced no `record_hash`; continuation failed).
Impact: the chain existed only in schema and in the audit-postgres writer; any
tenant relying on this Document Management audit writer would silently have zero
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
Drift materialized once more on 2026-09-27: the hash-input precision defect
(A-G09) sat in the shared `audit` crate but was only observable on one of the
three backends, which is precisely the cost this entry predicted. Evidence
strengthened, disposition unchanged (C2).
**A-G07 — processing outbox/messaging envelope lacks complete correlation/causation/producer propagation**
(API/event standard §16). Unified AuditEvent and audit_events already have correlation/causation/trace fields; they must not be described as absent. See Proposed ADR-0028 for per-surface facts. Same root as D-G06; closing it
changes published DTOs = public contract change → separate design proposal
with explicit blocker; C1-by-standard, DEFER to proposal (not silently fixed).
**A-G08 — redaction is a key blacklist** (`SENSITIVE_KEYS`), so a new
sensitive field name added elsewhere can leak until registered. Infisical's
EE logger redacts structurally at the DTO boundary. C2 DEFER (field-registry
lint or DTO-level redaction).
**A-G09 — C1, found by the first real PostgreSQL execution of `chain_pg`,
repaired in the final candidate.** `record_hash` hashed `occurred_at` and
`recorded_at` at nanosecond precision, while `audit_events.occurred_at` /
`recorded_at` are `TIMESTAMPTZ` (microseconds). The row was therefore signed
over a value PostgreSQL could not store, so re-verification recomputed a
different digest and `verify_chain` marked a freshly written, intact tenant
chain as broken. This violates the tamper-evidence claim on the *production*
backend while all 741 SQLite/unit tests stayed green, which is the strongest
available argument that `#[ignore]` + CI wiring is not evidence.
Repaired by `audit::canonical_timestamp`: every timestamp entering
`record_hash` is microsecond aligned at construction and in
`with_chain_metadata`. `rehydrate` deliberately does not normalize, so stored
rows keep the bytes they were signed with and no history is re-signed. Pinned by
`audit::hash_inputs_survive_a_microsecond_persistence_round_trip`. Residual
obligation: PostgreSQL rows written before the fix remain unverifiable and
require a separate reviewed repair plus an operator-facing detection query —
not automatic re-signing. Wave 4 follow-up.

### Found and repaired in Round 2c (author-repaired, awaiting independent review)

**A-G10 - Round 2b's precision fix broke recovery of historically signed rows.**
Current behavior (pre-fix, at `0ac031a`): `AuditEvent::rehydrate` validated a row by
delegating to `AuditEvent::new`, which canonicalizes `occurred_at`, and then wrote
back only `recorded_at`. A stored `occurred_at` carrying a sub-microsecond tail came
back truncated, so `hash_record` no longer equalled the stored `record_hash` and
`verify_chain` reported an intact chain as broken. `SQLite` stores timestamps as
RFC 3339 text, so pre-Round 2b rows genuinely carry that tail. The Round 2b record
and `PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE` both asserted that
`rehydrate` does not normalize - the documentation described the required behavior
and the code did not implement it.
Violated contract: SECURITY_ARCHITECTURE audit-integrity (tamper verification must
not produce false positives against intact history).
Repaired in Round 2c; see [Round 2c repair record](2026-09-27-wave3-round2c-repair.md).
Independent review NOT RUN.

**A-G11 - the timestamp conversion overflowed instead of normalizing.**
`canonical_timestamp` rebuilt the instant as
`Utc.timestamp_nanos(value.timestamp_micros() * 1_000)`. The epoch microsecond count
times 1000 exceeds `i64` nanoseconds after 2262-04-11, so the conversion panicked in
debug and wrapped in release - feeding a wrong instant into a signature. Boundary
tests now pin 1900, 1969, 2262 and 2500. Repaired in Round 2c; independent review
NOT RUN.
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

> **Superseded on 2026-09-27 by §14.** The PG paragraph above was accurate at
> the time it was written and is kept verbatim; the lanes were subsequently
> executed against a real PostgreSQL 18.4 on an operator-authorized dedicated
> database, and that run found a further C1 in this document's scope.

## 14. Round 2 final candidate — real PostgreSQL execution (2026-09-27)

Candidates, in order:

| Stage | SHA | Note |
| --- | --- | --- |
| Round 1 reviewed HEAD | `6b324ea76986640a3f1baef7ecebdb2c53fbd2c6` | independent verdict **FAIL / NEEDS REPAIR** |
| Round 2 repair candidate | `81640e79147128925d10efb0f6ff30a0c568ec33` | the two Round 1 regressions; [gates](2026-09-27-wave3-round2-gates/README.md) |
| **Final candidate** | `2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a` | adds the PostgreSQL-found repairs; [gates](2026-09-27-wave3-round2-final-candidate-gates/README.md) |

Authorization actually used: the operator authorized creating a dedicated
database on the local server. The legacy `TEST_POSTGRESQL_URL` /
`TEST_POSTGRESQL_RESTORED_URL` variables were **not** used as targets —
`contract_management_test` and `contract_management_restore_test` belong to
another project's contract-test harness per
`docs/reports/PLAN-0012-COMPLETION-AUDIT.md`. No shared service or
authentication configuration was modified and no password was guessed.

Environment and outcome: PostgreSQL **18.4**, dedicated database
`plan0014_wave3_r2`, project migration CLI
`cargo run -p migration -- --backend postgres up` exit 0, migrations 1 to 19,
43 public tables. Lanes with `--include-ignored --test-threads=1`:

| Lane | Result |
| --- | --- |
| `document-processing-postgres --test contracts` | 3 passed, 0 failed, 0 ignored |
| `document-processing-postgres --test cancel_factuality_pg` | 5 passed, 0 failed, 0 ignored |
| `audit-postgres --test chain_pg` | 3 passed, 0 failed, 0 ignored |
| `messaging --test outbox_reliability` | 7 passed, 0 failed, 0 ignored |
| `messaging --test inbox_idempotency` | 4 passed, 0 failed, 0 ignored |
| total | **22 passed, 0 failed, 0 ignored** |

What this changes in this document: A01, A02, A03, A05 and A06 move from
`NOT RUN`/`PG-ignored` to executed-Pass on the final candidate, and A02's
first real run **failed**, which is why A-G09 was opened rather than the row
simply relabeled. A04 is unchanged: a real database does not create an event
consumer, so out-of-order handling stays NOT PROVEN. The 50 ignored tests in
the workspace run are still not counted as executed acceptance, and CI wiring is
still not a claim that this candidate passed CI.

Local gates were re-run on the final candidate because product code changed: all
six exit 0, **742 passed / 0 failed / 50 ignored**, Architecture Fitness PASS.
Raw logs and the code snapshot are in
[2026-09-27-wave3-round2-final-candidate-gates](2026-09-27-wave3-round2-final-candidate-gates/README.md);
the failing first run is retained under
[pg-discovery](2026-09-27-wave3-round2-gates/pg-discovery/README.md).

Determination for this document: **READY FOR INDEPENDENT RE-REVIEW / NOT
INTEGRATED**, PostgreSQL executed. Independent re-review not yet performed; no
self-filled verdict. Remaining open C1: A-G07/D-G06 (ADR-0028, Proposed) and the
pre-fix unverifiable PostgreSQL rows noted under A-G09. R1 claim stands only for
a re-reviewed candidate; **R2 BLOCKED**. Wave 4/5 not started.

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
5. A-G09 leaves a population of PostgreSQL rows written before the fix that
   cannot verify. The repair deliberately does not re-sign them. Is a Wave 4
   operator-facing detection query sufficient for R1, or must that population be
   enumerated and dispositioned before R1 can be claimed?
6. The PG lanes require a database that starts empty because `claim_next` and
   `claim_batch` are global queues. Is fixing that fixture isolation a Wave 4
   prerequisite, or acceptable as a documented rerun precondition?

## 12. Independent reviewer record

<pending — to be filled by the independent reviewer only. Verdict vocabulary:
PASS / PASS WITH C2-C3 / FAIL. R2 admission is not on the table for Wave 3.</pending>

## 13. Round 2 independent finding and repair

Round 1 reviewed HEAD `6b324ea76986640a3f1baef7ecebdb2c53fbd2c6`: **FAIL / NEEDS REPAIR**. Document-only migration/create regressed and explicit version-0 rows with protected hashes were silently downgraded to valid legacy. Both were reproduced before fixing. [Round 2 repair](2026-09-27-wave3-round2-repair.md) and [ADR-0028](../adr/ADR-0028-processing-event-envelope-evolution.md) describe changes and unresolved scope. Current matrix rows override historical overclaims; this repair is not an independent PASS.

## 15. Round 2c - audit timestamp precision regression (2026-09-28)

Round 2b closed the unverifiable-new-row defect by normalizing hashed timestamps
before signing, and stated that `rehydrate` does not normalize. Round 2c found the
code did not honour that statement (A-G10) and that the normalization helper itself
overflowed (A-G11). Both were reproduced by failing tests before being fixed, and
both are recorded in the [Round 2c repair record](2026-09-27-wave3-round2c-repair.md).

**Round 2b's 22 passing PostgreSQL lanes did not cover this.** PostgreSQL stores
`TIMESTAMPTZ` at microsecond precision - confirmed in this round's probe as
`datetime_precision = 6` for both `occurred_at` and `recorded_at` - so on PostgreSQL
there is no stored nanosecond tail for a reader to corrupt. Only the `SQLite`
adapter, which stores RFC 3339 text, holds history that the Round 2b reader mangled,
and no test read such a row back. The new coverage is deliberately at that layer:
`crates/audit-sqlite/tests/historical_chain.rs` inserts a nanosecond historical row,
reads it through the real `SqliteAuditStore`, and expects a digest frozen from the
historical algorithm rather than recomputed by the code under test.

Round 2c validation (author's own checks, not an independent verdict): targeted
crates 25 passed / 0 failed; five PostgreSQL lanes on the fresh dedicated database
`plan0014_wave3_r2final` 23 passed / 0 failed / 0 ignored, including a new real-
PostgreSQL precision round-trip test; six repository gates in
`docs/reviews/2026-09-27-wave3-round2c-gates/`. An intermediate lane run superseded
by a later code change is retained under `superseded-intermediate-run/` with its
reason, not deleted.

State: **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**. Not integrated, not
pushed, no merge, Wave 4 and Wave 5 not started. ADR-0027 and ADR-0028 remain
Proposed / NOT ACCEPTED.
