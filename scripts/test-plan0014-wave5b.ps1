<#
.SYNOPSIS
    PLAN-0014 Wave 5B-S1 offline catalog gate: the positive and negative evidence
    for the rules `module-release catalog-check` implements.
.DESCRIPTION
    Wave 5B-S1 is an offline candidate (design §11, review §3.8/§3.9): nothing here
    may need a database, and nothing here may mutate the repository under test. So
    every case runs against a scratch copy under the build directory (override with
    `-Scratch`), the repository itself is only ever read, and its index is never
    touched.

    Three families of cases:

    * `catalog-cases`   — every directory under
      `tools/module-release-tool/tests/fixtures/wave5b/catalogs/<case>/` is copied
      into a scratch `migrations/modules/` and run through
      `module-release catalog-check --only module-catalog`. Expected verdicts come
      from `EXPECTATIONS.json` in the same fixture directory, which is also what the
      Rust test `tools/module-release-tool/tests/wave5b_catalog.rs` reads: one table,
      two readers, so the schema rules cannot drift apart.
    * `history-cases`   — a scratch clone of this repository (so the pinned commit
      really exists there) plus one repository with no pinned commit at all. These
      are the dirty/staged/committed/rewritten/line-ending scenarios of review §3.6.
    * `freeze-cases`    — the explicit candidate-only freeze (review §3.6 rule 5),
      which is deliberately never part of the default gate.

    The reconciliation table is written to the evidence directory and the script
    exits non-zero if any case disagrees with its expectation. Cases the platform
    cannot create (a Windows symbolic link without developer privileges) are
    recorded as NOT RUN with the reason, never as a pass.
.EXAMPLE
    pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -EvidenceDir docs\reviews\<gates-dir>
#>
[CmdletBinding()]
param(
    [string]$Repo = '',
    [string]$EvidenceDir = '',
    [string]$Scratch = '',
    [string]$TargetDir = '',
    [switch]$KeepScratch,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$script:Repo = if ([string]::IsNullOrWhiteSpace($Repo)) {
    (Resolve-Path (Split-Path -Parent $PSScriptRoot)).Path
} else {
    (Resolve-Path $Repo).Path
}
$script:Scratch = if ([string]::IsNullOrWhiteSpace($Scratch)) {
    Join-Path $script:Repo 'target/tmp/plan0014-wave5b'
} else {
    $Scratch
}
$script:RunLog = [System.Collections.Generic.List[string]]::new()
$script:Results = [System.Collections.Generic.List[object]]::new()

function Write-Log([string]$Line) {
    $script:RunLog.Add($Line) | Out-Null
    Write-Host $Line
}

function Get-CanonicalPath([string]$Path) {
    <#  Returns the canonical absolute form of Path. For non-existent paths,
        resolves the parent and appends the final component. #>
    if (Test-Path -LiteralPath $Path) {
        return (Resolve-Path -LiteralPath $Path).Path
    }
    $parent = Split-Path -Parent $Path
    $child = Split-Path -Leaf $Path
    $canonicalParent = Get-CanonicalPath $parent
    return Join-Path $canonicalParent $child
}

function Assert-PathSafe([string]$Candidate, [string]$Ancestor, [string]$Label) {
    <#  B1 fix: ensures Candidate is not the same as, or an ancestor of, Ancestor. #>
    $candidateCanonical = Get-CanonicalPath $Candidate
    $ancestorCanonical = Get-CanonicalPath $Ancestor
    if ($candidateCanonical -eq $ancestorCanonical) {
        throw "${Label}: candidate path '$Candidate' equals the protected path '$Ancestor'"
    }
    if ($ancestorCanonical.StartsWith($candidateCanonical + '\')) {
        throw "${Label}: candidate path '$Candidate' is an ancestor of the protected path '$Ancestor'"
    }
}

function Assert-PathsDoNotOverlap([string]$Path1, [string]$Path2, [string]$Label1, [string]$Label2) {
    <#  B1 fix: ensures neither path is the same as or contains the other. #>
    $canonical1 = Get-CanonicalPath $Path1
    $canonical2 = Get-CanonicalPath $Path2
    if ($canonical1 -eq $canonical2) {
        throw "$Label1 and $Label2 overlap: both resolve to '$canonical1'"
    }
    if ($canonical1.StartsWith($canonical2 + '\')) {
        throw "$Label1 and $Label2 overlap: '$Label1' ('$canonical1') is under '$Label2' ('$canonical2')"
    }
    if ($canonical2.StartsWith($canonical1 + '\')) {
        throw "$Label1 and $Label2 overlap: '$Label2' ('$canonical2') is under '$Label1' ('$canonical1')"
    }
}

function Remove-Tree {
    <#  Deletes a scratch file or tree, clearing attributes and unlinking
        junctions first: Windows refuses a recursive delete of a tree that still
        holds a reparse point, and a scratch directory must never survive a run. #>
    param([string]$Path, [int]$Attempts = 5)
    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }
    if (-not (Get-Item -LiteralPath $Path).PSIsContainer) {
        Remove-Item -LiteralPath $Path -Force
        return
    }
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            Get-ChildItem -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue |
                Sort-Object { $_.FullName.Length } -Descending | ForEach-Object {
                    try {
                        if ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                            [void]$_.Delete()
                        } elseif (-not $_.PSIsContainer) {
                            $_.Attributes = 'Normal'
                            [void]$_.Delete()
                        }
                    } catch {
                        # A file the OS still holds (antivirus, an open handle) is
                        # retried with the whole tree on the next attempt.
                        Write-Verbose "deferred delete of $($_.FullName): $($_.Exception.Message)"
                    }
                }
            [System.IO.Directory]::Delete($Path, $true)
            return
        } catch {
            if ($attempt -eq $Attempts) {
                throw
            }
            Start-Sleep -Milliseconds (200 * $attempt)
        }
    }
}

function New-EvidenceDirectory([string]$Path) {
    <#  Gate evidence is history: an existing directory is never emptied
        recursively, because a previous run's evidence is exactly what a reviewer
        compares against. Use -Force to replace one deliberately. #>
    if (Test-Path -LiteralPath $Path) {
        $existing = @(Get-ChildItem -LiteralPath $Path -Force -ErrorAction SilentlyContinue)
        if ($existing.Count -gt 0) {
            if (-not $Force) {
                throw "evidence directory '$Path' already holds $($existing.Count) item(s); pass -Force to replace it, or choose a new directory"
            }
            Remove-Tree -Path $Path
        }
    }
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
    return (Resolve-Path -LiteralPath $Path).Path
}

function New-FreshDirectory([string]$Path) {
    <#  B1 fix: do not delete the passed path at all. Create a unique per-run
        subdirectory under it and return that. The caller is responsible for
        validating that Path is safe before calling this function. #>
    $runId = [guid]::NewGuid().ToString('N')
    $runDir = Join-Path $Path ("run-" + $runId)
    if (Test-Path -LiteralPath $runDir) {
        throw "run directory '$runDir' already exists; this should never happen (GUID collision or stale state)"
    }
    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    return (Resolve-Path -LiteralPath $runDir).Path
}

function Invoke-Command2 {
    <#  Runs one external command, returns its exit code and combined output.
        Raw byte capture on purpose: a historical file must never be re-encoded
        by a PowerShell text pipe (review §3.6 rule 1). #>
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory = ''
    )
    $tag = [guid]::NewGuid().ToString('N')
    $stdout = Join-Path ([System.IO.Path]::GetTempPath()) "wave5b-$tag.out"
    $stderr = Join-Path ([System.IO.Path]::GetTempPath()) "wave5b-$tag.err"
    $previous = Get-Location
    $quoted = @($Arguments | ForEach-Object {
        if ($_ -match '\s') { '"' + $_ + '"' } else { $_ }
    })
    try {
        if (-not [string]::IsNullOrWhiteSpace($WorkingDirectory)) {
            Push-Location -LiteralPath $WorkingDirectory
        }
        $start = Start-Process -FilePath $FilePath -ArgumentList $quoted `
            -NoNewWindow -Wait -PassThru `
            -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $code = $start.ExitCode
    } finally {
        if (-not [string]::IsNullOrWhiteSpace($WorkingDirectory)) {
            Pop-Location
        }
    }
    $text = ''
    foreach ($file in @($stdout, $stderr)) {
        if (Test-Path -LiteralPath $file) {
            $bytes = [System.IO.File]::ReadAllBytes($file)
            $text += [System.Text.Encoding]::UTF8.GetString($bytes)
            Remove-Item -LiteralPath $file -Force
        }
    }
    Write-Log ("$ " + $FilePath + ' ' + ($Arguments -join ' ') + " -> exit=$code")
    return [pscustomobject]@{
        Code     = $code
        Out      = $text
        OutLines = @($text -split "`r?`n" | Where-Object { $_ -ne '' })
    }
}

function Get-GitBlobBytes {
    <#  `git cat-file blob <rev> <path>` as raw bytes, with process-level
        redirection so no PowerShell text decoding can touch the content. #>
    param([string]$Repository, [string]$Spec)
    $tag = [guid]::NewGuid().ToString('N')
    $stdout = Join-Path ([System.IO.Path]::GetTempPath()) "wave5b-$tag.blob"
    $stderr = Join-Path ([System.IO.Path]::GetTempPath()) "wave5b-$tag.err"
    try {
        $start = Start-Process -FilePath 'git' -ArgumentList @(
            '--no-optional-locks', '-C', $Repository, 'cat-file', 'blob', $Spec) `
            -NoNewWindow -Wait -PassThru `
            -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        if ($start.ExitCode -ne 0) {
            $reason = ''
            if (Test-Path -LiteralPath $stderr) {
                $reason = [System.Text.Encoding]::UTF8.GetString(
                    [System.IO.File]::ReadAllBytes($stderr))
            }
            throw "git cat-file blob $Spec failed in $Repository (exit $($start.ExitCode)): $reason"
        }
        return , [System.IO.File]::ReadAllBytes($stdout)
    } finally {
        Remove-Item -LiteralPath $stdout, $stderr -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-Validator {
    param([string[]]$Arguments, [string]$WorkingDirectory = '')
    $all = @('run', '-q', '-p', 'module-release-tool')
    if (-not [string]::IsNullOrWhiteSpace($TargetDir)) {
        $all += @('--target-dir', $TargetDir)
    }
    $all += @('--') + $Arguments
    $run = Invoke-Command2 -FilePath 'cargo' -Arguments $all -WorkingDirectory $WorkingDirectory
    # A cargo or build failure must never be scored as a validator verdict: a
    # negative case that "fails" because the tool did not run is a false green.
    $recognised = @($run.OutLines | Where-Object {
            $_ -match '^module-release:' -or $_ -match '^status=' -or
            $_ -match '^(checks|namespace|order|history\[|freeze |root=)'
        })
    if ($recognised.Count -eq 0) {
        Write-Log $run.Out
        throw "the validator produced no recognisable output (exit $($run.Code)); the case cannot be scored"
    }
    return $run
}

function Add-Case {
    param(
        [string]$Family,
        [string]$Id,
        [string]$Expect,
        [string]$Actual,
        [string]$Detail
    )
    $verdict = if ($Actual -eq 'not-run') { 'BLOCKED' } elseif ($Actual -eq $Expect) { 'MATCH' } else { 'MISMATCH' }
    $line = "[$Family] case=$Id expect=$Expect actual=$Actual verdict=$verdict detail=$Detail"
    Write-Log $line
    $script:Results.Add([pscustomobject]@{
            Family   = $Family
            Case     = $Id
            Expect   = $Expect
            Actual   = $Actual
            Verdict  = $verdict
            Detail   = ($Detail -replace '\s+', ' ')
        }) | Out-Null
}

function Get-VerdictOf {
    param($Run)
    if ($Run.Code -eq 0) { 'pass' } else { 'fail' }
}

function Copy-FixtureIntoModules {
    <#  Copies one fixture case into <work>/migrations/modules/. #>
    param([string]$Work, [string]$Source)
    $modules = Join-Path $Work 'migrations/modules'
    New-Item -ItemType Directory -Force -Path $modules | Out-Null
    Copy-Item -Path (Join-Path $Source '*') -Destination $modules -Recurse -Force
}

# ---------------------------------------------------------------------------
# 0 — preparation
# ---------------------------------------------------------------------------
$fixtureRoot = Join-Path $script:Repo 'tools/module-release-tool/tests/fixtures/wave5b/catalogs'
$expectationPath = Join-Path $fixtureRoot 'EXPECTATIONS.json'
if (-not (Test-Path -LiteralPath $expectationPath)) {
    throw "missing shared expectations: $expectationPath"
}
$keepEvidenceOutsideScratch = -not [string]::IsNullOrWhiteSpace($EvidenceDir)

# B1 fix: validate all path relationships before any deletion or creation.
$canonicalRepo = Get-CanonicalPath $script:Repo
$scratchPath = if ([string]::IsNullOrWhiteSpace($Scratch)) {
    Join-Path $script:Repo 'target/tmp/plan0014-wave5b'
} else {
    $Scratch
}
# For a non-existent scratch, resolve the parent first.
if (-not (Test-Path -LiteralPath $scratchPath)) {
    $scratchParent = Split-Path -Parent $scratchPath
    if (-not (Test-Path -LiteralPath $scratchParent)) {
        throw "scratch parent '$scratchParent' does not exist; cannot validate scratch path"
    }
    $scratchCanonicalParent = Get-CanonicalPath $scratchParent
    $scratchLeaf = Split-Path -Leaf $scratchPath
    $script:Scratch = Join-Path $scratchCanonicalParent $scratchLeaf
} else {
    $script:Scratch = (Resolve-Path -LiteralPath $scratchPath).Path
}
$canonicalScratch = Get-CanonicalPath $script:Scratch

# Reject scratch that equals or is an ancestor of the repo.
Assert-PathSafe -Candidate $canonicalScratch -Ancestor $canonicalRepo -Label "scratch validation"
# Reject scratch that is a drive root or filesystem root.
if ($canonicalScratch -match '^[A-Z]:\\$' -or $canonicalScratch -eq (Get-Location -PSProvider FileSystem).ProviderPath) {
    throw "scratch path '$script:Scratch' is a filesystem root; refuse to operate on root directories"
}

# Prepare evidence directory path (but do not create it yet).
if ($keepEvidenceOutsideScratch) {
    if ([string]::IsNullOrWhiteSpace($EvidenceDir)) {
        throw "-EvidenceDir must be specified when not keeping evidence inside scratch"
    }
    $evidencePath = if (Split-Path -IsAbsolute $EvidenceDir) {
        $EvidenceDir
    } else {
        Join-Path $script:Repo $EvidenceDir
    }
    $canonicalEvidence = Get-CanonicalPath $evidencePath
    # Ensure evidence doesn't overlap with scratch.
    Assert-PathsDoNotOverlap -Path1 $canonicalScratch -Path2 $canonicalEvidence -Label1 "scratch" -Label2 "evidence"
} else {
    $canonicalEvidence = $null
}

# R4 fix: every run root this run creates is registered together with the
# parent directory validated for it, and cleanup may delete exactly the
# registered roots themselves — never the parent the caller handed in, and
# never a path that was not registered. Protected paths (the repository, the
# scratch parent, an external evidence directory) refuse deletion even when a
# registration were wrong.
$script:RunRoots = [System.Collections.Generic.List[object]]::new()
$script:ProtectedPaths = [System.Collections.Generic.List[string]]::new()
$script:ProtectedPaths.Add($canonicalRepo)
$script:ProtectedPaths.Add($canonicalScratch)
if ($keepEvidenceOutsideScratch) {
    $script:ProtectedPaths.Add($canonicalEvidence)
}

function Register-RunRoot([string]$Root, [string]$Parent, [string]$Kind) {
    <#  Registers one run-scoped directory tree for guaranteed cleanup. The
        root must resolve inside the validated parent it is registered under. #>
    $canonicalRootPath = Get-CanonicalPath $Root
    $canonicalParentPath = (Get-CanonicalPath $Parent).TrimEnd('\')
    if (-not $canonicalRootPath.ToLowerInvariant().StartsWith(
            $canonicalParentPath.ToLowerInvariant() + '\')) {
        throw "run root '$canonicalRootPath' is not inside its validated parent '$canonicalParentPath'"
    }
    $script:RunRoots.Add([pscustomobject]@{
        Root   = $canonicalRootPath
        Parent = $canonicalParentPath
        Kind   = $Kind
    }) | Out-Null
    return $canonicalRootPath
}

function Remove-RunScratch {
    <#  R4: deletes exactly the registered run roots. Each root is re-checked
        against its registered parent, the protected paths and a case-
        insensitive separator boundary before anything is removed; the internal
        evidence subtree survives by design (review repair R4). #>
    if ($KeepScratch) {
        foreach ($entry in $script:RunRoots) {
            Write-Log "kept scratch root ($($entry.Kind)): $($entry.Root)"
        }
        return
    }
    foreach ($entry in $script:RunRoots) {
        $root = $entry.Root
        try {
            if (-not (Test-Path -LiteralPath $root)) {
                continue
            }
            $canonicalPath = Get-CanonicalPath $root
            $lowerRoot = $canonicalPath.ToLowerInvariant()
            $parentPrefix = $entry.Parent.ToLowerInvariant().TrimEnd('\') + '\'
            if ($lowerRoot -ne $root.ToLowerInvariant() -or
                -not $lowerRoot.StartsWith($parentPrefix)) {
                Write-Warning "refusing to delete '$root': it no longer resolves inside its registered parent '$($entry.Parent)'"
                continue
            }
            $blocked = $false
            foreach ($protected in $script:ProtectedPaths) {
                $lowerProtected = (Get-CanonicalPath $protected).ToLowerInvariant().TrimEnd('\')
                if ($lowerRoot -eq $lowerProtected -or
                    $lowerProtected.StartsWith($lowerRoot + '\')) {
                    Write-Warning "refusing to delete '$root': it is or contains the protected path '$protected'"
                    $blocked = $true
                    break
                }
            }
            if ($blocked) {
                continue
            }
            if ($entry.Kind -eq 'main' -and $script:InternalEvidence) {
                # Default evidence lives inside the run: the evidence subtree
                # is kept by explicit rule, everything else in the root goes.
                $keep = (Get-CanonicalPath $script:EvidenceInsideRun).ToLowerInvariant()
                Get-ChildItem -LiteralPath $root -Force | ForEach-Object {
                    if ((Get-CanonicalPath $_.FullName).ToLowerInvariant() -ne $keep) {
                        Remove-Tree -Path $_.FullName
                    }
                }
                Write-Log "internal evidence retained at '$script:EvidenceInsideRun'; the rest of the run root was cleaned"
            } else {
                Remove-Tree -Path $root
            }
        } catch {
            Write-Warning "scratch root '$root' could not be removed: $($_.Exception.Message)"
        }
    }
}

try {

$scratchRoot = Register-RunRoot -Root (New-FreshDirectory -Path $script:Scratch) `
    -Parent $script:Scratch -Kind 'main'

if (-not $keepEvidenceOutsideScratch) {
    $evidence = New-EvidenceDirectory -Path (Join-Path $scratchRoot 'evidence')
    $script:InternalEvidence = $true
    $script:EvidenceInsideRun = Join-Path $scratchRoot 'evidence'
} elseif (Split-Path -IsAbsolute $EvidenceDir) {
    $evidence = New-EvidenceDirectory -Path $EvidenceDir
    $script:InternalEvidence = $false
} else {
    $evidence = New-EvidenceDirectory -Path (Join-Path $script:Repo $EvidenceDir)
    $script:InternalEvidence = $false
}

Write-Log "wave5b-s1 offline catalog gate"
Write-Log "repo=$($script:Repo)"
Write-Log ("repository head=" + (Invoke-Command2 -FilePath 'git' -Arguments @('-C', $script:Repo, 'rev-parse', 'HEAD')).Out.Trim())
Write-Log ("git=" + (Invoke-Command2 -FilePath 'git' -Arguments @('--version')).Out.Trim())
Write-Log ("pwsh=" + $PSVersionTable.PSVersion.ToString())
Write-Log "scratch=$scratchRoot"
Write-Log "evidence=$evidence"
Write-Log "repository index and work tree are never written by this script"

$buildArguments = @('build', '-q', '-p', 'module-release-tool')
if (-not [string]::IsNullOrWhiteSpace($TargetDir)) {
    $buildArguments += @('--target-dir', $TargetDir)
    Write-Log "cargo target-dir=$TargetDir"
}
$build = Invoke-Command2 -FilePath 'cargo' -Arguments $buildArguments
if ($build.Code -ne 0) {
    Write-Log $build.Out
    throw "cargo build -p module-release-tool failed (exit $($build.Code))"
}

# ---------------------------------------------------------------------------
# 1 — catalog fixtures: one shared expectation table, two readers
# ---------------------------------------------------------------------------
$expectations = Get-Content -LiteralPath $expectationPath -Raw | ConvertFrom-Json
foreach ($case in $expectations.cases) {
    $work = Join-Path $scratchRoot ("catalog-" + $case.case)
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    Copy-FixtureIntoModules -Work $work -Source (Join-Path $fixtureRoot $case.case)
    $run = Invoke-Validator -Arguments @('catalog-check', '--repo', $work, '--only', 'module-catalog')
    $actual = Get-VerdictOf $run
    $detail = ''
    if ($case.PSObject.Properties.Name -contains 'contains') {
        if ($run.Out -notmatch [regex]::Escape([string]$case.contains)) {
            $detail = "expected-substring-missing='$($case.contains)'"
            $actual = 'mismatch-detail'
        } else {
            $detail = "matched-substring='$($case.contains)'"
        }
    }
    if ($case.PSObject.Properties.Name -contains 'status') {
        if ($run.Out -notmatch [regex]::Escape("status=$($case.status)")) {
            $detail = "expected-status='$($case.status)'"
            $actual = 'mismatch-detail'
        } else {
            $detail = "status=$($case.status)"
        }
    }
    if ([string]::IsNullOrWhiteSpace($detail)) {
        $detail = ($run.OutLines | Select-Object -Last 1)
    }
    Add-Case -Family 'catalog-cases' -Id $case.case -Expect $case.expect -Actual $actual -Detail $detail
}

# The fixture tree must cover every directory it holds, so a case can never be
# added without an expectation (a silently untested fixture is a false green).
$covered = @($expectations.cases | ForEach-Object { $_.case })
foreach ($directory in (Get-ChildItem -LiteralPath $fixtureRoot -Directory | Sort-Object Name)) {
    if ($covered -notcontains $directory.Name) {
        Add-Case -Family 'catalog-cases' -Id $directory.Name -Expect 'covered' -Actual 'uncovered' `
            -Detail 'fixture directory has no entry in EXPECTATIONS.json'
    }
}

# ---------------------------------------------------------------------------
# 2 — pinned history scenarios in a scratch clone
# ---------------------------------------------------------------------------
$clone = Join-Path $scratchRoot 'history-repo'
$cloned = Invoke-Command2 -FilePath 'git' -Arguments @(
    'clone', '--quiet', '--no-checkout', $script:Repo, $clone)
if ($cloned.Code -ne 0) {
    Write-Log $cloned.Out
    throw "the scratch clone failed; the history scenarios cannot run: $($cloned.Out)"
}
$head = (Invoke-Command2 -FilePath 'git' -Arguments @('-C', $script:Repo, 'rev-parse', 'HEAD')).Out.Trim()
$checkedOut = Invoke-Command2 -FilePath 'git' -Arguments @('-C', $clone, 'checkout', '--quiet', '--force', $head)
if ($checkedOut.Code -ne 0) {
    Write-Log $checkedOut.Out
    throw "the scratch clone could not check out $head"
}
$pinned = '5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601'

function Restore-Clone([string]$Repository) {
    [void](Invoke-Command2 -FilePath 'git' -Arguments @('-C', $Repository, 'checkout', '--quiet', '--force', $head))
    [void](Invoke-Command2 -FilePath 'git' -Arguments @('-C', $Repository, 'reset', '--quiet', '--hard', $head))
    [void](Invoke-Command2 -FilePath 'git' -Arguments @('-C', $Repository, 'clean', '-qfdx'))
}

function Get-HistoryVerdict([string]$Repository, [string[]]$ExtraArguments = @()) {
    return Invoke-Validator -Arguments (@('catalog-check', '--repo', $Repository, '--only', 'pinned-baseline-history') + $ExtraArguments)
}

# H1 — a clean clone of the pinned history.
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H1-clean-clone' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'baseline_revision=' }) -join ' ')

# H2 — a historical SQL rewritten in the working tree only.
$target = Join-Path $clone 'migrations/009_document_content_revision.sql'
$body = [System.IO.File]::ReadAllBytes($target)
$extra = [System.Text.Encoding]::UTF8.GetBytes("`n-- drift`n")
$both = New-Object byte[] ($body.Length + $extra.Length)
[Array]::Copy($body, 0, $both, 0, $body.Length)
[Array]::Copy($extra, 0, $both, $body.Length, $extra.Length)
[System.IO.File]::WriteAllBytes($target, $both)
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H2-worktree-drift' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
Restore-Clone $clone

# H3 — line endings only in the working tree: the documented tolerance.
$path = 'migrations/009_document_content_revision.sql'
$pinnedBlob = Get-GitBlobBytes -Repository $clone -Spec "${pinned}:$path"
$crlf = [System.Collections.Generic.List[byte]]::new()
foreach ($byte in $pinnedBlob) {
    if ($byte -eq 0x0A) { $crlf.Add(0x0D) }
    $crlf.Add($byte)
}
[System.IO.File]::WriteAllBytes((Join-Path $clone ($path -replace '/', [System.IO.Path]::DirectorySeparatorChar)), $crlf.ToArray())
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H3-worktree-line-endings-only' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail 'CRLF checkout of the pinned LF blob'
Restore-Clone $clone

# H4 — staged change (index), work tree restored.
$path = 'migrations/013_runtime_governance_revision1.sql'
$absolute = Join-Path $clone ($path -replace '/', [System.IO.Path]::DirectorySeparatorChar)
[System.IO.File]::WriteAllBytes($absolute, [System.Text.Encoding]::UTF8.GetBytes("-- staged drift`n"))
[void](Invoke-Command2 -FilePath 'git' -Arguments @('-C', $clone, 'add', '--', $path))
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H4-staged-drift' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
Restore-Clone $clone

# H5 — committed change, i.e. HEAD itself no longer matches the pinned commit.
[System.IO.File]::WriteAllBytes($absolute, [System.Text.Encoding]::UTF8.GetBytes("-- committed drift`n"))
[void](Invoke-Command2 -FilePath 'git' -Arguments @('-C', $clone, 'add', '--', $path))
[void](Invoke-Command2 -FilePath 'git' -Arguments @(
    '-C', $clone, '-c', 'user.email=wave5b@example.invalid', '-c', 'user.name=wave5b',
    'commit', '-q', '-m', 'wave5b negative: rewrite a historical migration'))
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H5-committed-drift' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
Restore-Clone $clone

# H6 — a historical SQL deleted from the working tree.
Remove-Item -LiteralPath $absolute -Force
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H6-worktree-deleted' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
Restore-Clone $clone

# H7 — a new baseline SQL added next to the frozen 19, nothing else touched.
[System.IO.File]::WriteAllBytes((Join-Path $clone 'migrations\020_wave5b_negative.sql'),
    [System.Text.Encoding]::UTF8.GetBytes("-- negative: a 20th baseline SQL`n"))
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H7-new-baseline-sql-history' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail 'immutability covers the pinned 19; the count rule is the explicit freeze'
$run = Invoke-Validator -Arguments @('catalog-check', '--repo', $clone, '--only', 'baseline-candidate-freeze')
Add-Case -Family 'freeze-cases' -Id 'F2-extra-baseline-sql' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')

# H8 — the historical manifest rewritten to describe that new file (both sides
#      "consistent" with each other, neither consistent with the pinned commit).
$manifestPath = Join-Path $clone 'migrations\MANIFEST.sha256'
$manifestText = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes($manifestPath))
$manifestText += ('{0}  020_wave5b_negative.sql' -f ('0' * 64)) + "`n"
[System.IO.File]::WriteAllBytes($manifestPath, [System.Text.Encoding]::UTF8.GetBytes($manifestText))
$run = Get-HistoryVerdict $clone
Add-Case -Family 'history-cases' -Id 'H8-manifest-extended' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
Restore-Clone $clone

# H11 — the manifest alone is rewritten: one listed digest is replaced by a
#       well-formed digest of something else, every SQL file stays untouched.
#       Only the manifest blob can catch this.
$manifestPath = Join-Path $clone 'migrations\MANIFEST.sha256'
# A clone of this repository checks the historical manifest out with CRLF
# (core.autocrlf), so the case edits the row it means to edit and puts the file
# back in the same form it had: an EOL-only change is the tolerance of H3, not
# the drift this case is about.
$manifestLines = @([System.Text.Encoding]::UTF8.GetString(
    [System.IO.File]::ReadAllBytes($manifestPath)) -split "`r?`n")
for ($index = 0; $index -lt $manifestLines.Count; $index++) {
    if ($manifestLines[$index] -match '^([0-9a-f]{64})(  001_initial\.sql)$') {
        $manifestLines[$index] = ('{0}{1}' -f ('b' * 64), $Matches[2])
        break
    }
}
[System.IO.File]::WriteAllBytes($manifestPath,
    [System.Text.Encoding]::UTF8.GetBytes(($manifestLines -join "`r`n")))
$run = Get-HistoryVerdict $clone
$detail = (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
$actual = Get-VerdictOf $run
if ($run.Out -notmatch 'MANIFEST\.sha256') {
    $actual = 'mismatch-detail'
    $detail = "expected the refusal to name the manifest, got: $detail"
}
Add-Case -Family 'history-cases' -Id 'H11-manifest-only-drift' -Expect 'fail' -Actual $actual -Detail $detail
Restore-Clone $clone

# H12 — a line is removed from the manifest: 19 baseline files, 18 rows.
$manifestPath = Join-Path $clone 'migrations\MANIFEST.sha256'
$manifestLines = @([System.Text.Encoding]::UTF8.GetString(
    [System.IO.File]::ReadAllBytes($manifestPath)) -split "`r?`n")
$kept = @($manifestLines | Where-Object { $_ -notmatch '  001_initial\.sql$' })
if ($kept.Count -ge $manifestLines.Count) {
    Add-Case -Family 'history-cases' -Id 'H12-manifest-line-removed' -Expect 'fail' `
        -Actual 'mismatch-detail' -Detail 'the fixture did not find a 001_initial.sql row to remove'
} else {
    [System.IO.File]::WriteAllBytes($manifestPath,
        [System.Text.Encoding]::UTF8.GetBytes(($kept -join "`r`n")))
    $run = Get-HistoryVerdict $clone
    $detail = (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
    $actual = Get-VerdictOf $run
    if ($run.Out -notmatch 'MANIFEST\.sha256') {
        $actual = 'mismatch-detail'
        $detail = "expected the refusal to name the manifest, got: $detail"
    }
    Add-Case -Family 'history-cases' -Id 'H12-manifest-line-removed' -Expect 'fail' -Actual $actual -Detail $detail
}
Restore-Clone $clone

# H9 — a module SQL under migrations/modules/ is not baseline: the freeze must
#      pass with it present (review §3.6 rule 5, design §11.3 #1).
$moduleCatalog = Join-Path $clone 'migrations\modules\policy'
New-Item -ItemType Directory -Force -Path $moduleCatalog | Out-Null
Copy-Item -Path (Join-Path $fixtureRoot 'validated/policy/*') `
    -Destination $moduleCatalog -Recurse -Force
$run = Invoke-Validator -Arguments @('catalog-check', '--repo', $clone, '--only', 'baseline-candidate-freeze')
Add-Case -Family 'freeze-cases' -Id 'F3-module-sql-not-baseline' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'freeze ' }) -join ' ')
$run = Invoke-Validator -Arguments @('catalog-check', '--repo', $clone, '--only', 'module-catalog')
Add-Case -Family 'catalog-cases' -Id 'F3-module-sql-catalog' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'status=VALIDATED' }) -join ' ')

# H10 — a repository that does not contain the pinned commit at all.
$orphan = Join-Path $scratchRoot 'no-pinned-commit'
New-Item -ItemType Directory -Force -Path $orphan | Out-Null
Copy-Item -Path (Join-Path $script:Repo 'migrations\*') -Destination (Join-Path $orphan 'migrations') -Recurse -Force
[void](Invoke-Command2 -FilePath 'git' -Arguments @('init', '-q', $orphan))
[void](Invoke-Command2 -FilePath 'git' -Arguments @('-C', $orphan, 'add', '-A'))
[void](Invoke-Command2 -FilePath 'git' -Arguments @(
    '-C', $orphan, '-c', 'user.email=wave5b@example.invalid', '-c', 'user.name=wave5b',
    'commit', '-q', '-m', 'wave5b negative: a history without the pinned commit'))
$run = Get-HistoryVerdict $orphan
$detail = (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
$actual = Get-VerdictOf $run
if ($run.Out -notmatch 'does not fall back') {
    $actual = 'mismatch-detail'
    $detail = "expected an explicit no-fallback refusal, got: $detail"
}
Add-Case -Family 'history-cases' -Id 'H10-no-pinned-commit' -Expect 'fail' -Actual $actual -Detail $detail
Restore-Clone $clone

# ---------------------------------------------------------------------------
# 3 — freeze scenarios on a plain scratch copy (no Git needed)
# ---------------------------------------------------------------------------
$plain = Join-Path $scratchRoot 'freeze-copy'
New-Item -ItemType Directory -Force -Path (Join-Path $plain 'migrations') | Out-Null
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $script:Repo 'migrations') -File) {
    Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $plain 'migrations') -Force
}
Copy-Item -Path (Join-Path $script:Repo 'migrations\modules') -Destination (Join-Path $plain 'migrations') -Recurse -Force
$run = Invoke-Validator -Arguments @('catalog-check', '--repo', $plain, '--only', 'baseline-candidate-freeze')
Add-Case -Family 'freeze-cases' -Id 'F1-clean-copy' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'freeze ' }) -join ' ')

New-Item -ItemType Directory -Force -Path (Join-Path $plain 'migrations\archive') | Out-Null
[System.IO.File]::WriteAllBytes((Join-Path $plain 'migrations\archive\000_archived.sql'),
    [System.Text.Encoding]::UTF8.GetBytes("-- negative: a subdirectory inside the baseline`n"))
$run = Invoke-Validator -Arguments @('catalog-check', '--repo', $plain, '--only', 'baseline-candidate-freeze')
Add-Case -Family 'freeze-cases' -Id 'F4-extra-subdirectory' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')

# F5 — a stray non-SQL file inside the baseline directory.
[System.IO.File]::WriteAllBytes((Join-Path $plain 'migrations\NOTES.txt'),
    [System.Text.Encoding]::UTF8.GetBytes("negative: a stray file in the baseline directory`n"))
$run = Invoke-Validator -Arguments @('catalog-check', '--repo', $plain, '--only', 'baseline-candidate-freeze')
Add-Case -Family 'freeze-cases' -Id 'F5-stray-file' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')

# ---------------------------------------------------------------------------
# 3b — single baseline embed point scenarios (review §3.7)
# ---------------------------------------------------------------------------
# The embed scan reads source files, not Git objects, so it runs against a copy
# of the *working tree* (a clone at HEAD would still contain the pre-fix test
# files and would say nothing about the code under review).
$embedTree = Join-Path $scratchRoot 'embed-tree'
New-Item -ItemType Directory -Force -Path $embedTree | Out-Null
foreach ($tree in @('apps', 'crates', 'migrations')) {
    $source = Join-Path $script:Repo $tree
    if (Test-Path -LiteralPath $source) {
        Copy-Item -Path $source -Destination $embedTree -Recurse -Force
    }
}
foreach ($noise in @('target', 'node_modules')) {
    Get-ChildItem -LiteralPath $embedTree -Recurse -Directory -Filter $noise -ErrorAction SilentlyContinue |
        Sort-Object { $_.FullName.Length } -Descending | ForEach-Object { Remove-Tree -Path $_.FullName }
}
Write-Log "embed-tree=$embedTree (working tree copy of apps/ crates/ migrations/)"

function Invoke-EmbedCheck([string]$Repository) {
    return Invoke-Validator -Arguments @('catalog-check', '--repo', $Repository, '--only', 'baseline-embed')
}

function Get-FailureDetail($Run) {
    $named = @($Run.OutLines | Where-Object { $_ -match 'module-release:' })
    if ($named.Count -gt 0) { return ($named -join ' ') }
    return (($Run.OutLines | Select-Object -Last 3) -join ' | ')
}

$run = Invoke-EmbedCheck $embedTree
Add-Case -Family 'embed-cases' -Id 'E1-working-tree' -Expect 'pass' -Actual (Get-VerdictOf $run) `
    -Detail (($run.OutLines | Where-Object { $_ -match '^embed scanned' }) -join ' ')

# E2 — a second crate embeds the host baseline catalog: the failure this rule
#      exists for (review P-4, the two PostgreSQL test call sites of §3.7).
$second = Join-Path $embedTree 'crates\messaging\src\second_embed.rs'
[System.IO.File]::WriteAllText(
    $second,
    "pub static SECOND: sqlx::migrate::Migrator = sqlx::migrate!(`"../../migrations`");`n",
    [System.Text.UTF8Encoding]::new($false))
$run = Invoke-EmbedCheck $embedTree
Add-Case -Family 'embed-cases' -Id 'E2-second-baseline-embed' -Expect 'fail' -Actual (Get-VerdictOf $run) `
    -Detail (Get-FailureDetail $run)
Remove-Tree -Path $second

# E3 — the argument is not a plain literal: the scan must refuse to guess.
[System.IO.File]::WriteAllText(
    $second,
    'pub static THIRD: sqlx::migrate::Migrator = sqlx::migrate!(std::env!("MIGRATIONS"));' + "`n",
    [System.Text.UTF8Encoding]::new($false))
$run = Invoke-EmbedCheck $embedTree
$detail = Get-FailureDetail $run
$actual = Get-VerdictOf $run
if ($run.Out -notmatch 'plain string literal') {
    $actual = 'mismatch-detail'
    $detail = "expected a 'cannot decide' refusal, got: $detail"
}
Add-Case -Family 'embed-cases' -Id 'E3-non-literal-argument' -Expect 'fail' -Actual $actual -Detail $detail
Remove-Tree -Path $second

# E4 — the allowed embed point is aimed at another crate's migrations: it leaves
#      both the host baseline and its own crate, which is refused by name.
$lib = Join-Path $embedTree 'crates\runtime-migration\src\lib.rs'
$originalLib = [System.IO.File]::ReadAllText($lib)
[System.IO.File]::WriteAllText(
    $lib,
    $originalLib.Replace('sqlx::migrate!("../../migrations")', 'sqlx::migrate!("../document-sqlite/migrations")'),
    [System.Text.UTF8Encoding]::new($false))
$run = Invoke-EmbedCheck $embedTree
$detail = Get-FailureDetail $run
$actual = Get-VerdictOf $run
if ($run.Out -notmatch 'leaves the owning crate') {
    $actual = 'mismatch-detail'
    $detail = "expected the cross-crate embed refusal, got: $detail"
}
Add-Case -Family 'embed-cases' -Id 'E4-embed-leaves-owning-crate' -Expect 'fail' -Actual $actual -Detail $detail

# E4b — the embed point is gone entirely: silently losing the baseline
#       migrations is exactly what the check must not allow to pass.
[System.IO.File]::WriteAllText(
    $lib,
    $originalLib.Replace('sqlx::migrate!("../../migrations")', 'Migrator::default()'),
    [System.Text.UTF8Encoding]::new($false))
$run = Invoke-EmbedCheck $embedTree
$detail = Get-FailureDetail $run
$actual = Get-VerdictOf $run
if ($run.Out -notmatch 'no file embeds the host baseline catalog') {
    $actual = 'mismatch-detail'
    $detail = "expected the missing-baseline-embed refusal, got: $detail"
}
Add-Case -Family 'embed-cases' -Id 'E4b-baseline-embed-missing' -Expect 'fail' -Actual $actual -Detail $detail
[System.IO.File]::WriteAllText($lib, $originalLib, [System.Text.UTF8Encoding]::new($false))

# E5 — the two conformance tests must go through the shared migrator instead of
#      embedding the baseline a second time (regression guard on the P-4 fix).
foreach ($site in @(
        'crates\messaging\tests\outbox_reliability.rs',
        'crates\audit-postgres\tests\chain_pg.rs'
    )) {
    $body = [System.IO.File]::ReadAllText((Join-Path $embedTree $site))
    $name = 'E5-' + [System.IO.Path]::GetFileName($site)
    if ($body -match '(?m)^\s*(runtime_migration::MIGRATOR|sqlx::migrate!)') {
        $used = if ($Matches[1] -eq 'sqlx::migrate!') { 'fail' } else { 'pass' }
        Add-Case -Family 'embed-cases' -Id $name -Expect 'pass' -Actual $used `
            -Detail $(if ($used -eq 'pass') { 'applies the baseline through runtime_migration::MIGRATOR' }
                      else { 'embeds a migration catalog instead of runtime_migration::MIGRATOR' })
    } else {
        Add-Case -Family 'embed-cases' -Id $name -Expect 'pass' -Actual 'mismatch-detail' `
            -Detail 'neither runtime_migration::MIGRATOR nor sqlx::migrate! is present'
    }
}

# ---------------------------------------------------------------------------
# 4 — link fixtures: created at run time, recorded as NOT RUN when refused
# ---------------------------------------------------------------------------
# The repository volume may not be able to hold a reparse point at all (an exFAT
# volume answers "the function is not correct" for both junctions and symbolic
# links), so the link scratch falls back to the OS temp volume. The rule under
# test is the validator's refusal, not the drive it runs on.
function Test-ReparseCapable([string]$Directory) {
    $probe = Join-Path $Directory 'link-probe'
    New-Item -ItemType Directory -Force -Path $probe | Out-Null
    $capable = $true
    try {
        New-Item -ItemType Junction -Path (Join-Path $Directory 'link-probe-junction') `
            -Target $probe -ErrorAction Stop | Out-Null
    } catch {
        $capable = $false
    }
    Remove-Tree -Path (Join-Path $Directory 'link-probe-junction')
    Remove-Tree -Path $probe
    return $capable
}

$linkScratch = $scratchRoot
if (-not (Test-ReparseCapable $scratchRoot)) {
    # B1 fix: use a unique GUID-based directory name instead of a fixed name
    # that would wipe out other runs.
    $linkScratchRunId = [guid]::NewGuid().ToString('N')
    $linkScratch = Register-RunRoot -Root (
        Join-Path ([System.IO.Path]::GetTempPath()) ("plan0014-wave5b-links-" + $linkScratchRunId)
    ) -Parent ([System.IO.Path]::GetTempPath()) -Kind 'fallback'
    New-Item -ItemType Directory -Force -Path $linkScratch | Out-Null
    Write-Log ("scratch volume cannot hold reparse points; link cases use $linkScratch")
}
Write-Log ("link-scratch=$linkScratch reparse-capable=" + (Test-ReparseCapable $linkScratch))

$link = Join-Path $linkScratch 'link-target'
New-Item -ItemType Directory -Force -Path $link | Out-Null
Copy-FixtureIntoModules -Work $link -Source (Join-Path $fixtureRoot 'validated')

$linkedWork = Join-Path $linkScratch 'link-junction'
New-Item -ItemType Directory -Force -Path (Join-Path $linkedWork 'migrations/modules') | Out-Null
$junctionCreated = $true
$junctionError = ''
try {
    New-Item -ItemType Junction -Path (Join-Path $linkedWork 'migrations/modules/policy') `
        -Target (Join-Path $link 'migrations/modules/policy') -ErrorAction Stop | Out-Null
} catch {
    $junctionCreated = $false
    $junctionError = $_.Exception.Message
}
if ($junctionCreated) {
    $run = Invoke-Validator -Arguments @('catalog-check', '--repo', $linkedWork, '--only', 'module-catalog')
    $actual = Get-VerdictOf $run
    $detail = (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
    if ($run.Out -notmatch 'reparse point') {
        $actual = 'mismatch-detail'
        $detail = "expected a reparse-point refusal, got: $detail"
    }
    Add-Case -Family 'path-cases' -Id 'P1-namespace-junction' -Expect 'fail' -Actual $actual -Detail $detail
} else {
    Add-Case -Family 'path-cases' -Id 'P1-namespace-junction' -Expect 'fail' -Actual 'not-run' `
        -Detail "the host refused to create a junction: $junctionError"
}

$symlinkWork = Join-Path $linkScratch 'link-symlink'
Copy-FixtureIntoModules -Work $symlinkWork -Source (Join-Path $fixtureRoot 'validated')
$catalogFile = Join-Path $symlinkWork 'migrations/modules/policy/catalog.json'
$symlinkCreated = $true
$symlinkError = ''
try {
    Move-Item -LiteralPath $catalogFile -Destination (Join-Path $link 'moved-catalog.json') -Force
    New-Item -ItemType SymbolicLink -Path $catalogFile `
        -Target (Join-Path $link 'moved-catalog.json') -ErrorAction Stop | Out-Null
} catch {
    $symlinkCreated = $false
    $symlinkError = $_.Exception.Message
}
if ($symlinkCreated) {
    $run = Invoke-Validator -Arguments @('catalog-check', '--repo', $symlinkWork, '--only', 'module-catalog')
    $actual = Get-VerdictOf $run
    $detail = (($run.OutLines | Where-Object { $_ -match 'module-release:' }) -join ' ')
    if ($run.Out -notmatch 'symbolic link or reparse point') {
        $actual = 'mismatch-detail'
        $detail = "expected a symbolic-link refusal, got: $detail"
    }
    Add-Case -Family 'path-cases' -Id 'P2-catalog-symlink' -Expect 'fail' -Actual $actual -Detail $detail
} else {
    Add-Case -Family 'path-cases' -Id 'P2-catalog-symlink' -Expect 'fail' -Actual 'not-run' `
        -Detail "the host refused to create a file symbolic link: $symlinkError (the repository volume is exFAT, which holds no reparse points)"
}

# ---------------------------------------------------------------------------
# 5 — reconciliation
# ---------------------------------------------------------------------------
$mismatched = @($script:Results | Where-Object { $_.Verdict -eq 'MISMATCH' })
$notRun = @($script:Results | Where-Object { $_.Expect -ne $_.Actual -and $_.Actual -eq 'not-run' })

foreach ($family in @('catalog-cases', 'history-cases', 'embed-cases', 'freeze-cases', 'path-cases')) {
    $rows = @($script:Results | Where-Object { $_.Family -eq $family })
    Write-Log ("summary {0} cases={1} matched={2} blocked={3} mismatched={4}" -f `
            $family, $rows.Count, `
            (@($rows | Where-Object { $_.Verdict -eq 'MATCH' -and $_.Actual -ne 'not-run' }).Count), `
            (@($rows | Where-Object { $_.Actual -eq 'not-run' }).Count), `
            (@($rows | Where-Object { $_.Verdict -eq 'MISMATCH' -and $_.Actual -ne 'not-run' }).Count))
}
Write-Log ("summary total cases=" + $script:Results.Count +
    " mismatched=" + $mismatched.Count + " blocked=" + $notRun.Count)
if ($notRun.Count -gt 0) {
    Write-Log "BLOCKED (could not run on this platform):"
    foreach ($row in $notRun) {
        Write-Log "  blocked [$($row.Family)] $($row.Case): $($row.Detail)"
    }
}

$script:Results |
    Select-Object Family, Case, Expect, Actual, Verdict, Detail |
    Export-Csv -LiteralPath (Join-Path $evidence 'catalog-reconciliation.csv') -NoTypeInformation -Encoding utf8
Set-Content -LiteralPath (Join-Path $evidence 'catalog-gate-log.txt') -Value $script:RunLog -Encoding utf8
foreach ($family in @('catalog-cases', 'history-cases', 'embed-cases', 'freeze-cases', 'path-cases')) {
    @($script:Results | Where-Object { $_.Family -eq $family }) |
        Select-Object Case, Expect, Actual, Verdict, Detail |
        Export-Csv -LiteralPath (Join-Path $evidence ("$family.csv")) -NoTypeInformation -Encoding utf8
}
Write-Log "evidence written to $evidence"

# Scratch cleanup: explicit at the end, and on the way out for an abnormal exit,
# so a run never leaves a repository clone behind in the build directory unless
# `-KeepScratch` asked for it.
Write-Log "scratch: $scratchRoot"
} finally {
    Remove-RunScratch
}

Write-Log "scratch: $scratchRoot"

if ($mismatched.Count -gt 0) {
    foreach ($row in $mismatched) {
        Write-Host "MISMATCH [$($row.Family)] $($row.Case): expect=$($row.Expect) actual=$($row.Actual) $($row.Detail)"
    }
    throw "$($mismatched.Count) Wave 5B-S1 case(s) disagreed with their expectation"
}
if ($notRun.Count -gt 0) {
    Write-Host ("Wave 5B-S1 offline catalog gate: every case that could run matched its " +
        "expectation; $($notRun.Count) case(s) are BLOCKED by this platform and are recorded " +
        'as such (see the BLOCKED lines above and the evidence directory).')
} else {
    Write-Host 'Wave 5B-S1 offline catalog gate: every case matched its expectation.'
}
