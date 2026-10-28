<#
  Review N2 probe, re-run against the repaired driver: a registered run root is
  replaced by a junction to an out-of-root tree. Reports the numbers the
  independent review measured: sentinel_survives, root_survives, messages.
#>
param([string]$Repo = 'F:\Workspace\business-platform')
$ErrorActionPreference = 'Stop'
$driver = Join-Path $Repo 'scripts/test-plan0014-wave5b.ps1'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($driver, [ref]$tokens, [ref]$errors)
if ($errors.Count -gt 0) { throw "driver does not parse: $($errors[0])" }
$wanted = @('Get-CanonicalPath','Test-ReparsePoint','Get-UnsafeLink','Assert-PathChainIsReal','Remove-Tree','Register-RunRoot','Remove-RunScratch','Write-Log')
$names = [System.Collections.Generic.List[string]]::new()
$text = ($ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true) |
    ForEach-Object { if ($wanted -contains $_.Name) { $names.Add($_.Name) | Out-Null; $_.Extent.Text } }) -join "`r`n"
$missing = @($wanted | Where-Object { $names -notcontains $_ })
if ($missing.Count -gt 0) { throw "missing production functions: $($missing -join ', ')" }
$script:RunLog = [System.Collections.Generic.List[string]]::new()
$script:RunRoots = [System.Collections.Generic.List[object]]::new()
$script:ProtectedPaths = [System.Collections.Generic.List[string]]::new()
$script:InternalEvidence = $false
$script:EvidenceInsideRun = $null
$KeepScratch = $false
Invoke-Expression $text

$base = Join-Path ([System.IO.Path]::GetTempPath()) 'wave5b-n2-probe'
if (Test-Path -LiteralPath $base) { Remove-Item -LiteralPath $base -Recurse -Force }
$parent = Join-Path $base 'scratch'
$outside = Join-Path $base 'outside'
New-Item -ItemType Directory -Force -Path $parent | Out-Null
New-Item -ItemType Directory -Force -Path $outside | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $outside 'nested') | Out-Null
$sentinel = Join-Path $outside 'sentinel.txt'
$deep = Join-Path $outside 'nested\deep.txt'
Set-Content -LiteralPath $sentinel -Value 'sentinel-bytes' -Encoding utf8NoBOM
Set-Content -LiteralPath $deep -Value 'deep-bytes' -Encoding utf8NoBOM
$root = Register-RunRoot -Root (Join-Path $parent 'run-probe') -Parent $parent -Kind 'work'
New-Item -ItemType Directory -Force -Path $root | Out-Null
New-Item -ItemType File -Force -Path (Join-Path $root 'work.txt') | Out-Null
$script:ProtectedPaths.Add((Get-CanonicalPath $parent))
# the replacement the review performed
Remove-Tree -Path $root
New-Item -ItemType Junction -Path $root -Target $outside | Out-Null
$warnings = @(Remove-RunScratch 3>&1 | ForEach-Object { $_.ToString() })
'sentinel_survives={0}' -f (Test-Path -LiteralPath $sentinel)
'sentinel_bytes={0}' -f $(if (Test-Path -LiteralPath $sentinel) { [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes($sentinel)) } else { 'GONE' })
'nested_survives={0}' -f (Test-Path -LiteralPath $deep)
'root_survives={0}' -f (Test-Path -LiteralPath $root)
'messages={0}' -f ((($script:RunLog + $warnings) | Where-Object { $_ -match 'reparse|unlinked|refus' }) -join ' | ')
'note=the junction lives under the temporary directory because the repository volume is exFAT and cannot host a reparse point at all'