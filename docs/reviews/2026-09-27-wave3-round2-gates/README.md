# Wave 3 Round 2 gates

- Base HEAD: 6b324ea76986640a3f1baef7ecebdb2c53fbd2c6.
- Candidate: uncommitted working tree on the existing Wave 3 branch.
- Implementer/runner: Codex, local Windows/PowerShell, 2026-09-27.
- Code and migration fingerprints: code-snapshot.json (SHA-256 of final bytes).
- These are implementation checks, not a new independent review.
- Raw command output and EXIT_CODE are retained separately from Round 1 logs.
- rustc hard-link/cache warnings are environment diagnostics; clippy exit code
  under -D warnings is the lint gate.
- The pre-existing operational.rs Git modification has no textual change after
  newline normalization; it was not repaired, reset or staged by this task.

## Commit binding and line-ending equivalence (added when the tree was fixed)

This working tree was later committed unchanged as
`81640e79147128925d10efb0f6ff30a0c568ec33`, so logs 01 to 06 are bound to a
commit instead of only to a floating tree. Verification, per file:

- All ten `code-snapshot.json` hashes matched the working tree byte for byte
  before staging, so logs 01 to 06 are evidence for exactly this tree.
- Four of the ten committed blobs are byte-identical to the snapshot. Six are
  stored CRLF-normalized because the repository has `core.autocrlf=true` and no
  `.gitattributes`, and those six working-tree files carry CRLF line endings.
- For every one of the ten files, the committed blob equals the gated
  working-tree text after newline normalization, and the byte delta equals the
  CR count exactly. Examples: `crates/audit-sqlite/src/lib.rs` 17563 to 17149,
  `crates/document-sqlite/tests/concurrency.rs` 6785 to 6588.
- This is a repository-wide line-ending condition predating the task, not a
  content change: no source token differs, so the Rust semantics the gates
  exercised are the semantics of the committed candidate.

`crates/document-sqlite/tests/operational.rs` is excluded from the snapshot and
from the commit. It was already flagged modified before this round, its working
tree bytes are identical to the HEAD blob
(SHA-256 `f863289401ffc98a00efb90e1c9ebd59ce24a4d654b275868ed575eb5c8028b3`,
1995 bytes on both sides), and `git diff` reports no content difference; the
flag is a stat/eol artifact of `core.autocrlf=true`. It was not repaired, reset,
checked out, cleaned or staged, and it is not counted as part of this repair.

## PostgreSQL status for this bundle

Superseded on 2026-09-27 by
[pg-discovery](pg-discovery/README.md) and the
[final candidate bundle](../2026-09-27-wave3-round2-final-candidate-gates/README.md):
an authorized dedicated test database was made available afterwards, so the five
lanes were executed for real. The paragraph below is kept as written; it was
accurate for this bundle, where no authorized connection existed yet.

Real PostgreSQL lanes remain NOT RUN. No authorized dedicated DATABASE_URL was
supplied. Round 2 read-only discovery found no initdb/pg_ctl/psql on PATH and no
usable installation under the standard Program Files/PostgreSQL location;
running postgres processes did not expose executable paths to this session.
No shared service or credentials were modified. Round 1 authentication-failure
logs remain historical, not repeated execution evidence.

| Log | Command |
| --- | --- |
| 01-fmt.log | cargo fmt --all -- --check |
| 02-check.log | cargo check --workspace --all-targets --all-features |
| 03-clippy.log | cargo clippy --workspace --all-targets --all-features -- -D warnings |
| 04-test.log | cargo test --workspace --all-features |
| 05-architecture.log | pwsh -NoProfile -File scripts/check-architecture.ps1 |
| 06-diff-check.log | git diff --check |

Real PostgreSQL lanes remain NOT RUN. No authorized dedicated DATABASE_URL was
supplied. Round 2 read-only discovery found no initdb/pg_ctl/psql on PATH and no
usable installation under the standard Program Files/PostgreSQL location;
running postgres processes did not expose executable paths to this session.
No shared service or credentials were modified. Round 1 authentication-failure
logs remain historical, not repeated execution evidence.

The six added tests cover the two regressions, migration compatibility and
rollback, cancellation immediate states, and transaction/file-reopen recovery.
A file reopen is not a killed worker process. Current PostgreSQL execution,
remaining crash boundaries, external provider idempotency, event-consumer
ordering/schema evolution and R2 release acceptance remain unverified.

## Final result

All six commands exited 0. Workspace tests: **741 passed,
0 failed, 50 ignored**, 169 result blocks.
Architecture metadata, OpenAPI contract and Architecture Fitness all PASS.
Ignored PostgreSQL tests are not counted as executed acceptance.

No push or merge was performed. The working-tree source hashes plus the
line-ending mapping above bind these results to commit
`81640e79147128925d10efb0f6ff30a0c568ec33` without claiming an independent
reviewer verdict. Real PostgreSQL execution of this tree is recorded separately,
because it exposed two further defects and therefore superseded this tree as the
candidate: see
[the final candidate bundle](../2026-09-27-wave3-round2-final-candidate-gates/README.md).
