# PLAN-0014 Wave 5A — local release-candidate assembly, verification and isolation.
#
# What this script does, in order, with every command, its exit code, its
# expectation and its raw output recorded under -EvidenceDir:
#
#   1. record the toolchain, the git state and where cargo reads its config from;
#   2. build the tool, then generate each pilot candidate twice into two fresh
#      temporary roots;
#   3. compare the two candidates (repeat-build consistency) and verify each one
#      in a fresh process;
#   4. build the packed crate from a copy (a build writes a Cargo.lock, so the
#      artifact itself is never built in place) and re-verify the artifact;
#   5. copy each out-of-workspace consumer, point it at the artifact, build it,
#      test it (real published-contract calls), rebuild it --locked --offline, and
#      judge isolation from the FULL resolved graph with `cargo metadata` plus
#      `module-release guard`;
#   6. rebuild one consumer from a directory outside the repository on the same
#      volume, recording the cargo configuration that applied;
#   7. run the negative cases. Every artifact case gets its own pristine copy of a
#      candidate that just verified, injects exactly one fault, and has to be
#      refused for the asserted reason (exit code and check identifier). Two of
#      them are about provenance rather than damage: one rewrites a packed
#      `Cargo.toml` and recomputes the integrity record around it (a payload that
#      agrees with itself and with nothing else), and three run `generate` in an
#      isolated checkout of the same commit — intact, missing a committed source,
#      carrying one that was never committed.
#
# Deliberate limits, recorded rather than hidden:
#   * no PostgreSQL lane and no migration execution (G-02 / G-203 stay open);
#   * no publish, register, install or deploy step exists in this script;
#   * a negative test that passes, or that fails for some other reason, fails the
#     run.
#
# Isolation is never decided in this file: the rule lives in
# tools/module-release-tool/src/isolation.rs and is applied through
# `module-release guard`, so the script and the Rust regressions cannot disagree.

[CmdletBinding()]
param(
    # Scratch root for candidates, packed-crate builds and consumers. It has to be
    # outside every cargo workspace's package set and outside the artifact
    # directories; it defaults to the repository's own build directory, which is
    # git-ignored and on the same volume as the repository.
    [string]$OutRoot = '',
    [Parameter(Mandatory = $true)]
    [string]$EvidenceDir,
    # Where this run's files go *inside* -EvidenceDir. A wave archives several runs
    # beside each other (gates, red, green, script regressions, this pilot), and each
    # of them owns one directory and nothing else — so the pilot owns a named
    # subdirectory of the archive instead of writing loose beside the others. Empty
    # means the run writes directly into -EvidenceDir, as it did before.
    [string]$EvidenceSubdir = '',
    [string[]]$Modules = @('identity', 'policy'),
    [string]$ModuleVersion = '0.1.0-rc.1',
    # Where the "outside the repository" consumer rebuild happens. It must be on
    # the same volume as the repository (a different volume changes cargo's
    # behaviour) and outside the repository directory.
    [string]$OutsideRoot = '',
    [switch]$SkipOutsideConsumer,
    [switch]$CleanScratch
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/plan0014-wave5a-lib.ps1')

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$stamp = Get-UtcStamp
if ([string]::IsNullOrWhiteSpace($OutRoot)) {
    $OutRoot = Join-Path $repo 'target\wave5a-repair1'
}
if (-not (Split-Path -IsAbsolute $OutRoot)) {
    $OutRoot = Join-Path $repo $OutRoot
}
if (-not (Split-Path -IsAbsolute $EvidenceDir)) {
    $EvidenceDir = Join-Path $repo $EvidenceDir
}
if (-not [string]::IsNullOrWhiteSpace($EvidenceSubdir)) {
    $EvidenceDir = Join-Path $EvidenceDir $EvidenceSubdir
}

# A run owns one directory and nothing else, so cleanup can never reach outside
# it and so two runs can never read each other's leftovers.
$runRoot = Join-Path $OutRoot "run-$stamp"
$OutRoot = (New-FreshDirectory -Path $runRoot -What 'run directory').TrimEnd('\')
$EvidenceDir = (New-FreshDirectory -Path $EvidenceDir -What 'evidence directory').TrimEnd('\')
if ([string]::IsNullOrWhiteSpace($OutsideRoot)) {
    $volume = (Get-Item -LiteralPath $repo).PSDrive.Root.TrimEnd('\')
    $OutsideRoot = Join-Path $volume "wave5a-repair1-outside\$stamp"
}
$outsideAllowed = -not $SkipOutsideConsumer

$State = @{
    Repo           = $repo
    Tool           = Join-Path $repo 'target\debug\module-release-tool.exe'
    Failures       = @()
    LogIndex       = @()
    Cases          = @()
    NegativeRoot   = Join-Path $OutRoot 'negatives'
    ScratchRoots   = @($OutRoot)
}

# ---------------------------------------------------------------- toolchain ---
$toolchainFile = Join-Path $EvidenceDir '00-toolchain.txt'
@(
    "generated-at (utc) : $(Get-UtcStamp)",
    "repository          : $repo",
    "run directory       : $OutRoot",
    "source-revision     : $(& git -C $repo rev-parse HEAD)",
    "branch              : $(& git -C $repo rev-parse --abbrev-ref HEAD)",
    "working-tree-dirty  : $(@(& git -C $repo status --porcelain).Count) entries"
) | Set-Content -LiteralPath $toolchainFile -Encoding utf8
foreach ($versionLine in @('cargo --version', 'rustc -vV', 'git --version')) {
    $parts = $versionLine -split ' '
    "`n$versionLine" | Add-Content -LiteralPath $toolchainFile -Encoding utf8
    & $parts[0] $parts[1] 2>&1 | Add-Content -LiteralPath $toolchainFile -Encoding utf8
}
Add-Content -LiteralPath $toolchainFile -Value "`nrust-toolchain.toml"
Get-Content (Join-Path $repo 'rust-toolchain.toml') | Add-Content -LiteralPath $toolchainFile -Encoding utf8
Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' |
    Select-Object ProductName, DisplayVersion, CurrentBuildNumber | Format-List |
    Out-String | Add-Content -LiteralPath $toolchainFile -Encoding utf8

# Where cargo reads its configuration from, recorded because a consumer build
# outside the repository is only comparable to an inside one if the same config
# applied to both.
$configFile = Join-Path $EvidenceDir '01-cargo-config.txt'
$null = Write-CargoConfigEvidence -LogFile $configFile -StartDir $repo -Label 'repository build'

# -------------------------------------------------------------- build tool ----
$null = Invoke-Step `
    -Name 'build-tool' `
    -Title 'build the release tool' `
    -Exe 'cargo' `
    -Arguments @('build', '-p', 'module-release-tool') `
    -LogFile (Join-Path $EvidenceDir '05-build-tool.log') `
    -Blocking `
    -State $State
if (-not (Test-Path -LiteralPath $State.Tool)) {
    throw "the release tool binary was not produced at $($State.Tool)"
}

# ----------------------------------------------------------------- generate ---
$runDirs = [ordered]@{ 'run-a' = Join-Path $OutRoot 'run-a'; 'run-b' = Join-Path $OutRoot 'run-b' }
foreach ($name in @('run-a', 'run-b')) {
    Ensure-Directory $runDirs[$name]
    foreach ($module in $Modules) {
        $null = Invoke-Step `
            -Name "generate-$name-$module" `
            -Title "generate $module (attempt $name)" `
            -Arguments @('generate', '--module', $module, '--out', $runDirs[$name]) `
            -LogFile (Join-Path $EvidenceDir "10-generate-$name-$module.log") `
            -Blocking:($name -eq 'run-a') `
            -State $State
    }
}

# ------------------------------------------------------ compare and verify ----
foreach ($module in $Modules) {
    $left = Join-Path $runDirs['run-a'] "$module-$ModuleVersion"
    $right = Join-Path $runDirs['run-b'] "$module-$ModuleVersion"
    $null = Invoke-Step `
        -Name "compare-$module" `
        -Title "repeat-build consistency for $module (payload bytes and declaration digests of both sides)" `
        -Arguments @('compare', '--left', $left, '--right', $right) `
        -LogFile (Join-Path $EvidenceDir "12-compare-$module.log") `
        -Blocking `
        -State $State
    $null = Invoke-Step `
        -Name "verify-$module" `
        -Title "independent verification of the $module candidate" `
        -Arguments @('verify', '--artifact', $left) `
        -LogFile (Join-Path $EvidenceDir "13-verify-$module.log") `
        -Blocking `
        -State $State

    $recordDir = Join-Path $EvidenceDir "artifacts\$module"
    Ensure-Directory $recordDir
    foreach ($record in @('artifact-manifest.json', 'release.json', 'release-bundle.json', 'verification.json')) {
        Copy-Item -LiteralPath (Join-Path $left $record) -Destination (Join-Path $recordDir $record) -Force
    }
    foreach ($payload in @('module.manifest.json', 'contract\contract-surface.json', 'migrations\migration-mapping.json')) {
        Copy-Item -LiteralPath (Join-Path $left "package\$payload") -Destination (Join-Path $recordDir (Split-Path -Leaf $payload)) -Force
    }
}
# -------------------------------------------------- packed crate real build ---
# The packed crate is built from a copy, not in place: `cargo build` writes a
# Cargo.lock next to the manifest it builds, and a build step must never mutate
# the artifact whose digest was just pinned. Building the copy proves the same
# thing (same manifest, same sources, own workspace root); the verification that
# follows proves the artifact itself was left untouched.
foreach ($module in $Modules) {
    $crate = Join-Path $runDirs['run-a'] "$module-$ModuleVersion\package\contract\packages\$module"
    $buildCopy = Join-Path $OutRoot "crate-build\$module"
    if (Test-Path -LiteralPath $buildCopy) {
        Remove-OwnScratch -Path $buildCopy -OwnRoot $OutRoot
    }
    Ensure-Directory (Split-Path -Parent $buildCopy)
    Copy-Tree -From $crate -To $buildCopy
    $null = Invoke-Step `
        -Name "crate-build-$module" `
        -Title "build the packed $module crate as its own workspace root (--all-features)" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', (Join-Path $buildCopy 'Cargo.toml'), '--all-features', '--target-dir', (Join-Path $OutRoot "build\$module")) `
        -LogFile (Join-Path $EvidenceDir "40-crate-build-$module.log") `
        -Blocking `
        -State $State
    $null = Invoke-Step `
        -Name "verify-after-build-$module" `
        -Title "re-verify the $module artifact after building: the build must not have touched it" `
        -Arguments @('verify', '--artifact', (Join-Path $runDirs['run-a'] "$module-$ModuleVersion")) `
        -LogFile (Join-Path $EvidenceDir "41-verify-after-build-$module.log") `
        -State $State
}

# ------------------------------------------------ external consumer workspaces --
# Isolation is judged once per consumer, from the resolved graph: `cargo metadata`
# resolves the whole dependency set including transitive path packages, and
# `module-release guard` decides. There is no path pattern matching in this file.
$consumers = @{}
foreach ($module in $Modules) {
    $crate = Join-Path $runDirs['run-a'] "$module-$ModuleVersion\package\contract\packages\$module"
    $consumer = Join-Path $OutRoot "consumers\$module"
    if (Test-Path -LiteralPath $consumer) {
        Remove-OwnScratch -Path $consumer -OwnRoot $OutRoot
    }
    Ensure-Directory (Split-Path -Parent $consumer)
    Copy-Tree -From (Join-Path $repo "tools\module-release-tool\consumers\$module") -To $consumer

    $manifestPath = Join-Path $consumer 'Cargo.toml'
    $forward = $crate -replace '\\', '/'
    $manifestText = (Get-Content -Raw -LiteralPath $manifestPath).Replace('__ARTIFACT_CRATE_DIR__', $forward)
    if ($manifestText -eq (Get-Content -Raw -LiteralPath $manifestPath)) {
        throw "the $module consumer template does not contain the __ARTIFACT_CRATE_DIR__ placeholder"
    }
    $manifestText | Set-Content -LiteralPath $manifestPath -Encoding utf8

    $null = Invoke-Step `
        -Name "consumer-build-$module" `
        -Title "build the out-of-workspace $module consumer against the released artifact" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', $manifestPath, '--all-features') `
        -LogFile (Join-Path $EvidenceDir "20-consumer-$module-build.log") `
        -WorkDir $consumer `
        -Blocking `
        -State $State
    $null = Invoke-Step `
        -Name "consumer-test-$module" `
        -Title "run the out-of-workspace $module test that calls a published contract" `
        -Exe 'cargo' `
        -Arguments @('test', '--manifest-path', $manifestPath, '--all-features') `
        -LogFile (Join-Path $EvidenceDir "21-consumer-$module-test.log") `
        -WorkDir $consumer `
        -State $State
    $null = Invoke-Step `
        -Name "consumer-offline-$module" `
        -Title "re-build the $module consumer with a locked dependency set, offline" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', $manifestPath, '--all-features', '--locked', '--offline') `
        -LogFile (Join-Path $EvidenceDir "22-consumer-$module-offline-locked.log") `
        -WorkDir $consumer `
        -State $State
    $null = Invoke-Step `
        -Name "consumer-tree-$module" `
        -Title "resolved dependencies and features of the $module consumer" `
        -Exe 'cargo' `
        -Arguments @('tree', '--manifest-path', $manifestPath, '-e', 'features,normal') `
        -LogFile (Join-Path $EvidenceDir "23-consumer-$module-tree.txt") `
        -WorkDir $consumer `
        -State $State
    Copy-Item -LiteralPath (Join-Path $consumer 'Cargo.lock') -Destination (Join-Path $EvidenceDir "24-consumer-$module-lock.txt") -Force

    $metadataFile = Join-Path $EvidenceDir "25-consumer-$module-cargo-metadata.json"
    $isolated = Invoke-IsolationGuard `
        -State $State `
        -Name "26-consumer-$module" `
        -ConsumerDir $consumer `
        -ArtifactCrateDirs @($crate) `
        -MetadataFile $metadataFile
    if (-not $isolated) {
        $State.Failures += "consumer-$module : isolation could not be confirmed from the resolved graph"
    }
    $consumers[$module] = @{ Dir = $consumer; Manifest = $manifestPath; Metadata = $metadataFile; Crate = $crate }
}

# ----------------------------------------- rebuild from outside the repository --
# The consumers live under the repository's own build directory: they are outside
# the workspace's package set, but they are not physically outside the repository.
# This step closes that gap: the same consumer is rebuilt from a directory that is
# outside the repository directory entirely, on the same volume, and the cargo
# configuration that applied there is recorded next to the result.
$outsideNote = 'NOT RUN: the outside-repository consumer rebuild was skipped with -SkipOutsideConsumer.'
if ($outsideAllowed) {
    $outsideModule = $Modules[0]
    $crate = $consumers[$outsideModule].Crate
    $outsideConsumer = Join-Path $OutsideRoot "consumers\$outsideModule"
    Ensure-Directory (Split-Path -Parent $outsideConsumer)
    Copy-Tree -From (Join-Path $repo "tools\module-release-tool\consumers\$outsideModule") -To $outsideConsumer
    $outsideManifest = Join-Path $outsideConsumer 'Cargo.toml'
    $outsideText = (Get-Content -Raw -LiteralPath $outsideManifest).Replace('__ARTIFACT_CRATE_DIR__', ($crate -replace '\\', '/'))
    $outsideText | Set-Content -LiteralPath $outsideManifest -Encoding utf8

    $outsideConfig = Join-Path $EvidenceDir "30-outside-cargo-config-$outsideModule.txt"
    $null = Write-CargoConfigEvidence `
        -LogFile $outsideConfig `
        -StartDir $outsideConsumer `
        -Label "outside-repository $outsideModule consumer"
    @(
        '',
        '# where this rebuild happened',
        "outside root        : $OutsideRoot",
        "outside consumer    : $outsideConsumer",
        "repository          : $repo",
        "same volume         : $((Get-Item -LiteralPath $repo).PSDrive.Root -eq (Get-Item -LiteralPath (Split-Path -Parent (Split-Path -Parent $outsideConsumer))).PSDrive.Root)",
        "relative to repo    : $([IO.Path]::GetRelativePath($repo, $outsideConsumer))",
        "artifact crate      : $crate"
    ) | Add-Content -LiteralPath $outsideConfig -Encoding utf8
    $null = Invoke-Step `
        -Name 'outside-consumer-build' `
        -Title "rebuild the $outsideModule consumer from a directory outside the repository" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', $outsideManifest, '--all-features') `
        -LogFile (Join-Path $EvidenceDir "31-outside-consumer-build-$outsideModule.log") `
        -WorkDir $outsideConsumer `
        -State $State
    $null = Invoke-Step `
        -Name 'outside-consumer-test' `
        -Title "test the outside-repository $outsideModule consumer" `
        -Exe 'cargo' `
        -Arguments @('test', '--manifest-path', $outsideManifest, '--all-features') `
        -LogFile (Join-Path $EvidenceDir "32-outside-consumer-test-$outsideModule.log") `
        -WorkDir $outsideConsumer `
        -State $State
    $null = Invoke-IsolationGuard `
        -State $State `
        -Name "33-outside-consumer-$outsideModule" `
        -ConsumerDir $outsideConsumer `
        -ArtifactCrateDirs @($crate) `
        -MetadataFile (Join-Path $EvidenceDir "34-outside-consumer-$outsideModule-cargo-metadata.json")
    $outsideNote = "RAN: consumer rebuilt and tested from $outsideConsumer (outside $repo, same volume)."
}
# --------------------------------------------------------------- negatives -----
# One fault per case, into a copy that just verified, and the refusal has to carry
# the asserted check identifier. A case that fails for a different reason is
# reported as WRONG REASON, not as a pass — that is how the Wave 5A evidence
# documented a module-id refusal that was really a bundle mismatch.
Ensure-Directory $State.NegativeRoot
$negModule = $Modules[0]
$pristine = Join-Path $runDirs['run-a'] "$negModule-$ModuleVersion"
$zeroRevision = '0' * 40
$specHistory = @(& git -C $repo log --format=%H -- "tools/module-release-tool/specs/$negModule.json")
if ($specHistory.Count -lt 1) {
    throw "no git history for the $negModule release spec; the stale-revision negative case cannot be built"
}
$introduced = $specHistory[-1].Trim()
$ancestorRevision = (& git -C $repo rev-parse "$introduced^").Trim()

function Set-RevisionInBothRecords {
    param([string]$Copy, [string]$Revision, [string]$Why)
    Set-JsonRecord -Path (Join-Path $Copy 'release.json') -Describe "$Why in release.json" -Edit {
        param($value) $value.source_revision = $Revision
    } | Out-Null
    Set-JsonRecord -Path (Join-Path $Copy 'release-bundle.json') -Describe "$Why in release-bundle.json" -Edit {
        param($value) $value.identity.source_revision = $Revision
    } | Out-Null
}

$null = Invoke-NegativeCase -State $State -Name 'n01-payload-byte' `
    -Title "one packed source byte changed: $negModule must fail the integrity digest" `
    -SourceArtifact $pristine -ExpectCheck 'artifact-integrity-digest' -Inject {
        param($copy)
        $source = Join-Path $copy "package\contract\packages\$negModule\src\lib.rs"
        Add-Content -LiteralPath $source -Value '// review tamper' -Encoding utf8
    }

$null = Invoke-NegativeCase -State $State -Name 'n02-missing-member' `
    -Title "a payload member removed: $negModule must fail the integrity digest" `
    -SourceArtifact $pristine -ExpectCheck 'artifact-integrity-digest' -Inject {
        param($copy)
        Remove-Item -LiteralPath (Join-Path $copy 'package\CHANGELOG.md') -Force
    }

$null = Invoke-NegativeCase -State $State -Name 'n03-unknown-algorithm' `
    -Title "artifact-manifest.json names an integrity algorithm nobody implements" `
    -SourceArtifact $pristine -ExpectCheck 'artifact-record-format' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'artifact-manifest.json') -Describe 'unknown algorithm' -Edit {
            param($value) $value.artifact_integrity_algorithm = 'unknown-algorithm.v9'
        } | Out-Null
    }

$null = Invoke-NegativeCase -State $State -Name 'n04-unknown-schema' `
    -Title "artifact-manifest.json declares a schema version this tool never wrote" `
    -SourceArtifact $pristine -ExpectCheck 'artifact-record-format' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'artifact-manifest.json') -Describe 'unknown schema' -Edit {
            param($value) $value.schema_version = 'plan-0014.wave5a.artifact-manifest.v99'
        } | Out-Null
    }

$null = Invoke-NegativeCase -State $State -Name 'n05-record-module-id' `
    -Title "artifact-manifest.json describes a different module than release.json" `
    -SourceArtifact $pristine -ExpectCheck 'record-consistency' -ExpectPattern 'module_id' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'artifact-manifest.json') -Describe 'module_id flip' -Edit {
            param($value) $value.module_id = 'policy'
        } | Out-Null
    }

$null = Invoke-NegativeCase -State $State -Name 'n06-record-package-digest' `
    -Title "artifact-manifest.json pins a package digest release.json does not claim" `
    -SourceArtifact $pristine -ExpectCheck 'record-consistency' -ExpectPattern 'package_digest' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'artifact-manifest.json') -Describe 'package_digest flip' -Edit {
            param($value) $value.package_digest = ('e' * 64)
        } | Out-Null
    }

$null = Invoke-NegativeCase -State $State -Name 'n07-bundle-crate-version' `
    -Title "release-bundle.json binds a crate version the packed Cargo.toml does not declare" `
    -SourceArtifact $pristine -ExpectCheck 'packed-crate-bindings' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'release-bundle.json') -Describe 'crate version bump' -Edit {
            param($value) $value.crate_versions[0].crate_version = '9.9.9'
        } | Out-Null
    }

$null = Invoke-NegativeCase -State $State -Name 'n08-bundle-crate-name' `
    -Title "release-bundle.json binds a crate the payload does not ship" `
    -SourceArtifact $pristine -ExpectCheck 'packed-crate-bindings' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'release-bundle.json') -Describe 'crate renamed' -Edit {
            param($value) $value.crate_versions[0].crate_name = "$negModule-renamed"
        } | Out-Null
    }

$null = Invoke-NegativeCase -State $State -Name 'n09-zero-revision' `
    -Title "both identity records claim a revision that is not a commit" `
    -SourceArtifact $pristine -ExpectCheck 'source-revision-exists' -Inject {
        param($copy)
        Set-RevisionInBothRecords -Copy $copy -Revision $zeroRevision -Why 'zeroed source_revision'
    }

$null = Invoke-NegativeCase -State $State -Name 'n11-stale-revision' `
    -Title "both identity records claim a real commit that does not contain the packed inputs" `
    -SourceArtifact $pristine -ExpectCheck 'source-inputs-match-revision' -ExpectPattern $negModule -Inject {
        param($copy)
        Set-RevisionInBothRecords -Copy $copy -Revision $ancestorRevision -Why 'stale source_revision'
    }

$null = Invoke-NegativeCase -State $State -Name 'n10-toolchain-claim' `
    -Title "the identity declares a toolchain rust-toolchain.toml at that commit does not" `
    -SourceArtifact $pristine -ExpectCheck 'toolchain-declaration' -Inject {
        param($copy)
        Set-JsonRecord -Path (Join-Path $copy 'release.json') -Describe 'toolchain bump in release.json' -Edit {
            param($value) $value.build_toolchain.version = '9.9.9'
        } | Out-Null
        Set-JsonRecord -Path (Join-Path $copy 'release-bundle.json') -Describe 'toolchain bump in release-bundle.json' -Edit {
            param($value) $value.identity.build_toolchain.version = '9.9.9'
        } | Out-Null
    }

# compare has to read both payloads, so damaging only one side cannot be reported
# as an ordinary repeat-build difference.
$compareCase = Join-Path $State.NegativeRoot 'n12-compare-right-damaged'
if (Test-Path -LiteralPath $compareCase) {
    Remove-OwnScratch -Path $compareCase -OwnRoot $OutRoot
}
Ensure-Directory $compareCase
$damagedRight = Join-Path $compareCase (Split-Path -Leaf $pristine)
Copy-Tree -From $pristine -To $damagedRight
$compareBefore = Get-TreeHashes -Root $damagedRight
$null = Invoke-Step -Name 'n12 baseline' -Title 'n12 : both sides must be identical before the damage' `
    -Arguments @('compare', '--left', $pristine, '--right', $damagedRight) `
    -LogFile (Join-Path $compareCase '00-compare-clean.log') -State $State
$rightSource = Join-Path $damagedRight "package\contract\packages\$negModule\src\lib.rs"
Add-Content -LiteralPath $rightSource -Value 'compile_error!("review tamper");' -Encoding utf8
$compareInjection = @(Compare-InjectedTrees -Before $compareBefore -After (Get-TreeHashes -Root $damagedRight))
if ($compareInjection.Count -eq 0 -or $compareInjection[0] -like 'nothing*') {
    throw 'the n12 injection did not change any byte'
}
$compareRun = Invoke-Step -Name 'n12-compare-right-damaged' `
    -Title 'a damaged payload on the right side must be refused, not reported as a build difference' `
    -Arguments @('compare', '--left', $pristine, '--right', $damagedRight) `
    -LogFile (Join-Path $compareCase '10-compare-damaged.log') -ExpectExit 1 -State $State
$compareCheck = if ($compareRun.Log -match '\[([a-z0-9-]+)\]') { $Matches[1] } else { 'no check identifier' }
$compareVerdict = if ($compareRun.Exit -eq 0) { 'NOT REFUSED' } elseif ($compareCheck -ne 'compare-payload-integrity') { 'WRONG REASON' } else { 'OK' }
if ($compareVerdict -ne 'OK') {
    $State.Failures += "n12-compare-right-damaged : verdict $compareVerdict (observed '$compareCheck')"
}
$State.Cases += [pscustomobject]@{
    Case = 'n12-compare-right-damaged'; Target = 'compare-payload-integrity'; Observed = $compareCheck; Verdict = $compareVerdict
}
@(
    '# n12-compare-right-damaged',
    '',
    'case       : a damaged payload on the right side must be refused, not reported as a build difference',
    'expect     : exit 1 refused by `compare-payload-integrity`',
    "observed   : exit $($compareRun.Exit) refused by ``$compareCheck``",
    "verdict    : $compareVerdict",
    '',
    '## what the injection actually changed (content hashes, pristine copy -> damaged copy)',
    ''
) + $compareInjection + @(
    '',
    '## commands',
    '',
    "1. ``module-release compare --left <pristine> --right <copy>`` before the injection — exit 0 expected",
    "2. ``module-release compare --left <pristine> --right <copy>`` after the injection — exit $($compareRun.Exit)",
    '',
    'Raw logs: `00-compare-clean.log`, `10-compare-damaged.log` (archived beside this file).',
    ''
) | Set-Content -LiteralPath (Join-Path $compareCase 'case.md') -Encoding utf8

# Repair 2, finding F1-A at the pilot layer: a payload whose record agrees with a
# member the pinned commit cannot produce. The packed `Cargo.toml` is rewritten to a
# dependency version no committed manifest states, and the integrity record is then
# recomputed around it, so the physical check has nothing left to complain about. The
# claim that fails is the one Repair 1 never tested: that the member is what that
# commit derives.
$null = Invoke-NegativeCase -State $State -Name 'n13-packed-manifest-not-from-the-commit' `
    -Title 'a packed Cargo.toml the pinned commit cannot produce, with a self-consistent record, must be refused' `
    -SourceArtifact $pristine -ExpectCheck 'derived-members-match-revision' -ExpectPattern $negModule -Inject {
        param($copy)
        $manifest = Join-Path $copy "package\contract\packages\$negModule\Cargo.toml"
        $text = Get-Content -Raw -LiteralPath $manifest
        $rewritten = $text -replace 'version = "=[0-9][^"]*"', 'version = "=99.0.0"'
        if ($rewritten -eq $text) {
            throw "the packed manifest of '$negModule' pins no dependency version; the case would assert nothing"
        }
        Set-Content -LiteralPath $manifest -Value $rewritten -NoNewline -Encoding utf8
        $recomputed = Set-IntegrityRecord -Artifact $copy
        if (-not $recomputed.Changed) {
            throw 'recomputing the integrity record changed nothing, so the damaged payload was never self-consistent'
        }
    }

# Repair 2, finding F1-B at the pilot layer: generation reads the commit, so a
# checkout that dropped a committed source — or added one that was never committed —
# has to be refused, and refused while no artifact directory exists yet.
#
# The three cases share one *isolated checkout*: a directory whose `.git` is a file
# naming this repository's object store, which resolves the same HEAD and the same
# commit objects while its own files are entirely its own. It contains only the files a
# candidate is built from, which is the point — the tool has to know from the commit
# what it expects to find. `n14` is the control and has to succeed: without it, the two
# refusals below would only show that a damaged checkout is damaged.
function Invoke-CheckoutGenerateCase {
    param(
        [Parameter(Mandatory = $true)][hashtable]$State,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][string]$CheckoutRoot,
        [Parameter(Mandatory = $true)][string]$Out,
        [Parameter(Mandatory = $true)][scriptblock]$Prepare,
        [string]$ExpectCheck = ''
    )
    $caseRoot = Join-Path $State.NegativeRoot $Name
    if (Test-Path -LiteralPath $caseRoot) {
        Remove-OwnScratch -Path $caseRoot -OwnRoot $OutRoot
    }
    Ensure-Directory $caseRoot
    $prepared = & $Prepare
    $expect = if ($ExpectCheck) { 1 } else { 0 }
    # `--repo` is the point of these cases: the tool is told which checkout to read, so
    # the run says in its own command line that a *second* checkout of the same commit
    # is what is being judged.
    $run = Invoke-Step -Name $Name -Title $Title `
        -Arguments @('generate', '--module', $negModule, '--out', $Out, '--repo', $CheckoutRoot) `
        -LogFile (Join-Path $caseRoot '10-generate.log') -ExpectExit $expect -State $State
    $observed = if ($run.Log -match '\[([a-z0-9-]+)\]') { $Matches[1] } else { 'no check identifier' }
    $created = Test-Path -LiteralPath (Join-Path $Out "$negModule-$ModuleVersion")
    $target = if ($ExpectCheck) { $ExpectCheck } else { 'generation succeeds' }
    $verdict = 'FAILED'
    if (-not $ExpectCheck) {
        if ($run.Exit -eq 0 -and $created) {
            $verdict = 'OK'
        }
        else {
            $verdict = 'BROKEN'
            $State.Failures += "$Name : the control checkout did not generate (exit $($run.Exit), artifact directory present: $created)"
        }
    }
    elseif ($run.Exit -eq 0) {
        $verdict = 'NOT REFUSED'
        $State.Failures += "$Name : generation accepted a checkout it has to refuse"
    }
    elseif ($observed -ne $ExpectCheck) {
        $verdict = 'WRONG REASON'
        $State.Failures += "$Name : refused for '$observed', expected '$ExpectCheck'"
    }
    elseif ($created) {
        $verdict = 'ARTIFACT CREATED'
        $State.Failures += "$Name : refused, but an artifact directory was created anyway"
    }
    else {
        $verdict = 'OK'
    }
    $State.Cases += [pscustomobject]@{
        Case     = $Name
        Target   = $target
        Observed = $(if ($ExpectCheck) { $observed } else { "exit $($run.Exit), artifact directory created: $created" })
        Verdict  = $verdict
    }
    @(
        "# $($Name)",
        '',
        "case       : $Title",
        "expect     : exit $expect" + $(if ($ExpectCheck) { ", refused by ``$ExpectCheck``, and no artifact directory" } else { ' and an artifact directory' }),
        "observed   : exit $($run.Exit), check ``$observed``, artifact directory created: $created",
        "verdict    : $verdict",
        '',
        "the checkout is ``$CheckoutRoot`` — a directory whose ``.git`` file names this repository's",
        'object store, holding only the files a candidate is built from.',
        '',
        "what this case did to it : $prepared",
        '',
        'Raw log: `10-generate.log` (archived beside this file).',
        ''
    ) | Set-Content -LiteralPath (Join-Path $caseRoot 'case.md') -Encoding utf8
    return ($verdict -eq 'OK')
}

$checkoutDirectory = Join-Path $OutRoot 'isolated-checkout'
if (Test-Path -LiteralPath $checkoutDirectory) {
    Remove-OwnScratch -Path $checkoutDirectory -OwnRoot $OutRoot
}
$checkout = New-IsolatedCheckout -Repo $repo -Destination $checkoutDirectory -Module $negModule
$specDocument = Get-Content -Raw -LiteralPath (Join-Path $repo "tools/module-release-tool/specs/$negModule.json") | ConvertFrom-Json
$crateSource = Join-Path $checkout.Root (($specDocument.packed_crates[0].source_dir -replace '/', '\') + '\src')
# Any committed file but the crate root: generation never compiles Rust, so the file
# only has to be one the pinned commit lists.
$dropped = @(Get-ChildItem -LiteralPath $crateSource -Recurse -File |
        Where-Object { $_.Name -ne 'lib.rs' } | Sort-Object -Property FullName)[0]
if (-not $dropped) {
    throw "the isolated checkout of '$negModule' has no source file besides lib.rs; the F1-B cases cannot be built"
}
$droppedRelative = $dropped.FullName.Substring($checkout.Root.Length).TrimStart('\')
$droppedInRepository = Join-Path $repo $droppedRelative
$extra = Join-Path $crateSource 'plan0014_repair_case_extra.rs'

$null = Invoke-CheckoutGenerateCase -State $State -Name 'n14-isolated-checkout-control' `
    -Title 'a complete isolated checkout of the pinned commit generates a candidate (control for n15 and n16)' `
    -Checkout $checkout.Root -Out (Join-Path $OutRoot 'checkout-generate\control') -Prepare {
        "nothing — the checkout is exactly the release inputs of commit as it stands ($($checkout.Files) files)"
    }

$null = Invoke-CheckoutGenerateCase -State $State -Name 'n15-checkout-dropped-a-committed-source' `
    -Title 'a checkout missing a committed source file must be refused, with no artifact created' `
    -Checkout $checkout.Root -Out (Join-Path $OutRoot 'checkout-generate\dropped') `
    -ExpectCheck 'source-member-set-matches-revision' -Prepare {
        if (-not (Test-Path -LiteralPath $dropped.FullName)) {
            throw 'the control already removed the file; n15 has nothing to drop'
        }
        Remove-Item -LiteralPath $dropped.FullName -Force
        $relative = $dropped.FullName.Substring($checkout.Root.Length).TrimStart('\') -replace '\\', '/'
        "removed '$relative', which the pinned commit contains"
    }

$null = Invoke-CheckoutGenerateCase -State $State -Name 'n16-checkout-added-an-uncommitted-source' `
    -Title 'a checkout with a source file the commit does not contain must be refused, with no artifact created' `
    -Checkout $checkout.Root -Out (Join-Path $OutRoot 'checkout-generate\added') `
    -ExpectCheck 'source-member-set-matches-revision' -Prepare {
        if (-not (Test-Path -LiteralPath $dropped.FullName)) {
            # n15 removed it; put it back so this case refuses for the file it added
            # and not for the one the previous case dropped.
            Copy-Item -LiteralPath $droppedInRepository -Destination $dropped.FullName -Force
        }
        if (Test-Path -LiteralPath $extra) {
            Remove-Item -LiteralPath $extra -Force
        }
        Set-Content -LiteralPath $extra -Value "// written by the pilot script, committed nowhere`n" -Encoding utf8
        "restored '$($droppedRelative -replace '\\', '/')' from the repository and added the never-committed '$(($extra -replace '\\', '/').Split('/')[-1])'"
    }
# ------------------------------------------------- isolation negative cases ----
# Three leak shapes the Wave 5A string scan could not see, injected into a real
# `cargo metadata` document and handed to the same guard the passing case used.
$isoSource = $consumers[$negModule].Metadata
$isoConsumer = $consumers[$negModule].Dir
$isoCrate = $consumers[$negModule].Crate
$repoCrateManifest = Join-Path $repo "crates\$negModule\Cargo.toml"
$repoContractsManifest = Join-Path $repo 'crates\business-module-contracts\Cargo.toml'
foreach ($requiredLeakTarget in @($repoCrateManifest, $repoContractsManifest)) {
    if (-not (Test-Path -LiteralPath $requiredLeakTarget)) {
        throw "the isolation negative cases need '$requiredLeakTarget' to exist"
    }
}

function Invoke-GuardNegativeCase {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Title,
        [Parameter(Mandatory = $true)][scriptblock]$Edit,
        [string]$ExpectCheck = 'consumer-graph-isolation'
    )
    $caseRoot = Join-Path $State.NegativeRoot $Name
    Ensure-Directory $caseRoot
    $metadata = Join-Path $caseRoot 'metadata.json'
    Copy-Item -LiteralPath $isoSource -Destination $metadata -Force
    Set-JsonRecord -Path $metadata -Describe $Title -Edit $Edit | Out-Null
    $run = Invoke-Step `
        -Name $Name `
        -Title $Title `
        -Arguments @('guard', '--metadata', $metadata, '--consumer', $isoConsumer, '--repo', $repo, '--artifact', $isoCrate) `
        -LogFile (Join-Path $caseRoot '10-guard.log') `
        -ExpectExit 1 `
        -State $State
    $observed = if ($run.Log -match '\[([a-z0-9-]+)\]') { $Matches[1] } else { 'no check identifier' }
    $verdict = if ($run.Exit -eq 0) { 'NOT REFUSED' } elseif ($observed -ne $ExpectCheck) { 'WRONG REASON' } else { 'OK' }
    if ($verdict -ne 'OK') {
        $State.Failures += "$Name : verdict $verdict (observed '$observed')"
    }
    $documentHash = (Get-FileHash -LiteralPath $metadata -Algorithm SHA256).Hash.ToLowerInvariant()
    @(
        "# $Name",
        '',
        "case       : $Title",
        "expect     : exit 1 refused by ``$ExpectCheck``",
        "observed   : exit $($run.Exit) refused by ``$observed``",
        "verdict    : $verdict",
        '',
        'The leak is injected into a real `cargo metadata` document taken from the',
        "passing consumer run, so the guard sees exactly the shape a real build would",
        'produce. The doctored document itself stays in the run directory (it embeds',
        'absolute paths of this machine); its sha256 pins what was judged:',
        '',
        "document   : ``metadata.json`` sha256 ``$documentHash``",
        "consumer   : ``$isoConsumer``",
        "repo root  : ``$repo``",
        "artifact   : ``$isoCrate``",
        '',
        'Raw log: `10-guard.log` (archived beside this file).',
        ''
    ) | Set-Content -LiteralPath (Join-Path $caseRoot 'case.md') -Encoding utf8
    $State.Cases += [pscustomobject]@{ Case = $Name; Target = $ExpectCheck; Observed = $observed; Verdict = $verdict }
}

$forwardRepoCrate = $repoCrateManifest -replace '\\', '/'
$null = Invoke-GuardNegativeCase -Name 'g01-absolute-into-repo' `
    -Title 'a resolved path dependency pointing at the repository crate with forward slashes' `
    -Edit {
        param($value)
        $leaked = @($value.packages | Where-Object { $_.manifest_path -replace '\\', '/' -like '*/contract/packages/*/Cargo.toml' })
        if ($leaked.Count -lt 1) { throw 'the metadata document has no artifact path package to leak' }
        foreach ($package in $leaked) { $package.manifest_path = $forwardRepoCrate }
    }

$relativeIntoRepo = [IO.Path]::GetRelativePath($isoConsumer, $repoCrateManifest)
$null = Invoke-GuardNegativeCase -Name 'g02-relative-into-repo' `
    -Title "a resolved path dependency spelled relative to the consumer ('$relativeIntoRepo')" `
    -Edit {
        param($value)
        $leaked = @($value.packages | Where-Object { $_.manifest_path -replace '\\', '/' -like '*/contract/packages/*/Cargo.toml' })
        foreach ($package in $leaked) { $package.manifest_path = (Join-Path $isoConsumer $relativeIntoRepo) }
    }

$null = Invoke-GuardNegativeCase -Name 'g03-transitive-into-repo' `
    -Title 'a transitive local dependency on a repository crate the consumer never names' `
    -Edit {
        param($value)
        $value.packages += [pscustomobject]@{
            name           = 'business-module-contracts'
            version        = '0.1.0'
            id             = 'business-module-contracts 0.1.0 (path+F:\Workspace\business-platform)'
            license        = 'MIT'
            source         = $null
            manifest_path  = $repoContractsManifest
            dependencies   = @()
            features       = [pscustomobject]@{}
        }
    }

# The per-case notes and raw logs live in the scratch run directory while the run
# is going on; the evidence keeps a copy of every one of them.
$archivedCases = Save-NegativeRecords -From $State.NegativeRoot -To (Join-Path $EvidenceDir 'negatives')

# ------------------------------------------------------------ assembly log -----
$summary = @(
    '# PLAN-0014 Wave 5A — assembly log',
    '',
    "run directory             : $OutRoot",
    "evidence directory        : $EvidenceDir",
    "repository                : $repo",
    "source revision under test: $(& git -C $repo rev-parse HEAD)",
    "working tree dirty entries: $(@(& git -C $repo status --porcelain).Count)",
    "outside-repository rebuild: $outsideNote",
    "negative-case records     : $archivedCases file(s) archived under ``negatives/``",
    '',
    'Every row is one command this script ran, with the exit code it produced and the',
    'exit code it had to produce. "expected 1" rows are negative cases: a refusal is',
    'the result. A step marked blocking stops the run when it fails, so no later',
    'step can report on state that was never produced.',
    '',
    '## commands',
    '',
    '| step | command | exit | expected | log |',
    '| --- | --- | --- | --- | --- |'
)
foreach ($entry in $State.LogIndex) {
    $summary += ('| {0} | `{1}` | {2} | {3} | `{4}` |' -f $entry.Title, $entry.Command, $entry.Exit, $entry.Expect, $entry.Log)
}
$summary += @(
    '',
    '## negative cases',
    '',
    'Each case starts from its own copy of a candidate that verified moments before,',
    'injects one fault through parse/serialise (never a blind string replacement),',
    'proves the injection landed by re-reading the file, and then has to be refused',
    'by the asserted check. WRONG REASON means it failed, for a different reason.',
    '',
    '| case | expected check | observed check | verdict |',
    '| --- | --- | --- | --- |'
)
foreach ($case in $State.Cases) {
    $summary += ('| {0} | `{1}` | `{2}` | {3} |' -f $case.Case, $case.Target, $case.Observed, $case.Verdict)
}
$summary += @(
    '',
    '## result',
    ''
)
if ($State.Failures.Count -eq 0) {
    $summary += 'RESULT: every step matched its expected exit code and every negative case was refused for the asserted reason.'
}
else {
    $summary += 'RESULT: FAILED'
    $summary += ($State.Failures | ForEach-Object { "- $_" })
}
$summary += @(
    '',
    '## not executed here, and therefore NOT RUN / BLOCKED',
    '',
    '* PostgreSQL migration execution and per-module PostgreSQL installation:',
    '  BLOCKED (G-02 / G-203, no database authorisation in this environment).',
    '* publish / register / install / deploy of a module: does not exist in this',
    '  script, and is not part of Wave 5A.',
    '* vulnerability, licence, secret and image scanning: NOT RUN.',
    '* CI: NOT RUN.',
    '* which toolchain actually produced the packed bytes: NOT PROVEN by the tool.',
    '  `toolchain-declaration` compares release.json with the toolchain *declared* at',
    '  the pinned commit; the `rustc -vV` line in 00-toolchain.txt is the separate,',
    '  external evidence for the run that built the candidate.',
    ''
)
$summary | Set-Content -LiteralPath (Join-Path $EvidenceDir 'assembly-log.md') -Encoding utf8

if ($State.Failures.Count -ne 0) {
    Write-Host "$($State.Failures.Count) step(s) failed. Evidence: $EvidenceDir"
    exit 1
}
if ($CleanScratch) {
    Remove-OwnScratch -Path $OutRoot -OwnRoot $OutRoot
    if ($outsideAllowed -and (Test-Path -LiteralPath $OutsideRoot)) {
        Remove-OwnScratch -Path $OutsideRoot -OwnRoot $OutsideRoot
    }
}
Write-Host "Wave 5A assembly and verification completed. Evidence: $EvidenceDir"
exit 0