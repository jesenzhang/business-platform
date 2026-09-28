# Round 2c gates - audit timestamp precision repair

Code candidate: `8946e46b03ac5516e7bf6e09f4f58382aafd4bd3`
(5 files: `crates/audit/src/lib.rs`, `crates/audit-sqlite/Cargo.toml`,
`crates/audit-sqlite/tests/historical_chain.rs`,
`crates/audit-postgres/tests/chain_pg.rs`, `Cargo.lock`).

Branch `feat/PLAN-0014-wave3-durable-audit-messaging-conformance`, base
`0ac031af1344af4a8fa4d91c314cf3563065e1bf`. Every command below ran on the tree
that became that commit; `cargo fmt --all` had already been applied, and the only
later difference is the commit object itself.

These are implementation checks by the repair author. They are **not** an
independent review verdict and **not** release acceptance.

## Failing-first reproduction

| Log | Command | Exit | Result |
| --- | --- | --- | --- |
| `00-repro-pre-fix-failures.log` | `cargo test -p audit --lib` with both fixes temporarily reverted | 101 | 5 passed, 2 failed. `attempt to multiply with overflow` at `canonical_timestamp`; `rehydrate must restore the stored occurred_at verbatim`. |
| `01-repro-repository-pre-fix.log` | `cargo test -p audit-sqlite --all-features` with only the R1 fix reverted | 101 | 0 passed, 2 failed. Both SQLite repository tests fail through the real `SqliteAuditStore`. |

Reverts were temporary, scripted, and reverted back; the fixed tree re-runs the
same commands green (log `02`).

## Targeted crates

| Log | Command | Exit | Passed | Failed | Ignored |
| --- | --- | --- | --- | --- | --- |
| `02-targeted-crates.log` | `cargo test -p audit -p audit-sqlite -p document-sqlite --all-features` | 0 | 25 | 0 | 0 |

## PostgreSQL - real execution, task-specific database

Database `plan0014_wave3_final`, created empty for this task by the migration CLI.
No shared database was reused or cleared. Log `16` records the cleanup afterwards:
it drops only databases matching `plan0014_wave3_%`, which were the five
task-specific databases this plan's Wave 3 work had accumulated
(`plan0014_wave3_test`, `plan0014_wave3_r2`, `plan0014_wave3_r2c`,
`plan0014_wave3_r2final`, `plan0014_wave3_final`). Afterwards 0 match the pattern
and 11 databases remain in the cluster, so nothing shared was touched. Credentials came
from the authorized environment, were never written to the repository, and are redacted
in every log here.

| Log | Command | Exit | Passed | Failed | Ignored |
| --- | --- | --- | --- | --- | --- |
| `03-server-environment.log` | temporary probe (deleted after the run) | 0 | 1 | 0 | 0 |
| `04-pg-migration-up.log` | `cargo run -p migration -- --backend postgres up` | 0 | - | - | - |
| `05-pg-document-processing-contracts.log` | `cargo test -p document-processing-postgres --test contracts --all-features -- --include-ignored --test-threads=1` | 0 | 3 | 0 | 0 |
| `06-pg-document-processing-cancel-factuality.log` | `cargo test -p document-processing-postgres --test cancel_factuality_pg --all-features -- --include-ignored --test-threads=1` | 0 | 5 | 0 | 0 |
| `07-pg-audit-chain.log` | `cargo test -p audit-postgres --test chain_pg --all-features -- --include-ignored --test-threads=1` | 0 | 4 | 0 | 0 |
| `08-pg-messaging-outbox-reliability.log` | `cargo test -p messaging --test outbox_reliability --all-features -- --include-ignored --test-threads=1` | 0 | 7 | 0 | 0 |
| `09-pg-messaging-inbox-idempotency.log` | `cargo test -p messaging --test inbox_idempotency --all-features -- --include-ignored --test-threads=1` | 0 | 4 | 0 | 0 |
| `16-pg-task-database-cleanup.log` | temporary cleanup probe (deleted after the run) | 0 | 1 | 0 | 0 |

Lanes total **23 passed / 0 failed / 0 ignored**, run serially.

Facts the probe recorded: PostgreSQL 18.4; `audit_events.occurred_at` and
`audit_events.recorded_at` are `timestamp with time zone` with
**`datetime_precision = 6`** - the authoritative statement of the microsecond
contract this round is about; `outbox_events.tenant_id` is `character varying`;
43 public tables; migration ledger 1-19; **0 audit rows before the lanes**, which
is what makes the database initial-and-empty rather than reused.

Log `07` is the new-write evidence: an event requested with
`occurred_at = ...123456789Z` is signed at `...123456000`, PostgreSQL stores it
without drift, and the row read back through `PostgresAuditStore` reproduces the
digest written at append time.

## Repository gates

| Log | Command | Exit |
| --- | --- | --- |
| `10-fmt.log` | `cargo fmt --all -- --check` | 0 |
| `11-check.log` | `cargo check --workspace --all-targets --all-features` | 0 |
| `12-clippy.log` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 |
| `13-test.log` | `cargo test --workspace --all-features` | 0 (747 passed / 0 failed / 51 ignored) |
| `14-architecture.log` | `pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 (`Architecture fitness: PASS`) |
| `15-diff-check.log` | `git diff --check` | 0 |

All six executed; none BLOCKED, none skipped.

## Superseded runs kept, not deleted

- `superseded-intermediate-run/` - the first lane run, overtaken by the
  `with_chain_metadata` hardening. Includes the one test failure that turned out
  to be a fixture error of mine (comparing a stored `recorded_at` against the
  pre-append event value, when `append_postgres` assigns `recorded_at` from the
  adapter clock while signing).
- `superseded-intermediate-run-2/` - a full run overtaken by removing an unused
  test local (a `-D warnings` clippy failure) and the `cargo fmt --all` that
  followed. Each directory has its own README stating what changed afterwards.

## Out of scope for this directory

Wave 4 and Wave 5 were not started. Nothing was pushed or merged, `main` was not
touched, and ADR-0027 / ADR-0028 remain Proposed / NOT ACCEPTED. Pre-existing
unverifiable PostgreSQL rows written before Round 2b are an operator concern needing
separate reviewed repair; this round neither enumerates nor re-signs them.
