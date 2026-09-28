# Superseded intermediate run 2 - not candidate evidence

These logs came from the tree immediately before the last two changes: removing
an unused local in a `#[cfg(test)]` test (a `-D warnings` clippy failure) and the
`cargo fmt --all` line-breaking that followed. The changes are whitespace and
test-only, but the logs predate them, so they do not bind the committed tree.

PostgreSQL database used: `plan0014_wave3_r2final`, created empty by the migration
CLI. Results at that moment: five lanes 23 passed / 0 failed / 0 ignored, targeted
crates 25 passed, workspace tests 747 passed / 0 failed / 51 ignored, architecture
fitness PASS, and two gates (fmt, clippy) failing - the clippy failure is what the
removed unused local fixed.

The retained evidence for the candidate is in the parent directory.
