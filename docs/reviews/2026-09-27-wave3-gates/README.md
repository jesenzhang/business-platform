# Wave 3 gate execution record

- Environment: 本地 Windows workstation (`win32 10.0.19045 x64`, Git Bash),
  repo `F:\Workspace\business-platform`, branch
  `feat/PLAN-0014-wave3-durable-audit-messaging-conformance` @ `4b4ee71`
  (+ these docs).
- Date executed: 2026-09-27 (all six gates re-run in one pass immediately
  before committing this record; timestamps are the local wall clock at run
  time).
- Runner note: raw `cargo` output is retained deliberately (not the `rtk`
  proxy summary, which collapses detail to one line); each log ends with its
  own `EXIT_CODE=` line.
- Filesystem noise note: `03-clippy.log` contains ~120 rustc lines of the
  form "hard linking files in the incremental compilation cache failed" —
  environment/cache-fs artifacts, not lints; the gate exits 0 under
  `-D warnings`, proving no lint diagnostics remain.

| File | Command | Exit |
| --- | --- | --- |
| `01-fmt.log` | `cargo fmt --all -- --check` | 0 |
| `02-check.log` | `cargo check --workspace --all-targets --all-features` | 0 |
| `03-clippy.log` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 |
| `04-test.log` | `cargo test --workspace --all-features` | 0 |
| `05-architecture.log` | `cargo run -p architecture-check -- check` | 0 (`Cargo metadata architecture fitness: PASS`) |
| `06-git-diff-check.log` | `git diff --check` | 0 |
| `07-pg-lane-blocked.md` | PostgreSQL lanes — NOT RUN, cause + rerun commands | BLOCKED (see file) |

Aggregates from raw logs: **735 passed, 50 ignored, 0 failed**, across 169
`test result:` lines (117 unit/integration binaries + 52 doc-test binaries).
Of the 50 ignored, the 8 new Wave 3 PG tests (`audit-postgres/tests/chain_pg.rs`
×3, `document-processing-postgres/tests/cancel_factuality_pg.rs` ×5) have
never executed against a real PostgreSQL in this session — historical CI,
current local, and current integration evidence are recorded separately in
`07-pg-lane-blocked.md` and in both Wave 3 review docs (§6/§9).
