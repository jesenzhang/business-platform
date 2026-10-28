$ErrorActionPreference='Stop'
$driver='F:/Workspace/business-platform/target/w5b-q123-review/scripts/test-plan0014-wave5b.ps1'
$t=$null;$err=$null
$a=[System.Management.Automation.Language.Parser]::ParseFile($driver,[ref]$t,[ref]$err)
$names=@('Get-CanonicalPath','Test-ReparsePoint','Get-UnsafeLink','Assert-PathChainIsReal','Remove-Tree','Register-RunRoot','Remove-RunScratch','Write-Log')
$source=($a.FindAll({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true) | Where-Object {$names -contains $_.Name} | ForEach-Object {$_.Extent.Text}) -join [Environment]::NewLine
Invoke-Expression $source
$base=Join-Path ([IO.Path]::GetTempPath()) ('wave5b-q123-independent-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $base | Out-Null

$real=Join-Path $base 'real-owned'
$out=Join-Path $base 'outside-owned-probe'
$link=Join-Path $base 'linked-parent'
New-Item -ItemType Directory -Path $real,$out | Out-Null
$sentinel=Join-Path $out 'sentinel.txt'
[IO.File]::WriteAllText($sentinel,'independent-external-file')
New-Item -ItemType Junction -Path $link -Target $out | Out-Null
$before=(Get-FileHash -LiteralPath $sentinel).Hash
$thrown=''
try {Remove-Tree -Path (Join-Path $link 'sentinel.txt')} catch {$thrown=$_.Exception.Message}
[ordered]@{base=$base; sentinel_survives=(Test-Path -LiteralPath $sentinel); thrown=$thrown; sha_before=$before} | ConvertTo-Json
