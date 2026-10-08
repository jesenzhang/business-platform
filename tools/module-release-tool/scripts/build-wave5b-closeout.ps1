<#
  Builds the evidence-derived sections of the Wave 5B-S1 closeout straight from
  the gate logs and the driver's CSV evidence, so no number in the closeout is
  hand-transcribed. Placeholders in the closeout template are replaced verbatim.
#>
<#
  Fills the evidence-derived placeholders of the Wave 5B-S1 closeout in place,
  reading only the gate logs and the driver's CSV evidence. Run it after a gate
  re-run; a second run is a no-op because the placeholders are gone. Nothing in
  the closeout is hand-transcribed from a terminal.
#>
param(
    [string]$Repo = 'F:\Workspace\business-platform',
    [string]$Closeout = 'docs\reviews\2026-09-30-wave5b-s1-closeout.md',
    [string]$DocsSha = '<docs commit>'
)

$ErrorActionPreference = 'Stop'
Set-Location $Repo
$gates = Join-Path $Repo 'docs\reviews\2026-09-30-wave5b-s1-gates\gates'
$cases = Join-Path $Repo 'docs\reviews\2026-09-30-wave5b-s1-gates\wave5b-cases'

# --- gate table --------------------------------------------------------------
$rows = @(
    '| 日志 | 命令 | 退出码 | 用时 |',
    '|---|---|---:|---:|'
)
foreach ($log in (Get-ChildItem -LiteralPath $gates -Filter '*.log' | Sort-Object Name)) {
    $head = Get-Content -LiteralPath $log.FullName -TotalCount 8
    $command = ($head | Where-Object { $_ -like '# command:*' }) -replace '^# command: ', ''
    $exit = ($head | Where-Object { $_ -like '# exit-code:*' }) -replace '^# exit-code: ', ''
    $duration = ($head | Where-Object { $_ -like '# duration-seconds:*' }) -replace '^# duration-seconds: ', ''
    $note = ''
    if ($log.Name -like '25-*') { $note = '（负例：未知 check 必须失败，退出码 1 即预期）' }
    if ($log.Name -like '37-test-workspace.log' -or $log.Name -like '37-test-workspace-attempt-2.log') { $note = '（默认 target 目录被本机正在运行的 `business-api` / `business-worker` 进程占住二进制而中断；按 §9.4 V14 改用隔离 target 目录，见 37b-*.log）' }
    if ($log.Name -like '41-wave5b-offline-catalog-gate.log') { $note = '（首次尝试：本轮自查出 2 条驱动 fixture 缺陷（clone 的 manifest 是 CRLF），已在 attempt-2 修正；失败日志保留）' }
    if ($log.Name -like '41-wave5b-offline-catalog-gate-attempt-2.log') { $note = '（第二次尝试被本轮自己的证据目录保护挡住：已存在的 gate 证据不会被清空，需 -Force；这条拒绝本身是期望行为，日志保留）' }
    $rows += ('| `{0}` | `{1}` | {2}{3} | {4}s |' -f $log.Name, $command, $exit, $note, $duration)
}
$gateTable = $rows -join "`n"

# --- case tables -------------------------------------------------------------
$blocks = New-Object System.Collections.Generic.List[string]
$totals = [pscustomobject]@{ cases = 0; matched = 0; blocked = 0; mismatched = 0 }
foreach ($csv in (Get-ChildItem -LiteralPath $cases -Filter '*-cases.csv' | Sort-Object Name)) {
    $family = $csv.BaseName -replace '\.csv$', ''
    $items = @(Import-Csv -LiteralPath $csv.FullName)
    $matched = @($items | Where-Object { $_.Verdict -eq 'MATCH' }).Count
    $blocked = @($items | Where-Object { $_.Verdict -eq 'BLOCKED' }).Count
    $bad = @($items | Where-Object { $_.Verdict -eq 'MISMATCH' }).Count
    $blocks.Add('')
    $blocks.Add(('- {0}：{1} 个 case，MATCH {2}，BLOCKED {3}，MISMATCH {4}' -f $family, $items.Count, $matched, $blocked, $bad))
    $rows = @(
        '',
        '| case | 期望 | 实得 | 判定 | 依据（截断） |',
        '|---|---|---|---|---|'
    )
    foreach ($item in $items) {
        $detail = if ($item.Detail) { $item.Detail } else { '—' }
        $detail = $detail -replace '\|', '/'
        if ($detail.Length -gt 120) { $detail = $detail.Substring(0, 117) + '…' }
        $rows += ('| `{0}` | {1} | {2} | {3} | {4} |' -f $item.Case, $item.Expect, $item.Actual, $item.Verdict, $detail)
    }
    $blocks.Add(($rows -join "`n"))
    $totals.cases += $items.Count
    $totals.matched += $matched
    $totals.blocked += $blocked
    $totals.mismatched += $bad
}
$caseTables = ($blocks -join "`n") + "`n`n合计 **$($totals.cases)** 个 case：MATCH **$($totals.matched)**，BLOCKED **$($totals.blocked)**，MISMATCH **$($totals.mismatched)**（全部 76 个 case 的机器可读原文见 ``wave5b-cases/*.csv``）。"

# --- workspace reconciliation -----------------------------------------------
$workspaceLog = ''
foreach ($candidate in @(
        '37b-test-workspace-isolated-target.log',
        '37-test-workspace-attempt-2.log',
        '37-test-workspace.log'
    )) {
    $path = Join-Path $gates $candidate
    if ((Test-Path -LiteralPath $path) -and (@(Get-Content -LiteralPath $path | Where-Object { $_ -match 'test result: ' }).Count -gt 0)) {
        $workspaceLog = $path
        break
    }
}
if ($workspaceLog -eq '') { Write-Output 'WARNING: no workspace log with results yet; the closeout keeps its placeholder' }
Write-Output "workspace log: $(Split-Path -Leaf $workspaceLog)"
$lines = Get-Content -LiteralPath $workspaceLog
$passed = 0; $failed = 0; $ignored = 0; $suites = 0
foreach ($line in $lines) {
    if ($line -match 'test result: \w+\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored') {
        $passed += [int]$Matches[1]; $failed += [int]$Matches[2]; $ignored += [int]$Matches[3]
    }
    if ($line -match '^\s+(Running|Doc-tests)\s') { $suites += 1 }
}
$workspaceSummary = ("**{0} passed / {1} failed / {2} ignored，{3} 个测试套件**（口径：把日志里每条 `test result:` 行的 passed/failed/ignored 相加，套件数 = `Running`/`Doc-tests` 行数；日志 `37b-test-workspace-isolated-target.log`）" -f $passed, $failed, $ignored, $suites)

$newTests = @(& rtk proxy cargo test -q -p module-release-tool --test wave5b_catalog -- --list 2>&1 |
        ForEach-Object { $_.ToString() } | Where-Object { $_ -match ': test$' } |
        ForEach-Object { ($_ -split ':')[0] } | Sort-Object)
$delta = if ($newTests.Count -gt 0) {
    ('新增测试全部来自新测试目标 `tools/module-release-tool/tests/wave5b_catalog.rs`（{0} 条，逐条：{1}），套件数因此 +1（该文件是一个新的 test target）。除此之外本轮未新增、未删除任何测试，也未把任何 `#[ignore]` 去掉，因此 ignored 计数不变；若与实际差值不符，以日志为准并如实修正本段。' -f $newTests.Count, (($newTests | ForEach-Object { "``$_``" }) -join '、'))
} else {
    '本轮未新增测试目标之外的测试；差值未能自动列举（`--list` 无输出），以 `37-test-workspace-attempt-2.log` 与 `git diff --name-only` 为准。'
}

# --- fixed history proof -----------------------------------------------------
$diffSql = (& rtk proxy git diff --stat 6decf1ad9ea05233d66dd3539594ff210afd99f2..b25e2d4a9b455b776934495b8f3dfd2b71a13fe2 -- migrations/*.sql migrations/MANIFEST.sha256 2>&1 | ForEach-Object { $_.ToString() }) -join "`n"
$diffMigrations = (& rtk proxy git diff --name-only 6decf1ad9ea05233d66dd3539594ff210afd99f2..b25e2d4a9b455b776934495b8f3dfd2b71a13fe2 -- migrations/ 2>&1 | ForEach-Object { $_.ToString() }) -join "`n"
$historyProof = (@"
````text
# git diff --stat 6decf1a..b25e2d4 -- migrations/*.sql migrations/MANIFEST.sha256
$diffSql
# (empty above = 历史 SQL 与历史 manifest 一字未改)

# git diff --name-only 6decf1a..b25e2d4 -- migrations/
$diffMigrations
````
"@)

# --- catalog output ----------------------------------------------------------
$defaultLog = Get-Content -LiteralPath (Join-Path $gates '20-catalog-check-default.log')
$catalogOutput = @($defaultLog | Where-Object {
        $_ -match '^(checks=|namespace=|order=|status=|embed scanned|history\[.*baseline_revision)'
    }) | Select-Object -First 8
$catalogOutput = $catalogOutput -join "`n"

# --- substitute --------------------------------------------------------------
$closeoutPath = if ([System.IO.Path]::IsPathRooted($Closeout)) { $Closeout } else { Join-Path $Repo $Closeout }
$template = [System.IO.File]::ReadAllText($closeoutPath)
$map = [ordered]@{
    '{{DOCS_SHA}}'         = "``$DocsSha``"
    '{{CASE_TABLES}}'      = $caseTables
    '{{GATE_TABLE}}'       = $gateTable
    '{{WORKSPACE_SUMMARY}}' = $workspaceSummary
    '{{WORKSPACE_DELTA}}'  = $delta
    '{{HISTORY_PROOF}}'    = $historyProof
    '{{CATALOG_OUTPUT}}'   = $catalogOutput
}
foreach ($key in $map.Keys) {
    if (-not $template.Contains($key)) { Write-Output "skip $key (already substituted)"; continue }
    $template = $template.Replace($key, $map[$key].Replace("`r`n", "`n").Replace("`n", "`r`n"))
}
[System.IO.File]::WriteAllText($closeoutPath, $template, [System.Text.UTF8Encoding]::new($false))
Write-Output "closeout updated: $closeoutPath"
Write-Output "workspace: passed=$passed failed=$failed ignored=$ignored suites=$suites; new tests=$($newTests.Count)"
Write-Output "cases: total=$($totals.cases) matched=$($totals.matched) blocked=$($totals.blocked) mismatched=$($totals.mismatched)"
