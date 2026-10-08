# Wave 5B-S1 final-review repair closeout — N1–N5 (2026-10-09)

Executor: qwen3.8 flash (zcode), executing
`docs/reviews/2026-10-08-wave5b-s1-final-independent-review.md` ("根据审阅结果进行修复"),
against review candidate `5fa0ff46ed29e7ed0dfdebf01f3542779b3e3a22`.

Final status: **READY FOR REVIEW / NOT INTEGRATED. R2 BLOCKED / NOT CLAIMED.**
Nothing was pushed, merged, deployed or integrated; S2 was not started; no database,
object store or provider was contacted; no ADR was accepted.

## 1. Start, the four dirty increments, and the new fixed candidate

* Start HEAD `840f5a0befcc6aaea815edc244e03835fdd28bf4` (the R1–R7 closeout docs commit),
  branch `codex/plan-0014-wave5a-local-release-pilot`. The review's candidate `5fa0ff4`
  is its ancestor; `6c61272` is the R1–R7 code candidate.
* The working tree carried exactly the four increments the review §6 named as needing
  reconciliation: `tools/module-release-tool/src/catalog.rs`,
  `scripts/test-plan0014-wave5b.ps1`, `scripts/test-plan0014-wave5b-repairs.ps1`,
  `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1`. The last of
  these is the `forms:` terminal-count fix the review §5 had seen only as an uncommitted
  change; it is now committed and re-verified by an actual generator run (section 9).
  The other three held the unfinished N1/N3/N4, N2 and N5 work, which this round
  completed, tested and committed. Disclosure: the session checkpoint did not persist
  the initial SHA-256 of those four working-tree files, so the authoritative identities
  are the blob hashes inside the candidate, listed below.
* **Candidate: `a90cf3277356471511647fbee9b507716c8c81bb`** — 55 files, +1534/−173,
  code, generators, shared fixtures and the review report itself. Blob identities:
  `catalog.rs` `00efe9b2c08c82548086819f946221187a355e78`,
  `test-plan0014-wave5b.ps1` `627a8c56ec5ecabc820a93b0fb1ebe85ce94003a`,
  `test-plan0014-wave5b-repairs.ps1` `2cb438cf1cc81c5f0fd6626ce0fb4e8448fde938`,
  `generate-wave5b-fixtures.ps1` `85bf7a35166f219bbd2e930ab82ea132e9627cc9`,
  `generate-wave5b-pinned-history.ps1` `6aa51cca39782df1ff39f3525680463fa6485004`,
  `EXPECTATIONS.json` `312aba5518dbb6cb564fadcdff6341ee57b77f28`,
  `wave5b_catalog.rs` `d9d76ed4faba6f7acf996159b5ea186d155c19d9` (unchanged: this round
  added no Rust integration test file, it extended the shared fixture tree that file
  already consumes).
* This closeout, and the two corrections in section 10, land in the commit that follows the
  candidate on this branch — a documents-only commit touching
  `docs/reviews/2026-10-09-wave5b-s1-final-repair-closeout.md` and
  `docs/reviews/2026-10-08-wave5b-s1-repair-closeout.md`. Every gate in section 9 ran at the
  code candidate itself with a clean tracked tree, so the layering is the same one the
  previous round used: code candidate first, closeout after it.
* `docs/OPERATIONAL_RUNBOOK.md` and `scripts/clean-target.ps1` remain untracked and were
  not read into the candidate. The gate-evidence directories stay untracked by design.
  The two older independent reports were already committed in Git at `6c61272`, and the
  independent review report this round answers is committed in the candidate above;
  "byte-identical" is not the same claim as "uncommitted", and the earlier closeout now
  says so (section 10).

## 2. N1 [P1] — a table list is now parsed to its end, or refused by name

Defect: `FROM`/`JOIN`/`DELETE … USING` item parsing stopped at an alias column list, at a
`JOIN … ON` condition, at a `USING` alias column list and at the inheritance `*`, so the
statement body scan walked past the terminator and a comma-separated foreign table behind
one of them was never owner-checked — four review inputs exited 0 with `status=VALIDATED`.

Repair, all in `tools/module-release-tool/src/catalog.rs`:

* `table_list_events` (`catalog.rs:2146`) is now the single reader of a comma-separated
  table list with its joins. It emits a reference event for every table name it reads and
  only returns at the end of the statement, at a closing parenthesis, or at a clause
  keyword from `TABLE_LIST_END_WORDS` (`catalog.rs:1923`). A token that is none of
  "another comma", "a join operator", "a clause keyword that ends the list" or "end of
  statement" is a named refusal that quotes the token it could not attribute
  ("the table list continues with '<tok>' … never read halfway").
* `table_item_end` (`catalog.rs:2013`) reads the whole `[AS] alias [( column , … )]` slot
  so the comma behind it still belongs to the list; `alias_column_list_end`
  (`catalog.rs:1976`) accepts only plain names and commas and refuses anything else, and a
  `(` behind an unaliased table name is refused as a table function / option list
  (`catalog.rs:2035`) rather than left for the body scan to walk past.
* `join_operator_end` (`catalog.rs:2054`) consumes `[NATURAL] [INNER|LEFT|RIGHT|FULL
  [OUTER]|CROSS] JOIN`; `join_condition_end` (`catalog.rs:2079`) reads the `ON` expression
  to the top-level comma (the list continues), to the next join, or to a clause keyword
  that follows a finished operand — and refuses a sub-query inside the condition by name
  (`SUB_QUERY_WORDS`, `catalog.rs:1948`), because the tables behind it cannot be
  attributed (§3.5).
* `refuse_inheritance_star` (`catalog.rs:1961`) refuses `FROM roles * r` — the star selects
  inheriting tables the owner scan cannot enumerate; the same rule the R-round already
  applied to `INHERITS`.
* The half-parsing helper `skip_alias` is deleted, and `DELETE … USING` passes
  `joined: $false` while `FROM`/`JOIN` pass the join flag, so `ON` is only a join
  condition where a join introduced it.

Proof: the four review inputs plus twelve related shapes are 16 new cases in the shared
fixture tree (`tools/module-release-tool/tests/fixtures/wave5b/catalogs/n1-*`, generated
by `generate-wave5b-fixtures.ps1`: 12 cases that must be refused by name and 4 same-owner
controls that still have to validate). Both readers run them: the real driver reports `catalog-cases cases=88
matched=88 mismatched=0` including every `n1-*` row, and
`wave5b_catalog.rs` (20 tests, unchanged file) reads the same tree. Post-fix probe over
the review's own shapes is in `n1-sql-shapes-post-fix.log`: the four review inputs exit 1
with named refusals, and the same-owner controls still exit 0 `status=VALIDATED`, which is
what separates an owner decision from a blanket parse failure. The inheritance star is
refused even with same-owner tables only — unsupported form, not owner guessing.

## 3. N2 [P1] — cleanup never deletes through a replaced root

Defect: `Get-CanonicalPath` used `Resolve-Path`, which on Windows returns a junction's own
path unchanged, and `Remove-Tree` enumerated recursively; if anything replaced a registered
run root with a junction to another tree, the driver deleted that other tree. The review
measured `sentinel_survives=false, root_survives=false, messages=[]`.

Repair, in `scripts/test-plan0014-wave5b.ps1`:

* `Test-ReparsePoint` reads the actual `ReparsePoint` attribute; `Get-UnsafeLink` walks a
  path from its root up to a boundary and returns the first component that is a reparse
  point (and throws if the chain does not actually reach the boundary);
  `Assert-PathChainIsReal` turns that into a named refusal quoting the link and its
  `.Target`. `Get-CanonicalPath`'s doc comment now says explicitly that it is a path
  normaliser and not a safety check.
* `Register-RunRoot` refuses a root whose chain is not real *at registration time*.
* `Remove-RunScratch` re-checks the chain immediately before deleting: if the root itself
  has been replaced by a link it unlinks the link only (`$root.Delete()`), logs
  `unlinked replaced run root (…); the tree behind the link was left alone`, and never
  touches the target; if an ancestor is a link it refuses by name and leaves everything.
* `Remove-Tree` was rewritten as an explicit stack walk that never descends a reparse
  point, deletes files before directories, and removes directories with
  `[System.IO.Directory]::Delete($dir, $false)` so a directory that turned out not to be
  empty is never silently taken apart.

Proof: `cleanup-root-junction-probe.ps1` (same shape as the review's probe) now reports
`sentinel_survives=True`, `sentinel_bytes=sentinel-bytes`, `nested_survives=True`,
`root_survives=False` (only the link is gone) and one named message. Three repair-driver
scenarios cover the boundary: `ast-cleanup-root-replaced-by-junction`,
`ast-cleanup-parent-replaced-by-junction`, `ast-cleanup-junction-inside-tree`.

Platform honesty: this repository lives on an exFAT volume, which cannot host a reparse
point at all (`New-Item -ItemType Junction` fails with `ERROR_INVALID_FUNCTION`), so the
fixtures build under a reparse-capable volume chosen by `Get-ReparseCapableScratch`, which
*proves* the capability by creating and checking a junction rather than assuming it. If no
writable volume on a host can host a junction, the three scenarios report
`BLOCKED: no writable volume on this host can host a junction …` and the summary counts
them separately — they are never counted as passing. On this host they run for real and
pass (section 9).

## 4. N3 [P2] — raw identifiers, and a `use` item that is read whole or refused

* `word_at` (`catalog.rs:3663`) consumes an `r#` prefix and returns the bare name, so
  `r#embed` is recognised as the identifier `embed` wherever a name is read;
  `is_raw_prefix` (`catalog.rs:3677`) and `keyword_at` (`catalog.rs:3687`) keep the other
  half of the rule: a raw identifier is never a keyword, so `let r#use = 1;` does not open
  a use item and `r#type` is a name, not the `type` keyword.
* `refuse_use_tail` (`catalog.rs:3694`) is the fail-closed half of §3.7: after each shape
  `parse_use_item` can read (glob, `{…}` group, `self`, `as` alias, plain path) any further
  token in the item is a named refusal — "the 'use' tree in '<file>' carries a token after
  the part the embed scan read; a use item is parsed completely or refused, never
  half-read". Fixing this on the real tree immediately caught a case the split had not
  covered: `use self::legacy::{…}` treated `self` as a finished item, so `self` now
  continues as a path root when `::` follows.

Proof: the review's two inputs (`use sqlx::migrate as r#embed;` + `r#embed!(…)`, and
`use sqlx::r#migrate as embed;` + `embed!(…)`) now yield the "2 call sites embed the host
baseline catalog" refusal instead of exit 0 — `n3n4-embed-shapes-post-fix-embed.log`,
cases `raw_alias`, `raw_macro_import`, `raw_direct_call`, `raw_use_ident`, plus
`half_use_tail` for the tail rule and `raw_path_segment` for a same-named binding from a
different path. Unit tests: `raw_identifier_alias_second_embed_is_counted`,
`raw_identifier_in_the_imported_path_second_embed_is_counted`,
`raw_identifier_direct_call_is_counted`, `a_raw_identifier_is_never_a_keyword`,
`a_use_tree_that_is_not_fully_read_is_refused_by_name`, and four more spellings added to
the end-to-end table `every_second_baseline_embed_form_is_refused_not_missed`.

## 5. N4 [P2] — one lexer for the Rust scan

`strip_rust_comments` and the second masking pass are deleted; a single
`lex_rust` (`catalog.rs:3395`) turns a source file into (code, significant-mask) once,
preserving positions, and everything else reads that result. One pass covers: line and
nested block comments, `r"`/`r#"` raw strings, `b"` byte strings, `br#"` raw byte strings,
`c"` C strings, `b'` byte chars, and char literals through `char_literal_end`
(`catalog.rs:3583`), which handles `'\''`, `'\\'`, `'\xNN'` and `'\u{…}'`; anything that
starts with a prefix and is not one of those shapes is masked by `mask_prefixed_literal`
(`catalog.rs:3519`). A `'` that is neither a closed char literal nor a lifetime is a named
refusal ("an apostrophe there is neither a closed char literal nor a lifetime") — an
undecidable spelling stops the check instead of silently hiding an embed point.

Proof: `const QUOTE: char = '"';` before a real `sqlx::migrate!` no longer hides it
(`a_double_quote_char_literal_does_not_hide_a_second_embed`, probe case
`double_quote_char`); escapes, byte chars, byte strings, raw strings and lifetime chains
are covered by `char_escapes_byte_chars_and_lifetimes_do_not_hide_a_second_embed`,
`prefixed_literal_spellings_do_not_count_as_embeds`,
`an_apostrophe_in_a_comment_does_not_disturb_the_scan` and
`an_apostrophe_that_is_neither_char_nor_lifetime_is_refused`; probe cases
`byte_and_raw`, `unterminated_quote`, `byte_char_raw_string` still count exactly one embed
point, and `unterminated_char` / `stray_apostrophe` refuse by name.
`rust-lexical-control.rs` compiles with `rustc --edition 2021 --crate-type lib` (exit 0),
so every spelling the fixtures use is legal Rust and the scanner is not being tested
against invented syntax.

## 6. N5 [P2] — the repair driver proves its own denominator

`scripts/test-plan0014-wave5b-repairs.ps1` now reads its own scenario list from its own
syntax tree (`[Parser]::ParseFile` over `$PSCommandPath`, collecting every
`Invoke-RepairScenario '<literal>'` call) *before* executing anything:

* an `-Only` value naming no declared scenario throws before the first scenario runs, with
  the unknown name and the count of declared scenarios;
* a sweep that executes nothing, and any declared scenario the run never reached, are both
  refusals rather than a green line;
* the summary reports the denominators: `repair-regressions: executed=N skipped=N
  failed=N blocked=N declared=N; logs in …`.

Proof: `filter-unknown-only-refuses-before-executing` asserts the unknown filter exits
non-zero, names the filter and leaves no scenario log;
`filter-counters-reported-for-a-valid-filter` runs a real child with a valid filter and
asserts the reported counters. The old behaviour (unknown filter → every scenario SKIP →
exit 0) can no longer be reached.

## 7. The rules are structural, not per-string

No finding was closed by matching a literal statement, file or message. The SQL side reads
token classes (`JOIN_START_WORDS`, `TABLE_LIST_END_WORDS`, `CONDITION_OPERATOR_WORDS`,
`SUB_QUERY_WORDS`) and either consumes the list or refuses the token it cannot attribute;
the Rust side has one lexer whose literals and comments are lexed, not searched; the
cleanup side checks the reparse attribute of path components and the ownership of the chain
against the registered boundary. The new fixtures exercise the *classes* (alias column
lists, four join spellings, `DELETE … USING`, the inheritance star, table functions,
sub-queries; raw identifiers in three positions, char/byte/raw/C literals, lifetimes;
root, parent and interior reparse replacements) rather than the four reported strings.
`check-architecture.ps1` still does not cover these rules, so the regressions live in the
two readers and the repair driver.

## 8. Shared fixture tree and both readers

`generate-wave5b-fixtures.ps1` regenerates the whole tree deterministically; 16 `n1-*`
catalog cases were added, so the tree is now 87 case directories — one per row of
`EXPECTATIONS.json`, which holds 87 cases, 75 expected fail and 12 expected pass — over 257
fixture files with LF-canonical `MANIFEST.sha256` digests. The generator was re-run after
the candidate commit and the tracked tree stayed byte-identical (`git status --short` empty,
`git diff --check` clean), so what both readers consume is exactly what the generator
produces. Those two readers are `scripts/test-plan0014-wave5b.ps1` and the Rust reader
`tools/module-release-tool/tests/wave5b_catalog.rs`; they agree case by case (driver:
`cases=88 matched=88`, the 88th being the driver's own `F3-module-sql-catalog`; Rust: 20/20
including the fixture-driven reconciliation tests).

## 9. Gates run at the candidate

Every command below ran against candidate `a90cf3277356471511647fbee9b507716c8c81bb` with a
clean tracked tree (gate 53 records only the untracked evidence directories); raw per-gate
logs are under `docs/reviews/2026-10-09-wave5b-s1-final-repair-gates/`.

| gate | what it runs | expected / actual |
| --- | --- | --- |
| fast 20–24 | `catalog-check` on the repo, and once per `--only` check (catalog, history, embed, freeze) | 0 / 0 |
| fast 25 | the negative: an unknown `--only` check name | 1 / 1 |
| fast 30 | `cargo fmt --all -- --check` | 0 / 0 |
| fast 50, 51 | `git diff --check`, `git status --short` | 0 / 0 |
| heavy 31, 32 | `cargo check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 / 0 |
| heavy 33, 34 | `cargo test -p runtime-migration --all-features`, `cargo test -p module-release-tool --all-features` | 0 / 0 |
| heavy 35, 36 | `--no-run` compile of the `outbox_reliability` and `chain_pg` ignored suites | 0 / 0 |
| post 37 | `cargo test --workspace --all-features` — 192 suites, **911 passed / 0 failed / 70 ignored** (901 before, +10 = the new lexer and `use`-tree tests) | 0 / 0 |
| post 40 | `scripts/check-architecture.ps1` — `Architecture fitness: PASS` | 0 / 0 |
| post 41 | `scripts/test-plan0014-wave5b.ps1` — **113 cases**: 88 catalog + 11 history + 7 embed + 5 freeze + 2 path, `mismatched=0`, `blocked=1` | 0 / 0 |
| post 42 | `cargo test -p module-release-tool --all-features` at the candidate — 12 suites, **108 passed / 0 failed** (31 lib tests, 20 `wave5b_catalog`, the rest packaging/pilot/provenance/verification) | 0 / 0 |
| post 52, 53, 54 | `git diff --check`, `git status --short`, and the candidate anchors, which name `a90cf32…` | 0 / 0 |

22 gates, actual equals expected for every one. Focused runs beyond the wrapper:

* repair-driver sweep at the candidate: 19 declared scenarios, `executed=19 skipped=0
  failed=0 blocked=0 declared=19`, exit 0 — including the real default-lifecycle entry
  `entry-r4-internal-default-lifecycle`, which the independent round had skipped, and the
  three reparse scenarios, which ran for real on a volume proved able to host a junction.
* `-Only` behaviour: an unknown filter refuses before anything executes
  (`filter-unknown-only-refuses-before-executing`); a valid filter reports its counters.
* The real driver was additionally run on its own with an explicit `-EvidenceDir`, the way
  the independent review ran it: `cases=113 mismatched=0 blocked=1`, exit 0, both scratch
  roots removed afterwards and the external evidence kept
  (`driver-explicit-run.log`, `driver-explicit-evidence/`).
* `generate-wave5b-pinned-history.ps1` into a fresh `-Out` reproduced the committed fixture
  byte for byte (`SHA-256 3FFA3E9633097AA85022D310E835AD9BF377B4970B7D3C17A34E117413DC80BB`)
  and printed `forms: crlf=7 lf=12`, the split the review §5 had only been able to see in an
  uncommitted working tree.
* `rust-lexical-control.rs` compiles with `rustc --edition 2021 --crate-type lib` (exit 0),
  so the new fixtures use legal Rust rather than scanner-convenient fiction.

## 10. Corrections to the previous closeout, and what is NOT RUN

Two wording corrections from the review §4 were applied to
`docs/reviews/2026-10-08-wave5b-s1-repair-closeout.md`:

* "old independent reports … uncommitted as before" was wrong: both older independent
  reports were already committed at `6c61272`; only their evidence directories are
  untracked. Unchanged content is not the same as uncommitted.
* "NOT RUN: nothing required is unrun this round" implied more than it should. That
  section now lists what really was not run, and this round repeats it: the 70 ignored
  workspace tests (real PostgreSQL, object storage, AI provider contracts) were not
  executed; no live database, broker, object store or provider was contacted anywhere in
  this plan — every SQL input is statically scanned and never executed; release, rollback
  and recovery rehearsal is S2/S3 scope and not started; Secret, vulnerability, licence
  and image scanning were NOT RUN and no scanner output exists; anything requiring
  elevation was not run (see BLOCKED below).

BLOCKED, named and not counted as passing: `P2-catalog-symlink` in the real driver (the
host refuses file symbolic-link creation without elevation, and the repository volume is
exFAT, which holds no reparse points at all). R2 stays BLOCKED / NOT CLAIMED: real-module
acceptance is outside this offline S1 gate.

## 11. Boundaries

No boundary, ownership or contract change. The pinned baseline (19 SQL files + manifest),
`RuntimeMigration`, business crates and apps, the CLI surface, the release output and all
four digest semantics are untouched; `Cargo.lock` did not change. The changes are the two
validator defect classes (SQL table lists, Rust lexing), the cleanup ownership/reparse
rules, the repair-driver denominator proof, their fixtures and regressions, and these
documents. Offline S1 validation is not production, release or security acceptance.

## 12. Evidence (untracked by design)

`docs/reviews/2026-10-09-wave5b-s1-final-repair-gates/` — per-gate raw logs for the three
wrapper phases (`gates-fast`, `gates-heavy`, `gates-post`, with the gate-41 case evidence
under `gates-post/wave5b-cases/`), a second standalone 113-case driver run with an explicit
`-EvidenceDir` (`driver-explicit-evidence/` plus `driver-explicit-run.log`, which holds the
`catalog-cases.csv` and `catalog-reconciliation.csv` rows quoted in section 2), the 19
per-scenario repair transcripts (`repair-regressions/` and `repair-regressions.log` with the
`executed/skipped/failed/blocked/declared` line), the N2 junction probe and its script, the
N1 SQL-shape and N3/N4 embed-shape probes with their post-fix logs, the same-shape Rust
lexical control (`rust-lexical-control.rs`), and the regenerated pinned-history JSON
(`pinned-history-rerun.json` with the `forms: crlf=7 lf=12` line). Raw session scratch under
`target/wave5b-n-*` was copied out and is disposable.

End state: **READY FOR REVIEW / NOT INTEGRATED. R2 BLOCKED / NOT CLAIMED.**