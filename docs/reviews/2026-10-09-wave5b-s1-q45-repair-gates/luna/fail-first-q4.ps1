$ErrorActionPreference='Stop'
$driver='F:/Workspace/business-platform/scripts/test-plan0014-wave5b.ps1'
$t=$null;$err=$null
$a=[System.Management.Automation.Language.Parser]::ParseFile($driver,[ref]$t,[ref]$err)
$names=@('Get-CanonicalPath','Test-ReparsePoint','Get-UnsafeLink','Assert-PathChainIsReal','Remove-Tree','Register-RunRoot','Remove-RunScratch','Write-Log')
$source=($a.FindAll({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true) | Where-Object {$names -contains $_.Name} | ForEach-Object {$_.Extent.Text}) -join [Environment]::NewLine
Invoke-Expression $source
$base=Join-Path ([IO.Path]::GetTempPath()) ('wave5b-q123-independent-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $base | Out-Null

$run=Join-Path $base 'run-owned'
$out=Join-Path $base 'outside-owned-probe'
New-Item -ItemType Directory -Path $run,$out | Out-Null
[IO.File]::WriteAllText((Join-Path $run 'shared.txt'),'owned-file')
$sentinel=Join-Path $out 'shared.txt'
[IO.File]::WriteAllText($sentinel,'independent-external-file')
$before=(Get-FileHash -LiteralPath $sentinel).Hash
$original=(Get-Command Test-ReparsePoint).ScriptBlock
$script:ProbeRun=$run;$script:ProbeOut=$out;$script:ProbeBase=$base
$script:ProbeChecks=0;$script:Swapped=$false;$script:NativeCheck=$original
function Test-ReparsePoint([string]$Path) {
 $answer=& $script:NativeCheck $Path
 if($Path -eq $script:ProbeRun) {
  $script:ProbeChecks++
  if($script:ProbeChecks -eq 2 -and -not $script:Swapped) {
   foreach($p in @($script:ProbeRun,$script:ProbeOut)) {
    if(-not ([IO.Path]::GetFullPath($p)).StartsWith([IO.Path]::GetFullPath($script:ProbeBase)+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'escaped owned probe'}
   }
   Move-Item -LiteralPath $script:ProbeRun -Destination (Join-Path $script:ProbeBase 'parked-run')
   New-Item -ItemType Junction -Path $script:ProbeRun -Target $script:ProbeOut | Out-Null
   $script:Swapped=$true
  }
 }
 return $answer
}
$thrown=''
try {Remove-Tree -Path $run -Attempts 1} catch {$thrown=$_.Exception.Message}
[ordered]@{base=$base; swapped_after_post_enumeration_check=$script:Swapped; root_checks=$script:ProbeChecks; sentinel_survives=(Test-Path -LiteralPath $sentinel); original_parked_file_survives=(Test-Path -LiteralPath (Join-Path $base 'parked-run/shared.txt')); thrown=$thrown; sha_before=$before} | ConvertTo-Json
