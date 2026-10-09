<#
.SYNOPSIS
    Bounded repair regressions for the Wave 5B-S1 review findings R3, R4 and R7.
.DESCRIPTION
    Three layers, as required by the repair instructions (sections 8 and 9):

    * static AST checks against the *current production scripts* - no bare
      `-Label` at an Assert-PathsDoNotOverlap call site (R3), no bare
      `$EvidenceDir + ...` append inside an argument array (R7);
    * isolated tests of the production cleanup functions extracted from the
      current `scripts/test-plan0014-wave5b.ps1` by AST (never a copied
      implementation): registered-root cleanup, internal-evidence retention,
      KeepScratch, protected-parent refusals, sentinel survival;
    * the real entry points: the real driver with an explicit -EvidenceDir
      (must pass preflight and refuse on the evidence rule, not on a parameter
      ambiguity - R3), the real driver's default internal mode and explicit
      external mode lifecycles (R4), and the real gate wrapper with a PATH-stub
      that captures the actual argument arrays (R7, B7 propagation, B8
      evidence protection).

    Every scenario writes raw output into -OutDir; nothing is overwritten.
#>
[CmdletBinding()]
param(
    [string]$Repo = '',
    [Parameter(Mandatory)][string]$OutDir,
    # Optional comma-separated scenario-name filter for iteration;
    # the default runs everything.
    [string]$Only = ''
)

$ErrorActionPreference = 'Stop'
$Repo = if ([string]::IsNullOrWhiteSpace($Repo)) {
    (Resolve-Path (Split-Path -Parent $PSScriptRoot)).Path
} else {
    (Resolve-Path $Repo).Path
}
$driver = Join-Path $Repo 'scripts/test-plan0014-wave5b.ps1'
$wrapper = Join-Path $Repo 'tools/module-release-tool/scripts/run-wave5b-gates.ps1'
foreach ($required in @($driver, $wrapper)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "repair-regressions: missing production script '$required'"
    }
}
if (Test-Path -LiteralPath $OutDir) {
    $existing = @(Get-ChildItem -LiteralPath $OutDir -Force -ErrorAction SilentlyContinue)
    if ($existing.Count -gt 0) {
        throw "repair-regressions: -OutDir '$OutDir' already holds $($existing.Count) item(s); choose a fresh directory"
    }
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$script:Failures = [System.Collections.Generic.List[string]]::new()
$script:OnlyList = @($Only -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
# Every scenario name the sweep knows about, counted as the sweep runs.
$script:ScenarioNames = [System.Collections.Generic.List[string]]::new()
$script:Executed = 0
$script:Skipped = 0
$script:Blocked = [System.Collections.Generic.List[string]]::new()
# Q3 (rereview): one identity for this run, and the list of objects this run
# created. Scratch that is not on this list is nobody's to delete.
$script:RunId = [guid]::NewGuid().ToString('N')
$script:RunOwned = [System.Collections.Generic.List[string]]::new()

function Write-RepairLog([string]$Line) { Write-Host $Line }

function Assert-Repair([bool]$Condition, [string]$Message) {
    if (-not $Condition) {
        throw $Message
    }
}

# ---------------------------------------------------------------------------
# scenario manifest - the filter is validated before anything runs (N5)
# ---------------------------------------------------------------------------
# The names this sweep can run are read out of this file's own syntax tree, so
# the manifest cannot drift from the scenarios, and an unknown -Only name is a
# refusal up front. It used to be discovered at the end, after every scenario
# had printed SKIP: the run then reported success having executed nothing.
$selfTokens = $null
$selfParseErrors = $null
$selfAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $PSCommandPath, [ref]$selfTokens, [ref]$selfParseErrors)
Assert-Repair ($selfParseErrors.Count -eq 0) `
    "repair-regressions: this script does not parse: $($selfParseErrors[0])"
$script:ScenarioManifest = [System.Collections.Generic.List[string]]::new()
foreach ($call in $selfAst.FindAll(
        { param($node) $node -is [System.Management.Automation.Language.CommandAst] }, $true)) {
    if ($call.GetCommandName() -ne 'Invoke-RepairScenario') {
        continue
    }
    $elements = @($call.CommandElements)
    if ($elements.Count -lt 2) {
        continue
    }
    $candidate = $elements[1]
    if ($candidate -isnot [System.Management.Automation.Language.StringConstantExpressionAst]) {
        continue
    }
    if ($script:ScenarioManifest -notcontains $candidate.Value) {
        $script:ScenarioManifest.Add($candidate.Value) | Out-Null
    }
}
Assert-Repair ($script:ScenarioManifest.Count -gt 0) `
    'repair-regressions: this script declares no scenarios; the sweep would be an empty success'
$unknownFilters = @($script:OnlyList | Where-Object { $script:ScenarioManifest -notcontains $_ })
Assert-Repair ($unknownFilters.Count -eq 0) (
    "-Only named no such scenario: '$($unknownFilters -join "', '")'; " +
    "this run knows $($script:ScenarioManifest.Count) scenarios: $($script:ScenarioManifest -join ', ')")

function Invoke-RepairScenario {
    <#  Runs one scenario body, tee-ing all streams into $OutDir\<Name>.log.
        A throwing body is a FAIL, never an interruption of the sweep. #>
    param([string]$Name, [scriptblock]$Body)
    Assert-Repair ($script:ScenarioManifest -contains $Name) `
        "repair-regressions: scenario '$Name' is not in the manifest read from this file"
    Assert-Repair (-not $script:ScenarioNames.Contains($Name)) `
        "repair-regressions: scenario '$Name' is declared twice; the second copy would never run"
    $script:ScenarioNames.Add($Name) | Out-Null
    if ($script:OnlyList.Count -gt 0 -and $script:OnlyList -notcontains $Name) {
        $script:Skipped += 1
        Write-RepairLog "SKIP $Name"
        return
    }
    $script:Executed += 1
    $logPath = Join-Path $OutDir "$Name.log"
    if (Test-Path -LiteralPath $logPath) {
        throw "repair-regressions: scenario log '$logPath' already exists"
    }
    $transcript = $null
    $thrown = $null
    try {
        Start-Transcript -Path $logPath -ErrorAction Stop | Out-Null
        $transcript = $true
        & $Body
    } catch {
        $thrown = $_
    } finally {
        if ($transcript) {
            Stop-Transcript | Out-Null
        }
        # Q3 (rereview): the scratch a scenario created is released by this
        # scenario's finally, whether its body passed, failed or was blocked.
        # Only recorded objects are released; OutDir evidence stays.
        Remove-RunOwnedScratch
    }
    if ($null -ne $thrown) {
        $message = $thrown.Exception.Message
        if ($message.StartsWith('BLOCKED:')) {
            # A named gap, never a pass: the scenario says what it could not
            # exercise and why, and the summary counts it separately.
            $script:Blocked.Add("$Name : $($message.Substring('BLOCKED:'.Length).Trim())") | Out-Null
            Write-RepairLog "BLOCKED $Name : $($message.Substring('BLOCKED:'.Length).Trim())"
        } else {
            $script:Failures.Add("$Name : $message")
            Write-RepairLog "FAIL $Name : $message"
        }
    } else {
        Write-RepairLog "PASS $Name"
    }
}

function New-ScenarioScratch([string]$Name) {
    $root = Join-Path $OutDir ("scenario-" + $Name)
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    return $root
}

function Remove-RunOwnedScratch {
    <#  Q3 (rereview): this sweep deletes exactly the objects it created and
        recorded - nothing else. OutDir is evidence and never enters the list, a
        directory that already existed is never entered, and the removal itself
        is the production Remove-Tree, which proves the path chain and retains
        what it cannot prove. #>
    while ($script:RunOwned.Count -gt 0) {
        $path = $script:RunOwned[$script:RunOwned.Count - 1]
        $script:RunOwned.RemoveAt($script:RunOwned.Count - 1)
        try {
            if (Test-Path -LiteralPath $path) {
                Remove-Tree -Path $path
            }
        } catch {
            Write-RepairLog "note: owned scratch '$path' was retained: $($_.Exception.Message)"
        }
    }
}

function Get-ReparseCapableScratch([string]$Name) {
    <#  A fresh scenario directory on a volume that can actually host a reparse
        point, or a named BLOCKED refusal when no writable volume can. The
        capability is proved by creating a junction, never assumed: exFAT
        volumes and network shares accept the directory name and reject the
        reparse point, and a fixture that silently degraded into a plain
        directory would "pass" without testing anything (review N2).

        Q3 (rereview): the probe base, the junction target, the link and the
        scenario directory all carry this run's id, and none of them is ever
        deleted on the way in. A name that exists is refused or stepped around,
        so two runs with the same scenario name - one after the other, or two at
        the same time - cannot land in one directory and cannot clear each
        other's fixtures.

        This function is also imported on its own by independent probes, so it
        provides its own run id and ownership list, and calls nothing but the
        host's commands. #>
    if ([string]::IsNullOrWhiteSpace($script:RunId)) {
        $script:RunId = [guid]::NewGuid().ToString('N')
    }
    if ($null -eq $script:RunOwned) {
        $script:RunOwned = [System.Collections.Generic.List[string]]::new()
    }
    <#  Creates one directory this run owns, and only one. A name that already
        exists is somebody else's, so it is stepped around rather than emptied:
        Q3 was a helper that deleted whatever a previous run had left at a fixed
        name, and a name is not ownership. #>
    $newOwnedDirectory = {
        param([string]$Base, [string]$Prefix)
        foreach ($try in 1..8) {
            $candidate = Join-Path $Base ($Prefix + '-' + [guid]::NewGuid().ToString('N'))
            if (Test-Path -LiteralPath $candidate) {
                Write-Host "note: '$candidate' already exists; choosing another name instead of taking it over"
                continue
            }
            New-Item -ItemType Directory -Path $candidate | Out-Null
            $canonical = (Resolve-Path -LiteralPath $candidate).Path
            if (-not $script:RunOwned.Contains($canonical)) {
                $script:RunOwned.Add($canonical) | Out-Null
            }
            return $canonical
        }
        throw "cannot create a unique directory under '$Base' with prefix '$Prefix'"
    }
    foreach ($probeBase in @(
            (Join-Path ([System.IO.Path]::GetTempPath()) ('wave5b-reparse-probe-' + $script:RunId)),
            (Join-Path $OutDir ('reparse-probe-' + $script:RunId)))) {
        if (Test-Path -LiteralPath $probeBase) {
            Write-Host "note: probe base '$probeBase' already exists; refusing to share a scratch this run did not create"
            continue
        }
        try {
            New-Item -ItemType Directory -Path $probeBase | Out-Null
            $script:RunOwned.Add((Resolve-Path -LiteralPath $probeBase).Path) | Out-Null
            $target = & $newOwnedDirectory $probeBase 'junction-target'
            $link = Join-Path $probeBase ('junction-link-' + [guid]::NewGuid().ToString('N'))
            New-Item -ItemType Junction -Path $link -Target $target -ErrorAction Stop | Out-Null
            if (-not ((Get-Item -LiteralPath $link -Force).Attributes -band
                    [System.IO.FileAttributes]::ReparsePoint)) {
                throw 'the created item does not carry the reparse-point attribute'
            }
            # The link is a name this run just created; removing the name is all
            # it takes, and the target stays for the scenario to use.
            (Get-Item -LiteralPath $link -Force).Delete()
            return & $newOwnedDirectory $probeBase ('scenario-' + $Name)
        } catch {
            Write-Host "note: probe base '$probeBase' is not usable: $($_.Exception.Message)"
            continue
        }
    }
    throw 'BLOCKED: no writable volume on this host can host a junction, so the ' +
        'reparse-point cleanup rules cannot be exercised here; they are untested, not passing'
}

function Get-FileSha([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

function Get-TreeItemCount([string]$Path) {
    <#  How much is really inside a tree, for a before/after comparison in a
        failure message. The .NET enumeration is used rather than Get-ChildItem
        because a Q2 scenario installs a scoped Get-ChildItem shim to schedule
        its swap, and a dynamic-language shim would be visible here too: a
        fixture must not be able to disturb the instrument that measures it. #>
    if (-not (Test-Path -LiteralPath $Path)) {
        return 0
    }
    return @([System.IO.Directory]::GetFileSystemEntries(
        $Path, '*', [System.IO.SearchOption]::AllDirectories)).Count
}

function Get-NormalizedOutput([string]$Text) {
    <#  A child pwsh renders one logical message across several lines and
        prefixes the continuation with '|'; normalise the whitespace so an
        assertion matches the message wherever the console chose to break it. #>
    ($Text -replace '\|', ' ') -replace '\s+', ' '
}

# ---------------------------------------------------------------------------
# production-AST function extraction (isolated layer)
# ---------------------------------------------------------------------------
$driverTokens = $null
$driverParseErrors = $null
$driverAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $driver, [ref]$driverTokens, [ref]$driverParseErrors)
Assert-Repair ($driverParseErrors.Count -eq 0) `
    "repair-regressions: production driver has parse errors: $($driverParseErrors[0])"
$wanted = @(
    'Get-CanonicalPath',
    'Test-ReparsePoint',
    'Get-UnsafeLink',
    'Assert-PathChainIsReal',
    'Remove-Tree',
    'Register-RunRoot',
    'Remove-RunScratch',
    'Write-Log'
)
$foundNames = [System.Collections.Generic.List[string]]::new()
$functionText = ($driverAst.FindAll(
    { param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] },
    $true) | ForEach-Object {
        if ($wanted -contains $_.Name) {
            $foundNames.Add($_.Name) | Out-Null
            $_.Extent.Text
        }
    }) -join "`r`n"
$missing = @($wanted | Where-Object { $foundNames -notcontains $_ })
Assert-Repair ($missing.Count -eq 0) `
    "repair-regressions: production functions not found in driver AST: $($missing -join ', ')"

# Harness globals the production functions close over, then import them.
$script:RunLog = [System.Collections.Generic.List[string]]::new()
$script:RunRoots = [System.Collections.Generic.List[object]]::new()
$script:ProtectedPaths = [System.Collections.Generic.List[string]]::new()
$script:InternalEvidence = $false
$script:EvidenceInsideRun = $null
$KeepScratch = $false
Invoke-Expression $functionText
Write-RepairLog "imported production cleanup functions from the current driver AST: $($wanted -join ', ')"

function Reset-RunHarness {
    $script:RunRoots = [System.Collections.Generic.List[object]]::new()
    $script:ProtectedPaths = [System.Collections.Generic.List[string]]::new()
    $script:RunLog = [System.Collections.Generic.List[string]]::new()
    $script:InternalEvidence = $false
    $script:EvidenceInsideRun = $null
    $KeepScratch = $false
}

# Anything that ends this run abnormally from here on still releases what this
# run created. Each scenario also has its own finally; this is the net under a
# terminating error raised outside a scenario body, and like that finally it
# releases only the recorded objects. It sits below the helper definitions and
# checks for them, because a trap that itself fails replaces the real error
# with a missing-command message - which is what it did on its first try here.
trap {
    if (Get-Command -Name Remove-RunOwnedScratch -CommandType Function -ErrorAction SilentlyContinue) {
        Remove-RunOwnedScratch
    }
    break
}

# ---------------------------------------------------------------------------
# static layer - R3 and R7 guards against reintroduction
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'static-r3-no-ambiguous-label' {
    $ambiguous = @()
    foreach ($call in $driverAst.FindAll(
            { param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true)) {
        if ($call.GetCommandName() -ne 'Assert-PathsDoNotOverlap') { continue }
        foreach ($element in $call.CommandElements) {
            if ($element -is [System.Management.Automation.Language.CommandParameterAst] -and
                $element.ParameterName -eq 'Label') {
                $ambiguous += "line $($element.Extent.StartLineNumber)"
            }
        }
    }
    Assert-Repair ($ambiguous.Count -eq 0) `
        "Assert-PathsDoNotOverlap is still called with bare -Label (ambiguous): $($ambiguous -join ', ')"
}

Invoke-RepairScenario 'static-r7-no-array-append' {
    $wrapperTokens = $null
    $wrapperErrors = $null
    $wrapperAst = [System.Management.Automation.Language.Parser]::ParseFile(
        $wrapper, [ref]$wrapperTokens, [ref]$wrapperErrors)
    Assert-Repair ($wrapperErrors.Count -eq 0) `
        "wrapper has parse errors: $($wrapperErrors[0])"
    $bad = @()
    foreach ($binary in $wrapperAst.FindAll(
            { param($n)
                $n -is [System.Management.Automation.Language.BinaryExpressionAst] -and
                $n.Operator -eq 'Add' -and
                $n.FindAll({ param($d)
                    $d -is [System.Management.Automation.Language.VariableExpressionAst] -and
                    $d.VariablePath.UserPath -eq 'EvidenceDir' }, $true).Count -gt 0 -and
                $n.FindAll({ param($d)
                    $d -is [System.Management.Automation.Language.StringConstantExpressionAst] }, $true).Count -gt 0
            }, $true)) {
        $bad += "line $($binary.Extent.StartLineNumber)"
    }
    Assert-Repair ($bad.Count -eq 0) `
        "wrapper still appends a bare EvidenceDir concatenation inside an argument array: $($bad -join ', ')"
}

Invoke-RepairScenario 'static-cleanup-surface-is-self-contained' {
    <#  An independent probe imports the cleanup surface by name: it parses the
        production driver, takes the extent text of the eight functions below and
        runs that text on its own (that is exactly how review Q1 and Q2 were
        reproduced). Anything one of them calls therefore has to live inside the
        extracted set, otherwise a probe of a real fix dies on a missing command
        and the finding reopens for the wrong reason. #>
    # Only top-level definitions are part of a file's surface: a scenario may
    # define a scoped shim over a real cmdlet (the Q2 probes do exactly that to
    # schedule a swap at one enumeration boundary), and that shim is not a
    # dependency of anything.
    $topLevelNames = {
        param($tree)
        $names = [System.Collections.Generic.List[string]]::new()
        foreach ($definition in $tree.FindAll(
                { param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] },
                $true)) {
            $blocks = 0
            $cursor = $definition.Parent
            while ($null -ne $cursor) {
                if ($cursor -is [System.Management.Automation.Language.ScriptBlockAst]) { $blocks++ }
                $cursor = $cursor.Parent
            }
            if ($blocks -le 1) { $names.Add($definition.Name) | Out-Null }
        }
        return , $names
    }
    $driverNames = & $topLevelNames $driverAst
    $helperNames = & $topLevelNames $selfAst
    $orphans = [System.Collections.Generic.List[string]]::new()
    foreach ($fn in $driverAst.FindAll(
            { param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] },
            $true)) {
        if ($wanted -notcontains $fn.Name) { continue }
        foreach ($call in $fn.FindAll(
                { param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true)) {
            $called = $call.GetCommandName()
            if ([string]::IsNullOrWhiteSpace($called)) { continue }
            if (($driverNames -contains $called -or $helperNames -contains $called) -and
                $wanted -notcontains $called) {
                $orphans.Add("$($fn.Name) -> $called") | Out-Null
            }
        }
    }
    <#  The same rule holds the other way round: the Q3 probe imports
        Get-ReparseCapableScratch by itself, so that one may call neither file's
        helpers at all. #>
    foreach ($fn in $selfAst.FindAll(
            { param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] },
            $true)) {
        if ($fn.Name -ne 'Get-ReparseCapableScratch') { continue }
        foreach ($call in $fn.FindAll(
                { param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true)) {
            $called = $call.GetCommandName()
            if ([string]::IsNullOrWhiteSpace($called)) { continue }
            if ($driverNames -contains $called -or $helperNames -contains $called) {
                $orphans.Add("'$($fn.Name)' is imported alone and may not call: $called") | Out-Null
            }
        }
    }
    Assert-Repair ($orphans.Count -eq 0) (
        'the probe-extractable cleanup surface calls functions outside itself, so a probe ' +
        "that imports only that surface would break: $(($orphans | Sort-Object -Unique) -join ', ')")
}

Invoke-RepairScenario 'static-no-recursive-delete-in-either-harness' {
    <#  Q3 (rereview): the repair helper cleared a previous run's scratch with
        `Remove-Item -Recurse -Force` on a path nobody had proved this run owns.
        That is a defect class, not a one-line slip, so neither harness may use a
        recursive Remove-Item at all: the only tree removal either file performs
        is Remove-Tree, which proves the path first and retains what it cannot
        prove. #>
    $offenders = [System.Collections.Generic.List[string]]::new()
    foreach ($file in @($driver, $PSCommandPath)) {
        $fileTokens = $null
        $fileErrors = $null
        $fileAst = [System.Management.Automation.Language.Parser]::ParseFile(
            $file, [ref]$fileTokens, [ref]$fileErrors)
        Assert-Repair ($fileErrors.Count -eq 0) "'$file' does not parse: $($fileErrors[0])"
        foreach ($call in $fileAst.FindAll(
                { param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true)) {
            if ($call.GetCommandName() -ne 'Remove-Item') { continue }
            foreach ($element in $call.CommandElements) {
                if ($element -is [System.Management.Automation.Language.CommandParameterAst] -and
                    $element.ParameterName -match '^(Recurse|r)$') {
                    $offenders.Add(("{0}:{1}" -f (Split-Path -Leaf $file),
                        $element.Extent.StartLineNumber)) | Out-Null
                }
            }
        }
    }
    Assert-Repair ($offenders.Count -eq 0) (
        'a recursive Remove-Item is back; it deletes content the run does not own at: ' +
        ($offenders -join ', '))
}

# ---------------------------------------------------------------------------
# isolated layer - production cleanup functions against real directories
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'ast-cleanup-normal-external' {
    Reset-RunHarness
    $parent = New-ScenarioScratch 'external'
    $repoStandIn = Join-Path (New-ScenarioScratch 'external-repo') 'repo'
    New-Item -ItemType Directory -Force -Path $repoStandIn | Out-Null
    $sentinel = Join-Path $parent 'sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'keep-me' -Encoding utf8
    $oldRun = Join-Path $parent 'run-stale-old'
    $oldFile = Join-Path $oldRun 'old.txt'
    New-Item -ItemType Directory -Force -Path $oldRun | Out-Null
    Set-Content -LiteralPath $oldFile -Value 'stale' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel
    $oldSha = Get-FileSha $oldFile

    New-Item -ItemType Directory -Force -Path (Join-Path $parent 'run-guidaaa') | Out-Null
    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidaaa') `
        -Parent $parent -Kind 'main'
    New-Item -ItemType File -Force -Path (Join-Path $runRoot 'work.txt') | Out-Null
    $fallbackParent = [System.IO.Path]::GetTempPath()
    $fallbackCandidate = Join-Path $fallbackParent (
        'wave5b-repair-fixture-fallback-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $fallbackCandidate | Out-Null
    $fallback = Register-RunRoot -Root $fallbackCandidate -Parent $fallbackParent -Kind 'fallback'
    $script:ProtectedPaths.Add($repoStandIn)
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))

    Remove-RunScratch

    Assert-Repair (-not (Test-Path -LiteralPath $runRoot)) 'main run root survived normal cleanup'
    Assert-Repair (-not (Test-Path -LiteralPath $fallback)) 'fallback root survived normal cleanup'
    Assert-Repair (Test-Path -LiteralPath $sentinel) 'sentinel in the scratch parent was deleted'
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'sentinel content changed'
    Assert-Repair (Test-Path -LiteralPath $oldRun) 'a pre-existing old run directory was deleted'
    Assert-Repair ((Get-FileSha $oldFile) -eq $oldSha) 'old run content changed'
}

Invoke-RepairScenario 'ast-cleanup-internal-evidence-retained' {
    Reset-RunHarness
    $parent = New-ScenarioScratch 'internal'
    New-Item -ItemType Directory -Force -Path (Join-Path $parent 'run-guidbbb') | Out-Null
    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidbbb') `
        -Parent $parent -Kind 'main'
    $evidence = Join-Path $runRoot 'evidence'
    New-Item -ItemType Directory -Force -Path $evidence | Out-Null
    $csv = Join-Path $evidence 'catalog-reconciliation.csv'
    Set-Content -LiteralPath $csv -Value 'Family,Case' -Encoding utf8
    $junk = Join-Path $runRoot 'repo-clone'
    New-Item -ItemType Directory -Force -Path $junk | Out-Null
    New-Item -ItemType File -Force -Path (Join-Path $junk 'clone.txt') | Out-Null
    $script:InternalEvidence = $true
    $script:EvidenceInsideRun = $evidence
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))

    Remove-RunScratch

    Assert-Repair (Test-Path -LiteralPath $runRoot) 'run root must stay as the evidence carrier'
    Assert-Repair (Test-Path -LiteralPath $csv) 'internal evidence did not survive cleanup'
    Assert-Repair (-not (Test-Path -LiteralPath $junk)) 'non-evidence content was not cleaned'
    Assert-Repair (($script:RunLog -join "`n") -match 'internal evidence retained') `
        'cleanup did not log the internal-evidence retention'
}

Invoke-RepairScenario 'ast-cleanup-keepscratch' {
    Reset-RunHarness
    $KeepScratch = $true
    $parent = New-ScenarioScratch 'keep'
    New-Item -ItemType Directory -Force -Path (Join-Path $parent 'run-guidccc') | Out-Null
    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidccc') `
        -Parent $parent -Kind 'main'
    New-Item -ItemType File -Force -Path (Join-Path $runRoot 'work.txt') | Out-Null

    Remove-RunScratch

    Assert-Repair (Test-Path -LiteralPath $runRoot) 'KeepScratch did not retain the run root'
    Assert-Repair (($script:RunLog -join "`n") -match 'kept scratch root') `
        'KeepScratch did not log the retained location'
}

Invoke-RepairScenario 'ast-cleanup-protected-refusals' {
    Reset-RunHarness
    $parent = New-ScenarioScratch 'protected'
    $sentinel = Join-Path $parent 'sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'keep' -Encoding utf8
    # A registration that names the protected parent itself must refuse.
    $wrong = [pscustomobject]@{
        Root   = (Get-CanonicalPath $parent)
        Parent = (Get-CanonicalPath (Split-Path -Parent $parent))
        Kind   = 'main'
    }
    $script:RunRoots.Add($wrong) | Out-Null
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))

    $warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
    $joinedWarnings = $warnings -join "`n"

    Assert-Repair (Test-Path -LiteralPath $sentinel) 'a protected parent was deleted by cleanup'
    Assert-Repair ($joinedWarnings -match 'protected path') `
        "refusal did not name the protected-path rule: $joinedWarnings"
}

Invoke-RepairScenario 'ast-cleanup-outside-registered-parent' {
    Reset-RunHarness
    $parent = New-ScenarioScratch 'outside'
    $unrelated = New-ScenarioScratch 'outside-unrelated'
    $runRoot = Join-Path $parent 'run-guidddd'
    New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
    # Hand-built registration whose parent claim is false (root not under parent).
    $script:RunRoots.Add([pscustomobject]@{
        Root   = (Get-CanonicalPath $runRoot)
        Parent = (Get-CanonicalPath $unrelated)
        Kind   = 'main'
    }) | Out-Null
    $warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
    $joinedWarnings = $warnings -join "`n"
    Assert-Repair (Test-Path -LiteralPath $runRoot) 'a root outside its registered parent was deleted'
    Assert-Repair ($joinedWarnings -match 'registered parent') "refusal was not explicit: $joinedWarnings"
}

Invoke-RepairScenario 'ast-register-refuses-root-equal-to-protected-parent' {
    Reset-RunHarness
    $parent = New-ScenarioScratch 'register'
    $thrown = $null
    try {
        [void](Register-RunRoot -Root $parent -Parent $parent -Kind 'main')
    } catch {
        $thrown = $_.Exception.Message
    }
    Assert-Repair ($null -ne $thrown -and $thrown -match 'not inside its validated parent') `
        'Register-RunRoot accepted a root equal to its parent'
}

# ---------------------------------------------------------------------------
# review N2 - a replaced root, a junction parent, a junction inside the tree
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'ast-cleanup-root-replaced-by-junction' {
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'linkroot'
    $parent = Join-Path $scratch 'parent'
    $outside = Join-Path $scratch 'outside'
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    New-Item -ItemType Directory -Force -Path $outside | Out-Null
    $sentinel = Join-Path $outside 'sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'do-not-delete' -Encoding utf8
    $nested = Join-Path $outside 'nested'
    New-Item -ItemType Directory -Force -Path $nested | Out-Null
    $deep = Join-Path $nested 'deep.txt'
    Set-Content -LiteralPath $deep -Value 'deep-content' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel
    $deepSha = Get-FileSha $deep

    New-Item -ItemType Directory -Force -Path (Join-Path $parent 'run-guidlink') | Out-Null
    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidlink') `
        -Parent $parent -Kind 'work'
    New-Item -ItemType File -Force -Path (Join-Path $runRoot 'work.txt') | Out-Null
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))

    # Something else replaces the registered root with a junction to content the
    # run never created (review N2 probe).
    Remove-Tree -Path $runRoot
    New-Item -ItemType Junction -Path $runRoot -Target $outside | Out-Null
    Assert-Repair ((Get-Item -LiteralPath $runRoot -Force).Attributes -band
        [System.IO.FileAttributes]::ReparsePoint) `
        'the fixture is not a reparse point; this scenario would prove nothing'

    $notes = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
    $joined = (($script:RunLog + $notes) -join "`n")
    Set-Content -LiteralPath (Join-Path $OutDir 'ast-cleanup-root-replaced-by-junction-notes.txt') `
        -Value $joined -Encoding utf8

    Assert-Repair (Test-Path -LiteralPath $sentinel) `
        'cleanup deleted out-of-root content through the replaced root'
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'the out-of-root sentinel changed bytes'
    Assert-Repair (Test-Path -LiteralPath $deep) 'nested out-of-root content was deleted'
    Assert-Repair ((Get-FileSha $deep) -eq $deepSha) 'nested out-of-root content changed bytes'
    Assert-Repair (-not (Test-Path -LiteralPath $runRoot)) `
        'the link the run created was left behind instead of unlinked'
    Assert-Repair ($joined -match 'replaced run root|reparse point') `
        "cleanup never said what it found at the replaced root: $joined"
}

Invoke-RepairScenario 'ast-cleanup-parent-replaced-by-junction' {
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'linkparent'
    $host_ = Join-Path $scratch 'host'
    New-Item -ItemType Directory -Force -Path $host_ | Out-Null
    $real = Join-Path $host_ 'real-parent'
    New-Item -ItemType Directory -Force -Path $real | Out-Null
    $link = Join-Path $host_ 'link-parent'
    New-Item -ItemType Junction -Path $link -Target $real | Out-Null
    $sentinel = Join-Path $real 'sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'keep-me' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel
    $rootInside = Join-Path $link 'run-guidbehind'
    New-Item -ItemType Directory -Force -Path $rootInside | Out-Null

    $thrown = $null
    try {
        [void](Register-RunRoot -Root $rootInside -Parent $link -Kind 'main')
    } catch {
        $thrown = $_.Exception.Message
    }
    Assert-Repair ($null -ne $thrown -and $thrown -match 'reparse point') `
        "Register-RunRoot accepted a chain running through a junction: $thrown"

    # The same tree reaching the delete stage anyway, through a hand-built
    # registration: cleanup must refuse it and leave everything behind the link.
    $script:RunRoots.Add([pscustomobject]@{
        Root   = (Get-CanonicalPath $rootInside)
        Parent = (Get-CanonicalPath $link)
        Kind   = 'main'
    }) | Out-Null
    $warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
    $joinedWarnings = $warnings -join "`n"
    Set-Content -LiteralPath (Join-Path $OutDir 'ast-cleanup-parent-replaced-by-junction-notes.txt') `
        -Value $joinedWarnings -Encoding utf8

    Assert-Repair (Test-Path -LiteralPath $sentinel) `
        'cleanup deleted content that lives behind a junction parent'
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'the content behind the junction changed bytes'
    Assert-Repair (Test-Path -LiteralPath $rootInside) 'a root behind a junction parent was removed anyway'
    Assert-Repair ($joinedWarnings -match 'reparse point') `
        "cleanup did not name the link rule: $joinedWarnings"
}

Invoke-RepairScenario 'ast-cleanup-junction-inside-tree' {
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'linkchild'
    $parent = Join-Path $scratch 'parent'
    $outside = Join-Path $scratch 'outside'
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    New-Item -ItemType Directory -Force -Path $outside | Out-Null
    $sentinel = Join-Path $outside 'sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'keep-me' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel

    New-Item -ItemType Directory -Force -Path (Join-Path $parent 'run-guidchild') | Out-Null
    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidchild') `
        -Parent $parent -Kind 'work'
    $inner = Join-Path $runRoot 'inner'
    New-Item -ItemType Directory -Force -Path $inner | Out-Null
    New-Item -ItemType File -Force -Path (Join-Path $inner 'mine.txt') | Out-Null
    New-Item -ItemType Junction -Path (Join-Path $runRoot 'escape') -Target $outside | Out-Null
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))

    Remove-RunScratch

    Assert-Repair (-not (Test-Path -LiteralPath $runRoot)) 'the run root survived its own cleanup'
    Assert-Repair (Test-Path -LiteralPath $sentinel) `
        'cleanup followed a junction out of the run and deleted what it points at'
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'the junction target changed bytes'
}

# ---------------------------------------------------------------------------
# rereview Q1 - the chain ABOVE the registered parent, and the identity of the
# directory the registration names
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'ast-cleanup-ancestor-above-parent-replaced-by-junction' {
    <#  Review Q1: registration stopped its chain check at the parent it was
        given, so a link planted *above* that parent was invisible and cleanup
        deleted the tree the link points at. The registered parent may not be an
        unproven start of trust, so this runs the review's own fixture at three
        different depths above the parent. #>
    $scratch = Get-ReparseCapableScratch 'ancestorlink'
    foreach ($depth in @(1, 2, 3)) {
        Reset-RunHarness
        $unit = Join-Path $scratch ("depth$depth")
        $anchor = Join-Path $unit 'anchor'
        $below = @()
        for ($level = 1; $level -lt $depth; $level++) { $below += ("level$level") }
        $below += 'scratch'
        $relative = ($below -join [System.IO.Path]::DirectorySeparatorChar)
        $parent = Join-Path $anchor $relative
        $runRoot = Join-Path $parent 'run-owned'
        $outside = Join-Path $unit 'outside-owned'
        $outsideRun = Join-Path (Join-Path $outside $relative) 'run-owned'
        New-Item -ItemType Directory -Force -Path $runRoot, $outsideRun | Out-Null
        $sentinel = Join-Path $outsideRun 'sentinel.txt'
        Set-Content -LiteralPath $sentinel -Value 'outside-owned sentinel' -Encoding utf8
        Set-Content -LiteralPath (Join-Path $runRoot 'mine.txt') -Value 'run-owned' -Encoding utf8
        $sentinelSha = Get-FileSha $sentinel
        $script:ProtectedPaths.Add((Get-CanonicalPath $parent))
        [void](Register-RunRoot -Root $runRoot -Parent $parent -Kind 'main')

        # Park the level the run knows nothing about and plant a junction in it.
        $parked = Join-Path $unit 'parked-anchor'
        Move-Item -LiteralPath $anchor -Destination $parked
        New-Item -ItemType Junction -Path $anchor -Target $outside -ErrorAction Stop | Out-Null
        $parkedMine = Join-Path (Join-Path $parked $relative) 'run-owned/mine.txt'
        $outsideBefore = Get-TreeItemCount $outside

        $warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
        $joined = (($script:RunLog + $warnings) -join "`n")
        $outsideAfter = Get-TreeItemCount $outside
        $reported = Get-UnsafeLink -Path $runRoot -Boundary $parent

        Assert-Repair (Test-Path -LiteralPath $sentinel) (
            "depth ${depth}Q1: cleanup deleted the external tree reached through an ancestor " +
            "the run never registered ('$sentinel' is gone, the external tree went from " +
            "$outsideBefore to $outsideAfter item(s))")
        Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) "depth ${depth}Q1: the external sentinel changed bytes"
        Assert-Repair ($reported.ToLowerInvariant() -eq $anchor.ToLowerInvariant()) (
            "depth ${depth}Q1: Get-UnsafeLink still stops at the registered parent - it reported " +
            "'$reported' while '$anchor' is a junction, so the chain above the parent is unchecked")
        Assert-Repair (Test-Path -LiteralPath $parkedMine) (
            "depth ${depth}Q1: the run's own tree was taken apart through the planted link")
        Assert-Repair ($joined -match 'reparse point') (
            "depth ${depth}Q1: the refusal did not name the link rule: $joined")
    }
}

Invoke-RepairScenario 'ast-register-refuses-link-already-above-parent' {
    <#  The other half of Q1: the link was there *before* registration, so
        nothing about the run created it. Registration has to refuse, and the
        delete stage has to refuse the same tree when the registration is
        hand-built around that refusal. #>
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'registerlink'
    $host_ = Join-Path $scratch 'host'
    $outside = Join-Path $scratch 'outside'
    New-Item -ItemType Directory -Force -Path (
        Join-Path $host_ 'real/scratch/run-owned'), (Join-Path $outside 'scratch/run-owned') | Out-Null
    $anchor = Join-Path $host_ 'anchor'
    Move-Item -LiteralPath (Join-Path $host_ 'real') -Destination (Join-Path $host_ 'parked')
    New-Item -ItemType Junction -Path $anchor -Target $outside -ErrorAction Stop | Out-Null
    $parent = Join-Path $anchor 'scratch'
    $root = Join-Path $parent 'run-owned'
    $sentinel = Join-Path $outside 'scratch/run-owned/sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'outside-owned sentinel' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel

    $outsideBefore = Get-TreeItemCount $outside
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))
    $script:RunRoots.Add([pscustomobject]@{
            Root   = (Get-CanonicalPath $root)
            Parent = (Get-CanonicalPath $parent)
            Kind   = 'main'
        }) | Out-Null
    $warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
    $joinedWarnings = $warnings -join "`n"
    $outsideAfter = Get-TreeItemCount $outside
    Assert-Repair (Test-Path -LiteralPath $sentinel) (
        'cleanup deleted the tree a pre-existing junction above the parent points at ' +
        "('$sentinel' is gone; the external tree went from $outsideBefore to $outsideAfter item(s))")
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'the external sentinel changed bytes'
    Assert-Repair ($joinedWarnings -match 'reparse point') "the refusal did not name the link rule: $joinedWarnings"

    $thrown = $null
    try {
        [void](Register-RunRoot -Root $root -Parent $parent -Kind 'main')
    } catch {
        $thrown = $_.Exception.Message
    }
    Assert-Repair ($null -ne $thrown -and $thrown -match 'reparse point') (
        "Register-RunRoot accepted a run path reached through a pre-existing junction: $thrown")
}

Invoke-RepairScenario 'ast-remove-tree-linked-ancestor-retains-tree' {
    <#  Q1 at the layer below the registry: Remove-Tree is also called on paths
        nobody registered (scratch files, noise directories), so it has to prove
        its own path chain too rather than trust its caller. #>
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'removetreelink'
    $unit = Join-Path $scratch 'unit'
    $outside = Join-Path $unit 'outside-owned'
    New-Item -ItemType Directory -Force -Path (Join-Path $outside 'run-owned/nested') | Out-Null
    $sentinel = Join-Path $outside 'run-owned/sentinel.txt'
    $deep = Join-Path $outside 'run-owned/nested/deep.txt'
    Set-Content -LiteralPath $sentinel -Value 'outside-owned sentinel' -Encoding utf8
    Set-Content -LiteralPath $deep -Value 'outside-owned nested' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel
    $deepSha = Get-FileSha $deep
    $anchor = Join-Path $unit 'anchor'
    New-Item -ItemType Junction -Path $anchor -Target $outside -ErrorAction Stop | Out-Null
    $through = Join-Path $anchor 'run-owned'

    $outsideBefore = Get-TreeItemCount $outside
    $thrown = $null
    try {
        Remove-Tree -Path $through
    } catch {
        $thrown = $_.Exception.Message
    }
    $outsideAfter = Get-TreeItemCount $outside
    Assert-Repair (Test-Path -LiteralPath $sentinel) (
        "Remove-Tree walked into a tree reached through a junction instead of refusing " +
        "('$sentinel' is gone; the external tree went from $outsideBefore to $outsideAfter " +
        "item(s)); it said: $thrown")
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'the external sentinel changed bytes'
    Assert-Repair (Test-Path -LiteralPath $deep) 'nested external content was deleted'
    Assert-Repair ((Get-FileSha $deep) -eq $deepSha) 'nested external content changed bytes'
    Assert-Repair ($null -ne $thrown -and $thrown -match 'reparse point') (
        "Remove-Tree left the junction target alone but never said why it stopped: $thrown")
    Assert-Repair (Test-Path -LiteralPath $anchor) 'the junction itself was removed rather than retained'
}

Invoke-RepairScenario 'ast-cleanup-ownership-witness-is-required' {
    <#  Q1 asks that registration and cleanup be bound to a physical anchor: the
        name of a path says nothing about the directory standing behind it. The
        run therefore anchors every registered root with an unguessable witness
        file it wrote there itself, and refuses a root whose witness is gone or
        no longer carries its token - which is what a wholesale replacement of
        the tree looks like. The scenario also proves the opposite direction,
        because a cleanup that refused everything would pass this too. #>
    Reset-RunHarness
    $parent = New-ScenarioScratch 'witness'
    $rootPath = Join-Path $parent 'run-guidwit'
    New-Item -ItemType Directory -Force -Path $rootPath | Out-Null
    $runRoot = Register-RunRoot -Root $rootPath -Parent $parent -Kind 'main'
    New-Item -ItemType File -Force -Path (Join-Path $runRoot 'work.txt') | Out-Null
    $script:ProtectedPaths.Add((Get-CanonicalPath $parent))

    $entry = $script:RunRoots[0]
    Assert-Repair ($null -ne $entry.Witness -and $null -ne $entry.Token) `
        'Q1: Register-RunRoot records no ownership witness, so cleanup cannot tell the run tree from a look-alike'
    $witnessPath = Join-Path $runRoot $entry.Witness
    Assert-Repair (Test-Path -LiteralPath $witnessPath) 'the ownership witness is not inside the registered root'

    Set-Content -LiteralPath $witnessPath -Value 'someone-elses-content' -Encoding utf8
    $warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
    $joined = $warnings -join "`n"
    Assert-Repair (Test-Path -LiteralPath $runRoot) (
        'cleanup deleted a registered root whose ownership witness no longer matched the registration')
    Assert-Repair ($joined -match 'witness') "the refusal did not name the witness rule: $joined"

    Set-Content -LiteralPath $witnessPath -Value ("wave5b-run-root-owner={0}" -f $entry.Token) -Encoding utf8
    Remove-RunScratch
    Assert-Repair (-not (Test-Path -LiteralPath $runRoot)) (
        'positive control failed: a witnessed, owned, unprotected root was not cleaned')
}

# ---------------------------------------------------------------------------
# rereview Q2 - the identity of a directory between verifying it and using it
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'ast-remove-tree-queued-directory-swapped-before-pop' {
    <#  Review Q2, window one: a child directory passes the attribute check while
        its parent is enumerated, and only its name is queued. Something replaces
        that name with a junction before the walk pops it, and the enumeration
        then walks into someone else's tree. The swap below is a deterministic
        scheduler injection at that exact boundary - the real cmdlet still
        enumerates, and the real production Remove-Tree still deletes. #>
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'queueswap'
    $unit = Join-Path $scratch 'unit'
    $run = Join-Path $unit 'run-owned'
    $child = Join-Path $run 'child'
    $sibling = Join-Path $run 'sibling'
    $outside = Join-Path $unit 'outside-owned'
    New-Item -ItemType Directory -Force -Path $child, $sibling, (Join-Path $outside 'nested') | Out-Null
    $sentinel = Join-Path $outside 'sentinel.txt'
    $deep = Join-Path $outside 'nested/deep.txt'
    Set-Content -LiteralPath $sentinel -Value 'outside-owned sentinel' -Encoding utf8
    Set-Content -LiteralPath $deep -Value 'outside-owned nested content' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $child 'child-file.txt') -Value 'run-owned' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $sibling 'mine.txt') -Value 'run-owned' -Encoding utf8
    $sentinelSha = Get-FileSha $sentinel
    $deepSha = Get-FileSha $deep

    # The swap is scheduled at the enumeration of the *parent*: the child is
    # checked, queued by name, and only then replaced (review Q2, window one).
    $script:SwapShim = [ordered]@{
        Done   = $false
        Watch  = $run
        Swap   = $child
        Parked = (Join-Path $unit 'parked-child')
        Into   = $outside
    }
    $script:RealChildItem = Get-Command -CommandType Cmdlet Get-ChildItem
    function Get-ChildItem {
        [CmdletBinding()]
        param([string]$LiteralPath, [switch]$Force)
        $forward = @{ ErrorAction = 'SilentlyContinue' }
        if ($PSBoundParameters.ContainsKey('LiteralPath')) { $forward['LiteralPath'] = $LiteralPath }
        if ($Force) { $forward['Force'] = $true }
        $items = @(& $script:RealChildItem @forward)
        $shim = $script:SwapShim
        if (-not $shim.Done -and $PSBoundParameters.ContainsKey('LiteralPath') -and
            $LiteralPath -eq $shim.Watch) {
            $root = [System.IO.Path]::GetFullPath($scratch) + [System.IO.Path]::DirectorySeparatorChar
            foreach ($candidate in @($shim.Swap, $shim.Parked, $shim.Into)) {
                if (-not [System.IO.Path]::GetFullPath($candidate).StartsWith($root,
                        [System.StringComparison]::OrdinalIgnoreCase)) {
                    throw 'the scheduled swap would escape the scenario scratch'
                }
            }
            Move-Item -LiteralPath $shim.Swap -Destination $shim.Parked
            New-Item -ItemType Junction -Path $shim.Swap -Target $shim.Into -ErrorAction Stop | Out-Null
            $shim.Done = $true
        }
        return $items
    }

    $outsideBefore = Get-TreeItemCount $outside
    $thrown = $null
    try {
        Remove-Tree -Path $run
    } catch {
        $thrown = $_.Exception.Message
    }
    $outsideAfter = Get-TreeItemCount $outside
    Assert-Repair ($script:SwapShim.Done) 'the scheduled swap never ran; this scenario would prove nothing'
    Assert-Repair (Test-Path -LiteralPath $sentinel) (
        'Q2: Remove-Tree deleted through a directory that became a junction after it was ' +
        "queued ('$sentinel' is gone; the external tree went from $outsideBefore to $outsideAfter " +
        "item(s)); Remove-Tree said: $thrown")
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'Q2: the external sentinel changed bytes'
    Assert-Repair (Test-Path -LiteralPath $deep) 'Q2: nested external content was deleted'
    Assert-Repair ((Get-FileSha $deep) -eq $deepSha) 'Q2: nested external content changed bytes'
    Assert-Repair ($null -ne $thrown -and $thrown -match 'reparse point|retain') (
        "Q2: Remove-Tree left the external tree alone but never said why it stopped: $thrown")
    Assert-Repair (Test-Path -LiteralPath (Join-Path $script:SwapShim.Parked 'child-file.txt')) `
        'Q2: the run-owned content that was moved aside did not survive'
    Assert-Repair (-not (Test-Path -LiteralPath (Join-Path $sibling 'mine.txt'))) `
        'positive control failed: the run-owned sibling the walk could prove was not cleaned'
    Assert-Repair (Test-Path -LiteralPath $run) 'the retained root vanished, so the refusal leaves nothing to inspect'
}

Invoke-RepairScenario 'ast-remove-tree-directory-swapped-after-enumeration' {
    <#  Review Q2, window two: the swap lands after the directory has been
        enumerated but before the entries of that enumeration are deleted, so the
        entries are stale and every one of them is now a path *through* a link.
        Both trees hold a file of the same name, and only the external one may
        survive. #>
    Reset-RunHarness
    $scratch = Get-ReparseCapableScratch 'enumswap'
    $unit = Join-Path $scratch 'unit'
    $run = Join-Path $unit 'run-owned'
    $child = Join-Path $run 'child'
    $sibling = Join-Path $run 'sibling'
    $outside = Join-Path $unit 'outside-owned'
    New-Item -ItemType Directory -Force -Path $child, $sibling, $outside | Out-Null
    Set-Content -LiteralPath (Join-Path $child 'shared.txt') -Value 'run-owned bytes' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $sibling 'mine.txt') -Value 'run-owned sibling bytes' -Encoding utf8
    $outsideFile = Join-Path $outside 'shared.txt'
    $sentinel = Join-Path $outside 'sentinel.txt'
    Set-Content -LiteralPath $outsideFile -Value 'outside-owned bytes' -Encoding utf8
    Set-Content -LiteralPath $sentinel -Value 'outside-owned sentinel' -Encoding utf8
    $outsideSha = Get-FileSha $outsideFile
    $sentinelSha = Get-FileSha $sentinel

    # The swap is scheduled at the enumeration of the directory itself: the
    # listing cleanup is about to act on is taken first, and the entry that
    # listing names is replaced under it before cleanup reaches it (review Q2,
    # window two).
    $script:SwapShim = [ordered]@{
        Done   = $false
        Watch  = $child
        Swap   = $child
        Parked = (Join-Path $unit 'parked-child')
        Into   = $outside
    }
    $script:RealChildItem = Get-Command -CommandType Cmdlet Get-ChildItem
    function Get-ChildItem {
        [CmdletBinding()]
        param([string]$LiteralPath, [switch]$Force)
        $forward = @{ ErrorAction = 'SilentlyContinue' }
        if ($PSBoundParameters.ContainsKey('LiteralPath')) { $forward['LiteralPath'] = $LiteralPath }
        if ($Force) { $forward['Force'] = $true }
        $items = @(& $script:RealChildItem @forward)
        $shim = $script:SwapShim
        if (-not $shim.Done -and $PSBoundParameters.ContainsKey('LiteralPath') -and
            $LiteralPath -eq $shim.Watch) {
            $root = [System.IO.Path]::GetFullPath($scratch) + [System.IO.Path]::DirectorySeparatorChar
            foreach ($candidate in @($shim.Swap, $shim.Parked, $shim.Into)) {
                if (-not [System.IO.Path]::GetFullPath($candidate).StartsWith($root,
                        [System.StringComparison]::OrdinalIgnoreCase)) {
                    throw 'the scheduled swap would escape the scenario scratch'
                }
            }
            Move-Item -LiteralPath $shim.Swap -Destination $shim.Parked
            New-Item -ItemType Junction -Path $shim.Swap -Target $shim.Into -ErrorAction Stop | Out-Null
            $shim.Done = $true
        }
        return $items
    }

    $outsideBefore = Get-TreeItemCount $outside
    $thrown = $null
    try {
        Remove-Tree -Path $run
    } catch {
        $thrown = $_.Exception.Message
    }
    $outsideAfter = Get-TreeItemCount $outside
    Assert-Repair ($script:SwapShim.Done) 'the scheduled swap never ran; this scenario would prove nothing'
    Assert-Repair (Test-Path -LiteralPath $outsideFile) (
        'Q2: Remove-Tree deleted the external file that shares a name with an entry it ' +
        "enumerated before the swap ('$outsideFile' is gone; the external tree went from " +
        "$outsideBefore to $outsideAfter item(s)); Remove-Tree said: $thrown")
    Assert-Repair ((Get-FileSha $outsideFile) -eq $outsideSha) 'Q2: the external file changed bytes'
    Assert-Repair (Test-Path -LiteralPath $sentinel) 'Q2: the external sentinel was deleted'
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'Q2: the external sentinel changed bytes'
    Assert-Repair ($null -ne $thrown -and $thrown -match 'reparse point|retain') (
        "Q2: Remove-Tree left the external tree alone but never said why it stopped: $thrown")
    Assert-Repair (Test-Path -LiteralPath (Join-Path $script:SwapShim.Parked 'shared.txt')) `
        'Q2: the run-owned file that was moved aside did not survive'
    Assert-Repair (-not (Test-Path -LiteralPath (Join-Path $sibling 'mine.txt'))) `
        'positive control failed: the run-owned sibling the walk could prove was not cleaned'
    Assert-Repair (Test-Path -LiteralPath $run) 'the retained root vanished, so the refusal leaves nothing to inspect'
}

# ---------------------------------------------------------------------------
# review N5 - the scenario filter is a decision, not a silent all-SKIP pass
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'filter-unknown-only-refuses-before-executing' {
    $out = Join-Path (New-ScenarioScratch 'filter-unknown') 'out'
    $output = & pwsh -NoProfile -File $PSCommandPath -Repo $Repo -OutDir $out `
        -Only 'no-such-scenario-name' 2>&1 | ForEach-Object { $_.ToString() }
    $exit = $LASTEXITCODE
    $joined = $output -join "`n"
    Set-Content -LiteralPath (Join-Path $OutDir 'filter-unknown-only-child-output.txt') `
        -Value $joined -Encoding utf8
    Assert-Repair ($exit -ne 0) "an unknown -Only name still produced exit code $exit"
    Assert-Repair ($joined -match 'no such scenario') `
        "the refusal did not name the filter rule: $joined"
    Assert-Repair ($joined -notmatch '(?m)^SKIP ') `
        "the sweep walked scenarios and only complained afterwards: $joined"
    Assert-Repair (@(Get-ChildItem -LiteralPath $out -Filter '*.log' -ErrorAction SilentlyContinue).Count -eq 0) `
        'scenarios executed even though the filter named none of them'
}

Invoke-RepairScenario 'filter-counters-reported-for-a-valid-filter' {
    $out = Join-Path (New-ScenarioScratch 'filter-counters') 'out'
    $output = & pwsh -NoProfile -File $PSCommandPath -Repo $Repo -OutDir $out `
        -Only 'static-r3-no-ambiguous-label' 2>&1 | ForEach-Object { $_.ToString() }
    $exit = $LASTEXITCODE
    $joined = $output -join "`n"
    Set-Content -LiteralPath (Join-Path $OutDir 'filter-counters-child-output.txt') `
        -Value $joined -Encoding utf8
    Assert-Repair ($exit -eq 0) "a valid filter run failed with exit code $exit`: $joined"
    Assert-Repair ($joined -match 'executed=1 skipped=\d+ failed=0 blocked=\d+ declared=\d+') `
        "the sweep did not report executed/skipped/failed counts: $joined"
    Assert-Repair (@(Get-ChildItem -LiteralPath $out -Filter '*.log').Count -eq 1) `
        'exactly one scenario log was expected for a one-scenario filter run'
}

# ---------------------------------------------------------------------------
# rereview Q3 - the capability probe scratch belongs to one run, not to the host
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'helper-scratch-never-reuses-previous-run-root' {
    <#  Q3: the probe used a fixed global base and a fixed scenario name, and
        deleted whatever it found there. A second run, or a run overlapping with
        another one, therefore erased another run's fixtures and evidence. The
        decoy below is a tree this scenario creates *first*, under the exact path
        the old helper used, with a sentinel: the helper may neither hand it back
        nor clean it. #>
    $name = 'q3decoy-' + $script:RunId
    $legacyBase = Join-Path ([System.IO.Path]::GetTempPath()) 'wave5b-reparse-probe'
    $decoy = Join-Path $legacyBase ('scenario-' + $name)
    if (Test-Path -LiteralPath $decoy) {
        throw "the legacy probe path '$decoy' already exists from an earlier run; clean it by hand so this scenario owns its own decoy"
    }
    New-Item -ItemType Directory -Path $decoy | Out-Null
    $script:RunOwned.Add((Get-CanonicalPath $decoy)) | Out-Null
    $sentinel = Join-Path $decoy 'preexisting-sentinel.txt'
    [System.IO.File]::WriteAllText($sentinel, 'belongs-to-the-earlier-run')
    $sentinelSha = Get-FileSha $sentinel

    $returned = Get-ReparseCapableScratch $name
    Assert-Repair ($null -ne $returned -and -not [string]::IsNullOrWhiteSpace($returned)) `
        'Get-ReparseCapableScratch returned no path'
    $script:RunOwned.Add((Get-CanonicalPath $returned)) | Out-Null
    Assert-Repair ((Get-CanonicalPath $returned).ToLowerInvariant() -ne (Get-CanonicalPath $decoy).ToLowerInvariant()) (
        "Q3: the helper handed back the scratch of an earlier run ('$returned') instead of creating its own; " +
        "the earlier run's sentinel '$sentinel' is now " +
        $(if (Test-Path -LiteralPath $sentinel) { 'intact' } else { 'deleted' }))
    Assert-Repair (Test-Path -LiteralPath $sentinel) `
        'Q3: the helper deleted a scenario directory a previous run had left behind'
    Assert-Repair ((Get-FileSha $sentinel) -eq $sentinelSha) 'Q3: the earlier run sentinel changed bytes'
    Assert-Repair (-not (Get-CanonicalPath $returned).ToLowerInvariant().StartsWith(
            (Get-CanonicalPath $legacyBase).ToLowerInvariant() + '\')) `
        "Q3: the helper still puts probe scratch under the shared fixed base '$legacyBase', so two runs can meet there"
    Assert-Repair ((Get-CanonicalPath $returned).ToLowerInvariant().StartsWith(
            (Get-CanonicalPath $OutDir).ToLowerInvariant() + '\') -or
        (Get-CanonicalPath $returned).ToLowerInvariant().Contains($script:RunId)) (
        "Q3: the probe scratch '$returned' is not namespaced to this run ($script:RunId)")
    Assert-Repair (Test-Path -LiteralPath $returned) 'the helper returned a directory that does not exist'
}

Invoke-RepairScenario 'helper-scratch-repeat-and-concurrent-runs-are-disjoint' {
    <#  Q3 asked for three things: a same-name directory left by an earlier run,
        a repeated run, and two runs in parallel. The first is the decoy
        scenario; this one runs the real helper in separate child processes -
        twice in a row and then twice at the same time - each of which plants a
        sentinel in the scratch it was given, holds it for a moment, and refuses
        to exit if anything touched it. Two runs that shared a root could not
        both report that. #>
    $childScript = Join-Path $OutDir 'helper-scratch-child-probe.ps1'
    $childLines = @(
        'param(',
        '    [Parameter(Mandatory)][string]$Repair,',
        '    [Parameter(Mandatory)][string]$Driver,',
        '    [Parameter(Mandatory)][string]$ChildOutDir,',
        '    [Parameter(Mandatory)][string]$ScenarioName,',
        '    [int]$HoldMilliseconds = 1200',
        ')',
        '$ErrorActionPreference = ''Stop''',
        '$OutDir = $ChildOutDir',
        'foreach ($sourceFile in @($Driver, $Repair)) {',
        '    $tokens = $null',
        '    $errors = $null',
        '    $ast = [System.Management.Automation.Language.Parser]::ParseFile(',
        '        $sourceFile, [ref]$tokens, [ref]$errors)',
        '    if ($errors.Count -gt 0) { Write-Output "RESULT=PARSE-FAILED $sourceFile"; exit 8 }',
        '    $definitions = @($ast.FindAll(',
        '        { param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] },',
        '        $true))',
        '    # Top level only: a scenario may define a scoped shim over a real cmdlet,',
        '    # and importing that shim here would replace the cmdlet for the whole child.',
        '    $text = ($definitions | Where-Object {',
        '        $blocks = 0',
        '        $cursor = $_.Parent',
        '        while ($null -ne $cursor) {',
        '            if ($cursor -is [System.Management.Automation.Language.ScriptBlockAst]) { $blocks++ }',
        '            $cursor = $cursor.Parent',
        '        }',
        '        $blocks -le 1',
        '    } | ForEach-Object { $_.Extent.Text }) -join "`r`n"',
        '    Invoke-Expression $text',
        '}',
        '$root = Get-ReparseCapableScratch $ScenarioName',
        '$root = (Resolve-Path -LiteralPath $root).Path',
        'Write-Output "RUNID=$script:RunId"',
        'Write-Output "ROOT=$root"',
        'Write-Output "BASE=$((Split-Path -Parent $root))"',
        '$sentinel = Join-Path $root ''child-own-sentinel.txt''',
        '[System.IO.File]::WriteAllText($sentinel, ''child run owns this'')',
        '$sha = (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash',
        'Start-Sleep -Milliseconds $HoldMilliseconds',
        'if (-not (Test-Path -LiteralPath $sentinel)) { Write-Output ''SENTINEL_ALIVE=false''; Write-Output ''RESULT=SENTINEL-DELETED-BY-SOMEONE-ELSE''; exit 3 }',
        'if ((Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash -ne $sha) { Write-Output ''RESULT=SENTINEL-BYTES-CHANGED''; exit 4 }',
        'Write-Output ''SENTINEL_ALIVE=true''',
        'if (Get-Command -Name Remove-RunOwnedScratch -CommandType Function -ErrorAction SilentlyContinue) {',
        '    Remove-RunOwnedScratch',
        '    if (Test-Path -LiteralPath $root) { Write-Output ''RESULT=OWNED-SCRATCH-SURVIVED''; exit 5 }',
        '    Write-Output ''OWNED_CLEANUP=gone''',
        '} else {',
        '    Write-Output ''OWNED_CLEANUP=absent''',
        '}',
        'Write-Output ''RESULT=OK''',
        'exit 0'
    )
    [System.IO.File]::WriteAllText($childScript, ($childLines -join "`r`n"),
        [System.Text.UTF8Encoding]::new($false))

    function Start-ScratchChild {
        <#  Starts one child run of the real helper and does not wait, so two of
           them can genuinely overlap. #>
        param([string]$Tag, [string]$ScenarioName, [int]$HoldMilliseconds)
        $childOut = Join-Path $OutDir "helper-scratch-$Tag-outdir"
        New-Item -ItemType Directory -Force -Path $childOut | Out-Null
        $stdout = Join-Path $OutDir "helper-scratch-$Tag.log"
        $process = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow `
            -RedirectStandardOutput $stdout `
            -ArgumentList @('-NoProfile', '-File', $childScript,
                '-Repair', $PSCommandPath, '-Driver', $driver,
                '-ChildOutDir', $childOut, '-ScenarioName', $ScenarioName,
                '-HoldMilliseconds', $HoldMilliseconds)
        return [pscustomobject]@{ Tag = $Tag; Process = $process; Stdout = $stdout }
    }

    function Complete-ScratchChild {
        <#  Waits for one started child and parses its key=value report. #>
        param($Running)
        $Running.Process.WaitForExit()
        $text = ''
        if (Test-Path -LiteralPath $Running.Stdout) {
            $text = [System.IO.File]::ReadAllText($Running.Stdout)
        }
        $fields = @{}
        foreach ($line in ($text -split "`r?`n")) {
            if ($line -match '^([A-Z_]+)=(.*)$') { $fields[$Matches[1]] = $Matches[2] }
        }
        return [pscustomobject]@{
            Tag    = $Running.Tag
            Code   = $Running.Process.ExitCode
            Fields = $fields
            Out    = $text
        }
    }

    $report = [System.Collections.Generic.List[string]]::new()
    $sequential = @()
    foreach ($tag in @('first', 'second')) {
        $sequential += Complete-ScratchChild (Start-ScratchChild -Tag $tag `
                -ScenarioName 'shared-name' -HoldMilliseconds 300)
        $last = $sequential[-1]
        $report.Add("sequential $($last.Tag): exit=$($last.Code) root=$($last.Fields['ROOT'])")
    }
    foreach ($child in $sequential) {
        Assert-Repair ($child.Code -eq 0) (
            "Q3: child run '$($child.Tag)' of the probe helper exited $($child.Code):`n$($child.Out)")
        Assert-Repair ($child.Fields['RESULT'] -eq 'OK') "Q3: child '$($child.Tag)' reported $($child.Fields['RESULT'])"
    }
    Assert-Repair ($sequential[0].Fields['ROOT'] -ne $sequential[1].Fields['ROOT']) (
        "Q3: two runs of the same scenario name share '$($sequential[0].Fields['ROOT'])'")
    Assert-Repair ($sequential[0].Fields['BASE'] -ne $sequential[1].Fields['BASE']) (
        "Q3: two runs share the probe base '$($sequential[0].Fields['BASE'])'")
    foreach ($child in @($sequential)) {
        # A child that inherited this run's identity would inherit its scratch too.
        Assert-Repair ($child.Fields['RUNID'] -ne $script:RunId) `
            'Q3: a child run reports this run id, so it is not a separate run'
    }
    Assert-Repair ($sequential[0].Fields['RUNID'] -ne $sequential[1].Fields['RUNID']) `
        'Q3: two child runs report the same run id'

    $pair = @(
        (Start-ScratchChild -Tag 'parallel-left' -ScenarioName 'shared-name' -HoldMilliseconds 2500),
        (Start-ScratchChild -Tag 'parallel-right' -ScenarioName 'shared-name' -HoldMilliseconds 2500)
    )
    $concurrent = @($pair | ForEach-Object { Complete-ScratchChild $_ })
    foreach ($child in $concurrent) {
        $report.Add("concurrent $($child.Tag): exit=$($child.Code) root=$($child.Fields['ROOT'])")
        Assert-Repair ($child.Code -eq 0) "Q3: concurrent child '$($child.Tag)' exited $($child.Code):`n$($child.Out)"
        Assert-Repair ($child.Fields['SENTINEL_ALIVE'] -eq 'true') `
            'Q3: a concurrent run touched the other run sentinel'
        Assert-Repair ($child.Fields['OWNED_CLEANUP'] -eq 'gone') (
            "Q3: child '$($child.Tag)' left its own scratch behind or had nothing to clean " +
            "($($child.Fields['OWNED_CLEANUP']))")
        Assert-Repair (-not (Test-Path -LiteralPath $child.Fields['BASE'])) `
            "Q3: the child probe base '$($child.Fields['BASE'])' outlived the child"
    }
    Assert-Repair ($concurrent[0].Fields['ROOT'] -ne $concurrent[1].Fields['ROOT']) (
        "Q3: two concurrent runs were handed the same scratch '$($concurrent[0].Fields['ROOT'])'")
    foreach ($child in $sequential) {
        Assert-Repair (-not (Test-Path -LiteralPath $child.Fields['BASE'])) `
            "Q3: the child probe base '$($child.Fields['BASE'])' outlived the child"
    }
    Set-Content -LiteralPath (Join-Path $OutDir 'helper-scratch-isolation-report.txt') `
        -Value ($report -join "`r`n") -Encoding utf8
}

# ---------------------------------------------------------------------------
# real-entry layer - the actual driver, preflight and lifecycle
# ---------------------------------------------------------------------------
Invoke-RepairScenario 'entry-r3-explicit-evidencedir-reaches-evidence-rule' {
    # The default scratch depth is the budget the scratch clone of the
    # history cases needs (no core.longpaths on this host), so the entry
    # scenarios exercise the driver's documented default location.
    $parent = Join-Path $Repo 'target/tmp/plan0014-wave5b'
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $before = @(Get-ChildItem -LiteralPath $parent -Directory -Filter 'run-*' -ErrorAction SilentlyContinue | ForEach-Object Name)
    $evidence = Join-Path (New-ScenarioScratch 'entry-external-ev') 'evidence'
    New-Item -ItemType Directory -Force -Path $evidence | Out-Null
    Set-Content -LiteralPath (Join-Path $evidence 'sentinel.txt') -Value 'old' -Encoding utf8
    $output = & pwsh -NoProfile -File $driver -Repo $Repo -Scratch $parent -EvidenceDir $evidence 2>&1 |
        ForEach-Object { $_.ToString() }
    $exit = $LASTEXITCODE
    $joined = $output -join "`n"
    Set-Content -LiteralPath (Join-Path $OutDir 'entry-r3-child-output.txt') -Value $joined -Encoding utf8
    Assert-Repair ($exit -ne 0) 'driver accepted an existing non-empty evidence directory'
    Assert-Repair ($joined -notmatch 'ambiguous') `
        "R3 regression: driver still fails on parameter binding; output: $joined"
    Assert-Repair ($joined -notmatch ([char]0x6B67 + [char]0x4E49)) `
        'R3 regression: driver still fails on parameter binding (localized message).'
    Assert-Repair ($joined -match 'already holds') `
        "driver did not reach the evidence-protection rule (preflight or binding failure?): $joined"
    Assert-Repair ((Get-Content -LiteralPath (Join-Path $evidence 'sentinel.txt') -Raw) -match 'old') `
        'the existing evidence sentinel was touched'
    $leftover = @(Get-ChildItem -LiteralPath $parent -Directory -Filter 'run-*' -ErrorAction SilentlyContinue |
        Where-Object { $before -notcontains $_.Name })
    Assert-Repair ($leftover.Count -eq 0) `
        "R4 regression: external-evidence finally cleanup left $($leftover.Count) run root(s) in '$parent'"
}

Invoke-RepairScenario 'entry-r4-internal-default-lifecycle' {
    $parent = Join-Path $Repo 'target/tmp/plan0014-wave5b'
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $before = @(Get-ChildItem -LiteralPath $parent -Directory -Filter 'run-*' -ErrorAction SilentlyContinue | ForEach-Object Name)
    $output = & pwsh -NoProfile -File $driver -Repo $Repo 2>&1 |
        ForEach-Object { $_.ToString() }
    $exit = $LASTEXITCODE
    $joined = $output -join "`n"
    Set-Content -LiteralPath (Join-Path $OutDir 'entry-r4-child-output.txt') -Value $joined -Encoding utf8
    Assert-Repair ($exit -eq 0) "default internal-mode driver run exited $exit; see entry-r4-child-output.txt"
    Assert-Repair ($joined -match 'summary total cases=\d+ mismatched=0') `
        'shared cases did not all match; see entry-r4-child-output.txt'
    $runs = @(Get-ChildItem -LiteralPath $parent -Directory -Filter 'run-*' -ErrorAction SilentlyContinue |
        Where-Object { $before -notcontains $_.Name })
    Assert-Repair ($runs.Count -eq 1) `
        "internal mode must retain exactly one new evidence-carrying run root, found $($runs.Count)"
    $inner = @(Get-ChildItem -LiteralPath $runs[0].FullName -Force -ErrorAction SilentlyContinue)
    Assert-Repair ($inner.Count -eq 1 -and $inner[0].Name -eq 'evidence') `
        "retained run root holds more than the evidence subtree: $(($inner | ForEach-Object Name) -join ', ')"
    Assert-Repair (Test-Path -LiteralPath (Join-Path $runs[0].FullName 'evidence/catalog-reconciliation.csv')) `
        'internal evidence reconciliation table missing after cleanup'
    Assert-Repair ($joined -match 'internal evidence retained') `
        'retention was not logged explicitly'
    if ($joined -match 'BLOCKED \(could not run on this platform\)') {
        Write-RepairLog 'NOTE: driver reported platform-BLOCKED cases (recorded, not failures)'
    }
}

# ---------------------------------------------------------------------------
# real-wrapper layer - R7 argument arrays, B7 propagation, B8 protection
# ---------------------------------------------------------------------------
function New-RtkStub([string]$StubDir) {
    New-Item -ItemType Directory -Force -Path $StubDir | Out-Null
    $stubPath = Join-Path $StubDir 'rtk.ps1'
    $stubLines = @(
        '$dir = $env:WAVE5B_REPAIR_RTK_STUB_DIR'
        'if ([string]::IsNullOrWhiteSpace($dir)) { Write-Error (''stub without capture dir''); exit 99 }'
        '$serial = @(Get-ChildItem -LiteralPath $dir -Filter (''call-*.txt'') -ErrorAction SilentlyContinue).Count'
        '[System.IO.File]::WriteAllLines('
        '    (Join-Path $dir (''call-{0:D3}.txt'' -f $serial)),'
        '    [string[]]$args)'
        'switch ($env:WAVE5B_REPAIR_RTK_STUB_MODE) {'
        '    ''always-7'' { exit 7 }'
        '    ''unknown-check-1'' {'
        '        if ($args -contains ''everything'') { exit 1 }'
        '        exit 0'
        '    }'
        '    default { exit 0 }'
        '}'
    )
    [System.IO.File]::WriteAllText($stubPath, ($stubLines -join "`r`n"), [System.Text.UTF8Encoding]::new($false))
    return $stubPath
}

function Invoke-Wrapper {
    <#  Runs the real wrapper in a child pwsh whose PATH front-loads the stub
        rtk; returns exit code plus captured argument arrays. #>
    param([string]$Phase, [string]$EvidenceDir, [string]$StubDir, [string]$Mode)
    $capture = Join-Path $StubDir 'capture'
    New-Item -ItemType Directory -Force -Path $capture | Out-Null
    $previousPath = $env:PATH
    $env:PATH = "$StubDir;$env:PATH"
    $env:WAVE5B_REPAIR_RTK_STUB_DIR = $capture
    $env:WAVE5B_REPAIR_RTK_STUB_MODE = $Mode
    try {
        $out = & pwsh -NoProfile -File $wrapper -Phase $Phase -Repo $Repo -EvidenceDir $EvidenceDir 2>&1 |
            ForEach-Object { $_.ToString() }
        $code = $LASTEXITCODE
    } finally {
        $env:PATH = $previousPath
        Remove-Item Env:WAVE5B_REPAIR_RTK_STUB_DIR -ErrorAction SilentlyContinue
        Remove-Item Env:WAVE5B_REPAIR_RTK_STUB_MODE -ErrorAction SilentlyContinue
    }
    $captured = [System.Collections.Generic.List[object]]::new()
    foreach ($file in (Get-ChildItem -LiteralPath $capture -Filter 'call-*.txt' | Sort-Object Name)) {
        $captured.Add(@(Get-Content -LiteralPath $file.FullName))
    }
    $calls = $captured.ToArray()
    return [pscustomobject]@{
        Code    = $code
        Out     = ($out -join "`n")
        Calls   = $calls
        Capture = $capture
    }
}

Invoke-RepairScenario 'wrapper-r7-capture-unique-evidencedir-argument' {
    $stubDir = Join-Path (New-ScenarioScratch 'r7') 'stub'
    [void](New-RtkStub $stubDir)
    $spaced = Join-Path $OutDir 'r7 evidence dir with spaces'
    New-Item -ItemType Directory -Force -Path $spaced | Out-Null
    $result = Invoke-Wrapper -Phase 'post' -EvidenceDir $spaced -StubDir $stubDir -Mode 'always-0'
    Set-Content -LiteralPath (Join-Path $OutDir 'wrapper-r7-child-output.txt') -Value $result.Out -Encoding utf8
    Assert-Repair ($result.Code -eq 0) "wrapper post exited $($result.Code) with every stub command succeeding; see wrapper-r7-child-output.txt"
    $driverCall = $result.Calls | Where-Object { ($_ -join ' ') -match 'test-plan0014-wave5b\.ps1' } | Select-Object -First 1
    Assert-Repair ($null -ne $driverCall) 'wrapper never invoked the real driver through rtk'
    $index = [Array]::IndexOf([string[]]$driverCall, '-EvidenceDir')
    Assert-Repair ($index -ge 0) 'the driver invocation lost its -EvidenceDir parameter'
    Assert-Repair ($driverCall.Count -eq ($index + 2)) `
        "argument array has $($driverCall.Count) elements but -EvidenceDir sits at index $index - a stray positional follows"
    $expectedCase = Join-Path $spaced 'wave5b-cases'
    Assert-Repair ($driverCall[$index + 1] -eq $expectedCase) `
        "EvidenceDir argument is '$($driverCall[$index + 1])' but the single joined path '$expectedCase' is required"
    Assert-Repair ($driverCall -notcontains '/wave5b-cases') `
        "the R7 split is back: '/wave5b-cases' appears as an extra positional element"
}

Invoke-RepairScenario 'wrapper-b7-known-negative-only-passes' {
    $stubDir = Join-Path (New-ScenarioScratch 'b7pass') 'stub'
    [void](New-RtkStub $stubDir)
    $evidence = Join-Path (New-ScenarioScratch 'b7pass-ev') 'gates-evidence'
    $result = Invoke-Wrapper -Phase 'fast' -EvidenceDir $evidence -StubDir $stubDir -Mode 'unknown-check-1'
    Assert-Repair ($result.Code -eq 0) `
        "wrapper failed although only the planned unknown-check negative was supposed to exit 1; output: $($result.Out)"
    $negativeLog = Join-Path $evidence 'gates/25-catalog-check-unknown-check.log'
    $unknown = $result.Calls | Where-Object { $_ -contains 'everything' } | Select-Object -First 1
    Assert-Repair ($null -ne $unknown) 'stub never saw the unknown-check command line'
    Assert-Repair ((Get-Content -LiteralPath $negativeLog -Raw) -match '# exit-code: 1') `
        'the planned negative did not record exit 1 in its gate log'
}

Invoke-RepairScenario 'wrapper-b7-stub-exit-7-propagates' {
    $stubDir = Join-Path (New-ScenarioScratch 'b7fail') 'stub'
    [void](New-RtkStub $stubDir)
    $evidence = Join-Path (New-ScenarioScratch 'b7fail-ev') 'gates-evidence'
    $result = Invoke-Wrapper -Phase 'fast' -EvidenceDir $evidence -StubDir $stubDir -Mode 'always-7'
    Assert-Repair ($result.Code -ne 0) 'wrapper reported success even though every stubbed command exited 7'
    $normalized = Get-NormalizedOutput $result.Out
    Assert-Repair ($normalized -match 'FAILED GATES') 'wrapper did not summarize failed gates'
    # 20-25 plus 30 are stubbed in the fast phase: seven gates expected a 0 or
    # a 1 and all got 7 - including the planned negative, which must NOT be
    # waved through just because the exit code was non-zero.
    Assert-Repair ($normalized -match '25-catalog-check-unknown-check: expected 1, got 7') `
        "the expected-1 gate was not recorded as a mismatch: $($result.Out)"
    $failCount = [regex]::Matches($normalized, 'expected \d+, got 7').Count
    Assert-Repair ($failCount -eq 7) "expected seven mismatches, recorded $failCount"
}

Invoke-RepairScenario 'wrapper-b8-evidence-not-overwritten' {
    $stubDir = Join-Path (New-ScenarioScratch 'b8') 'stub'
    [void](New-RtkStub $stubDir)
    $evidence = Join-Path (New-ScenarioScratch 'b8-ev') 'gates-evidence'
    $first = Invoke-Wrapper -Phase 'post' -EvidenceDir $evidence -StubDir $stubDir -Mode 'always-0'
    Assert-Repair ($first.Code -eq 0) "first wrapper post run failed: $($first.Out)"
    $logFile = Join-Path $evidence 'gates/37-test-workspace.log'
    Assert-Repair (Test-Path -LiteralPath $logFile) 'first run did not write its gate log'
    $before = Get-FileSha $logFile
    $second = Invoke-Wrapper -Phase 'post' -EvidenceDir $evidence -StubDir $stubDir -Mode 'always-0'
    Assert-Repair ($second.Code -ne 0) 'wrapper accepted a second run into the same evidence directory'
    Assert-Repair ((Get-NormalizedOutput $second.Out) -match 'already exists and is not empty') `
        "refusal did not name the evidence rule: $($second.Out)"
    Assert-Repair ((Get-FileSha $logFile) -eq $before) 'an archived gate log changed between runs'
}

# ---------------------------------------------------------------------------
# summary
# ---------------------------------------------------------------------------
$unrun = @($script:ScenarioManifest | Where-Object { $script:ScenarioNames -notcontains $_ })
Write-RepairLog ''
Write-RepairLog (
    "repair-regressions: executed=$($script:Executed) skipped=$($script:Skipped) " +
    "failed=$($script:Failures.Count) blocked=$($script:Blocked.Count) " +
    "declared=$($script:ScenarioManifest.Count); logs in $OutDir")
if ($script:Failures.Count -gt 0) {
    foreach ($failure in $script:Failures) {
        Write-RepairLog "  FAILED: $failure"
    }
    throw "$($script:Failures.Count) repair regression scenario(s) failed (raw logs in '$OutDir')"
}
# A sweep that ran nothing has verified nothing, whatever it printed before.
if ($script:Executed -eq 0) {
    throw "repair-regressions: no scenario executed (-Only '$Only' matched nothing to run); " +
        "refusing to report success for an empty sweep"
}
if ($unrun.Count -gt 0) {
    throw "repair-regressions: $($unrun.Count) declared scenario(s) were never reached: $($unrun -join ', ')"
}
foreach ($blockage in $script:Blocked) {
    Write-RepairLog "  NOT VERIFIED HERE: $blockage"
}
if ($script:Blocked.Count -gt 0) {
    Write-RepairLog (
        "Wave 5B-S1 repair regressions: $($script:Executed) executed scenarios matched " +
        "their expectation; $($script:Blocked.Count) scenario(s) are BLOCKED and untested on this host.")
} else {
    Write-RepairLog (
        "Wave 5B-S1 repair regressions: all $($script:Executed) executed scenarios " +
        'matched their expectation.')
}