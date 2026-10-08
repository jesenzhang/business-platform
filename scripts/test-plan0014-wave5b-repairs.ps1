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

function Get-ReparseCapableScratch([string]$Name) {
    <#  A fresh scenario directory on a volume that can actually host a reparse
        point, or a named BLOCKED refusal when no writable volume can. The
        capability is proved by creating a junction, never assumed: exFAT
        volumes and network shares accept the directory name and reject the
        reparse point, and a fixture that silently degraded into a plain
        directory would "pass" without testing anything (review N2). #>
    foreach ($probe in @(
            (Join-Path ([System.IO.Path]::GetTempPath()) 'wave5b-reparse-probe'),
            (Join-Path $OutDir 'reparse-probe'))) {
        $target = Join-Path $probe 'junction-target'
        $link = Join-Path $probe 'junction-link'
        try {
            New-Item -ItemType Directory -Force -Path $target | Out-Null
            if (Test-Path -LiteralPath $link) {
                (Get-Item -LiteralPath $link -Force).Delete()
            }
            New-Item -ItemType Junction -Path $link -Target $target -ErrorAction Stop | Out-Null
            if (-not ((Get-Item -LiteralPath $link -Force).Attributes -band
                    [System.IO.FileAttributes]::ReparsePoint)) {
                throw 'the created item does not carry the reparse-point attribute'
            }
            (Get-Item -LiteralPath $link -Force).Delete()
            $scratch = Join-Path $probe ('scenario-' + $Name)
            if (Test-Path -LiteralPath $scratch) {
                Remove-Item -LiteralPath $scratch -Recurse -Force
            }
            New-Item -ItemType Directory -Force -Path $scratch | Out-Null
            return $scratch
        } catch {
            continue
        }
    }
    throw 'BLOCKED: no writable volume on this host can host a junction, so the ' +
        'reparse-point cleanup rules cannot be exercised here; they are untested, not passing'
}

function Get-FileSha([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
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

    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidaaa') `
        -Parent $parent -Kind 'main'
    New-Item -ItemType File -Force -Path (Join-Path $runRoot 'work.txt') | Out-Null
    $fallbackParent = [System.IO.Path]::GetTempPath()
    $fallback = Register-RunRoot -Root (Join-Path $fallbackParent ('wave5b-repair-fixture-fallback-' + [guid]::NewGuid().ToString('N'))) `
        -Parent $fallbackParent -Kind 'fallback'
    New-Item -ItemType Directory -Force -Path $fallback | Out-Null
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

    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidlink') `
        -Parent $parent -Kind 'work'
    New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
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

    $runRoot = Register-RunRoot -Root (Join-Path $parent 'run-guidchild') `
        -Parent $parent -Kind 'work'
    New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
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