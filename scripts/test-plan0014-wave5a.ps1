# Regression entry for the PLAN-0014 Wave 5A orchestration helpers.
#
# The Wave 5A review (F5) found negative cases that were not testing what they
# claimed: one case's damage came from the previous case, another edited JSON with
# a string replacement, and every case accepted "some non-zero exit" as a refusal.
# This script is the repeatable regression entry for that machinery. It exercises
# the same helpers the orchestration uses (dot-sourced from
# scripts/lib/plan0014-wave5a-lib.ps1), including the *self-tests*: a negative case
# whose injection silently does nothing, or whose refusal arrives under a different
# check, has to be reported as a failure by the harness. If those self-tests ever
# pass silently, the negatives are decorative again.
#
# Run it with:
#   pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1
# and, to keep the log as evidence:
#   pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1 -EvidenceDir docs\reviews\<dir>

[CmdletBinding()]
param(
    [string]$OutRoot = '',
    [string]$EvidenceDir = '',
    [string]$Module = 'identity',
    [string]$ModuleVersion = '0.1.0-rc.1'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/plan0014-wave5a-lib.ps1')

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$stamp = Get-UtcStamp
if ([string]::IsNullOrWhiteSpace($OutRoot)) {
    $OutRoot = Join-Path $repo "target\wave5a-script-regression\$stamp"
}
$runRoot = New-FreshDirectory -Path $OutRoot -What 'regression run directory'
if (-not [string]::IsNullOrWhiteSpace($EvidenceDir)) {
    if (-not (Split-Path -IsAbsolute $EvidenceDir)) {
        $EvidenceDir = Join-Path $repo $EvidenceDir
    }
    $EvidenceDir = New-FreshDirectory -Path $EvidenceDir -What 'regression evidence directory'
}
else {
    $EvidenceDir = $runRoot
}

$State = @{
    Repo         = $repo
    Tool         = Join-Path $repo 'target\debug\module-release-tool.exe'
    Failures     = @()
    LogIndex     = @()
    Cases        = @()
    NegativeRoot = Join-Path $runRoot 'negatives'
}
Ensure-Directory $State.NegativeRoot
$null = Invoke-Step -Name 'build-tool' -Title 'regression: build the release tool' -Exe 'cargo' `
    -Arguments @('build', '-p', 'module-release-tool') -LogFile (Join-Path $runRoot '00-build-tool.log') `
    -Blocking -State $State

$null = Invoke-Step -Name 'candidate' -Title "regression: generate one $Module candidate to damage" `
    -Arguments @('generate', '--module', $Module, '--out', (Join-Path $runRoot 'candidate')) `
    -LogFile (Join-Path $runRoot '01-generate.log') -Blocking -State $State
$pristine = Join-Path $runRoot "candidate\$Module-$ModuleVersion"

$results = @()
function Add-Result {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Expectation,
        [Parameter(Mandatory = $true)][bool]$Passed,
        [string]$Detail = ''
    )
    $script:results += [pscustomobject]@{
        Name = $Name; Expectation = $Expectation; Verdict = $(if ($Passed) { 'PASS' } else { 'FAIL' }); Detail = $Detail
    }
}

function Test-Throws {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Expectation,
        [Parameter(Mandatory = $true)][string]$ExpectPattern,
        [Parameter(Mandatory = $true)][scriptblock]$Body
    )
    $caught = ''
    $threw = $false
    try {
        & $Body | Out-Null
    }
    catch {
        $threw = $true
        $caught = $_.Exception.Message
    }
    $ok = $threw -and $caught -match $ExpectPattern
    Add-Result -Name $Name -Expectation $Expectation -Passed $ok `
        -Detail $(if ($threw) { $caught } else { 'no exception was raised at all' })
}

# R1 — evidence and run directories are never reused silently.
$stale = Join-Path $runRoot 'stale-run'
Ensure-Directory $stale
Set-Content -LiteralPath (Join-Path $stale 'previous-run.log') -Value 'old evidence' -Encoding utf8
Test-Throws -Name 'R1-fresh-directory' -Expectation 'a non-empty run directory is refused' `
    -ExpectPattern 'refusing to reuse' -Body { New-FreshDirectory -Path $stale -What 'run directory' }

# R2 — an injection that does not change the record must be fatal, not a pass.
$recordCopy = Join-Path $runRoot 'record-copy'
Copy-Tree -From $pristine -To $recordCopy
Test-Throws -Name 'R2-json-edit-no-op' -Expectation 'a JSON mutation that changes nothing is refused' `
    -ExpectPattern 'did not change' -Body {
        Set-JsonRecord -Path (Join-Path $recordCopy 'release.json') -Describe 'a mutation that does nothing' -Edit {
            param($value) $value.module_id = $value.module_id
        }
    }
$landed = $true
$landedDetail = ''
try {
    $change = Set-JsonRecord -Path (Join-Path $recordCopy 'release.json') -Describe 'a real mutation' -Edit {
        param($value) $value.module_version = '9.9.9'
    }
    if ($change.Before -eq $change.After) {
        $landed = $false
        $landedDetail = 'Before and After serialised identically'
    }
}
catch {
    $landed = $false
    $landedDetail = $_.Exception.Message
}
Add-Result -Name 'R2-json-edit-lands' -Expectation 'a JSON mutation that does change the record is accepted' `
    -Passed $landed -Detail $landedDetail

# R3 — the harness itself: a refusal under the wrong check, and a correct refusal.
$null = Invoke-NegativeCase -State $State -Name 'r3a-wrong-check' `
    -Title 'self-test: a refusal under a different check must be reported, not accepted' `
    -SourceArtifact $pristine -ExpectCheck 'this-check-does-not-exist' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'artifact-manifest.json') -Describe 'unknown algorithm' -Edit {
            param($value) $value.artifact_integrity_algorithm = 'unknown-algorithm.v9'
        } | Out-Null
    }
$wrong = $State.Cases[-1]
Add-Result -Name 'R3a-wrong-reason-detected' `
    -Expectation 'a case expecting the wrong check id is verdict WRONG REASON' `
    -Passed ($wrong.Verdict -eq 'WRONG REASON' -and $wrong.Observed -eq 'artifact-record-format') `
    -Detail "verdict=$($wrong.Verdict) observed=$($wrong.Observed)"
$State.Failures = @()

$null = Invoke-NegativeCase -State $State -Name 'r3b-right-check' `
    -Title 'self-test: the same fault with the right expectation is a passing negative' `
    -SourceArtifact $pristine -ExpectCheck 'artifact-record-format' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'artifact-manifest.json') -Describe 'unknown algorithm' -Edit {
            param($value) $value.artifact_integrity_algorithm = 'unknown-algorithm.v9'
        } | Out-Null
    }
$right = $State.Cases[-1]
Add-Result -Name 'R3b-correct-refusal' `
    -Expectation 'a case expecting the right check id is verdict OK' `
    -Passed ($right.Verdict -eq 'OK') -Detail "verdict=$($right.Verdict) observed=$($right.Observed)"
$State.Failures = @()

# R4 — the baseline is not decoration: a copy that cannot verify is BROKEN.
$brokenSource = Join-Path $runRoot 'already-damaged'
Copy-Tree -From $pristine -To $brokenSource
Add-Content -LiteralPath (Join-Path $brokenSource 'package\CHANGELOG.md') -Value 'damage' -Encoding utf8
$null = Invoke-NegativeCase -State $State -Name 'r4-broken-baseline' `
    -Title 'self-test: a case starting from a candidate that does not verify is BROKEN, never OK' `
    -SourceArtifact $brokenSource -ExpectCheck 'artifact-integrity-digest' -Inject {
        param($copy)
    }
$broken = $State.Cases[-1]
Add-Result -Name 'R4-baseline-not-green' `
    -Expectation 'a case whose pristine copy fails baseline verify is verdict BROKEN' `
    -Passed ($broken.Verdict -eq 'BROKEN') -Detail "verdict=$($broken.Verdict)"
$State.Failures = @()

# R5 — the isolation guard, through the real CLI, on a metadata document.
$isoRoot = Join-Path $runRoot 'isolation'
$consumer = Join-Path $isoRoot 'consumer'
$artifactCrate = Join-Path $isoRoot 'artifact\contract\packages\identity'
$forbidden = Join-Path $isoRoot 'repo\crates\identity'
foreach ($directory in @($consumer, $artifactCrate, $forbidden)) {
    Ensure-Directory $directory
    Set-Content -LiteralPath (Join-Path $directory 'Cargo.toml') -Value "[package]`n" -Encoding utf8
}
function New-MetadataDocument {
    param([string]$Root, [object[]]$Packages)
    @{
        packages        = $Packages
        workspace_root  = $Root
        version         = 1
    } | ConvertTo-Json -Depth 20
}
$cleanMetadata = New-MetadataDocument -Root $consumer -Packages @(
    [pscustomobject]@{ name = 'consumer'; version = '0.1.0'; source = $null; manifest_path = (Join-Path $consumer 'Cargo.toml') },
    [pscustomobject]@{ name = 'identity'; version = '0.1.0-rc.1'; source = $null; manifest_path = (Join-Path $artifactCrate 'Cargo.toml') },
    [pscustomobject]@{ name = 'chrono'; version = '0.4.45'; source = 'registry+https://github.com/rust-lang/crates.io-index'; manifest_path = '' }
)
$cleanFile = Join-Path $isoRoot 'metadata-clean.json'
$cleanMetadata | Set-Content -LiteralPath $cleanFile -Encoding utf8
$clean = Invoke-Step -Name 'R5-guard-clean' -Title 'guard: an isolated consumer graph passes' `
    -Arguments @('guard', '--metadata', $cleanFile, '--consumer', $consumer, '--repo', $repo, '--artifact', $artifactCrate) `
    -LogFile (Join-Path $isoRoot 'guard-clean.log') -State $State
Add-Result -Name 'R5a-guard-passes-when-isolated' -Expectation 'exit 0 on an isolated resolved graph' `
    -Passed ($clean.Exit -eq 0) -Detail "exit=$($clean.Exit)"

$leaked = ($cleanMetadata | ConvertFrom-Json)
$leaked.packages[1].manifest_path = (Join-Path $repo 'crates\identity\Cargo.toml') -replace '\\', '/'
$leakedFile = Join-Path $isoRoot 'metadata-leaked.json'
($leaked | ConvertTo-Json -Depth 20) | Set-Content -LiteralPath $leakedFile -Encoding utf8
$guardRun = Invoke-Step -Name 'R5-guard-leak' -Title 'guard: a forward-slash path dependency into the repository is refused' `
    -Arguments @('guard', '--metadata', $leakedFile, '--consumer', $consumer, '--repo', $repo, '--artifact', $artifactCrate) `
    -LogFile (Join-Path $isoRoot 'guard-leak.log') -ExpectExit 1 -State $State
$leakCheck = if ($guardRun.Log -match '\[([a-z0-9-]+)\]') { $Matches[1] } else { 'no check identifier' }
Add-Result -Name 'R5b-guard-refuses-repo-path' `
    -Expectation 'exit 1 with consumer-graph-isolation, naming the package and path' `
    -Passed ($guardRun.Exit -eq 1 -and $leakCheck -eq 'consumer-graph-isolation' -and $guardRun.Log -match 'crates') `
    -Detail "exit=$($guardRun.Exit) check=$leakCheck"
$State.Failures = @()

# R6 — scratch cleanup cannot escape this run's own directory.
$escape = (Split-Path -Parent $runRoot)
Test-Throws -Name 'R6-cleanup-scoped' -Expectation 'cleanup outside the run directory is refused' `
    -ExpectPattern 'not inside' -Body { Remove-OwnScratch -Path $escape -OwnRoot $runRoot }

# ---------------------------------------------------------------- report ------
$report = @(
    '# PLAN-0014 Wave 5A — orchestration helper regressions',
    '',
    "run directory: $runRoot",
    "command      : pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1",
    "utc          : $(Get-UtcStamp)",
    '',
    '| regression | what it pins | verdict | detail |',
    '| --- | --- | --- | --- |'
)
foreach ($result in $results) {
    $report += ('| {0} | {1} | {2} | {3} |' -f $result.Name, $result.Expectation, $result.Verdict, $result.Detail)
}
$failed = @($results | Where-Object { $_.Verdict -eq 'FAIL' })
$report += @(
    '',
    "RESULT: $(if ($failed.Count -eq 0) { 'all helper regressions behaved as required' } else { "$($failed.Count) regression(s) failed" })",
    ''
)
$report | Set-Content -LiteralPath (Join-Path $EvidenceDir 'script-regression-log.md') -Encoding utf8
$results | Format-Table -AutoSize | Out-String -Width 160 | Write-Host
if ($failed.Count -ne 0) {
    exit 1
}
Write-Host "Wave 5A orchestration regressions passed. Log: $EvidenceDir"
exit 0