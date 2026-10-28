param(
    [Parameter(Mandatory)][string]$Repair,
    [Parameter(Mandatory)][string]$Driver,
    [Parameter(Mandatory)][string]$ChildOutDir,
    [Parameter(Mandatory)][string]$ScenarioName,
    [int]$HoldMilliseconds = 1200
)
$ErrorActionPreference = 'Stop'
$OutDir = $ChildOutDir
$script:RunId = [guid]::NewGuid().ToString('N')
$script:RunOwned = [System.Collections.Generic.List[string]]::new()
$script:RunLog = [System.Collections.Generic.List[string]]::new()
$script:RunRoots = [System.Collections.Generic.List[object]]::new()
$script:ProtectedPaths = [System.Collections.Generic.List[string]]::new()
$script:InternalEvidence = $false
$script:EvidenceInsideRun = $null
$KeepScratch = $false
foreach ($sourceFile in @($Driver, $Repair)) {
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile(
        $sourceFile, [ref]$tokens, [ref]$errors)
    if ($errors.Count -gt 0) { Write-Output "RESULT=PARSE-FAILED $sourceFile"; exit 8 }
    $text = ($ast.FindAll(
        { param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] },
        $true) | ForEach-Object { $_.Extent.Text }) -join "`r`n"
    Invoke-Expression $text
}
$root = Get-ReparseCapableScratch $ScenarioName
$root = (Resolve-Path -LiteralPath $root).Path
Write-Output "ROOT=$root"
Write-Output "BASE=$((Split-Path -Parent $root))"
$sentinel = Join-Path $root 'child-own-sentinel.txt'
[System.IO.File]::WriteAllText($sentinel, 'child run owns this')
$sha = (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash
Start-Sleep -Milliseconds $HoldMilliseconds
if (-not (Test-Path -LiteralPath $sentinel)) { Write-Output 'SENTINEL_ALIVE=false'; Write-Output 'RESULT=SENTINEL-DELETED-BY-SOMEONE-ELSE'; exit 3 }
if ((Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash -ne $sha) { Write-Output 'RESULT=SENTINEL-BYTES-CHANGED'; exit 4 }
Write-Output 'SENTINEL_ALIVE=true'
if (Get-Command -Name Remove-RunOwnedScratch -CommandType Function -ErrorAction SilentlyContinue) {
    Remove-RunOwnedScratch
    if (Test-Path -LiteralPath $root) { Write-Output 'RESULT=OWNED-SCRATCH-SURVIVED'; exit 5 }
    Write-Output 'OWNED_CLEANUP=gone'
} else {
    Write-Output 'OWNED_CLEANUP=absent'
}
Write-Output 'RESULT=OK'
exit 0