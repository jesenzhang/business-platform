$ErrorActionPreference='Stop'
$driver='F:\Workspace\business-platform\target\w5b-q123-review\scripts\test-plan0014-wave5b.ps1'
$t=$null; $errors=$null
$a=[System.Management.Automation.Language.Parser]::ParseFile($driver,[ref]$t,[ref]$errors)
$names=@('Get-CanonicalPath','Test-ReparsePoint','Get-UnsafeLink','Assert-PathChainIsReal','Remove-Tree','Register-RunRoot','Remove-RunScratch','Write-Log')
$src=($a.FindAll({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true) | Where-Object {$names -contains $_.Name} | ForEach-Object {$_.Extent.Text}) -join [Environment]::NewLine
Invoke-Expression $src
$base=Join-Path ([System.IO.Path]::GetTempPath()) ('wave5b-review1009-ancestor-'+[guid]::NewGuid().ToString('N'))
$anchor=Join-Path $base 'anchor'
$parent=Join-Path $anchor 'scratch'
$run=Join-Path $parent 'run-owned'
$external=Join-Path $base 'outside-owned-test'
$externalRun=Join-Path $external 'scratch/run-owned'
New-Item -ItemType Directory -Force -Path $run,$externalRun | Out-Null
$sentinel=Join-Path $externalRun 'sentinel.txt'
Set-Content -LiteralPath $sentinel -Value 'probe-owned external sentinel' -Encoding utf8
$sha=(Get-FileHash -LiteralPath $sentinel).Hash
$script:RunRoots=[System.Collections.Generic.List[object]]::new()
$script:ProtectedPaths=[System.Collections.Generic.List[string]]::new()
$script:RunLog=[System.Collections.Generic.List[string]]::new()
$script:InternalEvidence=$false; $KeepScratch=$false
$script:ProtectedPaths.Add($parent)
[void](Register-RunRoot -Root $run -Parent $parent -Kind 'main')
foreach($probePath in @($anchor,$external,$run,$externalRun)) {
 if(-not ([System.IO.Path]::GetFullPath($probePath)).StartsWith([System.IO.Path]::GetFullPath($base)+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'probe escaped base'}
}
Move-Item -LiteralPath $anchor -Destination (Join-Path $base 'parked-original-anchor')
New-Item -ItemType Junction -Path $anchor -Target $external | Out-Null
$reportedUnsafe=Get-UnsafeLink -Path $run -Boundary $parent
$messages=@(Remove-RunScratch 3>&1 | ForEach-Object {$_.ToString()})
[pscustomobject]@{base=$base; anchor_link=$anchor; parent=$parent; root=$run; outside_actual_root=$externalRun; anchor_attributes=(Get-Item -LiteralPath $anchor).Attributes.ToString(); reported_unsafe_link=$reportedUnsafe; sentinel_survives=(Test-Path -LiteralPath $sentinel); sha_before=$sha; original_parked_root_survives=(Test-Path -LiteralPath (Join-Path $base 'parked-original-anchor/scratch/run-owned')); messages=$messages} | ConvertTo-Json -Depth 5
