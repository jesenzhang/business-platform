# PLAN-0014 Wave 5A — orchestration helper regressions

run directory: F:\Workspace\business-platform\target\wave5a-script-regression\20260929T080515Z
command      : pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1
utc          : 20260929T080530Z

| regression | what it pins | verdict | detail |
| --- | --- | --- | --- |
| R1-fresh-directory | a non-empty run directory is refused | PASS | refusing to reuse run directory 'F:\Workspace\business-platform\target\wave5a-script-regression\20260929T080515Z\stale-run': it already exists and is not empty. Evidence is immutable — start a new run instead of overwriting history. |
| R2-json-edit-no-op | a JSON mutation that changes nothing is refused | PASS | a mutation that does nothing did not change 'F:\Workspace\business-platform\target\wave5a-script-regression\20260929T080515Z\record-copy\release.json'; the negative case would assert nothing. |
| R2-json-edit-lands | a JSON mutation that does change the record is accepted | PASS |  |
| R3a-wrong-reason-detected | a case expecting the wrong check id is verdict WRONG REASON | PASS | verdict=WRONG REASON observed=artifact-record-format |
| R3b-correct-refusal | a case expecting the right check id is verdict OK | PASS | verdict=OK observed=artifact-record-format |
| R4-baseline-not-green | a case whose pristine copy fails baseline verify is verdict BROKEN | PASS | verdict=BROKEN |
| R5a-guard-passes-when-isolated | exit 0 on an isolated resolved graph | PASS | exit=0 |
| R5b-guard-refuses-repo-path | exit 1 with consumer-graph-isolation, naming the package and path | PASS | exit=1 check=consumer-graph-isolation |
| R6-cleanup-scoped | cleanup outside the run directory is refused | PASS | refusing to delete 'F:\Workspace\business-platform\target\wave5a-script-regression': it is not inside this run's own scratch root 'F:\Workspace\business-platform\target\wave5a-script-regression\20260929T080515Z' |

RESULT: all helper regressions behaved as required
