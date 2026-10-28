# Independent re-run of the Wave 5B-S1 gates - raw manifest

Runner: independent gate runner (delegated agent), strict serial execution, one command at a time.
Repository: `F:\Workspace\business-platform`
HEAD at run: `325d249e744e499796db5dbeecf486488c5dc9bc` - `325d249 docs(reports): record the 2026-10-22 workspace audit and cleanup`
Host: Windows, PowerShell 7.6.7, git 2.50.1.windows.1

Preflight (2026-10-22T13:07, before step 1):
- `Test-Path target\tmp` -> True
- `Get-Process cargo,rustc` -> none running; no build in progress, no wait required
- `git status --porcelain` -> only untracked `??` entries; no tracked file modified

Wall clock: first command 13:07:43.321, last command finished 13:23:00.413 (+08:00).
Sum of command durations: 860.671 s (14 m 20.7 s).

## Results

| # | Command | Exit | Duration (s) | Start -> Stop | Key counters |
|---|---------|------|--------------|---------------|--------------|
| 1 | `cargo fmt --all -- --check` | 0 | 1.940 | 13:07:43.321 -> 13:07:45.265 | no output (clean) |
| 2 | `pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | 14.665 | 13:07:55.756 -> 13:08:10.425 | Cargo metadata architecture fitness: PASS; OpenAPI contract: PASS; Architecture fitness: PASS |
| 3 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -EvidenceDir docs/reviews/2026-10-22-independent-rereview-gates/driver113` | 0 | 285.983 | 13:08:29.259 -> 13:13:15.246 | cases=113 matched=112 blocked=1 mismatched=0 |
| 4 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b-repairs.ps1 -OutDir docs/reviews/2026-10-22-independent-rereview-gates/repair-scenarios` | 0 | 296.084 | 13:13:24.398 -> 13:18:20.486 | executed=29 skipped=0 failed=0 blocked=0 declared=29 |
| 5 | `cargo test -p module-release-tool --all-features` | 0 | 229.170 | 13:18:27.969 -> 13:22:17.141 | passed=108 failed=0 ignored=0 (12 suites) |
| 6 | `cargo test -p runtime-migration --all-features` | 0 | 32.829 | 13:22:27.580 -> 13:23:00.413 | passed=1 failed=0 ignored=0 (2 suites) |

## Step 3 per-family counters

```
summary catalog-cases cases=88 matched=88 blocked=0 mismatched=0
summary history-cases cases=11 matched=11 blocked=0 mismatched=0
summary embed-cases   cases=7  matched=7  blocked=0 mismatched=0
summary freeze-cases  cases=5  matched=5  blocked=0 mismatched=0
summary path-cases    cases=2  matched=1  blocked=1 mismatched=0
summary total         cases=113 mismatched=0 blocked=1
```

Blocked case (verbatim):

```
  blocked [path-cases] P2-catalog-symlink: the host refused to create a file symbolic link: 此操作需要管理员权限。 (the repository volume is exFAT, which holds no reparse points)
```

Final line: `Wave 5B-S1 offline catalog gate: every case that could run matched its expectation; 1 case(s) are BLOCKED by this platform and are recorded as such`

## Step 4 summary (verbatim)

```
NOTE: driver reported platform-BLOCKED cases (recorded, not failures)
repair-regressions: executed=29 skipped=0 failed=0 blocked=0 declared=29; logs in docs/reviews/2026-10-22-independent-rereview-gates/repair-scenarios
Wave 5B-S1 repair regressions: all 29 executed scenarios matched their expectation.
```

## Step 5 per-suite results

```
lib                 running 31 tests  ok. 31 passed; 0 failed; 0 ignored
main                running  0 tests  ok.  0 passed; 0 failed; 0 ignored
canonical           running  5 tests  ok.  5 passed; 0 failed; 0 ignored
compatibility       running  8 tests  ok.  8 passed; 0 failed; 0 ignored
isolation           running  8 tests  ok.  8 passed; 0 failed; 0 ignored
packaging           running  3 tests  ok.  3 passed; 0 failed; 0 ignored
pilot_candidates    running 10 tests  ok. 10 passed; 0 failed; 0 ignored
provenance          running  6 tests  ok.  6 passed; 0 failed; 0 ignored
source_derivation   running  9 tests  ok.  9 passed; 0 failed; 0 ignored
verification_guards running  8 tests  ok.  8 passed; 0 failed; 0 ignored
wave5b_catalog      running 20 tests  ok. 20 passed; 0 failed; 0 ignored
doc-tests           running  0 tests  ok.  0 passed; 0 failed; 0 ignored
```

Informational line inside step 5 (not a failure): `test the_descriptive_records_must_describe_one_module_and_version has been running for over 60 seconds`

## Step 6 result

```
lib       running 1 test   ok. 1 passed; 0 failed; 0 ignored    (test tests::classifies_empty_behind_equal_and_ahead)
doc-tests running 0 tests  ok. 0 passed; 0 failed; 0 ignored
```

## Evidence files

| Path | Content |
|------|---------|
| `01-cargo-fmt.log` | step 1 stdout/stderr (0 bytes - clean) |
| `02-check-architecture.log` | step 2 stdout/stderr (40 lines) |
| `03-driver113-wave5b.log` | step 3 stdout/stderr (285 lines) |
| `04-repair-scenarios.log` | step 4 stdout/stderr (37 lines) |
| `05-cargo-test-module-release-tool.log` | step 5 stdout/stderr (184 lines) |
| `06-cargo-test-runtime-migration.log` | step 6 stdout/stderr (152 lines) |
| `driver113/` | step 3 machine evidence: catalog-cases.csv, history-cases.csv, embed-cases.csv, freeze-cases.csv, path-cases.csv, catalog-reconciliation.csv, catalog-gate-log.txt |
| `repair-scenarios/` | step 4 per-scenario logs and captured child output |

## Anomalies

1. A tracked file changed during the run, and these gates did not write it.
   `docs/reviews/2026-10-09-wave5b-s1-rereview-repair-closeout.md` was clean at preflight 13:07 and shows ` M` afterwards, LastWriteTime 2026-10-22 13:16:04 - inside step 4's window. The diff is a prose correction of the "2026-10-22 workspace-cleanup note" (7 insertions, 4 deletions). None of the three harness scripts reference this path (a Select-String for `rereview-repair-closeout|probe-ancestor-midwalk|independent-rereview` returned no matches), and their writes are confined to -OutDir, -EvidenceDir, target\tmp and %TEMP%. Most consistent explanation: a concurrent writer sharing this workspace (parent or sibling agent) edited it at 13:16:04. Left untouched. Consequence: `git status --short` / `git diff --check` at this HEAD is not clean.

2. Unexplained artifacts inside this evidence root. `probe-ancestor-midwalk/` (ancestor-midwalk-control.ps1, ancestor-midwalk-probe.ps1 and logs, written 13:12:54 / 13:13:28) was not produced by any command run here and is not referenced by the three harness scripts. Same external-writer explanation as anomaly 1.

3. Step 3 has 1 platform-BLOCKED case, and the exit code is still 0 by design. `P2-catalog-symlink` needs a file symbolic link; the host refused ("此操作需要管理员权限"), and the repository volume is exFAT, which holds no reparse points. The driver therefore keeps a reparse-capable scratch on `C:\Users\admin\AppData\Local\Temp\plan0014-wave5b-links-<guid>` to run `P1-namespace-junction`. Blocked is recorded, never counted as pass. Per-family totals: 112 matched of 113, 1 blocked, 0 mismatched.

4. Scratch written outside the repository by the harnesses. Step 3 used `%TEMP%\plan0014-wave5b-links-<guid>`; step 4 left `%TEMP%\wave5b-reparse-probe` (LastWriteTime 13:13:32). These are harness scratch, not tracked files. The runner deleted nothing outside the evidence directory.

5. Step 3 scratch was self-cleaned. `target\tmp\plan0014-wave5b` still reports LastWriteTime 11:59:46 (pre-run): the run root `run-8f744230...` was removed by the driver's own default cleanup and did not leak.

6. Step 5 emits one git line-ending notice: `warning: in the working copy of 'migrations/001_initial.sql', LF will be replaced by CRLF the next time Git touches it` - informational, from git inside the tests, not a test failure.

7. Step 6 rebuilt about 40 dependency crates (32.6 s) and runs a single unit test. The `--all-features` combination for runtime-migration had not been built yet, hence the rebuild. Factual note only: at this HEAD this crate's test surface is one unit test and zero doc-tests, so its green result is a narrow signal.

8. Tooling note: this manifest was written through PowerShell because the agent filesystem writer uses an atomic hard-link step that fails with EISDIR on the exFAT repository volume.

No command failed. No gate reported a mismatch. Nothing was interpreted or repaired by the runner.
