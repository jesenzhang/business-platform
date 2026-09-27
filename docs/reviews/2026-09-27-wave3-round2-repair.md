# PLAN-0014 Wave 3 Round 2 repair

> Date: 2026-09-27
> Status: READY FOR INDEPENDENT RE-REVIEW / NOT INTEGRATED
> Base: 6b324ea76986640a3f1baef7ecebdb2c53fbd2c6
> Branch: feat/PLAN-0014-wave3-durable-audit-messaging-conformance
> Implementer: Codex (also author of the preceding review; this is not a new independent verdict)

## Confirmed regressions and repair

1. Document-only MIGRATOR followed by create returned `Err(Failed)`. A failing
   test reproduced it. Migration 006 now supplies the shared audit chain columns
   through the Document bootstrap, without requiring Processing tables. Audit
   remains the semantic owner; Document already owned this SQLite table bootstrap.
2. A protected row changed to explicit chain_version=0 with hashes still present
   returned valid=true/legacy_count=1. A failing test reproduced it. The reader
   now normalizes only the historical format with all three new chain metadata
   fields NULL. Explicit version 0 plus hashes reaches domain validation and
   fails. A total rewrite into indistinguishable historical data is outside the
   guarantee without a trusted external checkpoint.
3. D04/A03/A04/A07/A08 evidence was overstated or mismatched. The current matrices
   are corrected; historical logs remain intact. Unknown or unexecuted behavior
   is not labeled PASS.

## Migration compatibility and recovery

`document_sqlite::MIGRATOR` retains the used `run(&SqlitePool)` and `iter()`
entry points, but its concrete type is now `DocumentMigrator`, not the generic
SQLx Migrator. Callers requiring a generic SQLx migrator must use the adapter
entry point. All repository call sites are covered by workspace compilation.
No public HTTP/Event contract or business data owner changes.

The embedded SQLx catalog still owns `_sqlx_migrations` and checks historical
checksums, missing versions and dirty records. Migration 006 conditionally adds
columns under BEGIN IMMEDIATE, then SQLx applies its backfill/index SQL and ledger
record inside the same transaction. This is required because SQLite has no
ADD COLUMN IF NOT EXISTS and Processing 004 may already have installed the columns.
Historical migration files/checksums are unchanged. Use the adapter entry point,
not direct execution of 006 SQL, whose column precondition is explicit.

New tests cover Document-only creation/replay, new composed catalogs, old composed
catalog upgrade preserving v1 hashes, old Document-only legacy backfill, repeated
initialization checksum mismatch rejection, and migration failure rolling back columns plus ledger before a successful retry. Existing contract/concurrency/
revision tests again run without Processing migrations. Only composition tests
retain that dev fixture dependency.

Backfill marks old rows unverified and clears their obsolete hashes only when all
three chain metadata fields are NULL. Existing v1 hashes are preserved. It does
not repair arbitrary corrupt chain metadata. Back up before upgrade. Rollback
retains additive columns; do not drop audit history or rewrite old checksums.

## Evidence scope

The new cancellation file test injects failure before the final unified audit
insert for request, complete_step and reclaim; checks state/version/outbox/audit
rollback, closes/reopens the file database, retries and reopens after commit,
then checks fact counts and idempotent cancellation/reclaim. This is transaction
failure and file-reopen evidence, not process-kill or real PostgreSQL evidence.
Queued and WaitingForReview immediate cancellation now have explicit tests.

Remaining D04 commit boundaries and process-kill cases: NOT RUN in Round 2.
Message out-of-order and schema evolution acceptance: NOT PROVEN by audit tests.
Real PostgreSQL lanes: NOT RUN (no dedicated authorized connection supplied).
External source pins, real provider side effects, release/R2 acceptance: NOT RUN.

Open C1 designs are now concrete proposals:
- [ADR-0027 pipeline pinning](../adr/ADR-0027-processing-pipeline-version-pinning.md).
- [ADR-0028 event envelope](../adr/ADR-0028-processing-event-envelope-evolution.md).

These are Proposed / NOT ACCEPTED, not implemented or closed. No independent
PASS is assigned by this repair. Gate results are recorded separately in
[Round 2 gates](2026-09-27-wave3-round2-gates/README.md).

## Remaining limits

The migration does not make an already contradictory/mixed audit history valid:
existing protected hashes are never rewritten to hide a broken chain. Such
history remains an operator-visible verification failure requiring a separate
reviewed repair, not automatic re-signing.

## Round 2b — real PostgreSQL execution (2026-09-27, after the repair above)

The repair above was solidified as commit `81640e79147128925d10efb0f6ff30a0c568ec33`
after all ten `code-snapshot.json` hashes were confirmed against the working
tree. An operator then authorized creating a dedicated database on the local
PostgreSQL server, which turned the previously BLOCKED lanes into executed ones.
The legacy `TEST_POSTGRESQL_URL` / `TEST_POSTGRESQL_RESTORED_URL` variables were
**not** used as targets: they point at `contract_management_test` and
`contract_management_restore_test`, which `docs/reports/PLAN-0012-COMPLETION-AUDIT.md`
records as another project's contract-test harness. Migrations and cleanup were
never run against them. No shared service or authentication configuration was
changed and no password was guessed.

Environment and results:
[final candidate gates](2026-09-27-wave3-round2-final-candidate-gates/README.md).
Discovery and root-cause analysis:
[pg-discovery](2026-09-27-wave3-round2-gates/pg-discovery/README.md).
Server PostgreSQL 18.4, dedicated database `plan0014_wave3_r2`, project
migration CLI `--backend postgres up` exit 0, migrations 1 to 19. All five lanes
then passed: **22 passed, 0 failed, 0 ignored**.

Executing them for the first time found two defects that no SQLite test could
see, and both are now fixed in commit
`2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a`:

1. **Product, new C1.** The PostgreSQL audit chain could not verify any row it
   had just written. `record_hash` covered `occurred_at` and `recorded_at` at
   nanosecond precision while `audit_events` stores `TIMESTAMPTZ`, i.e.
   microseconds, so the digest recomputed from the stored row never matched.
   `audit::canonical_timestamp` now normalizes every hashed timestamp to
   microsecond precision at construction and in `with_chain_metadata`;
   `rehydrate` does not normalize, so stored history keeps the bytes it was
   signed with and nothing is re-signed. SQLite was unaffected because it
   persists RFC 3339 text, which is precisely why 741 green local tests hid a
   broken tamper-evidence authority on the production backend. Rows written to
   PostgreSQL before this fix stay unverifiable and remain an operator-visible
   failure for separate reviewed repair.
2. **Test fixture only.** `cancel_factuality_pg` bound a `Uuid` to
   `outbox_events.tenant_id`, the legacy `VARCHAR(100)` column from migration
   001; PostgreSQL rejected it with SQLSTATE 42883 while SQLite accepted it
   silently. Production already binds that column as text.

Re-verified on the final candidate: the six local gates all exit 0 with
**742 passed, 0 failed, 50 ignored**, and the five PostgreSQL lanes pass on a
fresh dedicated database. These remain implementation checks; no independent
reviewer verdict is claimed here.

Two honest qualifications. First, the lanes need a database that starts empty:
`claim_next` and `claim_batch` are global queues, so a second run over rows left
behind by an earlier aborted run fails two lanes. That is a fixture isolation
limitation, recorded rather than papered over. Second, the A-G02 and A-G06 rows
in the audit review were written when the PostgreSQL chain lane had never run;
they are corrected below rather than silently reused.
