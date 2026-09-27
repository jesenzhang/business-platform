# Wave 3 Round 2 final candidate - local gates and real PostgreSQL lanes

- Base: `6b324ea76986640a3f1baef7ecebdb2c53fbd2c6`.
- Round 2 repair candidate: `81640e79147128925d10efb0f6ff30a0c568ec33`
  ([its own gate bundle](../2026-09-27-wave3-round2-gates/README.md)).
- **Candidate measured here: `2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a`**, which
  adds the two defects that real PostgreSQL execution exposed. See
  [`pg-discovery`](../2026-09-27-wave3-round2-gates/pg-discovery/README.md) for
  what the first run found and
  [the repair record](../2026-09-27-wave3-round2-repair.md) for the analysis.
- Runner: implementation agent, local Windows/PowerShell, 2026-09-27.
- These are implementation checks. No independent reviewer verdict is claimed.

## Local gates (logs 01 to 06)

| Log | Command | Exit |
| --- | --- | --- |
| `01-fmt.log` | `cargo fmt --all -- --check` | 0 |
| `02-check.log` | `cargo check --workspace --all-targets --all-features` | 0 |
| `03-clippy.log` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 |
| `04-test.log` | `cargo test --workspace --all-features` | 0 |
| `05-architecture.log` | `pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 |
| `06-diff-check.log` | `git diff --check` | 0 |

Workspace tests: **742 passed, 0 failed, 50 ignored**, 169 result blocks.
Architecture metadata, OpenAPI contract and Architecture Fitness all PASS.
The count is one higher than the Round 2 candidate's 741 because this candidate
adds `hash_inputs_survive_a_microsecond_persistence_round_trip`.
The rustc "hard linking ... incremental compilation cache" warnings are
filesystem diagnostics, not lint or compile failures; the gate is the exit code
under `-D warnings`.

Ignored PostgreSQL tests are **not** counted as executed acceptance. The 50
ignored tests stay unexecuted in log 04; the five lanes below are separate,
explicitly requested runs.

## Real PostgreSQL lanes (logs 07 to 12)

Environment, captured in `00-server-version.log`:

| Item | Value |
| --- | --- |
| Server | PostgreSQL **18.4** on `localhost:5432` |
| `lc_messages` | `Chinese (Simplified)_China.936` |
| Database | `plan0014_wave3_r2`, created for this task by the project migration CLI |
| Catalog | 43 public tables, `_sqlx_migrations` versions 1 to 19 |
| Credentials | supplied by the operator for this task; not printed anywhere |

`07-pg-migration-up.log` is the project migration CLI, not an ad-hoc schema
script: `cargo run -p migration -- --backend postgres up`, exit `0`.

| Log | Lane | Exit | Result |
| --- | --- | --- | --- |
| `08-pg-document-processing-contracts.log` | `document-processing-postgres --test contracts` | 0 | 3 passed, 0 failed, 0 ignored |
| `09-pg-document-processing-cancel-factuality.log` | `document-processing-postgres --test cancel_factuality_pg` | 0 | 5 passed, 0 failed, 0 ignored |
| `10-pg-audit-chain.log` | `audit-postgres --test chain_pg` | 0 | 3 passed, 0 failed, 0 ignored |
| `11-pg-messaging-outbox-reliability.log` | `messaging --test outbox_reliability` | 0 | 7 passed, 0 failed, 0 ignored |
| `12-pg-messaging-inbox-idempotency.log` | `messaging --test inbox_idempotency` | 0 | 4 passed, 0 failed, 0 ignored |

Total across the five lanes: **22 passed, 0 failed, 0 ignored**.

Every lane except `cancel_factuality_pg` applies migrations itself
(`sqlx::migrate!` or `runtime_migration::MIGRATOR`), so no lane depends on
another test having migrated first; `cancel_factuality_pg` documents the
migration CLI prerequisite, which is why log 07 is run explicitly first. Each
lane deletes only its own tenant-scoped rows.

## Required properties of a rerun

The lanes need a **dedicated database that starts empty**. Re-running them on a
database holding rows from an earlier aborted run makes `contracts` and
`outbox_reliability` fail, because `claim_next` and `claim_batch` are global
queues that pick up the leftovers first. This was observed and is recorded in
the discovery README; it is a fixture isolation limitation, not a product
defect. CI is unaffected because it provisions a fresh service container, and CI
wiring is still not a claim that the current candidate passed CI.

To reproduce: create or reuse a dedicated database, point `DATABASE_URL` at it,
run `cargo run -p migration -- --backend postgres up`, then the five commands in
logs 08 to 12.

## Still not covered by this bundle

Process kill between commit boundaries; real provider side effects and external
source pins; message out-of-order and event schema evolution; multi-worker
PostgreSQL rolling upgrade; D-G03 pipeline pinning and D-G06/A-G07 event
envelope (open C1, [ADR-0027](../../adr/ADR-0027-processing-pipeline-version-pinning.md)
and [ADR-0028](../../adr/ADR-0028-processing-event-envelope-evolution.md), both
Proposed / NOT ACCEPTED); R2 release acceptance. Wave 4 and Wave 5 not started.
