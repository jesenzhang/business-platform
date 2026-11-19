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

# R7 — the orchestrator re-implements the payload digest rule only so that a negative
# case can build a payload that is *self-consistent* with a damaged member (review
# F1-A). A re-implementation that quietly disagrees with the tool would turn every
# such case into a test of the wrong thing, so the re-implementation is pinned here
# first: on a freshly generated candidate it has to reproduce the recorded member
# digests, in the recorded order, and the recorded integrity digest.
$integrityCopy = Join-Path $runRoot 'integrity-copy'
Copy-Tree -From $pristine -To $integrityCopy
$recorded = Get-Content -Raw -LiteralPath (Join-Path $integrityCopy 'artifact-manifest.json') | ConvertFrom-Json
$recomputed = Set-IntegrityRecord -Artifact $integrityCopy -DryRun
$differences = @()
$recordedFiles = @($recorded.files)
if ($recordedFiles.Count -ne $recomputed.Files.Count) {
    $differences += "member count: recorded $($recordedFiles.Count), recomputed $($recomputed.Files.Count)"
}
$compareTo = $recordedFiles.Count
if ($recomputed.Files.Count -lt $compareTo) { $compareTo = $recomputed.Files.Count }
for ($index = 0; $index -lt $compareTo; $index++) {
    $want = $recordedFiles[$index]
    $got = $recomputed.Files[$index]
    if ($want.path -ne $got.path) {
        $differences += "order at $index : recorded '$($want.path)', recomputed '$($got.path)'"
    }
    if ($want.sha256 -ne $got.sha256) {
        $differences += "digest of '$($want.path)' : recorded $($want.sha256), recomputed $($got.sha256)"
    }
    if ([string] $want.bytes -ne [string] $got.bytes) {
        $differences += "size of '$($want.path)' : recorded $($want.bytes), recomputed $($got.bytes)"
    }
}
if ($recorded.artifact_integrity_digest -ne $recomputed.Digest) {
    $differences += "integrity digest: recorded $($recorded.artifact_integrity_digest), recomputed $($recomputed.Digest)"
}
Add-Result -Name 'R7a-integrity-recompute-matches-the-tool' `
    -Expectation 'the script re-implementation reproduces every recorded member digest and the integrity digest' `
    -Passed ($differences.Count -eq 0 -and -not $recomputed.Changed) `
    -Detail $(if ($differences.Count -eq 0) { "$($recomputed.Files.Count) member(s), digest $($recomputed.Digest.Substring(0, 12))…" } else { $differences -join '; ' })

# R7b — and the re-implementation is strong enough to defeat the physical check: a
# packed `Cargo.toml` whose dependency version no committed manifest can state, with
# the record recomputed around it, must now be refused by the derivation check rather
# than by the integrity digest. This is Repair 1 finding F1-A, run through the same
# injection the pilot uses.
$f1aCopy = Join-Path $runRoot 'f1a-copy'
Copy-Tree -From $pristine -To $f1aCopy
$packedManifest = Join-Path $f1aCopy "package\contract\packages\$Module\Cargo.toml"
$manifestText = Get-Content -Raw -LiteralPath $packedManifest
$tamperedManifest = $manifestText -replace 'async-trait = \{ version = "=0\.1\.92"' , 'async-trait = { version = "=99.0.0"'
if ($tamperedManifest -eq $manifestText) {
    throw 'the F1-A injection did not change the packed manifest; the case would assert nothing'
}
Set-Content -LiteralPath $packedManifest -Value $tamperedManifest -NoNewline -Encoding utf8
$rehashed = Set-IntegrityRecord -Artifact $f1aCopy
$f1aRun = Invoke-Step -Name 'R7b-rehashed-packed-manifest' `
    -Title 'a packed manifest the pinned commit cannot produce, with a consistent record, must be refused' `
    -Arguments @('verify', '--artifact', $f1aCopy) `
    -LogFile (Join-Path $runRoot 'r7b-verify.log') -ExpectExit 1 -State $State
$f1aCheck = if ($f1aRun.Log -match '\[([a-z0-9-]+)\]') { $Matches[1] } else { 'no check identifier' }
Add-Result -Name 'R7b-rehashed-packed-manifest' `
    -Expectation 'exit 1 refused by derived-members-match-revision, not by artifact-integrity-digest' `
    -Passed ($f1aRun.Exit -eq 1 -and $f1aCheck -eq 'derived-members-match-revision' -and $rehashed.Changed) `
    -Detail "exit=$($f1aRun.Exit) check=$f1aCheck record-rewritten=$($rehashed.Changed)"
$State.Failures = @()

# R8 — a second checkout of the same commit, damaged without touching this one.
# Repair 1 enumerated the checkout, so a deleted tracked source disappeared from both
# sides of the comparison at once (finding F1-B); the control below is what makes the
# two refusals mean anything: with the inputs intact the same checkout generates.
$checkout = New-IsolatedCheckout -Repo $repo -Destination (Join-Path $runRoot 'isolated-checkout') -Module $Module
# Any committed file of the packed crate but its root: generation never compiles Rust,
# so the file only has to be one the pinned commit lists.
$crateSource = Join-Path $checkout.Root (((Get-Content -Raw -LiteralPath (Join-Path $repo "tools/module-release-tool/specs/$Module.json") | ConvertFrom-Json).packed_crates[0].source_dir -replace '/', '\') + '\src')
$checkoutSource = @(Get-ChildItem -LiteralPath $crateSource -Recurse -File | Where-Object { $_.Name -ne 'lib.rs' } | Sort-Object -Property FullName)[0]
if (-not $checkoutSource) {
    throw "the isolated checkout of '$Module' has no source file besides lib.rs; R8 cannot be built"
}
$checkoutSourceRelative = $checkoutSource.FullName.Substring($checkout.Root.Length).TrimStart('\')
$controlOut = Join-Path $runRoot 'isolated-control'
$control = Invoke-Step -Name 'R8a-isolated-checkout-control' `
    -Title 'a complete isolated checkout of the pinned commit generates a candidate' `
    -Arguments @('generate', '--module', $Module, '--out', $controlOut, '--repo', $checkout.Root) `
    -WorkDir $checkout.Root `
    -LogFile (Join-Path $runRoot 'r8a-generate-control.log') -State $State
Add-Result -Name 'R8a-isolated-checkout-control' `
    -Expectation "exit 0 from a checkout that only contains the release inputs ($($checkout.Files) files)" `
    -Passed ($control.Exit -eq 0 -and (Test-Path -LiteralPath (Join-Path $controlOut "$Module-$ModuleVersion"))) `
    -Detail "exit=$($control.Exit) files=$($checkout.Files)"

foreach ($shape in @(
        @{ Name = 'R8b-dropped-committed-source'; File = $checkoutSource; Remove = $true },
        @{ Name = 'R8c-uncommitted-extra-source'
            File = Join-Path $crateSource 'plan0014_repair_case_extra.rs'; Remove = $false })) {
    if ($shape.Remove) {
        if (-not (Test-Path -LiteralPath $shape.File)) {
            throw 'the isolated checkout is missing the source file the case removes; the control is not comparable'
        }
        Remove-Item -LiteralPath $shape.File -Force
    }
    else {
        Set-Content -LiteralPath $shape.File -Value "// not committed anywhere`n" -Encoding utf8
    }
    $refusedOut = Join-Path $runRoot ($shape.Name + '-out')
    $run = Invoke-Step -Name $shape.Name `
        -Title "$($shape.Name): generation must refuse, and refuse before creating an artifact" `
        -Arguments @('generate', '--module', $Module, '--out', $refusedOut, '--repo', $checkout.Root) `
        -WorkDir $checkout.Root `
        -LogFile (Join-Path $runRoot ($shape.Name.ToLower() + '.log')) -ExpectExit 1 -State $State
    $check = if ($run.Log -match '\[([a-z0-9-]+)\]') { $Matches[1] } else { 'no check identifier' }
    $artifactDirectory = Join-Path $refusedOut "$Module-$ModuleVersion"
    $created = Test-Path -LiteralPath $artifactDirectory
    Add-Result -Name $shape.Name `
        -Expectation 'exit 1 by source-member-set-matches-revision and no artifact directory created' `
        -Passed ($run.Exit -eq 1 -and $check -eq 'source-member-set-matches-revision' -and -not $created) `
        -Detail "exit=$($run.Exit) check=$check artifact-directory-created=$created"
    $State.Failures = @()
    if ($shape.Remove) {
        Copy-Item -LiteralPath (Join-Path $repo $checkoutSourceRelative) -Destination $shape.File -Force
    }
    else {
        Remove-Item -LiteralPath $shape.File -Force
    }
}

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