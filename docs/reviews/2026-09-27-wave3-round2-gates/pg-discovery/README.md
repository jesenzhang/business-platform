# Wave 3 PostgreSQL lanes - first real execution (defect discovery)

This folder is **defect-discovery evidence**, not acceptance evidence for the
final candidate. Acceptance logs are in
[../../2026-09-27-wave3-round2-final-candidate-gates/README.md](../../2026-09-27-wave3-round2-final-candidate-gates/README.md).

## What ran

- Date: 2026-09-27. Workstation: local Windows, PowerShell.
- Candidate under test: `81640e79147128925d10efb0f6ff30a0c568ec33` (the Round 2
  repair candidate; its committed blobs equal the working tree recorded in
  [`../code-snapshot.json`](../code-snapshot.json) modulo CRLF to LF).
- Server: PostgreSQL on `localhost:5432`.
- Database: `plan0014_wave3_test`, created for this task by the project
  migration CLI. It is not the shared `business_platform` database and not the
  legacy `contract_management_*` harness databases.
- Step 1: `cargo run -p migration -- --backend postgres up` exited `0`
  (`06-pg-migration-up.log`); it created the database and applied migrations
  001 to 019.
- Step 2: the five Wave 3 lanes, each with `--include-ignored --test-threads=1`.
- No `DATABASE_URL`, password or credential appears in any retained log.

## Result

| Log | Lane | Result |
| --- | --- | --- |
| `07-pg-document-processing-contracts.log` | `document-processing-postgres --test contracts` | ok - 3 passed, 0 failed, 0 ignored |
| `08-pg-document-processing-cancel-factuality.log` | `document-processing-postgres --test cancel_factuality_pg` | **FAILED** - 0 passed, 5 failed |
| `09-pg-audit-chain.log` | `audit-postgres --test chain_pg` | **FAILED** - 0 passed, 3 failed |
| `10-pg-messaging-outbox-reliability.log` | `messaging --test outbox_reliability` | ok - 7 passed, 0 failed, 0 ignored |
| `11-pg-messaging-inbox-idempotency.log` | `messaging --test inbox_idempotency` | ok - 4 passed, 0 failed, 0 ignored |

These five lanes had never executed against a real PostgreSQL before. Their
`#[ignore]` status and their CI wiring were not evidence, and treating them as
such is exactly what had kept both defects below invisible.

## Defect 1 - product: the PostgreSQL audit chain could not verify any row

`chain_pg` failed on the first row of every tenant
(`checked: 0, verified_count: 0, valid: false`, `first_broken_id` = first row).

A temporary probe isolated it. `record_hash` is computed by `hash_record` over
`canonical_payload`, which includes `occurred_at` and `recorded_at`
(`crates/audit/src/lib.rs`). Those carried nanosecond-precision
`chrono::Utc::now()` values, while `audit_events.occurred_at` and
`recorded_at` are `TIMESTAMPTZ`, i.e. microsecond precision
(`migrations/013_runtime_governance_revision1.sql`). The digest was taken over a
value the database could not store, so recomputing it from the stored row
produced a different digest.

Observed on one freshly appended row:

```text
MEM  recorded_at = "2026-09-27T15:14:06.470962900Z"
DB   recorded_at = "2026-09-27T15:14:06.532409Z"
occurred_roundtrip_equal = false  recorded_roundtrip_equal = false
prev_is_genesis = true
hash_matches_stored = false
```

Linkage was correct; only the record digest differed. The SQLite adapters were
unaffected because they persist timestamps as RFC 3339 text and round-trip
nanoseconds exactly, which is why 741 local tests were green while the tamper
evidence authority on the production backend was broken. This is a new C1
finding inside the Wave 3 scope, not one of the two Round 1 regressions.

Repaired in `crates/audit/src/lib.rs` by normalizing every timestamp that enters
`record_hash` to microsecond precision (`canonical_timestamp`), applied at event
construction and in `with_chain_metadata`. `rehydrate` deliberately does not
normalize, so stored history keeps the bytes it was signed with and nothing is
re-signed. Pinned by
`hash_inputs_survive_a_microsecond_persistence_round_trip`.

## Defect 2 - test fixture: UUID bound to a VARCHAR column

`cancel_factuality_pg` failed inside its own helpers with
`operator does not exist: character varying = uuid` (SQLSTATE 42883).
`outbox_events.tenant_id` is the legacy `VARCHAR(100)` column from
`migrations/001_initial.sql`, and every production writer binds it as text
(`.bind(job.tenant_id().to_string())`). The new test file bound a `Uuid`, which
SQLite accepts silently. Repaired in
`crates/document-processing-postgres/tests/cancel_factuality_pg.rs`. Fixture
defect, not a product defect.

## Isolation limit exposed by re-running on the same database

After the aborted tests above, a second run against the same
`plan0014_wave3_test` made two previously green lanes fail
(`contracts` 2 passed 1 failed, `outbox_reliability` 4 passed 3 failed). Cause:
the aborted `cancel_factuality_pg` tests never reached their cleanup, so their
rows remained in `document_processing_jobs` and `outbox_events`, and both
`claim_next` and `claim_batch` are global queues that pick those leftovers up
first. This is a fixture isolation limitation, not a product regression: the
lanes need a dedicated database that starts empty, which is how CI provisions
it. The acceptance run therefore uses a freshly created database, and this run
is retained as the reason.
