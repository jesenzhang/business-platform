# Shared helpers for the PLAN-0014 Wave 5A orchestration.
#
# Both scripts/plan0014-wave5a.ps1 (the orchestration) and
# scripts/test-plan0014-wave5a.ps1 (its regression entry) dot-source this file, so
# the logic that decides pass or fail is the same code that runs the release
# candidate assembly — the regressions cannot drift away from the real run.
#
# Rules this file exists to enforce (review F5):
#   * every negative case starts from its own pristine copy of a candidate that
#     just verified, so one injected fault cannot be credited for another's;
#   * a JSON edit is a parse, a mutation and a serialise — never a string
#     replacement whose whitespace assumptions decide the outcome;
#   * the mutation is re-read and proved to have landed before the refusal is
#     asserted, so a silent no-op cannot masquerade as a passing negative;
#   * a negative must fail for the asserted reason: exit code alone is not a
#     verdict, the check identifier has to match;
#   * a step that other steps depend on stops the run when it fails, instead of
#     letting a later step report on state that was never produced;
#   * scratch is only ever removed from inside this run's own directory.

Set-StrictMode -Version 3.0

function Get-UtcStamp {
    (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
}

function Ensure-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

# A run directory is fresh or nothing: evidence is immutable, and silently
# appending to an older run's directory is how a stale log gets read as new.
function New-FreshDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [string]$What = 'directory'
    )
    if (Test-Path -LiteralPath $Path) {
        $left = @(Get-ChildItem -LiteralPath $Path -Force -ErrorAction SilentlyContinue)
        if ($left.Count -gt 0 -or (Get-Item -LiteralPath $Path).PSIsContainer -eq $false) {
            throw "refusing to reuse $What '$Path': it already exists and is not empty. Evidence is immutable — start a new run instead of overwriting history."
        }
    }
    Ensure-Directory $Path
    return (Resolve-Path -LiteralPath $Path).Path
}

function Write-LogHeader {
    param(
        [Parameter(Mandatory = $true)][string]$LogFile,
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][string]$CommandLine,
        [string]$WorkDir = ''
    )
    Ensure-Directory (Split-Path -Parent $LogFile)
    $lines = @(
        "title      : $Title",
        "command    : $CommandLine",
        "workdir    : $(if ($WorkDir) { $WorkDir } else { (Get-Location).Path })",
        "utc        : $(Get-UtcStamp)"
    )
    $lines | Set-Content -LiteralPath $LogFile -Encoding utf8
}

# Runs one command, records it, and returns its exit code. A step marked
# -Blocking also stops the run: a step whose successors depend on its output must
# not be followed by a green-looking successor that measured nothing.
function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$LogFile,
        [string]$Exe = '',
        [string]$WorkDir = '',
        [int]$ExpectExit = 0,
        [switch]$Blocking,
        [hashtable]$State
    )
    if ([string]::IsNullOrWhiteSpace($Exe)) {
        $Exe = if ($State) { $State.Tool } else { 'cargo' }
    }
    if ([string]::IsNullOrWhiteSpace($WorkDir)) {
        $WorkDir = if ($State) { $State.Repo } else { (Get-Location).Path }
    }
    Write-LogHeader -LogFile $LogFile -Title $Title -CommandLine "$([string]::Join(' ', @($Exe) + $Arguments))" -WorkDir $WorkDir
    $previous = (Get-Location).Path
    Push-Location -LiteralPath $WorkDir
    $stdout = [Collections.Generic.List[string]]::new()
    try {
        & $Exe @Arguments 2>&1 | ForEach-Object {
            if ($_ -is [System.Management.Automation.ErrorRecord]) {
                # stderr belongs to the log too, but never to captured stdout: a
                # document a later step parses has to be exactly what the command
                # printed on stdout.
                [pscustomobject]@{ Side = 'err'; Text = $_.Exception.Message }
            }
            else {
                $stdout.Add([string]$_)
                [pscustomobject]@{ Side = 'out'; Text = [string]$_ }
            }
        } | ForEach-Object { $_.Text } | Add-Content -LiteralPath $LogFile -Encoding utf8
        $code = $LASTEXITCODE
    }
    finally {
        Pop-Location -ErrorAction SilentlyContinue
        Set-Location -LiteralPath $previous
    }
    "`nexit code : $code (expected $ExpectExit)" | Add-Content -LiteralPath $LogFile -Encoding utf8
    if ($State) {
        $State.LogIndex += [pscustomobject]@{
            Title  = $Title
            Command = "$([string]::Join(' ', @($Exe) + $Arguments))"
            Exit   = $code
            Expect = $ExpectExit
            Log    = Split-Path -Leaf $LogFile
        }
    }
    $ok = ($code -eq $ExpectExit)
    if (-not $ok -and $State) {
        $State.Failures += "$Name exited $code, expected $ExpectExit (log: $(Split-Path -Leaf $LogFile))"
    }
    if (-not $ok -and $Blocking) {
        throw "$Name failed with exit $code (expected $ExpectExit); stopping the run because later steps depend on it. Log: $LogFile"
    }
    return [pscustomobject]@{
        Exit   = $code
        Log    = (Get-Content -Raw -LiteralPath $LogFile)
        Stdout = ($stdout -join [Environment]::NewLine)
    }
}

function Copy-Tree {
    param(
        [Parameter(Mandatory = $true)][string]$From,
        [Parameter(Mandatory = $true)][string]$To
    )
    Ensure-Directory $To
    foreach ($entry in Get-ChildItem -LiteralPath $From -Force) {
        $target = Join-Path $To $entry.Name
        if ($entry.PSIsContainer) {
            Copy-Tree -From $entry.FullName -To $target
        }
        else {
            Copy-Item -LiteralPath $entry.FullName -Destination $target -Force
        }
    }
}

function Convert-JsonCanonical {
    param([Parameter(Mandatory = $true)]$Value)
    return ($Value | ConvertTo-Json -Depth 40 -Compress)
}

# Parses a JSON record, mutates it, writes it back, and proves the mutation
# landed. A negative case whose injection silently did nothing is worse than no
# negative case, because it reads as a passing refusal test.
function Set-JsonRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][scriptblock]$Edit,
        [string]$Describe = 'the mutation'
    )
    $original = Convert-JsonCanonical (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
    $value = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    & $Edit $value
    (ConvertTo-Json -Depth 40 -InputObject $value) | Set-Content -LiteralPath $Path -Encoding utf8
    $written = Convert-JsonCanonical (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
    if ($written -eq $original) {
        throw "$Describe did not change '$Path'; the negative case would assert nothing."
    }
    return [pscustomobject]@{ Before = $original; After = $written }
}

# One negative case: pristine copy, green baseline, one injection, then the
# refusal has to arrive with the asserted identifier.
function Invoke-NegativeCase {
    param(
        [Parameter(Mandatory = $true)][hashtable]$State,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][string]$SourceArtifact,
        [Parameter(Mandatory = $true)][scriptblock]$Inject,
        [Parameter(Mandatory = $true)][string]$ExpectCheck,
        [string]$ExpectPattern = ''
    )
    $caseRoot = Join-Path $State.NegativeRoot $Name
    if (Test-Path -LiteralPath $caseRoot) {
        Remove-Item -LiteralPath $caseRoot -Recurse -Force
    }
    Ensure-Directory $caseRoot
    $copy = Join-Path $caseRoot (Split-Path -Leaf $SourceArtifact)
    Copy-Tree -From $SourceArtifact -To $copy

    $baselineLog = Join-Path $caseRoot '00-baseline-verify.log'
    $baseline = Invoke-Step `
        -Name "$Name baseline" `
        -Title "$Name : the untouched copy must verify before it is damaged" `
        -Arguments @('verify', '--artifact', $copy) `
        -LogFile $baselineLog `
        -State $State
    if ($baseline.Exit -ne 0) {
        $State.Failures += "$Name : the pristine copy did not even verify (exit $($baseline.Exit)); the case cannot prove anything"
        $State.Cases += [pscustomobject]@{
            Case = $Name; Target = $ExpectCheck; Observed = 'baseline not green'; Verdict = 'BROKEN'
        }
        return $false
    }

    & $Inject $copy

    $logFile = Join-Path $caseRoot '10-verify.log'
    $run = Invoke-Step `
        -Name $Name `
        -Title $Title `
        -Arguments @('verify', '--artifact', $copy) `
        -LogFile $logFile `
        -ExpectExit 1 `
        -State $State
    $observed = 'no check identifier'
    if ($run.Log -match '\[([a-z0-9-]+)\]') {
        $observed = $Matches[1]
    }
    $verdict = 'FAILED'
    if ($run.Exit -eq 0) {
        $verdict = 'NOT REFUSED'
        $State.Failures += "$Name : the tampered artifact verified anyway"
    }
    elseif ($observed -ne $ExpectCheck) {
        $verdict = 'WRONG REASON'
        $State.Failures += "$Name : refused for '$observed', expected '$ExpectCheck' (log: $(Split-Path -Leaf $logFile))"
    }
    elseif ($ExpectPattern -and $run.Log -notmatch $ExpectPattern) {
        $verdict = 'WRONG DETAIL'
        $State.Failures += "$Name : refused by '$observed' but the log does not match '$ExpectPattern'"
    }
    else {
        $verdict = 'OK'
    }
    $State.Cases += [pscustomobject]@{
        Case = $Name; Target = $ExpectCheck; Observed = $observed; Verdict = $verdict
    }
    return ($verdict -eq 'OK')
}

# Isolation is judged by the resolved graph, through the same helper the Rust
# tests call. The script does not implement any path rule of its own.
function Invoke-IsolationGuard {
    param(
        [Parameter(Mandatory = $true)][hashtable]$State,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$ConsumerDir,
        [Parameter(Mandatory = $true)][string[]]$ArtifactCrateDirs,
        [Parameter(Mandatory = $true)][string]$MetadataFile,
        [int]$ExpectExit = 0
    )
    $metadataLog = Join-Path (Split-Path -Parent $MetadataFile) "$Name-cargo-metadata.log"
    $metadata = Invoke-Step `
        -Name "$Name metadata" `
        -Title "$Name : resolve the consumer dependency graph (cargo metadata, full graph)" `
        -Exe 'cargo' `
        -Arguments @('metadata', '--format-version', '1', '--locked', '--offline', '--filter-platform', (Get-DefaultTargetTriple)) `
        -LogFile $metadataLog `
        -WorkDir $ConsumerDir `
        -State $State
    if ($metadata.Exit -ne 0) {
        $metadata = Invoke-Step `
            -Name "$Name metadata (no filter)" `
            -Title "$Name : resolve the consumer dependency graph (cargo metadata, unfiltered)" `
            -Exe 'cargo' `
            -Arguments @('metadata', '--format-version', '1', '--locked', '--offline') `
            -LogFile $metadataLog `
            -WorkDir $ConsumerDir `
            -State $State
    }
    if ($metadata.Exit -ne 0) {
        $State.Failures += "$Name : cargo metadata failed, so isolation could not be judged"
        return $false
    }
    $metadata.Stdout | Set-Content -LiteralPath $MetadataFile -Encoding utf8

    $guardArguments = @('guard', '--metadata', $MetadataFile, '--consumer', $ConsumerDir, '--repo', $State.Repo)
    foreach ($directory in $ArtifactCrateDirs) {
        $guardArguments += @('--artifact', $directory)
    }
    $guardLog = Join-Path (Split-Path -Parent $MetadataFile) "$Name-guard.log"
    $guard = Invoke-Step `
        -Name "$Name guard" `
        -Title "$Name : decide isolation from the resolved graph (module-release guard)" `
        -Arguments $guardArguments `
        -LogFile $guardLog `
        -ExpectExit $ExpectExit `
        -State $State
    return ($guard.Exit -eq $ExpectExit)
}

function Get-DefaultTargetTriple {
    $output = (& rustc --print host-tuple 2>$null)
    if ($LASTEXITCODE -eq 0 -and $output) {
        return ($output | Select-Object -Last 1).ToString().Trim()
    }
    return ''
}

# Removes a path only if it lives inside this run's own scratch root. Nothing in
# this orchestration is allowed to delete anything else, ever.
function Remove-OwnScratch {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$OwnRoot
    )
    $resolved = (Resolve-Path -LiteralPath $Path).Path.TrimEnd('\', '/')
    $root = (Resolve-Path -LiteralPath $OwnRoot).Path.TrimEnd('\', '/')
    # Containment is decided by the filesystem's own notion of a relative path: a
    # path that needs '..' to reach, or that resolves onto another root, is not
    # inside this run's scratch and is never deleted.
    $relative = [IO.Path]::GetRelativePath($root, $resolved)
    $first = ($relative -split '[\\/]', 2)[0]
    if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative) -or $first -eq '..') {
        throw "refusing to delete '$resolved': it is not inside this run's own scratch root '$root'"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
# Where cargo reads its configuration from, recorded as evidence. `cargo config
# get` is a nightly-only subcommand, so this walks the documented search path
# itself: every .cargo/config.toml from the build directory up to the volume root,
# then the cargo home, then the CARGO_* variables that override all of them.
function Write-CargoConfigEvidence {
    param(
        [Parameter(Mandatory = $true)][string]$LogFile,
        [Parameter(Mandatory = $true)][string]$StartDir,
        [string]$Label = 'this build'
    )
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $lines = @(
        "# cargo configuration in force for the $Label build",
        "start directory : $StartDir",
        "CARGO_HOME      : $cargoHome",
        "utc             : $(Get-UtcStamp)",
        '',
        'CARGO_* environment variables (these override every config file):'
    )
    $variables = @(& Get-ChildItem 'Env:' | Where-Object { $_.Name -like 'CARGO_*' })
    if ($variables.Count -eq 0) {
        $lines += '  (none)'
    }
    foreach ($variable in $variables) {
        $lines += "  $($variable.Name)=$($variable.Value)"
    }
    $lines += @('', 'Config files cargo would read, nearest ancestor first:')
    $found = @()
    $directory = (Resolve-Path -LiteralPath $StartDir).Path
    while ($true) {
        foreach ($name in @('config.toml', 'config')) {
            $candidate = Join-Path (Join-Path $directory '.cargo') $name
            if (Test-Path -LiteralPath $candidate) {
                $found += $candidate
                $lines += "  PRESENT $candidate"
                foreach ($line in @(Get-Content -LiteralPath $candidate)) {
                    $lines += "      | $line"
                }
            }
            else {
                $lines += "  absent  $candidate"
            }
        }
        $parent = Split-Path -Parent $directory
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $directory) {
            break
        }
        $directory = $parent
    }
    foreach ($name in @('config.toml', 'config')) {
        $candidate = Join-Path $cargoHome $name
        if (Test-Path -LiteralPath $candidate) {
            $found += $candidate
            $lines += "  PRESENT $candidate (cargo home)"
            foreach ($line in @(Get-Content -LiteralPath $candidate)) {
                $lines += "      | $line"
            }
        }
        else {
            $lines += "  absent  $candidate (cargo home)"
        }
    }
    $lines += @(
        '',
        "conclusion: $(if ($found.Count -eq 0) { 'no cargo config file applies; cargo runs with built-in defaults plus the environment above' } else { "$($found.Count) config file(s) apply: $($found -join ', ')" })"
    )
    Ensure-Directory (Split-Path -Parent $LogFile)
    $lines | Set-Content -LiteralPath $LogFile -Encoding utf8
    return [pscustomobject]@{ Found = $found; Conclusion = $lines[-1] }
}