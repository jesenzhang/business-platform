# PLAN-0014 Wave 5A — orchestration helper regressions

run directory: F:\Workspace\business-platform\target\wave5a-script-regression\20260930T030820Z
command      : pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1
utc          : 20260930T030915Z

| regression | what it pins | verdict | detail |
| --- | --- | --- | --- |
| R1-fresh-directory | a non-empty run directory is refused | PASS | refusing to reuse run directory 'F:\Workspace\business-platform\target\wave5a-script-regression\20260930T030820Z\stale-run': it already exists and is not empty. Evidence is immutable — start a new run instead of overwriting history. |
| R2-json-edit-no-op | a JSON mutation that changes nothing is refused | PASS | a mutation that does nothing did not change 'F:\Workspace\business-platform\target\wave5a-script-regression\20260930T030820Z\record-copy\release.json'; the negative case would assert nothing. |
| R2-json-edit-lands | a JSON mutation that does change the record is accepted | PASS |  |
| R3a-wrong-reason-detected | a case expecting the wrong check id is verdict WRONG REASON | PASS | verdict=WRONG REASON observed=artifact-record-format |
| R3b-correct-refusal | a case expecting the right check id is verdict OK | PASS | verdict=OK observed=artifact-record-format |
| R4-baseline-not-green | a case whose pristine copy fails baseline verify is verdict BROKEN | PASS | verdict=BROKEN |
| R5a-guard-passes-when-isolated | exit 0 on an isolated resolved graph | PASS | exit=0 |
| R5b-guard-refuses-repo-path | exit 1 with consumer-graph-isolation, naming the package and path | PASS | exit=1 check=consumer-graph-isolation |
| R6-cleanup-scoped | cleanup outside the run directory is refused | PASS | refusing to delete 'F:\Workspace\business-platform\target\wave5a-script-regression': it is not inside this run's own scratch root 'F:\Workspace\business-platform\target\wave5a-script-regression\20260930T030820Z' |
| R7a-integrity-recompute-matches-the-tool | the script re-implementation reproduces every recorded member digest and the integrity digest | PASS | 21 member(s), digest 68deba81c942… |
| R7b-rehashed-packed-manifest | exit 1 refused by derived-members-match-revision, not by artifact-integrity-digest | PASS | exit=1 check=derived-members-match-revision record-rewritten=True |
| R8a-isolated-checkout-control | exit 0 from a checkout that only contains the release inputs (26 files) | PASS | exit=0 files=26 |
| R8b-dropped-committed-source | exit 1 by source-member-set-matches-revision and no artifact directory created | PASS | exit=1 check=source-member-set-matches-revision artifact-directory-created=False |
| R8c-uncommitted-extra-source | exit 1 by source-member-set-matches-revision and no artifact directory created | PASS | exit=1 check=source-member-set-matches-revision artifact-directory-created=False |

RESULT: all helper regressions behaved as required

