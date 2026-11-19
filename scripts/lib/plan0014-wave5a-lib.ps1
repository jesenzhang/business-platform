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
    $beforeHashes = Get-TreeHashes -Root $copy

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
    $injection = @(Compare-InjectedTrees -Before $beforeHashes -After (Get-TreeHashes -Root $copy))

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
    elseif ($injection.Count -eq 1 -and $injection[0] -like 'nothing*') {
        $verdict = 'INJECTION DID NOT LAND'
        $State.Failures += "$Name : the injection changed no bytes, so the refusal cannot be credited to it"
    }
    else {
        $verdict = 'OK'
    }
    @(
        "# $($Name)",
        '',
        "case       : $Title",
        "expect     : exit 1 refused by ``$ExpectCheck``",
        "observed   : exit $($run.Exit) refused by ``$observed``",
        "verdict    : $verdict",
        '',
        '## what the injection actually changed (content hashes, pristine copy -> damaged copy)',
        ''
    ) + $injection + @(
        '',
        '## commands',
        '',
        "1. ``module-release verify --artifact <copy>`` before the injection — exit $($baseline.Exit)",
        "   (the copy has to be a candidate that verifies, or the case proves nothing)",
        "2. ``module-release verify --artifact <copy>`` after the injection — exit $($run.Exit)",
        '',
        "Raw logs: ``00-baseline-verify.log``, ``10-verify.log`` (archived beside this file).",
        ''
    ) | Set-Content -LiteralPath (Join-Path $caseRoot 'case.md') -Encoding utf8
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
# What an injection actually changed, in bytes: the file set of the pristine copy
# against the damaged copy, by content hash. "I tampered with the artifact" is not
# evidence; "package/CHANGELOG.md 6672a0f7… -> removed" is.
function Get-TreeHashes {
    param([Parameter(Mandatory = $true)][string]$Root)
    $hashes = @{}
    $prefix = $Root.TrimEnd('\', '/')
    foreach ($file in @(Get-ChildItem -LiteralPath $Root -Recurse -File -Force -ErrorAction SilentlyContinue)) {
        $relative = $file.FullName.Substring($prefix.Length).TrimStart('\', '/') -replace '\\', '/'
        $hashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $hashes
}

function Compare-InjectedTrees {
    param(
        [Parameter(Mandatory = $true)][hashtable]$Before,
        [Parameter(Mandatory = $true)][hashtable]$After
    )
    $lines = @()
    foreach ($path in @($Before.Keys | Sort-Object)) {
        if (-not $After.ContainsKey($path)) {
            $lines += "removed   ``$path`` (was ``$($Before[$path].Substring(0, 12))…``)"
        }
        elseif ($After[$path] -ne $Before[$path]) {
            $lines += "changed   ``$path`` (``$($Before[$path].Substring(0, 12))…`` -> ``$($After[$path].Substring(0, 12))…``)"
        }
    }
    foreach ($path in @($After.Keys | Where-Object { -not $Before.ContainsKey($_) } | Sort-Object)) {
        $lines += "added     ``$path`` (``$($After[$path].Substring(0, 12))…``)"
    }
    if ($lines.Count -eq 0) {
        $lines += 'nothing — the injection did not change a single byte'
    }
    return $lines
}

# Case notes and per-case raw logs belong with the evidence, not in the gitignored
# scratch directory: a reader has to be able to see why a case refused.
function Save-NegativeRecords {
    param(
        [Parameter(Mandatory = $true)][string]$From,
        [Parameter(Mandatory = $true)][string]$To
    )
    if (-not (Test-Path -LiteralPath $From)) {
        return 0
    }
    Ensure-Directory $To
    $saved = 0
    # One level per case directory, on purpose: the damaged artifact copies live in a
    # subdirectory of each case and only the run scratch needs them, while the notes
    # and raw logs sit directly in the case root.
    foreach ($caseDirectory in @(Get-ChildItem -LiteralPath $From -Directory -Force -ErrorAction SilentlyContinue)) {
        $destinationDirectory = Join-Path $To $caseDirectory.Name
        Ensure-Directory $destinationDirectory
        foreach ($file in @(Get-ChildItem -LiteralPath $caseDirectory.FullName -File -Force -ErrorAction SilentlyContinue |
                Where-Object { $_.Extension -in @('.log', '.md') })) {
            Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $destinationDirectory $file.Name) -Force
            $saved++
        }
    }
    return $saved
}
# ------------------------------------------------------- the digest rule again --
#
# `module-release` owns the payload digest rule and refuses a payload whose record
# disagrees with its bytes. That is the check Repair 1 had; what Repair 1 could not
# express is a payload whose record *does* agree with a member the pinned commit
# cannot produce — a packed `Cargo.toml` whose dependency version was rewritten and
# every hash beside it recomputed (review F1-A). Building such a counterexample needs
# the orchestrator to recompute the record itself, so this is a deliberate duplicate
# of `canonical.rs`, and the regression suite (R6) is what keeps the duplicate honest:
# on a freshly generated candidate the recomputation has to reproduce the recorded
# member digests and the recorded integrity digest without changing a single byte.
function Get-OrdinalPathKey {
    param([Parameter(Mandatory = $true)][string]$Path)
    # `inventory()` sorts members by path *bytes*, and the sort feeds the digest. A
    # culture-sensitive sort would order `contract-x` and `contract/…` differently and
    # produce a different digest for the same payload, so the key is the hex of each
    # code unit; every member path is ASCII, where code units are bytes.
    $key = ''
    foreach ($character in $Path.ToCharArray()) {
        $key += '{0:x4}' -f [int]$character
    }
    return $key
}

function Get-CanonicalFileDigest {
    param([Parameter(Mandatory = $true)][string]$Path)
    # The canonical hashed form of one member: UTF-8 text, byte order mark removed,
    # CRLF and lone CR folded to LF, exactly one trailing LF. `Get-Content -Raw`
    # decodes UTF-8 and drops a leading BOM; the folding is spelled out, and the
    # digest is taken over bytes — a scratch file — rather than over a string.
    $text = Get-Content -Raw -LiteralPath $Path
    if ($null -eq $text) {
        $text = ''
    }
    $text = ($text -replace "`r`n", "`n") -replace "`r", "`n"
    if (-not $text.EndsWith("`n")) {
        $text += "`n"
    }
    $scratch = Join-Path $env:TEMP ("plan0014-canonical-" + (New-Guid).Guid + ".txt")
    Set-Content -LiteralPath $scratch -Value $text -NoNewline -Encoding utf8
    try {
        return [pscustomobject]@{
            Sha256 = (Get-FileHash -LiteralPath $scratch -Algorithm SHA256).Hash.ToLowerInvariant()
            Bytes  = [int64] (Get-Item -LiteralPath $scratch).Length
        }
    }
    finally {
        Remove-Item -LiteralPath $scratch -Force -ErrorAction SilentlyContinue
    }
}

function Get-IntegrityDigest {
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Files)
    # The pre-image `plan-0014.artifact-integrity.v1` hashes: the algorithm name, then
    # one `<sha256> <path>` line per member in path-byte order.
    $preimage = "plan-0014.artifact-integrity.v1`n"
    foreach ($file in $Files) {
        $preimage += "$($file.sha256) $($file.path)`n"
    }
    if ($preimage -match '[^\x00-\x7F]') {
        throw 'the integrity pre-image must be ASCII; a member path outside ASCII is not supported here'
    }
    $scratch = Join-Path $env:TEMP ("plan0014-preimage-" + (New-Guid).Guid + ".txt")
    Set-Content -LiteralPath $scratch -Value $preimage -NoNewline -Encoding ascii
    try {
        return (Get-FileHash -LiteralPath $scratch -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    finally {
        Remove-Item -LiteralPath $scratch -Force -ErrorAction SilentlyContinue
    }
}

# Recomputes `files` and `artifact_integrity_digest` in an artifact copy from the
# bytes that are actually in `package/`. With -DryRun nothing is written, which is how
# the self-test compares this re-implementation with the tool's own record.
function Set-IntegrityRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Artifact,
        [switch]$DryRun
    )
    $payload = Join-Path $Artifact 'package'
    if (-not (Test-Path -LiteralPath $payload)) {
        throw "'$payload' is not a release payload; there is no inventory to recompute"
    }
    $recordPath = Join-Path $Artifact 'artifact-manifest.json'
    $record = Get-Content -Raw -LiteralPath $recordPath | ConvertFrom-Json
    $before = $record.artifact_integrity_digest
    $prefix = (Resolve-Path -LiteralPath $payload).Path.TrimEnd('\', '/')
    $files = @(
        foreach ($file in @(Get-ChildItem -LiteralPath $payload -Recurse -File -Force)) {
            $relative = $file.FullName.Substring($prefix.Length).TrimStart('\', '/') -replace '\\', '/'
            $digest = Get-CanonicalFileDigest -Path $file.FullName
            [pscustomobject]@{ path = $relative; sha256 = $digest.Sha256; bytes = $digest.Bytes }
        }
    ) | Sort-Object -Property { Get-OrdinalPathKey $_.path }
    $digest = Get-IntegrityDigest -Files $files
    if (-not $DryRun) {
        $record.files = @($files | ForEach-Object {
                [ordered]@{ path = $_.path; sha256 = $_.sha256; bytes = $_.bytes }
            })
        $record.artifact_integrity_digest = $digest
        (ConvertTo-Json -Depth 40 -InputObject $record) | Set-Content -LiteralPath $recordPath -Encoding utf8
    }
    return [pscustomobject]@{
        Files   = @($files)
        Digest  = $digest
        Before  = $before
        Changed = ($before -ne $digest)
    }
}

# ------------------------------------------------- a second checkout of the commit --
#
# Review F1-B is a claim about the *checkout*, so a script-level case needs a checkout
# that can be damaged without touching the developer's. A directory whose `.git` is a
# **file** naming this repository's object store resolves the same HEAD and the same
# commit objects while its working files are entirely its own — the same trick a linked
# worktree uses, without asking Git to register anything in this repository. Only the
# files a candidate is built from are copied, which is also the point: the tool has to
# be able to say, from the commit alone, what it expects to find, and it must refuse a
# checkout that dropped one of those files or added a file the commit does not have.
function Get-ReleaseInputPath {
    param(
        [Parameter(Mandatory = $true)][string]$Repo,
        [Parameter(Mandatory = $true)][string]$Module
    )
    $rootLength = (Resolve-Path -LiteralPath $Repo).Path.Length
    $spec = Get-Content -Raw -LiteralPath (Join-Path $Repo "tools/module-release-tool/specs/$Module.json") | ConvertFrom-Json
    $paths = @(
        'Cargo.toml',
        'Cargo.lock',
        'rust-toolchain.toml',
        "tools/module-release-tool/specs/$Module.json",
        $spec.changelog_file
    )
    foreach ($crate in @($spec.packed_crates)) {
        $directory = $crate.source_dir.TrimEnd('/')
        $paths += "$directory/Cargo.toml"
        $sourceRoot = Join-Path $Repo ($directory -replace '/', '\')
        foreach ($file in @(Get-ChildItem -LiteralPath (Join-Path $sourceRoot 'src') -Recurse -File -Force)) {
            $paths += $file.FullName.Substring($rootLength + 1) -replace '\\', '/'
        }
    }
    foreach ($set in @($spec.migration_sets)) {
        foreach ($file in @($set.files)) {
            $paths += "$($set.catalog)/$file"
        }
        $paths += $set.catalog_manifest
    }
    return @($paths | Where-Object { $_ } | Select-Object -Unique)
}

function New-IsolatedCheckout {
    param(
        [Parameter(Mandatory = $true)][string]$Repo,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][string]$Module
    )
    if (-not $env:TEMP) {
        throw 'no temporary directory to build an isolated checkout in'
    }
    $root = New-FreshDirectory -Path $Destination -What 'isolated checkout'
    $gitDirectory = (Resolve-Path -LiteralPath (Join-Path $Repo '.git')).Path
    Set-Content -LiteralPath (Join-Path $root '.git') -Value ('gitdir: ' + ($gitDirectory -replace '\\', '/')) -NoNewline -Encoding ascii
    $copied = 0
    foreach ($relative in @(Get-ReleaseInputPath -Repo $Repo -Module $Module)) {
        $source = Join-Path $Repo ($relative -replace '/', '\')
        if (-not (Test-Path -LiteralPath $source)) {
            throw "'$relative' is a release input for '$Module' but is absent from the repository; the isolated checkout cannot be built"
        }
        $target = Join-Path $root ($relative -replace '/', '\')
        Ensure-Directory (Split-Path -Parent $target)
        Copy-Item -LiteralPath $source -Destination $target -Force
        $copied++
    }
    return [pscustomobject]@{ Root = $root; Files = $copied }
}
