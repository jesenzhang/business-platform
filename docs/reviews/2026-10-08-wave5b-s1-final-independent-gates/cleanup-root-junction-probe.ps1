$ErrorActionPreference = 'Stop'
$driver = 'F:\Workspace\business-platform\target\wave5b-final-independent-20261008\candidate\scripts\test-plan0014-wave5b.ps1'
$t=$null; $e=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($driver,[ref]$t,[ref]$e)
$wanted=@('Get-CanonicalPath','Remove-Tree','Register-RunRoot','Remove-RunScratch','Write-Log')
$body=($ast.FindAll({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true) | Where-Object {$wanted -contains $_.Name} | ForEach-Object {$_.Extent.Text}) -join "`n"
Invoke-Expression $body
$base=Join-Path ([System.IO.Path]::GetTempPath()) ('wave5b-independent-cleanup-' + [guid]::NewGuid().ToString('N'))
$parent=Join-Path $base 'registered-parent'
$outside=Join-Path $base 'outside-owned-test-target'
$run=Join-Path $parent 'run-owned-test'
New-Item -ItemType Directory -Path $parent,$outside,$run -Force | Out-Null
$sentinel=Join-Path $outside 'sentinel.txt'
Set-Content -LiteralPath $sentinel -Value 'owned independent review sentinel' -Encoding utf8
$before=(Get-FileHash -LiteralPath $sentinel).Hash
$script:RunRoots=[System.Collections.Generic.List[object]]::new()
$script:ProtectedPaths=[System.Collections.Generic.List[string]]::new()
$script:RunLog=[System.Collections.Generic.List[string]]::new()
$script:InternalEvidence=$false; $KeepScratch=$false
$script:ProtectedPaths.Add($parent)
[void](Register-RunRoot -Root $run -Parent $parent -Kind 'main')
# All targets are explicitly owned by this probe and verified within its unique base.
foreach($target in @($run,$outside)) {
    $full=[System.IO.Path]::GetFullPath($target)
    if(-not $full.StartsWith([System.IO.Path]::GetFullPath($base)+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'probe target escaped owned base'}
}
Remove-Item -LiteralPath $run
New-Item -ItemType Junction -Path $run -Target $outside | Out-Null
$canonical=Get-CanonicalPath $run
$attrs=(Get-Item -LiteralPath $run).Attributes.ToString()
$messages=@(Remove-RunScratch 3>&1 | ForEach-Object {$_.ToString()})
$exists=Test-Path -LiteralPath $sentinel
[pscustomobject]@{base=$base; registered_root=$run; actual_target=$outside; canonical=$canonical; attributes=$attrs; before_sha=$before; sentinel_survives=$exists; root_survives=(Test-Path -LiteralPath $run); messages=$messages} | ConvertTo-Json -Depth 5
