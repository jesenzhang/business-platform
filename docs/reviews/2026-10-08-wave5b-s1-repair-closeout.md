# Wave 5B-S1 repair closeout — R1–R7 (2026-10-08)

Executor: qwen3.8 flash (zcode), per
`docs/reviews/2026-10-08-wave5b-s1-qwen38-flash-zcode-repair-instructions.md`, against the
findings of `docs/reviews/2026-10-08-wave5b-s1-repair-independent-review.md`.

Final status: **READY FOR REVIEW / NOT INTEGRATED. R2 BLOCKED / NOT CLAIMED.**
Nothing was pushed, merged, deployed, or integrated; S2 was not started.

## 1. Start and fixed candidate

* Start HEAD `87e8a318fa32ff43d41ad45da8a0f83b3b19a2a4`, branch
  `codex/plan-0014-wave5a-local-release-pilot`; dirty carrier files at start:
  `scripts/test-plan0014-wave5b.ps1`, `tools/module-release-tool/src/catalog.rs`,
  `tools/module-release-tool/scripts/run-wave5b-gates.ps1`,
  `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1`.
  Disclosure: the session checkpoint did not persist initial SHA-256 of those four
  working-tree files; the git blob hashes inside the candidate are recorded below as
  the authoritative file identities.
* **Code candidate: `6c612722a841a7d0be5d5d56569de55e7fe4b9bf`** (75 files,
  +2891/-103 vs start), plus the follow-up candidate commit
  **`5fa0ff46ed29e7ed0dfdebf01f3542779b3e3a22`** (repair-driver assertion
  normalisation only, +12/-4; final code HEAD). The driver was re-run 14/14 green in
  its final form (log dir in section 6), which is the affected-gate re-run for that
  follow-up.
* Blob identities in `5fa0ff4`: `test-plan0014-wave5b.ps1`
  `b4f8c3b1e1512262730b5ddfe2a9add94bf3e1c3`, `catalog.rs`
  `bba1d023aed338dc8f5adb5ccf1d183cd4c934ef`, `run-wave5b-gates.ps1`
  `9564b14fa113bf0fbed544b1cdc91475445ae002`, `generate-wave5b-pinned-history.ps1`
  `7e4ff5af08ab64f987190f091770925b22417679`, `wave5b_catalog.rs`
  `d9d76ed4faba6f7acf996159b5ea186d155c19d9`, `test-plan0014-wave5b-repairs.ps1`
  `6784892a82f833700f3e42e8f2873b82cfdae676`. Working-tree SHA-256 recorded in
  `target/wave5b-s1-repair/` session logs.
* Forbidden files untouched: `docs/OPERATIONAL_RUNBOOK.md`, `scripts/clean-target.ps1`
  remain untracked and were never read into the commit.
  *Correction (2026-10-09, review §4):* the sentence above claimed the older
  independent reports and their evidence directories were "uncommitted as before".
  That was wrong for the reports: both older independent reports
  (`2026-10-08-wave5b-s1-repair-independent-review.md` and
  `2026-10-08-wave5b-s1-qwen38-flash-zcode-repair-instructions.md`) were already
  committed in Git at `6c6127`; unchanged content is not the same thing as uncommitted.
  Only the evidence *directories* remain untracked. The wording is corrected here
  rather than silently rewritten in place.

## 2. R1–R7

* **R1** — `catalog.rs:131 BASELINE_RESERVED_TABLE_NAMES` adds the four pinned 019
  rename targets (`v0_demo_permissions`, `v0_demo_roles`, `v0_demo_user_roles`,
  `v0_demo_role_permissions`); enforcement at `catalog.rs:2161`. New real
  reconciliation test `wave5b_catalog.rs:928` (helper `collect_reconciled_names:866`)
  derives the expectation from `git ls-tree`/`git cat-file` blobs of the pinned
  revision — never from the reserved set itself — and asserts 19 files, exactly four
  `RENAME TO` targets, and set equality both directions. Failing-regression proof:
  mutation probe (`target/wave5b-s1-repair/r1-mutation-probe.log`) removed the four
  names and the probe flipped the four `r1-rename-target-*` fixtures to VALIDATED
  while the test failed naming exactly the four missing entries; with the fix,
  `r1-test.log`/`r1-r2-r6-tests.log` are green. New fixtures: 4 rename targets
  (expect fail), `r1-reserved-documents`, `r1-reserved-sqlx-migrations` (expect fail),
  `r1-owned-host-table-reference` (positive control, same-owner references stay legal).
* **R2/R6** — statement-context parsing in `analyse_sql` (`catalog.rs:1384`) and
  `object_after` (`catalog.rs:1590`): qualified names after a comma, alias/AS lists,
  multi-table FROM, every JOIN reference checked individually, `JOIN … USING (cols)`
  column lists are not tables, `CREATE INDEX … USING btree` on an owned table passes
  while an unknown index method is refused by name, `INHERITS` handled, and
  ATTACH/DETACH PARTITION detected (`catalog.rs:2060`). Unresolvable constructs return
  named refusals instead of half-parsed VALIDATED. 14 new fixtures
  (`r2-foreign-{first,middle,last}-in-list`, `r2-foreign-in-{as-alias,from-alias,using}`,
  `r2-qualified-after-comma`, `r2-same-owner-alias-list`, `r2-comment-string-not-reference`,
  `r6-index-using-btree`, `r6-index-unknown-method`, `r6-attach-partition`,
  `r6-inherits-parent`, `r6-join-using-columns`); foreign in any list position fails,
  same-owner controls pass. Logs: `r2-r6-tests.log`, `r1-r2-r6-tests.log` (20/20),
  plus gate 41 (`[catalog-cases] 72/72 MATCH`).
* **R3** — driver line 349 now calls
  `Assert-PathsDoNotOverlap … -Label1 "scratch" -Label2 "evidence"`; the bare `-Label`
  was ambiguous against `-Label1/-Label2` and aborted every explicit-`-EvidenceDir`
  run at startup. Regressions (repair driver): `static-r3-no-ambiguous-label` (AST
  guard against reintroduction) and `entry-r3-explicit-evidencedir-reaches-evidence-rule`
  (real driver, explicit existing non-empty evidence dir: exit not 0, output contains the
  evidence rule "already holds", contains no parameter-binding/ambiguity wording, the
  evidence sentinel is untouched, and no run root leaks). Full "enters actual cases"
  criterion: gate 41 ran all 97 cases through this exact entry.
* **R4** — `scripts/test-plan0014-wave5b.ps1:354-440`: every run root is registered
  with its validated parent (`Register-RunRoot:368`); cleanup (`Remove-RunScratch:385`)
  deletes exactly the registered roots — main and OS-temp link fallback — re-canonicalises
  each root before deletion, requires case-insensitive containment inside the registered
  parent with a separator boundary, refuses any path equal to or containing a protected
  path (repo, scratch parent, external evidence), handles reparse points, and keeps the
  internal evidence subtree by explicit rule with a logged line. Regressions:
  `ast-cleanup-normal-external` (both roots removed; sentinel and stale older run byte-
  identical), `ast-cleanup-internal-evidence-retained`, `ast-cleanup-keepscratch`
  (locations logged), `ast-cleanup-protected-refusals`, `ast-cleanup-outside-registered-parent`,
  `ast-register-refuses-root-equal-to-protected-parent`, plus real-entry lifecycle
  `entry-r4-internal-default-lifecycle`: driver exits 0, `mismatched=0`, exactly one new
  retained run root `target/tmp/plan0014-wave5b/run-68fd6385091b4216ab5e3a27d49f2387`
  containing only `evidence/`, retention logged. External-evidence mode: gate 41 wrote
  evidence under the run's `-EvidenceDir` while the run root was cleaned.
* **R5** — `migrate_macro_calls` two-pass scan: `significant_mask:3137` (strings, byte
  strings, raw literals masked; char literals and lifetimes stay visible),
  `collect_migrate_bindings:3273` + `parse_use_items:3322`/`parse_use_item:3353` (plain,
  `as`-aliased, grouped and grouped-aliased imports; declaration order irrelevant),
  `find_migrate_calls:3520` (whitespace/comments between `sqlx`/`::`/`migrate`/`!`/`(`
  tolerated; a bare name that is not bound to `sqlx::migrate` and any call behind another
  path is refused by name; globs, `use sqlx as …` and foreign `migrate` imports are named
  refusals, never silent zero-counts). 18 new lib tests in `catalog.rs embed_scan_tests`
  including the synthetic second-baseline-embed integration (all four spelling variants
  counted: "2 call sites embed the host baseline"). Red proof `r5-red.log` (14 failures
  before the rewrite), green `r5-green.log` (21/21). SQLite positive controls
  (`sqlx::migrate::{Migrate, Migration}` + `sqlx::migrate!("./migrations")`) parse, bind
  nothing, and still pass gate 23 and gate 41.
* **R7** — wrapper line 140: `$caseEvidenceDir = Join-Path $EvidenceDir 'wave5b-cases'`
  assembled in a scalar, then the single element after `-EvidenceDir`; the previous
  in-array `$EvidenceDir + '/wave5b-cases'` parsed as an array append and handed the
  driver a stray positional. Regressions: `static-r7-no-array-append` (AST, covers both
  the element-append and whole-array-append spellings — mutation-probed at
  `target/wave5b-s1-repair/r7-mutation-probe-161435`, where the reverted wrapper made
  the capture scenario fail with "8 elements … a stray positional follows"), and
  `wrapper-r7-capture-unique-evidencedir-argument`: a PATH-stub `rtk` records the actual
  argument arrays; the recorded driver invocation has exactly 7 arguments,
  `-EvidenceDir` at index 5, a single joined absolute path under an evidence root whose
  name contains spaces, and no `/wave5b-cases` element. Real entry: gate 41 command line
  ends with a single `-EvidenceDir docs\reviews\wave5b-s1-repair-…-post\wave5b-cases`, exit 0.

## 3. B5–B8 and compatibility

* **B5**: module manifests accept the canonical LF digest only (`catalog.rs:1272` names
  the refusal); the pinned historical manifest keeps raw/LF/CRLF tolerance
  (`catalog.rs:2519-2543`). `the_seven_c1_entries_are_pinned_by_blob_even_though_the_manifest_is_crlf`
  and the `crlf-checkout-validated` positive fixture pass in both readers.
* **B6**: generator re-run to a fresh temp `-Out` reproduced
  `pinned-baseline-5c8355d….json` byte-identically (SHA-256
  `3FFA3E9633097AA85022D310E835AD9BF377B4970B7D3C17A34E117413DC80BB`); 20 entries =
  19 baseline SQL (forms: 12 LF / 7 CRLF) + 1 baseline-manifest.
* **B7**: stub-always-exit-7 makes the wrapper exit non-zero and "FAILED GATES" names all
  seven mismatches including the planned negative (`25 … expected 1, got 7` — a non-zero
  exit is never waved through); stub known-negative-only makes the wrapper exit 0 and the
  gate log records `exit-code: 1` against `expected-exit-code: 1`. Both in
  `wrapper-b7-stub-exit-7-propagates` / `wrapper-b7-known-negative-only-passes`.
* **B8**: unique RunId default, second run into a non-empty evidence directory is
  refused ("already exists and is not empty") and archived gate logs are SHA-256
  unchanged (`wrapper-b8-evidence-not-overwritten`); fast/heavy/post each got their own
  evidence directory this round (paths in section 6).
* Pinned revision, CLI surface, JSON/status output, the four digest semantics and the
  historical pin are unchanged; gates 20-24 (default and every `--only` plus the planned
  unknown-check negative) exit as expected in this round's fast phase.

## 4. Counts, both readers, non-greens

* New fixtures: 21 catalog fixture directories (63 files). New Rust tests: 18
  (`embed_scan_tests`) + 1 integration (`baseline_reserved_tables_are_reconciled_with_pinned_sql`);
  `wave5b_catalog` totals 20, crate totals 98, workspace 192 suites / 901 passed / 0 failed.
* Two-reader reconciliation: one `EXPECTATIONS.json` drives both
  `wave5b_catalog.rs` and the PowerShell driver; gate 41 this round:
  72 catalog + 11 history + 7 embed + 5 freeze + 2 path = **97 cases, mismatched=0**.
* **BLOCKED (platform, recorded not failed)**: `P2-catalog-symlink` — creating a file
  symlink needs elevation on this host (the volume holds no file reparse points);
  identical to every prior round on this machine. Junction case P1 passes.
* **ignored**: 70 workspace tests (pre-existing database/contract `#[ignore]`s, unchanged).
* **NOT RUN** (the sentence in the first version of this section read "nothing required
  is unrun this round", which the independent review §4 rejected as over-claiming; this
  is the accurate list):
  * the 70 `#[ignore]`d workspace tests — real PostgreSQL, object storage and AI provider
    contracts were never executed, on this host or in any gate this round;
  * real-service E2E: no live database, message broker, object store or provider was
    contacted; every catalog/SQL input in this plan is statically scanned, never executed;
  * release/rollback and recovery rehearsal (Plan 0014 S2/S3 scope) — not started;
  * Secret, vulnerability, licence and image scanning — no scanner was run and no
    scanner output exists for this round;
  * anything requiring elevation (file symbolic-link creation, see BLOCKED above).
  What *was* run is listed in section 6 and nowhere else; offline S1 validation is not
  production, release or security acceptance.

## 5. Scratch lifecycle this round

Registered roots are deleted by the run that created them; retained this round only by
design: `target/tmp/plan0014-wave5b/run-68fd6385091b4216ab5e3a27d49f2387/evidence`
(internal-evidence carrier from the entry-R4 regression, explicitly logged), and the
driver scratch for gate 41 was cleaned while its external evidence directory under
`…-post/wave5b-cases` survives. Protected-path and sentinel survivals are asserted in
the AST scenarios (byte SHA-256 unchanged); `KeepScratch` logs each retained root.

## 6. Gate matrix (exact commands in each log header; RunId `20261008-163628-fed216f4`)

* fast — `docs/reviews/wave5b-s1-repair-20261008-163628-fed216f4-fast/gates/`:
  gates 20/21/22/23/24 exit 0, 25 exit 1 (expected 1), 30 fmt 0, 50/51 git 0 — wrapper exit 0.
* heavy — `…-heavy/gates/`: 31 check 0, 32 clippy `-D warnings` 0, 33 runtime-migration 0,
  34 crate 0, 35/36 `--no-run` builds 0 — wrapper exit 0.
* post (at candidate) — `…-post/gates/`: 37 workspace test 0, 40 check-architecture 0,
  41 offline catalog gate (real driver, joined `-EvidenceDir`, 97 cases mismatched=0,
  1 BLOCKED) 0, 42 crate at candidate 0, 52/53/54 git anchors 0 — wrapper exit 0.
  Gate 42 was already green pre-commit because `Cargo.lock` is committed-clean; the
  committed-input dependency of the packaging test is recorded honestly as satisfied
  in-tree (no unexplained "green after commit" claims).
* Direct: `cargo fmt --all -- --check` 0; `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` 0 (`target/wave5b-s1-repair/clippy-workspace-final.log`);
  `cargo test --workspace --all-features` 0 (`target/wave5b-s1-repair/test-workspace-final.log`);
  `scripts/check-architecture.ps1` via gate 40 exit 0.
* Repair driver (final form): `target/wave5b-s1-repair/final-repair-driver-170435/`
  — 14/14 scenarios PASS; per-scenario raw transcripts included.

## 7. Boundaries

No boundary, ownership or contract change: the pinned baseline (19 SQL + manifest),
RuntimeMigration, business crates/apps, the CLI/release output and all four digest
semantics are untouched; changes are confined to the validator defect fixes, their
regressions, the gate scripts, and these documents.

## 8. Evidence this round (untracked by design, like prior gate directories)

* `docs/reviews/wave5b-s1-repair-20261008-163628-fed216f4-{fast,heavy,post}/` (post
  contains the driver evidence under `wave5b-cases/`).
* `target/wave5b-s1-repair/` raw session logs (R1 mutation probe, R5 red/green, R2/R6,
  clippy/test finals, repair-driver transcripts and mutation probe).

End state: **READY FOR REVIEW / NOT INTEGRATED. R2 BLOCKED / NOT CLAIMED.**