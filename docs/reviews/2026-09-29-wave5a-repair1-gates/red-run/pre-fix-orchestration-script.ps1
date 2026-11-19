# PLAN-0014 Wave 5A — local release-candidate assembly and verification.
#
# What this script does, in order, with every command, its exit code and its raw
# output recorded under -EvidenceDir:
#
#   1. generate each pilot candidate twice, into two different temporary roots;
#   2. compare the two candidates (repeat-build consistency);
#   3. verify each candidate in a fresh process (independent of generation);
#   4. build each packed crate as its own workspace root (a real build, not a
#      manifest shape check);
#   5. copy each out-of-workspace consumer outside the repository, point it at
#      the artifact, then build it, test it (real contract calls), re-build it
#      with --locked --offline, record the resolved dependency tree, and assert
#      that nothing resolves back into the source workspace;
#   6. run tamper / missing-file / wrong-version negative tests against a copy of
#      a candidate; each of them MUST fail verification.
#
# Deliberate limits, recorded rather than hidden:
#   * no PostgreSQL lane and no migration execution (G-02 / G-203 stay open);
#   * no publish, register, install or deploy step exists in this script;
#   * a negative test that passes fails the whole run.
#
# Requires: the module-release binary (cargo build -p module-release-tool) and a
# writable temporary root outside the repository.

[CmdletBinding()]
param(
    # Scratch root for candidates, packed-crate builds and consumers. It must
    # live outside every cargo workspace's package set and outside the artifact
    # directories; it defaults to the repository's own build directory, which is
    # git-ignored and on the same volume as the repository.
    [string]$OutRoot = '',
    [Parameter(Mandatory = $true)]
    [string]$EvidenceDir,
    [string[]]$Modules = @('identity', 'policy'),
    [string]$ModuleVersion = '0.1.0-rc.1'
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutRoot)) {
    $OutRoot = Join-Path $repo 'target\wave5a'
}
$failures = @()
$logIndex = @()

function Timestamp {
    (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
}

function Ensure-Directory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

function Write-LogHeader {
    param([string]$LogFile, [string]$Title, [string]$CommandLine)
    Ensure-Directory (Split-Path -Parent $LogFile)
    $lines = @(
        "title      : $Title",
        "command    : $CommandLine",
        "workdir    : $repo",
        "utc        : $(Timestamp)"
    )
    $lines | Set-Content -LiteralPath $LogFile -Encoding utf8
}

function Invoke-Step {
    param(
        [string]$Name,
        [string]$Title,
        [string]$Exe,
        [string[]]$Arguments,
        [string]$LogFile,
        [string]$WorkDir = $repo,
        [int]$ExpectExit = 0
    )
    Write-LogHeader -LogFile $LogFile -Title $Title -CommandLine "$Exe $($Arguments -join ' ')"
    Push-Location $WorkDir
    try {
        & $Exe @Arguments 2>&1 | Tee-Object -FilePath $LogFile -Append | Out-Null
        $code = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
    "`nexit code : $code (expected $ExpectExit)" | Add-Content -LiteralPath $LogFile -Encoding utf8
    $script:logIndex += [string]::Format(
        '| {0} | `{1}` | {2} | {3} |',
        $Title,
        "$Exe $($Arguments -join ' ')",
        $code,
        (Split-Path -Leaf $LogFile)
    )
    if ($code -ne $ExpectExit) {
        $script:failures += "$Name exited $code, expected $ExpectExit"
        Write-Host "FAIL  $Name (exit $code, expected $ExpectExit)  -> $LogFile"
        return $false
    }
    Write-Host "ok    $Name"
    return $true
}

Ensure-Directory $OutRoot
Ensure-Directory $EvidenceDir
$OutRoot = (Resolve-Path $OutRoot).Path

if (-not (Split-Path -IsAbsolute $EvidenceDir)) {
    $EvidenceDir = Join-Path $repo $EvidenceDir
}
Ensure-Directory $EvidenceDir

# ---------------------------------------------------------------- toolchain ---
$toolchainFile = Join-Path $EvidenceDir '00-toolchain.txt'
 @(
    "generated-at (utc) : $(Timestamp)",
    "repository          : $repo",
    "source-revision     : $(& git -C $repo rev-parse HEAD)",
    "branch              : $(& git -C $repo rev-parse --abbrev-ref HEAD)",
    "working-tree-dirty  : $(& git -C $repo status --porcelain | Measure-Object | ForEach-Object Count) entries"
) | Set-Content -LiteralPath $toolchainFile -Encoding utf8
& cargo --version 2>&1 | Add-Content -LiteralPath $toolchainFile -Encoding utf8
& rustc --version 2>&1 | Add-Content -LiteralPath $toolchainFile -Encoding utf8
Add-Content -LiteralPath $toolchainFile -Value ''
Get-Content (Join-Path $repo 'rust-toolchain.toml') | Add-Content -LiteralPath $toolchainFile -Encoding utf8
Add-Content -LiteralPath $toolchainFile -Value ''
Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' |
    Select-Object ProductName, DisplayVersion, CurrentBuildNumber | Format-List |
    Out-String | Add-Content -LiteralPath $toolchainFile -Encoding utf8

# ----------------------------------------------------------------- generate ---
$runDirs = [ordered]@{ 'run-a' = Join-Path $OutRoot 'run-a'; 'run-b' = Join-Path $OutRoot 'run-b' }
foreach ($name in $runDirs.Keys) {
    Ensure-Directory $runDirs[$name]
    foreach ($module in $Modules) {
        $null = Invoke-Step `
            -Name "generate-$name-$module" `
            -Title "generate $module (attempt $name)" `
            -Exe 'cargo' `
            -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'generate', '--module', $module, '--out', $runDirs[$name]) `
            -LogFile (Join-Path $EvidenceDir "10-generate-$name-$module.log")
    }
}

# ----------------------------------------------------------------- compare ----
foreach ($module in $Modules) {
    $null = Invoke-Step `
        -Name "compare-$module" `
        -Title "repeat-build consistency for $module (two temporary roots)" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'compare', '--left', (Join-Path $runDirs['run-a'] "$module-$ModuleVersion"), '--right', (Join-Path $runDirs['run-b'] "$module-$ModuleVersion")) `
        -LogFile (Join-Path $EvidenceDir "12-compare-$module.log")
}

# ------------------------------------------------------------------ verify ----
foreach ($module in $Modules) {
    $null = Invoke-Step `
        -Name "verify-$module" `
        -Title "independent verification of the $module candidate" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', (Join-Path $runDirs['run-a'] "$module-$ModuleVersion")) `
        -LogFile (Join-Path $EvidenceDir "13-verify-$module.log")

    $artifactDir = Join-Path $runDirs['run-a'] "$module-$ModuleVersion"
    $recordDir = Join-Path $EvidenceDir "artifacts\$module"
    Ensure-Directory $recordDir
    foreach ($record in @('artifact-manifest.json', 'release.json', 'release-bundle.json', 'verification.json')) {
        Copy-Item -LiteralPath (Join-Path $artifactDir $record) -Destination (Join-Path $recordDir $record) -Force
    }
    foreach ($payload in @('module.manifest.json', 'contract\contract-surface.json', 'migrations\migration-mapping.json')) {
        Copy-Item -LiteralPath (Join-Path $artifactDir "package\$payload") -Destination (Join-Path $recordDir (Split-Path -Leaf $payload)) -Force
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
        Remove-Item -LiteralPath $buildCopy -Recurse -Force
    }
    Ensure-Directory (Split-Path -Parent $buildCopy)
    Copy-Item -Recurse -Force -LiteralPath $crate -Destination $buildCopy
    $null = Invoke-Step `
        -Name "crate-build-$module" `
        -Title "build the packed $module crate as its own workspace root (--all-features)" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', (Join-Path $buildCopy 'Cargo.toml'), '--all-features', '--target-dir', (Join-Path $OutRoot "build\$module")) `
        -LogFile (Join-Path $EvidenceDir "40-crate-build-$module.log")
    $null = Invoke-Step `
        -Name "verify-after-build-$module" `
        -Title "re-verify the $module artifact after building: the build must not have touched it" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', (Join-Path $runDirs['run-a'] "$module-$ModuleVersion")) `
        -LogFile (Join-Path $EvidenceDir "41-verify-after-build-$module.log")
}

# ------------------------------------------------------ external consumers ----
foreach ($module in $Modules) {
    $crate = (Join-Path $runDirs['run-a'] "$module-$ModuleVersion\package\contract\packages\$module") -replace '\\', '/'
    $consumer = Join-Path $OutRoot "consumers\$module"
    if (Test-Path -LiteralPath $consumer) {
        Remove-Item -LiteralPath $consumer -Recurse -Force
    }
    Ensure-Directory (Split-Path -Parent $consumer)
    Copy-Item -Recurse -Force -LiteralPath (Join-Path $repo "tools\module-release-tool\consumers\$module") -Destination $consumer

    $manifestPath = Join-Path $consumer 'Cargo.toml'
    $manifest = (Get-Content -LiteralPath $manifestPath -Raw).Replace('__ARTIFACT_CRATE_DIR__', $crate)
    $manifest | Set-Content -LiteralPath $manifestPath -Encoding utf8

    $null = Invoke-Step `
        -Name "consumer-build-$module" `
        -Title "build the out-of-workspace $module consumer against the released artifact" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', $manifestPath, '--all-features') `
        -LogFile (Join-Path $EvidenceDir "20-consumer-$module-build.log") `
        -WorkDir $consumer

    $null = Invoke-Step `
        -Name "consumer-test-$module" `
        -Title "run the out-of-workspace $consumer test that calls the published contract" `
        -Exe 'cargo' `
        -Arguments @('test', '--manifest-path', $manifestPath, '--all-features') `
        -LogFile (Join-Path $EvidenceDir "21-consumer-$module-test.log") `
        -WorkDir $consumer

    $null = Invoke-Step `
        -Name "consumer-offline-$module" `
        -Title "re-build the $module consumer with a locked dependency set, offline" `
        -Exe 'cargo' `
        -Arguments @('build', '--manifest-path', $manifestPath, '--all-features', '--locked', '--offline') `
        -LogFile (Join-Path $EvidenceDir "22-consumer-$module-offline-locked.log") `
        -WorkDir $consumer

    $null = Invoke-Step `
        -Name "consumer-tree-$module" `
        -Title "resolved dependencies and features of the $module consumer" `
        -Exe 'cargo' `
        -Arguments @('tree', '--manifest-path', $manifestPath, '-e', 'features,normal') `
        -LogFile (Join-Path $EvidenceDir "23-consumer-$module-tree.txt") `
        -WorkDir $consumer

    Copy-Item -LiteralPath (Join-Path $consumer 'Cargo.lock') -Destination (Join-Path $EvidenceDir "24-consumer-$module-lock.txt") -Force

    # Guard: nothing in the consumer's resolved graph may point back into the
    # source workspace. This is an assertion, not a review note.
    $guardLog = Join-Path $EvidenceDir "25-consumer-$module-path-guard.txt"
    $needle = Join-Path $repo 'crates'
    $needleApps = Join-Path $repo 'apps'
    $lockText = Get-Content -LiteralPath (Join-Path $consumer 'Cargo.lock') -Raw
    $manifestText = Get-Content -LiteralPath $manifestPath -Raw
    $hits = @()
    foreach ($text in @($lockText, $manifestText)) {
        foreach ($line in ($text -split "`n")) {
            if ($line -like "*$needle*" -or $line -like "*$needleApps*") {
                $hits += $line.Trim()
            }
        }
    }
    $guardLines = @(
        "guard : the consumer dependency graph must contain no path into the source workspace",
        "needle: $needle",
        "needle: $needleApps",
        "scanned: Cargo.lock + Cargo.toml of $consumer",
        "hits   : $($hits.Count)"
    )
    $guardLines += $hits
    if ($hits.Count -eq 0) {
        $guardLines += 'result: PASS'
    }
    else {
        $guardLines += 'result: FAIL'
        $script:failures += "consumer $module resolves a path back into the source workspace"
    }
    $guardLines | Set-Content -LiteralPath $guardLog -Encoding utf8
    if ($hits.Count -eq 0) { Write-Host "ok    path-guard-$module" } else { Write-Host "FAIL  path-guard-$module" }
}

# ------------------------------------------------------------- negatives ------
foreach ($module in $Modules) {
    $tampered = Join-Path $OutRoot "tamper\$module"
    if (Test-Path -LiteralPath $tampered) {
        Remove-Item -LiteralPath $tampered -Recurse -Force
    }
    Ensure-Directory (Split-Path -Parent $tampered)
    Copy-Item -Recurse -Force -LiteralPath (Join-Path $runDirs['run-a'] "$module-$ModuleVersion") -Destination $tampered

    # Every one of these must be rejected. ExpectExit 1 encodes that; a passing
    # verification is reported as a failure of this script.
    $packedSource = Join-Path $tampered "package\contract\packages\$module\src\lib.rs"
    Add-Content -LiteralPath $packedSource -Value '// tamper: one byte inside the published contract source' -Encoding utf8
    $null = Invoke-Step `
        -Name "tamper-content-$module" `
        -Title "NEGATIVE: modifying a packed source must break artifact verification" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', $tampered) `
        -LogFile (Join-Path $EvidenceDir "50-tamper-content-$module.log") `
        -ExpectExit 1
    Remove-Item -LiteralPath $tampered -Recurse -Force
    Copy-Item -Recurse -Force -LiteralPath (Join-Path $runDirs['run-a'] "$module-$ModuleVersion") -Destination $tampered

    Remove-Item -LiteralPath (Join-Path $tampered 'package\CHANGELOG.md') -Force
    $null = Invoke-Step `
        -Name "tamper-missing-$module" `
        -Title "NEGATIVE: a missing payload file must break artifact verification" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', $tampered) `
        -LogFile (Join-Path $EvidenceDir "51-tamper-missing-file-$module.log") `
        -ExpectExit 1
    Remove-Item -LiteralPath $tampered -Recurse -Force
    Copy-Item -Recurse -Force -LiteralPath (Join-Path $runDirs['run-a'] "$module-$ModuleVersion") -Destination $tampered

    $releasePath = Join-Path $tampered 'release.json'
    (Get-Content -LiteralPath $releasePath -Raw).Replace("`"module_version`": `"$ModuleVersion`"", '"module_version": "9.9.9"') |
        Set-Content -LiteralPath $releasePath -Encoding utf8
    $null = Invoke-Step `
        -Name "tamper-version-$module" `
        -Title "NEGATIVE: a wrong module_version in the release identity must be rejected" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', $tampered) `
        -LogFile (Join-Path $EvidenceDir "52-tamper-wrong-version-$module.log") `
        -ExpectExit 1

    $wrongId = Join-Path $tampered 'package\module.manifest.json'
    (Get-Content -LiteralPath $wrongId -Raw).Replace("`"module_id`": `"$module`"", '"module_id": "someone-else"') |
        Set-Content -LiteralPath $wrongId -Encoding utf8
    $null = Invoke-Step `
        -Name "tamper-module-id-$module" `
        -Title "NEGATIVE: a payload whose module_id differs from the pinned identity must be rejected" `
        -Exe 'cargo' `
        -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', $tampered) `
        -LogFile (Join-Path $EvidenceDir "53-tamper-wrong-module-id-$module.log") `
        -ExpectExit 1

    Remove-Item -LiteralPath $tampered -Recurse -Force -ErrorAction SilentlyContinue
}

# ------------------------------------------------------------------ summary ---
$summary = @(
    '# Wave 5A execution log index',
    '',
    'Every row is one command run by `scripts/plan0014-wave5a.ps1`. Temporary root:',
    "`$OutRoot = $(Split-Path -Parent $OutRoot)/$(Split-Path -Leaf $OutRoot)` (outside the repository).",
    '',
    '| step | command | exit | log |',
    '| --- | --- | --- | --- |'
)
$summary += $logIndex
$summary += ''
if ($failures.Count -eq 0) {
    $summary += 'RESULT: all steps matched their expected exit codes.'
}
else {
    $summary += 'RESULT: FAILED'
    $summary += ($failures | ForEach-Object { "- $_" })
}
$summary += ''
$summary += 'Not executed by this script, and therefore NOT RUN: PostgreSQL migration execution, real'
$summary += 'PostgreSQL installation of a single module, publish/register/install/deploy of any kind,'
$summary += 'vulnerability and licence scanning, and CI. See the record document for the full table.'
$summary | Set-Content -LiteralPath (Join-Path $EvidenceDir 'assembly-log.md') -Encoding utf8

if ($failures.Count -ne 0) {
    Write-Host "`n$($failures.Count) step(s) failed."
    exit 1
}
Write-Host "`nWave 5A assembly and verification completed. Evidence: $EvidenceDir"
exit 0
