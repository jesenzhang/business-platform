$ErrorActionPreference='Stop'
$driver='F:\Workspace\business-platform\target\w5b-q123-review\scripts\test-plan0014-wave5b.ps1'
$t=$null; $errors=$null
$a=[System.Management.Automation.Language.Parser]::ParseFile($driver,[ref]$t,[ref]$errors)
$names=@('Get-CanonicalPath','Test-ReparsePoint','Get-UnsafeLink','Assert-PathChainIsReal','Remove-Tree','Register-RunRoot','Remove-RunScratch','Write-Log')
$src=($a.FindAll({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true) | Where-Object {$names -contains $_.Name} | ForEach-Object {$_.Extent.Text}) -join [Environment]::NewLine
Invoke-Expression $src
$base=Join-Path ([System.IO.Path]::GetTempPath()) ('wave5b-review1009-queue-'+[guid]::NewGuid().ToString('N'))
$run=Join-Path $base 'run-owned'
$child=Join-Path $run 'child'
$external=Join-Path $base 'outside-owned-test'
New-Item -ItemType Directory -Force -Path $child,$external | Out-Null
$sentinel=Join-Path $external 'sentinel.txt'
Set-Content -LiteralPath $sentinel -Value 'probe-owned queued child external sentinel' -Encoding utf8
$script:ProbeBase=$base
$script:SwapRoot=$run; $script:SwapChild=$child; $script:SwapExternal=$external; $script:SwapDone=$false
function Get-ChildItem {
 param([string]$LiteralPath,[switch]$Force,[string]$ErrorAction)
 $items=@(Microsoft.PowerShell.Management\Get-ChildItem -LiteralPath $LiteralPath -Force)
 if(-not $script:SwapDone -and $LiteralPath -eq $script:SwapRoot) {
  foreach($item in $items) {$null=$item.Attributes}
  foreach($probePath in @($script:SwapChild,$script:SwapExternal)) {
   if(-not ([System.IO.Path]::GetFullPath($probePath)).StartsWith([System.IO.Path]::GetFullPath($script:ProbeBase)+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'probe escaped base'}
  }
  Move-Item -LiteralPath $script:SwapChild -Destination (Join-Path $script:ProbeBase 'parked-original-child')
  New-Item -ItemType Junction -Path $script:SwapChild -Target $script:SwapExternal | Out-Null
  $script:SwapDone=$true
 }
 return $items
}
$before=(Get-FileHash -LiteralPath $sentinel).Hash
try { Remove-Tree -Path $run } catch { Write-Output ('NAMED_REFUSAL='+$_.Exception.Message) }
[pscustomobject]@{base=$base; swapped_after_enumeration=$script:SwapDone; sentinel_survives=(Test-Path -LiteralPath $sentinel); original_parked_child_survives=(Test-Path -LiteralPath (Join-Path $base 'parked-original-child')); sha_before=$before} | ConvertTo-Json
