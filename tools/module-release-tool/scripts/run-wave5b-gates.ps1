<#
  Runs the Wave 5B-S1 gate list from review §3.9 and writes one log file per
  command into the evidence directory. Every log records the exact command line,
  the wall-clock start/stop and the exit code, so a reviewer can re-run any line
  verbatim. A command that cannot run is recorded as such; nothing is silent.
#>
param(
    [ValidateSet('fast', 'heavy', 'post')]
    [string]$Phase = 'fast',
    [string]$Repo = 'F:\Workspace\business-platform',
    [string]$EvidenceDir = ''
)

$ErrorActionPreference = 'Stop'
Set-Location $Repo

# B8 fix: require a unique EvidenceDir/RunId and refuse to overwrite existing files.
if ([string]::IsNullOrWhiteSpace($EvidenceDir)) {
    $runId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
    $EvidenceDir = Join-Path $Repo ("docs/reviews/wave5b-s1-gates-" + $runId)
}
$gates = Join-Path $EvidenceDir 'gates'
if (Test-Path -LiteralPath $gates) {
    $existing = @(Get-ChildItem -LiteralPath $gates -Force -ErrorAction SilentlyContinue)
    if ($existing.Count -gt 0) {
        throw "Evidence directory '$EvidenceDir' already exists and is not empty; choose a new directory or use a different RunId"
    }
}
New-Item -ItemType Directory -Force -Path $gates | Out-Null
$script:FailedGates = [System.Collections.Generic.List[object]]::new()

function Invoke-Gate {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string[]]$Executable,
        [string[]]$Arguments = @(),
        [int]$ExpectedExitCode = 0
    )
    $log = Join-Path $gates "$Name.log"
    if (Test-Path -LiteralPath $log) {
        throw "Gate log '$log' already exists; refusing to overwrite archived evidence"
    }
    $commandLine = (@($Executable + $Arguments)) -join ' '
    $started = Get-Date
    $captured = $null
    $exit = $null
    $failure = ''
    try {
        $captured = & $Executable @Arguments 2>&1
        $exit = $LASTEXITCODE
    } catch {
        $failure = $_.Exception.Message
        $exit = -1
    }
    $ended = Get-Date
    $lines = @(
        "# gate: $Name"
        "# command: $commandLine"
        "# started: $($started.ToString('yyyy-MM-dd HH:mm:ss zzz'))"
        "# finished: $($ended.ToString('yyyy-MM-dd HH:mm:ss zzz'))"
        "# duration-seconds: $([int]($ended - $started).TotalSeconds)"
        "# exit-code: $exit"
        "# expected-exit-code: $ExpectedExitCode"
        if ($failure -ne '') { "# error: $failure" }
        ''
    )
    $body = @()
    if ($null -ne $captured) {
        $body = @($captured | ForEach-Object {
                if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.ToString() } else { $_.ToString() }
            })
    }
    [System.IO.File]::WriteAllText(
        $log,
        (($lines + $body) -join "`r`n"),
        [System.Text.UTF8Encoding]::new($false))
    Write-Output ("gate {0}: exit={1} lines={2} -> {3}" -f $Name, $exit, $body.Count, $log)
    if ($exit -ne $ExpectedExitCode) {
        $script:FailedGates.Add([pscustomobject]@{
            Name = $Name
            Expected = $ExpectedExitCode
            Actual = $exit
            Log = $log
        })
    }
    return $exit
}

switch ($Phase) {
    'fast' {
        Invoke-Gate -Name '20-catalog-check-default' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo) -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '21-catalog-check-only-catalog' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'module-catalog') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '22-catalog-check-only-history' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'pinned-baseline-history') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '23-catalog-check-only-embed' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'baseline-embed') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '24-catalog-check-only-freeze' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'baseline-candidate-freeze') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '25-catalog-check-unknown-check' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'everything') -ExpectedExitCode 1 | Out-Null
        Invoke-Gate -Name '30-fmt-check' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'fmt', '--all', '--', '--check') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '50-git-diff-check' -Executable 'git' -Arguments @('diff', '--check') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '51-git-status-short' -Executable 'git' -Arguments @('status', '--short') -ExpectedExitCode 0 | Out-Null
    }
    'heavy' {
        Invoke-Gate -Name '31-check-workspace' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'check', '--workspace', '--all-targets', '--all-features') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '32-clippy-workspace' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'clippy', '--workspace', '--all-targets',
            '--all-features', '--', '-D', 'warnings') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '33-test-runtime-migration' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'runtime-migration', '--all-features') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '34-test-module-release-tool' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'module-release-tool', '--all-features') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '35-build-no-run-outbox-reliability' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'messaging', '--test',
            'outbox_reliability', '--no-run') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '36-build-no-run-chain-pg' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'audit-postgres', '--test',
            'chain_pg', '--no-run') -ExpectedExitCode 0 | Out-Null
    }
    'post' {
        Invoke-Gate -Name '37-test-workspace' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '--workspace', '--all-features') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '40-check-architecture' `
            -Executable 'rtk' -Arguments @('proxy', 'pwsh', '-NoProfile', '-File',
            'scripts/check-architecture.ps1') -ExpectedExitCode 0 | Out-Null
        # R7 fix: assemble the case evidence path in a scalar variable first.
        # `$EvidenceDir + '/wave5b-cases'` inside the array literal parses as an
        # array append, which handed the driver an extra positional argument.
        $caseEvidenceDir = Join-Path $EvidenceDir 'wave5b-cases'
        Invoke-Gate -Name '41-wave5b-offline-catalog-gate' `
            -Executable 'rtk' -Arguments @('proxy', 'pwsh', '-NoProfile', '-File',
            'scripts/test-plan0014-wave5b.ps1',
            '-EvidenceDir', $caseEvidenceDir) -ExpectedExitCode 0 | Out-Null
        # Re-run at the candidate commit: packaging.rs compares the packed inputs
        # with the committed tree, so it can only be green once Cargo.lock is in.
        Invoke-Gate -Name '42-test-module-release-tool-at-candidate' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'module-release-tool', '--all-features') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '52-git-diff-check-at-candidate' -Executable 'git' -Arguments @('diff', '--check') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '53-git-status-short-at-candidate' -Executable 'git' -Arguments @('status', '--short') -ExpectedExitCode 0 | Out-Null
        Invoke-Gate -Name '54-git-candidate-anchors' -Executable 'git' -Arguments @(
            'log', '--oneline', '-n', '3', '--format=%H %ad %s', '--date=iso-strict') -ExpectedExitCode 0 | Out-Null
    }
}
Write-Output "phase $Phase complete"

# B7 fix: propagate failure exit codes and summarize mismatches.
if ($script:FailedGates.Count -gt 0) {
    Write-Host "FAILED GATES:"
    foreach ($gate in $script:FailedGates) {
        Write-Host "  $($gate.Name): expected $($gate.Expected), got $($gate.Actual) (log: $($gate.Log))"
    }
    throw "$($script:FailedGates.Count) gate(s) failed with unexpected exit codes"
}
