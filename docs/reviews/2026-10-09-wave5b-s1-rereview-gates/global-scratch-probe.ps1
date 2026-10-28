$ErrorActionPreference = 'Stop'
$source = 'F:/Workspace/business-platform/target/w5b-review-1009/scripts/test-plan0014-wave5b-repairs.ps1'
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
$fn=$ast.Find({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-ReparseCapableScratch'},$true)
Invoke-Expression $fn.Extent.Text
$OutDir='F:/Workspace/business-platform/docs/reviews/2026-10-09-wave5b-s1-rereview-gates/global-scratch-fallback'
$name='independent-'+[guid]::NewGuid().ToString('N')
$base=Join-Path ([IO.Path]::GetTempPath()) 'wave5b-reparse-probe'
$expected=Join-Path $base ('scenario-'+$name)
New-Item -ItemType Directory -Path $expected | Out-Null
$sentinel=Join-Path $expected 'preexisting-sentinel.txt'
[IO.File]::WriteAllText($sentinel,'belongs-to-preexisting-run')
$returned=Get-ReparseCapableScratch $name
[ordered]@{returned=$returned; reused_preexisting_root=($returned -eq $expected); sentinel_survives=(Test-Path -LiteralPath $sentinel)} | ConvertTo-Json
