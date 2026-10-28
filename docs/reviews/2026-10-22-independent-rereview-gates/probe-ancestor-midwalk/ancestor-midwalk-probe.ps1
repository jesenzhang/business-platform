$ErrorActionPreference = 'Stop'
$driver = 'F:\Workspace\business-platform\scripts\test-plan0014-wave5b.ps1'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($driver, [ref]$null, [ref]$null)
$wanted = @('Get-CanonicalPath','Test-ReparsePoint','Remove-Tree')
$text = ($ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true) |
    Where-Object { $wanted -contains $_.Name } | ForEach-Object { $_.Extent.Text }) -join "`r`n"
Invoke-Expression $text

$base = Join-Path $env:TEMP ('w5b-race-' + [guid]::NewGuid().ToString('N'))
$anchor = Join-Path $base 'anchor'
$tree = Join-Path $anchor 'scratch\run-owned'
$outside = Join-Path $base 'outside'
New-Item -ItemType Directory -Force -Path (Join-Path $tree 'child') | Out-Null
Set-Content -LiteralPath (Join-Path $tree 'child\owned.txt') -Value 'owned'
New-Item -ItemType Directory -Force -Path (Join-Path $outside 'scratch\run-owned\child') | Out-Null
$sentinel = Join-Path $outside 'scratch\run-owned\child\external-sentinel.txt'
Set-Content -LiteralPath $sentinel -Value 'belongs-to-someone-else'
$shaBefore = (Get-FileHash $sentinel -Algorithm SHA256).Hash

$script:swapped = $false
$script:tree = $tree
$script:anchor = $anchor
$script:outside = $outside
function global:Get-ChildItem {
    param([string]$LiteralPath, [switch]$Force)
    if (-not $script:swapped -and $LiteralPath -eq $script:tree) {
        $script:swapped = $true
        Move-Item -LiteralPath $script:anchor -Destination ($script:anchor + '.parked')
        New-Item -ItemType Junction -Path $script:anchor -Target $script:outside | Out-Null
    }
    Microsoft.PowerShell.Management\Get-ChildItem -LiteralPath $LiteralPath -Force:$Force
}

$threw = ''
try { Remove-Tree -Path $tree } catch { $threw = $_.Exception.Message }
$survives = Test-Path -LiteralPath $sentinel
"swapped_during_walk=$($script:swapped)"
"external_sentinel_survives=$survives"
"remove_tree_message=$threw"

# cleanup: unlink the junction first, never recurse through it
$j = Get-Item -LiteralPath $anchor -Force -ErrorAction SilentlyContinue
if ($j -and ($j.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { $j.Delete() }
Remove-Item -LiteralPath $base -Recurse -Force -ErrorAction SilentlyContinue
"cleanup_done=$(-not (Test-Path -LiteralPath $base))"
