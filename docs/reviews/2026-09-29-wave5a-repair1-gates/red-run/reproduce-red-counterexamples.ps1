# Reproduce the Wave 5A review counterexamples against the PRE-FIX code candidate.
#
# This is not a test of the repaired tool. It checks out the code candidate the
# review condemned (8036bc65) into a throwaway worktree on the same volume, runs
# the reviewer's counterexamples against it, and records what the old tool did.
# Every "RED" line below is the old tool doing something it should not have done;
# the repaired tool refuses each of them (see ../F1-F5-closure.md for the check
# that now owns each case).
#
#   pwsh -NoProfile -File reproduce-red-counterexamples.ps1 -OutDir <evidence>\red-run
#
# Nothing outside the worktree and the output directory is touched, and the
# worktree is removed again unless -KeepWorktree is given.

[CmdletBinding()]
param(
    [string]$RepoRoot = 'F:\Workspace\business-platform',
    [string]$BaseSha = '8036bc65ba7c77666e48afaa6439911ba7c083d7',
    [string]$Worktree = 'F:\wave5a-red-8036bc65',
    [Parameter(Mandatory = $true)][string]$OutDir,
    [switch]$KeepWorktree
)

function Ensure-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
Ensure-Directory $OutDir
$logFile = Join-Path $OutDir 'red-run-log.txt'
$rows = @()

function Write-Log {
    param([string]$Text = '')
    $Text | Add-Content -LiteralPath $logFile -Encoding utf8
}

function Invoke-Recorded {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [string]$Exe = '',
        [string]$WorkDir = '',
        [string]$LogFile = ''
    )
    if ([string]::IsNullOrWhiteSpace($Exe)) { $Exe = 'cargo' }
    if ([string]::IsNullOrWhiteSpace($LogFile)) { $LogFile = Join-Path $OutDir "$Name.log" }
    Write-Log "`n=== $Name ==="
    Write-Log "`$ $([string]::Join(' ', @($Exe) + $Arguments))"
    $previous = (Get-Location).Path
    Push-Location -LiteralPath $WorkDir
    try {
        $lines = & $Exe @Arguments 2>&1 | ForEach-Object {
            if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { [string]$_ }
        }
        $code = $LASTEXITCODE
    }
    finally {
        Set-Location -LiteralPath $previous
    }
    $lines | Add-Content -LiteralPath $LogFile -Encoding utf8
    Write-Log ("exit code : $code")
    foreach ($line in $lines) {
        if ($line -match 'module-release|error\[|error:') { Write-Log "  | $line" }
    }
    return [pscustomobject]@{ Exit = $code; Log = ($lines -join "`n") }
}

function Copy-Candidate {
    param([string]$From, [string]$Name)
    $target = Join-Path $OutDir "cases\$Name"
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    Copy-Item -LiteralPath $From -Destination $target -Recurse -Force
    return $target
}

function Set-Json {
    param([string]$Path, [scriptblock]$Edit)
    $value = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    & $Edit $value
    ConvertTo-Json -Depth 40 -InputObject $value | Set-Content -LiteralPath $Path -Encoding utf8
}

"Wave 5A red-run reproduction against the pre-fix candidate" | Set-Content -LiteralPath $logFile -Encoding utf8
Write-Log "repository   : $RepoRoot"
Write-Log "pre-fix SHA  : $BaseSha"
Write-Log "worktree     : $Worktree"
Write-Log "utc          : $((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'))"

if (Test-Path -LiteralPath $Worktree) {
    throw "worktree path '$Worktree' already exists; refusing to reuse it"
}
$null = Invoke-Recorded -Name '00-worktree-add' -Exe 'git' -WorkDir $RepoRoot `
    -Arguments @('worktree', 'add', $Worktree, $BaseSha, '--detach')
try {
    $null = Invoke-Recorded -Name '01-build-pre-fix-tool' -Arguments @('build', '-p', 'module-release-tool') `
        -WorkDir $Worktree
    $tool = Join-Path $Worktree 'target\debug\module-release-tool.exe'
    if (-not (Test-Path -LiteralPath $tool)) {
        throw "the pre-fix tool was not produced at $tool"
    }
    Ensure-Directory (Join-Path $OutDir 'cases')
    $null = Invoke-Recorded -Name '02-generate-candidate' -Exe $tool -WorkDir $Worktree `
        -Arguments @('generate', '--module', 'identity', '--out', (Join-Path $Worktree 'red-out'))
    $pristine = Join-Path $Worktree 'red-out\identity-0.1.0-rc.1'

    # F1 — the reviewer's counterexample: an all-zero source revision.
    $f1 = Copy-Candidate -From $pristine -Name 'f1-zero-revision'
    Set-Json -Path (Join-Path $f1 'release.json') -Edit { param($v) $v.source_revision = ('0' * 40) }
    Set-Json -Path (Join-Path $f1 'release-bundle.json') -Edit { param($v) $v.identity.source_revision = ('0' * 40) }
    $f1run = Invoke-Recorded -Name '03-f1-verify-zero-revision' -Exe $tool -WorkDir $RepoRoot `
        -Arguments @('verify', '--artifact', $f1)
    $rows += [pscustomobject]@{
        Finding = 'F1'; Counterexample = 'both identity records claim 0000…0000 as source_revision'
        Command = 'module-release verify (pre-fix)'
        Observed = "exit $($f1run.Exit)"; Verdict = $(if ($f1run.Exit -eq 0) { 'RED CONFIRMED (verified)' } else { 'not reproduced' })
    }

    # F2 — compare must not care about the payload it claims to compare.
    $f2 = Copy-Candidate -From $pristine -Name 'f2-damaged-right'
    Add-Content -LiteralPath (Join-Path $f2 'package\contract\packages\identity\src\lib.rs') `
        -Value 'compile_error!("review tamper");' -Encoding utf8
    $f2run = Invoke-Recorded -Name '04-f2-compare-damaged-side' -Exe $tool -WorkDir $RepoRoot `
        -Arguments @('compare', '--left', $pristine, '--right', $f2)
    $rows += [pscustomobject]@{
        Finding = 'F2'; Counterexample = 'one packed source byte changed on the right side only'
        Command = 'module-release compare (pre-fix)'
        Observed = "exit $($f2run.Exit)"; Verdict = $(if ($f2run.Exit -eq 0) { 'RED CONFIRMED (declared identical)' } else { 'not reproduced' })
    }
    $f2verify = Invoke-Recorded -Name '05-f2-verify-same-damage' -Exe $tool -WorkDir $RepoRoot `
        -Arguments @('verify', '--artifact', $f2)
    Write-Log "note: verify did see the damage (exit $($f2verify.Exit)) — the defect is specific to compare."
    $rows += [pscustomobject]@{
        Finding = 'F2'; Counterexample = 'the same damage, through verify'
        Command = 'module-release verify (pre-fix)'
        Observed = "exit $($f2verify.Exit)"
        Verdict = $(if ($f2verify.Exit -ne 0) { 'control: verify caught it, compare did not' } else { 'unexpected' })
    }

    # F3 — descriptive records nobody reads.
    $f3 = Copy-Candidate -From $pristine -Name 'f3-unknown-algorithm'
    Set-Json -Path (Join-Path $f3 'artifact-manifest.json') -Edit {
        param($v) $v.artifact_integrity_algorithm = 'unknown-algorithm.v9'
    }
    $f3run = Invoke-Recorded -Name '06-f3-verify-unknown-algorithm' -Exe $tool -WorkDir $RepoRoot `
        -Arguments @('verify', '--artifact', $f3)
    $rows += [pscustomobject]@{
        Finding = 'F3'; Counterexample = 'artifact_integrity_algorithm replaced by an algorithm nobody implements'
        Command = 'module-release verify (pre-fix)'
        Observed = "exit $($f3run.Exit)"; Verdict = $(if ($f3run.Exit -eq 0) { 'RED CONFIRMED (verified)' } else { 'not reproduced' })
    }
    $f3b = Copy-Candidate -From $pristine -Name 'f3b-record-version'
    Set-Json -Path (Join-Path $f3b 'artifact-manifest.json') -Edit { param($v) $v.module_version = '9.9.9' }
    $f3brun = Invoke-Recorded -Name '07-f3-verify-record-version-claim' -Exe $tool -WorkDir $RepoRoot `
        -Arguments @('verify', '--artifact', $f3b)
    $rows += [pscustomobject]@{
        Finding = 'F3'; Counterexample = 'artifact-manifest.json claims a module_version release.json does not'
        Command = 'module-release verify (pre-fix)'
        Observed = "exit $($f3brun.Exit)"; Verdict = $(if ($f3brun.Exit -eq 0) { 'RED CONFIRMED (verified)' } else { 'not reproduced' })
    }
    $f3c = Copy-Candidate -From $pristine -Name 'f3c-bundle-crate-binding'
    Set-Json -Path (Join-Path $f3c 'release-bundle.json') -Edit { param($v) $v.crate_versions[0].crate_version = '9.9.9' }
    $f3crun = Invoke-Recorded -Name '08-f3-verify-bundle-crate-binding' -Exe $tool -WorkDir $RepoRoot `
        -Arguments @('verify', '--artifact', $f3c)
    $rows += [pscustomobject]@{
        Finding = 'F3'; Counterexample = 'release-bundle.json binds crate version 9.9.9 while the packed Cargo.toml says 0.1.0-rc.1'
        Command = 'module-release verify (pre-fix)'
        Observed = "exit $($f3crun.Exit)"; Verdict = $(if ($f3crun.Exit -eq 0) { 'RED CONFIRMED (verified)' } else { 'not reproduced' })
    }

    # The old script's own text, for F4 (the scan) and F5 (the shared dirty copy).
    git -C $RepoRoot show "${BaseSha}:scripts/plan0014-wave5a.ps1" | Set-Content -LiteralPath (
        Join-Path $OutDir 'pre-fix-orchestration-script.ps1') -Encoding utf8
    Write-Log "`n=== pre-fix orchestration script archived (F4 scan at lines 253-277, F5 shared copy at lines 289-343) ==="
}
finally {
    if (-not $KeepWorktree) {
        $null = Invoke-Recorded -Name '99-worktree-remove' -Exe 'git' -WorkDir $RepoRoot `
            -Arguments @('worktree', 'remove', '--force', $Worktree)
    }
}
$rows | Format-Table -AutoSize | Out-String -Width 200 | Add-Content -LiteralPath $logFile -Encoding utf8
$rows | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutDir 'red-run-summary.json') -Encoding utf8
$rows | ForEach-Object { "| $($_.Finding) | $($_.Counterexample) | ``$($_.Command)`` | $($_.Observed) | $($_.Verdict) |" } |
    Set-Content -LiteralPath (Join-Path $OutDir 'red-run-table.md') -Encoding utf8
$rows | ForEach-Object { "{0}: {1} -> {2}" -f $_.Finding, $_.Observed, $_.Verdict }