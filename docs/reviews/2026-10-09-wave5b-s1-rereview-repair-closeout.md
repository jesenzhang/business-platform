# Wave 5B-S1 re-review repair closeout — Q1–Q3 (2026-10-09)

Executor: qwen3.8 flash (zcode), executing
`docs/reviews/2026-10-09-wave5b-s1-repair-rereview.md` ("继续根据审阅结果进行修复"),
against review candidate `a90cf3277356471511647fbee9b507716c8c81bb`.

Final status: **READY FOR REVIEW / NOT INTEGRATED. R2 BLOCKED / NOT CLAIMED.**
Nothing was pushed, merged, deployed or integrated; S2 was not started; no database, object
store, provider or network service was contacted; no ADR was accepted. Only Q1–Q3 were
worked on: the SQL and Rust parser repairs the review kept (`catalog.rs`, the fixture
generators, `EXPECTATIONS.json`) were not touched, reopened or re-shaped.

> **2026-11-19 remediation note.** The independent re-review of 2026-10-22
> (`docs/reviews/2026-10-22-wave5b-s1-independent-rereview.md`) judged this closeout's
> central safety claim too broad and issued CHANGES REQUESTED with C-1..C-4. Section 11
> records the bounded follow-up that answers them, including a genuine fail-first
> regression for C-1, the source correction for section 7, the scope/hash correction for
> section 10 and the default-scratch fix. The verdict **R2 BLOCKED / NOT CLAIMED is
> unchanged**; this document does not claim R2 admission or a full-workspace CI pass.

## 1. Start state, the boundary of the change, and the new fixed candidate

* Start HEAD `0f295731d4cfc5f57cd8638d5a1fd2ca234f0caa` (the N1–N5 closeout docs commit),
  branch `codex/plan-0014-wave5a-local-release-pilot`. The review candidate `a90cf32` is its
  ancestor. The start state is recorded before any edit in
  `docs/reviews/2026-10-09-wave5b-s1-rereview-repair-gates/start-state.json`: branch, HEAD,
  SHA-256 of the two scripts under repair (driver `606CCECE…`, repair harness `E6717866…`)
  and of the review report (`FDAF35F2…`), plus the 11 untracked entries the tree carried.
* Dirty state at start: 11 untracked paths, no modified tracked file. Two of them belong to
  someone else and were left exactly as found — `docs/OPERATIONAL_RUNBOOK.md` and
  `scripts/clean-target.ps1`. They are still untracked, unread into any commit, unmodified.
  Gate-evidence directories stay untracked by design; the rereview report this closeout
  answers, `docs/reviews/2026-10-09-wave5b-s1-repair-rereview.md`, arrived as an untracked file
  and is committed with it, as the previous round committed the report it answered.
* The change is two files. `git show --stat` for the candidate below lists
  `scripts/test-plan0014-wave5b.ps1` (+224/−38) and `scripts/test-plan0014-wave5b-repairs.ps1`
  (+756/−21) and nothing else: no migration, no manifest, no runner, no business crate, no
  shared helper file added. No SQL was executed, nothing was inserted into `plan0014_s1_*`.
* **Candidate: `6bcdd89d14d188f62e99bbd068cf786b8207699b`**
  (`fix(plan-0014): prove the wave 5b-s1 cleanup path, not its name`).
  Blob identities at that commit: `test-plan0014-wave5b.ps1`
  `71d44b8554ef89b13184ec3ee06bb38e238f99bb`, `test-plan0014-wave5b-repairs.ps1` `5b2e8a2536ccdb36b5412fe58b78da0ab636e7ce`.
  This closeout lands in the documents-only commit that follows it, and every gate in
  section 8 ran at the candidate itself with a clean tracked tree — the same layering the
  previous round used.

## 2. The one mistake behind all three findings

Q1, Q2 and Q3 are the same error in three places: **a path was treated as the directory it
spells.** A junction keeps the name of the directory it replaced, so every name-based test —
`Resolve-Path`, `StartsWith`, equality, the reparse attribute of a *parent* read a moment
earlier — still answers about the spelling while the delete acts on something else. Two rules
run through the fix:

* **Identity, not name.** Every decision cleanup makes is taken from a no-follow attribute
  read of the object itself, repeated at the moment of the action, for the whole chain it is
  reached through. `[System.IO.File]::GetAttributes` is the call the shell itself makes
  underneath `Get-Item`; it opens the reparse point rather than its target, so the attribute
  of the *name* is what is read — which is the object a delete acts on. `Get-UnsafeLink` now
  walks from the path to the **volume root**, not to the registered boundary: the boundary is
  still required (a path that no longer sits under the name it was registered under is a
  refusal of its own) but it is no longer trusted as the place proof stops. A chain that
  never reaches its boundary throws instead of answering "safe".
* **Ownership, not path.** `Register-RunRoot` now anchors each root with a witness file only
  that run knows the name of (`.wave5b-run-owned-<token>`, content
  `wave5b-run-root-owner=<token>`), written when the root is registered and read back before
  anything under it is removed. A root whose witness is missing or names another run is
  retained. The repair harness keeps the same idea as an explicit list: it deletes the
  objects it recorded creating, and never a path it merely computed.

Where a user-mode process cannot prove what it is about to do — the last read before a
delete can never be atomic with that delete — the fix **retains and refuses** rather than
guessing, and the function says so in its own comment. The residual window is named in
section 6; it is not claimed away.

## 3. Q1 [P1] — the chain above the registered parent, and who owns the root

Defect, as the review reproduced it: `Get-UnsafeLink` walked from the run root up to the
registered `Parent` and stopped. Move one of the levels *above* that parent aside and plant a
junction on it, and `parent\run-owned` still spells itself, still passes every containment
test, and now names a tree the run never created — `Remove-RunScratch` deleted it. The chain
check was certifying a spelling.

Repaired in `scripts/test-plan0014-wave5b.ps1`:

* `Get-UnsafeLink($Path, $Boundary)` no longer returns at the boundary. It records that the
  boundary was met (`$reachedBoundary`) and keeps walking to the **volume root**, returning the
  first component that carries the reparse attribute. The boundary requirement survives as a
  refusal of its own: a chain that never reaches it throws
  `"'$Path' never reaches its boundary '$canonicalBoundary'"` instead of answering safe.
* `Test-ReparsePoint` reads attributes with the no-follow platform call the shell itself uses
  under `Get-Item` (`[System.IO.File]::GetAttributes`), which is both correct for the question
  ("is *this name* a link?") and cheap enough to repeat: 0.14 s for 4 601 probes against
  1.64 s for the previous `Get-Item` form. That is what makes the Q2 re-checks affordable.
* `Register-RunRoot($Root, $Parent, $Kind)` now requires the root to exist as a directory and
  writes an ownership witness into it — `.wave5b-run-owned-<token>`, content
  `wave5b-run-root-owner=<token>`, with the token kept in the registration record. The witness
  is written by this run into this directory; a directory that was swapped in afterwards does
  not hold it.
* `Remove-RunScratch` reads that witness back with `[System.IO.File]::ReadAllText` before it
  deletes anything, and refuses (leaving the tree in place, naming
  `run ownership witness` in the message) when it is missing or names a different run. In the
  internal-evidence branch the witness is removed with the other children, after the evidence
  it protects has been reported as retained.
* `Assert-PathChainIsReal` keeps its name and its callers; its comment now states plainly that
  the proof does not stop at the boundary.

This is deliberately not "move the boundary one level up", which the review ruled out and
which would only relocate the same mistake: the walk has no stopping name, and identity comes
from a witness only the run can have written.

| Regression | What it does on the candidate |
| --- | --- |
| `ast-cleanup-ancestor-above-parent-replaced-by-junction` | swaps in a junction at depth 1, 2 and 3 above the registered parent, with an external sentinel below it; the review's own probe shape |
| `ast-register-refuses-link-already-above-parent` | the link is planted **before** `Register-RunRoot`, so nothing the run observed was ever a real directory |
| `ast-remove-tree-linked-ancestor-retains-tree` | calls `Remove-Tree` directly on a tree reached through a link above everything registered |
| `ast-cleanup-ownership-witness-is-required` | a look-alike root with the right name and no witness is retained, not deleted |

## 4. Q2 [P1] — the identity of a directory between checking it and using it

Defect: a child directory passed the attribute check while its parent was enumerated, and only
its `FullName` **string** was queued. Between that check and the pop it could be replaced by a
junction, and the pop then enumerated and deleted *through* it, into the external tree.

`Remove-Tree` now proves each directory at every point where it uses it:

1. **Once above the root.** Before the walk starts, every component above the tree root up to
   the volume root is checked (no-follow). A tree reached through one is refused outright:
   `Remove-Tree: '<tree>' is reached through '<above>', which is a reparse point; the tree is
   retained and nothing under it was touched`. This is the Q1 case arriving through a function
   that was handed a path nobody had registered.
2. **On the pop**, before enumerating: the directory itself and every component between it and
   the walk root (`$unsafeWithin`).
3. **Immediately after the enumeration**, the same walk again — because the entries just read
   are only paths *inside that directory* while the directory still is that directory.
4. **Immediately before each directory is deleted**, the same walk again, then
   `[System.IO.Directory]::Delete($directory, $false)`: never recursive.

A failed proof does not delete: the directory is put on a retain list, its enumerated entries
are left alone, the walk stops, and the whole call ends in a named refusal —
`Remove-Tree retained N object(s) instead of deleting them: '<dir>' is itself a reparse point
[while it was being enumerated] … A directory that stopped being the directory that was queued
is not entered, the entries read from it are not deleted, and the tree is left where it
stands.` Retention is a failure the caller has to report; a scratch directory surviving is
visible, which is the point.

Two regressions, one per window, both driven by a scoped shim that calls the **real**
`Get-ChildItem` and then performs one real `Move-Item` plus one real `New-Item -ItemType
Junction` — no stub return values, and the shim refuses to touch anything outside its own
scenario scratch:

| Regression | The window it opens |
| --- | --- |
| `ast-remove-tree-queued-directory-swapped-before-pop` | swap during the enumeration of the **parent**: the child is checked, queued by name, then replaced |
| `ast-remove-tree-directory-swapped-after-enumeration` | swap during the enumeration of the **directory itself**: the listing is taken, then the entry it names is replaced. Both trees hold a file with the same name, so a stale delete lands outside |

Each asserts, in this order: the external sentinel and the same-named external file still exist
with their **original SHA-256**, the external item count is unchanged, a named refusal was
raised, and only then the positive controls — the run-owned sibling the walk could prove really
is gone, the moved-aside run-owned content really survived, the retained root is still there to
inspect.

## 5. Q3 [P2] — the probe scratch belongs to one run, not to the host

Defect: `Get-ReparseCapableScratch` built `%TEMP%\wave5b-reparse-probe\scenario-<name>` from a
fixed base, a fixed scenario name, and `Remove-Item -Recurse -Force` on whatever it found
there. A second run, or a run overlapping another, cleared the first one's fixtures — the
review's probe planted a sentinel at that exact path and watched it disappear.

Repaired in `scripts/test-plan0014-wave5b-repairs.ps1`:

* One identity per process: `$script:RunId = [guid]::NewGuid().ToString('N')`, recorded
  ownership list `$script:RunOwned`.
* Every path the helper builds carries it: probe base
  `%TEMP%\wave5b-reparse-probe-<RunId>`, falling back to `<OutDir>\reparse-probe-<RunId>`, then
  `junction-target-<guid>`, `junction-link-<guid>`, `scenario-<Name>-<guid>`. A base that
  already exists is **refused** (the helper moves to the next candidate rather than sharing it),
  and a name that already exists is stepped around, up to eight attempts, never emptied.
* Nothing is deleted on the way in any more. The only removal during setup is the probe
  junction's own **name**, which this helper just created, so the target it points at stays for
  the scenario to use.
* Release is by recorded ownership: `Remove-RunOwnedScratch` pops `$script:RunOwned` and hands
  each entry to the production `Remove-Tree`. `OutDir` is evidence and never enters the list.
  It runs in each scenario's `finally` and under a script `trap` — which sits *below* the helper
  definitions and checks for them, because a trap that fails replaces the real error with a
  missing-command message, which is exactly what it did on its first try.
* The refusal keeps its shape: when no writable volume can host a junction the helper throws
  `BLOCKED: no writable volume on this host can host a junction … they are untested, not
  passing`. Capability is still proved by creating one, never assumed, and no stub replaces that
  check.
* The helper is imported **alone** by the review's Q3 probe, so it now provides its own run id
  and ownership list and calls nothing but host commands. Its extraction surface is pinned by a
  guard scenario (section 7).

| Regression | What it proves |
| --- | --- |
| `helper-scratch-never-reuses-previous-run-root` | a decoy tree planted at the exact legacy path, with a sentinel: the helper may neither hand that path back nor touch what is in it, and the scratch it returns must carry this run's id |
| `helper-scratch-repeat-and-concurrent-runs-are-disjoint` | four real child processes running the real helper, two in sequence and two genuinely at the same time (`Start-Process -PassThru`, held open 2 500 ms); each plants a sentinel in the scratch it was given, and exits non-zero if anything deleted it, changed its bytes, or if its own scratch outlived it. Bases, roots and run ids must differ pairwise, and no child may report this run's id |

## 6. What is now guaranteed, and what is only handled

Stated plainly, because the review is right that "non-recursive API" and a static attribute read
are not proofs:

* **Guaranteed.** Every identity decision in the cleanup surface is taken from a no-follow
  attribute read of the name itself, and every delete is either the removal of a reparse point's
  *name* or `[System.IO.Directory]::Delete($dir, $false)` on a directory just proved empty. No
  directory is entered, enumerated or deleted unless its own attribute and that of every
  component from it to the volume root were read at that moment. (Wording corrected after
  the 2026-10-22 re-review: the original text said "between it and the walk root", which
  was literally true but stopped exactly where Q1 began. The C-1 fix in section 11 made
  the stronger statement true in code as well. See the residual-window note below for
  what a read still cannot prove.) A registered root is
  additionally identified by the *content* of a witness file only that run wrote, and that read
  happens only after the chain has been proved real — `Remove-RunScratch` checks registered-parent
  containment, then the full chain, then the protected paths, then the witness, in that order,
  so even the witness read cannot be led through a link.
* **Not guaranteed, and said so.** One attribute read still precedes one delete by a syscall,
  and no user-mode check makes the pair atomic. If a directory is swapped in that gap, the
  delete that follows still cannot walk through the link — it removes the *name*, and the tree
  behind it is untouched — so the residual cost is one lost directory name inside a tree this
  run created, never external content. Anything the walk *can* see has already been retained
  and refused. `Remove-Tree`'s own comment carries this limit. At the time this closeout was
  first written, "the gate treats a retained scratch as a failure" described an intention,
  not the exit code: `Remove-RunScratch` only issued `Write-Warning`. The C-2 fix in section
  11 made it literal — every refusal is recorded and `Assert-NoRetainedScratch` makes the
  process exit non-zero.
* **Not exercised here.** Creating a *file* symlink needs an elevated token on this host, so
  the driver's `P2-catalog-symlink` case stays **BLOCKED** and is reported as such. Junctions
  work on the host's NTFS volumes, so every link scenario above really creates one; the repo
  volume `F:` is exFAT and refuses them, which is why link scratch falls back to the `%TEMP%`
  base — a fallback that is itself proved by creating the junction, not assumed.

## 7. The regressions, and how they were shown to fail first

Ten new scenarios, all AST-manifested, all on the real production functions imported from the
current driver text — no stub returns a safe answer. The review's three probes are their
origin, and their shapes are preserved: real directories, real junctions, real sentinels,
external trees this round created under its own scratch.

**Fail-first, against the code under repair.** Two harnesses, both archived with their logs.

*Source correction (2026-11-19, prompted by the independent re-review §7).* The Q1/Q2
fail-first run did **not** use the final harness. It used the harness at
`0169d85af7cadf36e32fc48ad0ad24cad24aff0d` ("fix(plan-0014): prove the wave 5b-s1 cleanup
path, not its name"), whose blob is `8b3f272ab43884fcb044b0c173b3beff1d16e373`; the final
candidate blob is `5b2e8a2536ccdb36b5412fe58b78da0ab636e7ce`. The diff is **42 insertions /
11 deletions** and lands in the two Q2 scenarios:
  - `ast-remove-tree-queued-directory-swapped-before-pop`: the watched target changes from
    the child (`At = $child`) to the parent (`Watch = $run`), with the swapped child kept in
    its own `Swap = $child` field;
  - `ast-remove-tree-directory-swapped-after-enumeration`: a sibling positive control
    (`sibling/mine.txt`) and a scratch-escape guard are added.

  The remaining hunk only expands the Q3 failure message. Therefore the "queued before pop"
  window has **no fail-first evidence from the final harness** — the archived draft watched
  the child itself, not the parent. This is a source gap and is not claimed as covered here;
  the new fail-first evidence produced with the final harness is in section 11.

* Q1 and Q2: the `0169d85` harness above, run with `-Repo` pointed at a tree holding
  `git show 0f29573:scripts/test-plan0014-wave5b.ps1` — the candidate `a90cf32` content,
  SHA-256 `FB56AE7E…`. Result `executed=7 skipped=22 failed=6 blocked=0 declared=29`, exit 1.
  Six failures, each naming the external object that was destroyed: an ancestor link at three
  depths took an external tree from 3 items to 1; a link planted before registration did the
  same; `Remove-Tree` under a linked ancestor took 4 items to 0; the queued-directory swap took
  3 to 0; the post-enumeration swap deleted the external file that shares a name with the entry
  it had just enumerated (2 to 1). The witness scenario fails because the candidate records no
  witness at all. The seventh scenario, the self-containment guard, passes there by design: it
  constrains the surface as it is now written, not the old code.
* Q3: the same final scenarios with only `Get-ReparseCapableScratch` replaced by its pre-fix
  text (the hybrid is archived beside the log). Both fail: the helper "handed back the scratch
  of an earlier run … the earlier run's sentinel … **is now deleted**", and a child run exited 5
  because a helper that records no ownership cannot even release its own scratch.

**After the repair.** `executed=29 skipped=0 failed=0 blocked=0 declared=29` (section 8 re-runs
this at the final SHA). The 19 scenarios that existed before still exist and still pass,
including the ones the review listed as must-keep: normal owned-root cleanup, internal-evidence
retention, `KeepScratch`, the protected-path / case / separator refusals, the exception
finally, and the root, parent and static-child junction cases. Seven of them were also corrected
to create the run root **before** handing it to `Register-RunRoot`: the repaired function refuses
a root that does not exist as a directory, and those scenarios had been registering a path they
only created afterwards. That is a contract change in the harness, disclosed here rather than
buried, and the pre-existing 19 pass with it.

**Controls, in every one of those scenarios.** Existence *and* SHA-256 of each external
sentinel, external item counts before and after, and a positive control proving ordinary
run-owned content really is deleted — so a "fixed" version that simply stopped cleaning cannot
pass. The post-fix review probes repeat the same checks from outside: the ancestor probe reports
`reported_unsafe_link=…\anchor`, `sentinel_survives=true`,
`original_parked_root_survives=true`; the queued-directory probe reports `swapped_after_enumeration=true`,
`sentinel_survives=true`, `original_parked_child_survives=true` with
`sha_before == sha_after = 3F962E42C8471C12F5E11DAB2391F98C70FC6DC4626AFFDEEDE72379F9CAF4CA`
and the refusal `Remove-Tree retained 1 object(s) … '…\run-owned\child' is itself a reparse
point …`; the scratch probe, imported function and all, reports
`reused_preexisting_root=false, sentinel_survives=true` against a base carrying its own run id.

**Two static guards, because these defects were structural.** `static-cleanup-surface-is-
self-contained` proves the eight functions an independent probe imports call nothing outside
the imported set (this is not decoration: the Q3 probe imports one function alone, and a fix
that reached for a harness-local helper made that probe die on a missing command — the finding
would have reopened for the wrong reason), and that the helper it imports alone calls neither
file's helpers. `static-no-recursive-delete-in-either-harness` forbids a recursive
`Remove-Item` in either script: every deletion is a step a reviewer can read. Top-level
definitions only are compared, so a scenario's scoped shim over a real cmdlet is not mistaken
for a dependency.

**The filter proves its own denominator.** `filter-unknown-only-refuses-before-executing`
requires a non-zero exit, the words `no such scenario`, no `SKIP` line at all and no scenario
log in the output directory; `filter-counters-reported-for-a-valid-filter` requires the
`declared/executed/skipped/failed/blocked` counters on a valid filter. `declared=29` is printed
by the harness from the AST manifest, not typed in.


## 8. Gates run at the candidate

Every step ran serially at `6bcdd89d14d188f62e99bbd068cf786b8207699b` with a clean tracked tree.
`gate-timeline.log` records the HEAD, the exact command line, the wall-clock start and stop and
the exit code of each step; the gate directories hold one log per command with the same header,
so any line can be re-run verbatim. No step overlapped another, and nothing ran against the
build directory while `module-release-tool.exe` was executing.

| Step | Result at `6bcdd89` |
| --- | --- |
| `repair-scenarios` (304 s) | exit 0 — `executed=29 skipped=0 failed=0 blocked=0 declared=29` |
| `driver-offline-113` (274 s) | exit 0 — `summary total cases=113 mismatched=0 blocked=1`: **112 matched, 1 BLOCKED, 0 mismatched**, the blocked one being `P2-catalog-symlink`. Not 113/113, and not written as such |
| `gates-fast` (23 s) | 9 gates, every exit code as expected: five `catalog-check` scopes, `25` unknown-check at 1, `30-fmt-check` (`cargo fmt --all -- --check`) 0, `50-git-diff-check` 0, `51-git-status-short` 0 |
| `gates-heavy` (323 s) | `31` `cargo check --workspace --all-targets --all-features` 0; `32` `cargo clippy --workspace --all-targets --all-features -- -D warnings` 0; `33` `cargo test -p runtime-migration --all-features` 0; `34` `cargo test -p module-release-tool --all-features` 0 — **108 passed, 0 failed, 0 ignored across 12 suites**; `35`/`36` outbox-reliability and chain-pg compiled with `--no-run` |
| `gates-post` (991 s) | `37` `cargo test --workspace --all-features` 0 — 192 suites, **911 passed, 0 failed, 70 ignored**; `40` `scripts/check-architecture.ps1` — "Architecture fitness: PASS"; `41` the real offline driver again, `cases=113 mismatched=0 blocked=1`; `42` the crate suite a second time at the candidate, 108 passed; `52`/`53` `git diff --check` and `git status --short` clean of tracked changes; `54` anchors `6bcdd89 → 0f29573 → a90cf32` |
| `probes/` (4 s) | the review's three probes, unmodified in logic, against the repaired tree — values in section 7 |

The 70 ignored workspace tests are the database- and service-dependent ones listed in section 9;
they are counted as ignored, never as passed. `Cargo.lock` and every Rust source are unchanged
in this candidate, which is why the crate and workspace numbers match the independent rereview's
own figures (108 passed / 12 suites; 192 suites / 911 passed / 70 ignored) rather than moving.

## 9. Boundaries, and what was not run

R2 stays **BLOCKED / NOT CLAIMED**: real-module acceptance is outside this offline S1 gate.
`P2-catalog-symlink` in the real driver stays **BLOCKED** — this host refuses file symbolic-link
creation without elevation, and the repository volume `F:` is exFAT, which holds no reparse
points at all; the junction regressions therefore run on the NTFS volume the scratch helper
proves by creating a link on it. Neither is counted as passing anywhere in this report.

NOT RUN, with no output to point at: the 70 `#[ignore]`d tests that need PostgreSQL or another
external service; secret, vulnerability, licence and image scanning; anything requiring an
elevated token; deletion safety against a concurrent multi-process adversary, against file
symlinks or mount points, and on network or ReFS volumes — the regressions cover a single
process doing the swapping, which is the threat the review demonstrated. The workspace-level
gates in section 8 were run here, by the submitter; an independent re-run remains the reviewer's
to do, and this closeout does not pre-empt it. Killing the first gate run mid-way (see below)
also means no gate result in this report comes from a tree that differed from `6bcdd89`.

No boundary, ownership or contract change. `tools/module-release-tool/src/catalog.rs` is not in
this candidate at all: the SQL table-list and Rust-lexer repairs the review kept were not
reopened, and no case expectation was reshaped. The pinned baseline (19 SQL files + manifest),
`RuntimeMigration`, business crates and apps, the CLI surface, the release output and the four
digest semantics are untouched, and `Cargo.lock` did not change. No migration, module manifest,
gate runner or business crate was edited, and no shared cleanup helper file was added — the
review asked for an argument if one were needed, and none was: both defects belonged in the two
scripts that already owned them.

The gates ran serially, one step at a time, never concurrently with an executing
`module-release-tool.exe` in the same build directory. Where a run did not bind cleanly it was
discarded rather than reused: the first sequence started at `c3511c3`, a retention message was
tightened while it was running, so that whole sequence was cancelled, its partial evidence
deleted, the change folded into the candidate, and every step above re-run at `6bcdd89`.

## 10. Evidence (untracked by design)

Under `docs/reviews/2026-10-09-wave5b-s1-rereview-repair-gates/`:

| Path | What it is |
| --- | --- |
| `start-state.json` | branch, HEAD, SHA-256 of both scripts and of the review report, and the 11 untracked paths, captured before any edit |
| `failfirst/q1-q2-against-candidate-driver.log` | the final Q1/Q2 scenarios against the candidate's driver; 6 named failures, each with the external tree's item counts before and after |
| `failfirst/q3-against-candidate-helper.log` + `hybrid-final-scenarios-prefix-helper.ps1` | the same for Q3 with only the helper reverted; the hybrid is archived so the substitution is checkable |
| `failfirst/*-outdir/` | per-scenario raw logs of both fail-first runs |
| `repair-scenarios.log`, `repair-scenarios/` | the full 29-scenario sweep at the final SHA, with each scenario's own log |
| `driver113.log`, `driver113/` | the real offline driver, declared/executed/mismatched/blocked counters |
| `gates-fast/`, `gates-heavy/`, `gates-post/` | the repository gate list, one log per command with command line, start, stop and exit code |
| `probes/ancestor-link-probe.log`, `probes/queued-directory-link-probe-postfix.log`, `probes/global-scratch-probe.log` | the review's own three probes re-run against the repaired tree |
| `gate-timeline.log` | every step with its HEAD, command line, duration and exit code |
| `final-state.json` | final SHA and hashes of both scripts and of the logs that were actually included; **not** `gate-timeline.log`, `git-diff-check.log` or `git-status-short.log` (independent re-review §7). "Hashes of every log above" was an overstatement and is withdrawn. |
| `repair-before-fix/` | the earlier fail-first capture, kept as history |

The three probe scripts live in `target/wave5b-q123/` (scratch, untracked): two are the
reviewer's files with only their paths repointed, and the third differs from the reviewer's
original by a `try`/`catch` around `Remove-Tree` plus two output lines — necessary because a
correct refusal is now a thrown, named refusal under `$ErrorActionPreference = 'Stop'`.

> 2026-10-22 workspace-cleanup note (corrected): `target/` was deleted in full, so the
> `target/wave5b-q123/` scratch copies no longer exist. The `.ps1` files under
> `docs/reviews/2026-10-09-wave5b-s1-rereview-gates/` are the **reviewer-round originals**
> (they still point at the deleted `target/w5b-review-1009/…`); they are **not** the executed
> copies, which carried a `try`/`catch` around `Remove-Tree` plus two extra output lines and
> were not preserved. The §7 probe values therefore cannot be reproduced from the repository
> alone; the 2026-10-22 independent re-review records this as an evidence gap.

### Hash 口径 (unified 2026-11-19)

Every script SHA-256 in this report and in the 2026-10-22 re-review is the hash of the
**LF-normalized (git blob) bytes**, not of a `core.autocrlf=true` working-tree checkout.
The two forms of the same candidate content are:

| Candidate blob | LF bytes (the quoted hash) | CRLF checkout (`Get-FileHash`) | `git hash-object` |
| --- | --- | --- | --- |
| driver `6bcdd89` | `7B7F209577324697C8BC89B364D23A80CEF42A711451F1A05A825C7D1C842471` | `0E748E10D83802D55704994F5FA65A2AE28D4E21623755DE4850819940CB0BA6` | `71d44b8554ef89b13184ec3ee06bb38e238f99bb` |
| harness `6bcdd89` | `F235F5F271CFB20EFE6C8496F8A099AE77DF1C01D78823AE297BBF9A97177665` | `897ACB64077D0100C33A20615B56CB9CDA72CD1B9577289B8F49DAD581583BD9` | `5b2e8a2536ccdb36b5412fe58b78da0ab636e7ce` |

The earlier `606CCECE…` / `FB56AE7E…` pair is **not** asserted here to be a line-ending pair:
the archived evidence does not establish what bytes produced them, and the independent
re-review flagged the missing explanation. Content identity for the candidate is the
`git hash-object` column. `Get-FileHash` on a Windows checkout is expected to differ from the
LF figure and is not by itself evidence that the file changed.

## 11. 2026-11-19 bounded remediation of the CHANGES REQUESTED

Executor: subagent `plan0014-s1` on branch
`codex/plan0014-wave5b-s1-cleanup-boundary` (created from `eb53250`; no push, no merge,
main worktree untouched). Authority: `docs/reviews/2026-10-22-wave5b-s1-independent-rereview.md`
§8–§11. All evidence below is in the untracked-by-convention directory
`docs/reviews/2026-11-19-wave5b-s1-cleanup-boundary-gates/`, whose `RUN-MANIFEST.md`
records the exact commands, timestamps and counters. No Rust source or `Cargo.lock` changed.

### C-1 — the per-action chain proof now reaches the volume root

`Remove-Tree`'s local `$unsafeWithin` returned as soon as it reached `$WalkRoot`; the only
proof above the walk root was one check made before the attempt loop. Replacing that
ancestor on the first enumeration redirected the rest of the walk into an external tree and
the call returned success. `$unsafeWithin` now walks to the volume root on **every** call
and still requires the walk root to be on the chain. Section 6 and the function comment were
corrected to match.

The new `ast-remove-tree-ancestor-swapped-on-first-enumeration` scenario uses the final
harness, a real junction and a scoped `Get-ChildItem` shim that parks the ancestor and plants
the link **before the first enumeration**. It asserts the external sentinel and nested file
keep their SHA-256, the external item count is unchanged, a named refusal is raised, the
parked run tree is intact and the planted junction is retained.

| Run | Result |
| --- | --- |
| fail-first, unfixed driver (`6bcdd89` bytes) | `FAIL … external tree went from 5 to 1 item(s)); Remove-Tree said: ` (empty); `executed=1 skipped=29 failed=1 blocked=0 declared=30`, exit **1** (`failfirst-c1-stdout.log`) |
| after fix | `PASS`; `executed=1 skipped=29 failed=0 blocked=0 declared=30`, exit **0** (`positive-c1-stdout.log`), refusal `Remove-Tree retained 1 object(s) … is reached through the reparse point …\anchor while it was being enumerated` |

### C-2 — P1-1 and P1-2 are genuinely repaired, not downgraded

* **P1-1.** Each child is now re-read with `Get-Item -LiteralPath … -Force` at the moment of
  the action. The fresh identity (link/container) is compared with the enumeration-time
  `FileInfo`; a child that changed is **retained and refused**, while a child that was
  already a link when listed is still unlinked as a name (that is the run's own fixture, and
  unlinking a junction never follows it). The first iteration of this fix treated the
  driver's legitimate `link-junction/.../policy` fixture as retention and made a clean run
  exit non-zero; the comparison form above was adopted after that evidence. Both Q2
  regressions still pass.
* **P1-2.** `Remove-RunScratch` records every refusal in `$script:RetainedScratch` (including
  the fallback link scratch, which is just another registered root), and the new
  `Assert-NoRetainedScratch` is called after the cleanup `finally` so any record makes the
  process exit non-zero. The intentional internal-evidence retention is not a refusal and is
  not counted. New scenarios `ast-retention-fails-the-gate` (live imported functions, with an
  empty-list positive control) and `static-retention-fails-gate` (the call exists and the list
  is populated) pin this.

### C-3 — source and hash corrections

Section 7 now names the actual fail-first harness (`0169d85`, blob `8b3f272`, **42/11** diff
against the final `5b2e8a25`) and states that the "queued before pop" window has no fail-first
evidence from the final harness. Section 10 no longer claims that `final-state.json` hashed
every log: `gate-timeline.log`, `git-diff-check.log` and `git-status-short.log` are excluded.
The LF/CRLF 口径 table above gives both byte forms for each candidate script; the probe
execution copies are explicitly recorded as not preserved and their numbers as not
reproducible from the repository.

### C-4 — default scratch from an empty `target/`

The driver remembers whether `-Scratch` was supplied. Only its own default
`target/tmp/plan0014-wave5b` may have a missing parent created; an explicit caller path is
still refused. The new `entry-c4-default-scratch-from-empty-target` scenario parks any
existing `target/tmp`, starts the real driver with the default scratch and a fail-fast
evidence directory, and requires it to pass preflight and create the default parent.

### Gates for this remediation

See `RUN-MANIFEST.md` for raw commands, wall-clock stamps and counters. `cargo fmt --all --
--check` exit 0, `scripts/check-architecture.ps1` exit 0 (`Architecture fitness: PASS`),
driver `113 cases / 112 matched / 1 BLOCKED / 0 mismatched` exit 0, repair sweep
`executed=33 skipped=0 failed=0 blocked=0 declared=33` exit 0, `module-release-tool` **108
passed / 0 failed / 0 ignored** (12 result blocks, re-run on a standalone clone because this
linked worktree's `.git` is a file and three `source_derivation` tests assume a directory),
`runtime-migration` **1 passed / 0 failed / 0 ignored** exit 0. Two driver runs in this deep
worktree needed support: a short scratch path for the
standalone gate, and a scoped `GIT_CONFIG_* core.longpaths=true` environment override for
the harness (the default path exceeds Windows MAX_PATH during the scratch clone). Neither
changes repository code or configuration. The default-path run is separately covered by the
C-4 scenario.

NOT RUN here: full-workspace `cargo check/clippy/test --workspace --all-targets
--all-features`, `check-openapi.ps1`, the 70 ignored external-service tests, Linux symlink
coverage, file symlinks, mount points, ReFS/network volumes, secret/licence/vulnerability
scans and a real multi-process adversary. **R2 admission remains BLOCKED**; this section is
remediation evidence for re-review, not an acceptance claim.

End state: **READY FOR RE-REVIEW / NOT INTEGRATED. R2 BLOCKED / NOT CLAIMED.** No push, no
merge, no deploy, no integration.

