<#
  Runs the Wave 5B-S1 gate list from review §3.9 and writes one log file per
  command into the evidence directory. Every log records the exact command line,
  the wall-clock start/stop and the exit code, so a reviewer can re-run any line
  verbatim. A command that cannot run is recorded as such; nothing is silent.
#>
param(
    [ValidateSet('fast', 'heavy', 'post')]
    [string]$Phase = 'fast',
    [string]$Repo = 'F:\Workspace\business-platform'
)

$ErrorActionPreference = 'Stop'
Set-Location $Repo
$gates = Join-Path $Repo 'docs\reviews\2026-09-30-wave5b-s1-gates\gates'
New-Item -ItemType Directory -Force -Path $gates | Out-Null

function Invoke-Gate {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string[]]$Executable,
        [string[]]$Arguments = @()
    )
    $log = Join-Path $gates "$Name.log"
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
    return $exit
}

switch ($Phase) {
    'fast' {
        Invoke-Gate -Name '20-catalog-check-default' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo) | Out-Null
        Invoke-Gate -Name '21-catalog-check-only-catalog' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'module-catalog') | Out-Null
        Invoke-Gate -Name '22-catalog-check-only-history' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'pinned-baseline-history') | Out-Null
        Invoke-Gate -Name '23-catalog-check-only-embed' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'baseline-embed') | Out-Null
        Invoke-Gate -Name '24-catalog-check-only-freeze' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'baseline-candidate-freeze') | Out-Null
        Invoke-Gate -Name '25-catalog-check-unknown-check' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'run', '-q', '-p', 'module-release-tool', '--',
            'catalog-check', '--repo', $Repo, '--only', 'everything') | Out-Null
        Invoke-Gate -Name '30-fmt-check' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'fmt', '--all', '--', '--check') | Out-Null
        Invoke-Gate -Name '50-git-diff-check' -Executable 'git' -Arguments @('diff', '--check') | Out-Null
        Invoke-Gate -Name '51-git-status-short' -Executable 'git' -Arguments @('status', '--short') | Out-Null
    }
    'heavy' {
        Invoke-Gate -Name '31-check-workspace' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'check', '--workspace', '--all-targets', '--all-features') | Out-Null
        Invoke-Gate -Name '32-clippy-workspace' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'clippy', '--workspace', '--all-targets',
            '--all-features', '--', '-D', 'warnings') | Out-Null
        Invoke-Gate -Name '33-test-runtime-migration' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'runtime-migration', '--all-features') | Out-Null
        Invoke-Gate -Name '34-test-module-release-tool' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'module-release-tool', '--all-features') | Out-Null
        Invoke-Gate -Name '35-build-no-run-outbox-reliability' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'messaging', '--test',
            'outbox_reliability', '--no-run') | Out-Null
        Invoke-Gate -Name '36-build-no-run-chain-pg' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'audit-postgres', '--test',
            'chain_pg', '--no-run') | Out-Null
    }
    'post' {
        Invoke-Gate -Name '37-test-workspace' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '--workspace', '--all-features') | Out-Null
        Invoke-Gate -Name '40-check-architecture' `
            -Executable 'rtk' -Arguments @('proxy', 'pwsh', '-NoProfile', '-File',
            'scripts/check-architecture.ps1') | Out-Null
        Invoke-Gate -Name '41-wave5b-offline-catalog-gate' `
            -Executable 'rtk' -Arguments @('proxy', 'pwsh', '-NoProfile', '-File',
            'scripts/test-plan0014-wave5b.ps1',
            '-EvidenceDir', 'docs/reviews/2026-09-30-wave5b-s1-gates/wave5b-cases') | Out-Null
        # Re-run at the candidate commit: packaging.rs compares the packed inputs
        # with the committed tree, so it can only be green once Cargo.lock is in.
        Invoke-Gate -Name '42-test-module-release-tool-at-candidate' `
            -Executable 'rtk' -Arguments @('proxy', 'cargo', 'test', '-p', 'module-release-tool', '--all-features') | Out-Null
        Invoke-Gate -Name '52-git-diff-check-at-candidate' -Executable 'git' -Arguments @('diff', '--check') | Out-Null
        Invoke-Gate -Name '53-git-status-short-at-candidate' -Executable 'git' -Arguments @('status', '--short') | Out-Null
        Invoke-Gate -Name '54-git-candidate-anchors' -Executable 'git' -Arguments @(
            'log', '--oneline', '-n', '3', '--format=%H %ad %s', '--date=iso-strict') | Out-Null
    }
}
Write-Output "phase $Phase complete"
