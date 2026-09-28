# PLAN-0014 Wave 3 Round 2c - audit timestamp normalization repair record

- Date: 2026-09-27/28 (local Windows workstation).
- Base: `0ac031af1344af4a8fa4d91c314cf3563065e1bf` (Round 2b closeout HEAD).
- Code candidate: `8946e46b03ac5516e7bf6e09f4f58382aafd4bd3` (5 files:
  `crates/audit/src/lib.rs`, `crates/audit-sqlite/Cargo.toml`,
  `crates/audit-sqlite/tests/historical_chain.rs`,
  `crates/audit-postgres/tests/chain_pg.rs`, `Cargo.lock`). This document and the
  status/navigation updates are a separate closeout commit on top of it, so the
  candidate stays reviewable without evidence churn.
- Commit chain: base `0ac031af1344af4a8fa4d91c314cf3563065e1bf` -> code candidate
  `8946e46b03ac5516e7bf6e09f4f58382aafd4bd3` -> documentation closeout
  `016b9ed070bfc64787ac4c7f58248db0cb7f698a` -> the pointer commit that adds this
  line. A commit cannot contain its own SHA, so the branch tip is stated in the
  delivery report and readable from `git log --oneline`.
- Scope: audit timestamp canonicalization and historical recovery only.
- Status: **READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**.
- These are implementation checks by the repair author. No independent reviewer
  verdict is claimed or implied, and nothing here is release acceptance.

## Why this round exists

Round 2b executed the audit chain for real on PostgreSQL and closed the defect
where a freshly written row could not verify itself. Its fix normalized hashed
timestamps at construction time, and the record plus the persistence architecture
doc stated: "`rehydrate` deliberately does not normalize, so stored history keeps
the bytes it was signed with and nothing is re-signed."

**The code did not do what that sentence claimed.** Round 2b therefore shipped a
new regression, and the 22 passing PostgreSQL lanes did not cover it: every
PostgreSQL row is stored at microsecond precision, so on PostgreSQL there is no
nanosecond tail for a reader to corrupt. Only the SQLite backend, which stores
RFC 3339 text, can hold history that the new reader would mangle - and no test
read such a row back.

## Defect R1 - historical recovery changed signed content (product)

`AuditEvent::rehydrate` validated the row by calling `AuditEvent::new`, which
canonicalizes `occurred_at`, and then restored only `recorded_at`. So
`occurred_at` came back microsecond-truncated while `record_hash` had been
computed over the original nanosecond value.

Consequence: any SQLite audit row whose `occurred_at` carried a sub-microsecond
tail - which is every row the pre-Round 2b writer produced from `Utc::now()` -
stopped verifying after Round 2b. `verify_chain` recomputes `record_hash` from the
rehydrated event, so it reported an intact historical chain as broken. Round 2b
traded "new rows do not verify on PostgreSQL" for "old rows do not verify on
SQLite"; neither is acceptable.

Fix in `crates/audit/src/lib.rs`: `rehydrate` restores the stored `occurred_at`
verbatim alongside `recorded_at`, after `new` has run all domain validation.
Validation is unchanged, so the existing legality rules, including the rejection
of contradictory `chain_version = 0` metadata, still apply.

## Defect R2 - timestamp conversion overflowed (product)

`canonical_timestamp` rebuilt the instant as
`Utc.timestamp_nanos(value.timestamp_micros() * 1_000)`. The epoch microsecond
count times 1000 exceeds `i64` past 2262-04-11, so instead of truncating, the
conversion overflowed - a debug-build panic, and in release a silently wrapped
value fed into a signature.

Fix: truncate the sub-second component in place with
`value.with_nanosecond(value.timestamp_subsec_nanos() / 1_000 * 1_000)`. No
epoch-wide arithmetic, so the calendar date and whole second are preserved for
every representable `DateTime`, pre-epoch included.

## Hardening found while testing the fix

`with_chain_metadata` normalizes `recorded_at` but not `occurred_at`. It is the
transition that signs a row about to be written, so an event that reached an
adapter through `rehydrate` with historical nanosecond precision and was then
re-signed would have been written with a digest PostgreSQL cannot reproduce -
the original Round 2b defect, reached by a different route. It now normalizes
both hashed timestamps. Pinned by
`re_signing_a_historical_row_leaves_no_nanosecond_hashed_input`.

No hash algorithm, canonical payload, `chain_version` meaning, genesis rule,
ordering or historical signature was changed. Nothing was re-signed and no
migration touches stored timestamps.

## Failing-first evidence

`docs/reviews/2026-09-27-wave3-round2c-gates/`:

| Log | What it shows |
| --- | --- |
| `00-repro-pre-fix-failures.log` | Both fixes temporarily reverted. `canonical_timestamp_truncates_without_leaving_the_representable_range` panics with `attempt to multiply with overflow`; `rehydrate_preserves_historical_nanosecond_signed_content` fails `rehydrate must restore the stored occurred_at verbatim`. 5 passed, 2 failed, exit 101. |
| `01-repro-repository-pre-fix.log` | R1 fix only reverted. Both repository-level SQLite tests fail through the real `SqliteAuditStore`, exit 101. |
| `02-targeted-crates.log` | `cargo test -p audit -p audit-sqlite -p document-sqlite --all-features` on the final tree. |
| `03`-`09` | Server environment, migration CLI, and the five PostgreSQL lanes on the final tree. |
| `superseded-intermediate-run/` | An earlier lane run overtaken by the `with_chain_metadata` hardening, retained with its own explanation. |

Expected values are not produced by the code under test. `HISTORICAL_RECORD_HASH`
`12b8823dbe1ea4ebaaf7896c676a52b790dbc004668166b203988e42f7cdf815` is the SHA-256
of the fixture's canonical payload at full nanosecond precision, frozen from the
historical algorithm and independently re-hashed outside this crate. A test that
derived the expected digest from `new`, `rehydrate` or `canonical_timestamp` would
have passed while history stayed corrupt.

## Tests added

| Test | Layer | Pins |
| --- | --- | --- |
| `audit::rehydrate_preserves_historical_nanosecond_signed_content` | unit, real `rehydrate` | both timestamps verbatim; stored digest reproduces; two one-nanosecond tamper negatives prove verification is live |
| `audit::canonical_timestamp_truncates_without_leaving_the_representable_range` | unit | 1900, 1969, 2262, 2500 boundaries: no panic, date and whole second unchanged, result microsecond aligned, truncation never rounds up |
| `audit::re_signing_a_historical_row_leaves_no_nanosecond_hashed_input` | unit | a rehydrated row re-signed for storage carries no sub-microsecond tail and does not overwrite the historical digest |
| `audit-sqlite/tests/historical_chain.rs::repository_reverifies_a_historical_nanosecond_row` | real repository + real migration catalog | nanosecond row inserted as history, read via `SqliteAuditStore::get`, digest reproduces, `verify_chain` valid, then a fresh microsecond append chains onto it and both verify |
| `audit-sqlite/tests/historical_chain.rs::tampering_with_the_historical_row_breaks_verification` | real repository | editing the payload alone still breaks the chain, so the passing test above is not a skipped check |
| `audit-postgres/tests/chain_pg.rs::nanosecond_input_verifies_after_a_postgres_precision_round_trip` | real PostgreSQL 18.4 | an `occurred_at` requested with 123456789 ns is signed at 123456000, stored by `TIMESTAMPTZ` unchanged, and the repository read reproduces the stored digest |

The SQLite tests use a dev-dependency on `document-sqlite` only to run the real
migration catalog that owns the shared `audit_events` schema, instead of
hand-copying a `CREATE TABLE` that can drift. It is a test-only edge; the
reverse dev-dependency already exists, so no production dependency cycle is
introduced.

## Honest limits

- The fixture's historical digest is frozen for one shape of historical row. Any
  row written before Round 2b on SQLite is the same class, but the test pins one
  representative, not the population.
- PostgreSQL rows written before the Round 2b fix remain unverifiable and stay an
  operator-visible failure needing separate reviewed repair. This round does not
  re-sign them and does not enumerate them.
- A file close-and-reopen in the SQLite tests is not a process kill; no test here
  exercises a crash between commit boundaries.
- The gate README's first version, committed in `016b9ed`, stated the task database
  was "dropped afterwards" before any drop had run - a claim written as fact without
  the action behind it. The drop has since actually been executed and logged as
  `16-pg-task-database-cleanup.log`, and the README now states what was really
  dropped. Recorded here because overstating cleanup is the same class of error this
  round exists to fix.
- These are the author's checks. No independent reviewer verdict is recorded;
  both Wave 3 review documents keep their independent sections pending.
