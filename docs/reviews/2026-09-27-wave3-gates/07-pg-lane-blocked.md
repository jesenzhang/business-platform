# PG lanes — BLOCKED on the local workstation (2026-09-27)

> **SUPERSEDED later the same day — do not treat this as the current PG
> status.** The blocker was cleared by an operator-supplied connection and the
> five lanes were executed for real. Current status, results and the retention
> of the rerun procedure:
> [final candidate gates](../2026-09-27-wave3-round2-final-candidate-gates/README.md)
> (§14 of both review docs). What follows is kept verbatim as the Round 1 record
> of *why* the lanes had not run; it was accurate when written and is not being
> rewritten into a pass. The first real run then FAILED two of the five lanes and
> exposed a product defect, recorded in
> [pg-discovery](../2026-09-27-wave3-round2-gates/pg-discovery/README.md) — so
> this file's "never executed" sentence is the reason that discovery was
> necessary, not a resolved formatting issue.

Environment: 本地 Windows 10 (19045) workstation. A local `postgres.exe`
service listens on 127.0.0.1:5432, but no usable credential set is available
to this session: `postgres:postgres`, empty password and `postgres:password`
all fail authentication. The server answers auth errors in a non-UTF-8
(GBK-locale) encoding, which sqlx surfaces as a protocol error instead of a
plain auth failure. `psql` and `docker` are not installed. Per the task
constraint the service configuration was NOT modified (changing a machine
service requires explicit user confirmation).

Consequence: the PostgreSQL lanes of this Wave (`document-processing-postgres`
`contracts` + `cancel_factuality_pg`, `audit-postgres` `chain_pg`,
`messaging` `outbox_reliability` / `inbox_idempotency`) are recorded as
BLOCKED / NOT RUN locally. They are wired into the CI integration job and
historical CI evidence for the pre-existing lanes stays valid; the two NEW
files have never executed against a real PostgreSQL (see review docs, rows
marked NOT RUN).

Probe evidence (connection attempt only, no database mutation):

```text
$ DATABASE_URL='postgres://postgres:postgres@localhost:5432/plan0014_wave3_test' \
    cargo test -p audit-postgres --test chain_pg --all-features -- --include-ignored --test-threads=1
failed to connect to PostgreSQL: Protocol("Postgres protocol error (reading
ErrorResponse): Postgres returned a non-UTF-8 string for its error message.
This is most likely due to an error that occurred during authentication and
the default lc_messages locale is not binary-compatible with UTF-8. ...")
test result: FAILED. 0 passed; 3 failed; 0 ignored ... (connection failures)
```

Rerun procedure (requires an accessible PostgreSQL; use a dedicated test DB,
never a shared or production database):

```bash
export DATABASE_URL='postgres://<user>:<password>@localhost:5432/plan0014_wave3_test'
cargo run -p migration -- --backend postgres up
cargo test -p audit-postgres --test chain_pg --all-features -- --include-ignored --test-threads=1
cargo test -p document-processing-postgres --test contracts --all-features -- --include-ignored --test-threads=1
cargo test -p document-processing-postgres --test cancel_factuality_pg --all-features -- --include-ignored --test-threads=1
cargo test -p messaging --test outbox_reliability --all-features -- --include-ignored --test-threads=1
cargo test -p messaging --test inbox_idempotency --all-features -- --include-ignored --test-threads=1
```
