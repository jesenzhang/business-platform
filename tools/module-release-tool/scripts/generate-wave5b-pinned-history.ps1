<#
  Rebuilds the pinned baseline-history table from git blob bytes.
  Digests are SHA-256 of `git cat-file blob <sha>:<path>`; a git object id is
  never used as a digest. Re-running this against a new SHA is a design-level
  act (it moves what "history" is pinned to) and needs independent review.
#>
param(
    [string]$Repo = (Join-Path (Get-Location) '.'),
    [string]$PinnedSha = '5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601',
    [string]$Out = '
)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path $Repo).Path
$pinned = $PinnedSha
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) 'wave5b-pinned-probe'
if ([string]::IsNullOrWhiteSpace($Out)) {
    $Out = Join-Path $root "tools/module-release-tool/tests/fixtures/wave5b/history/pinned-baseline-$pinned.json"
}
$out = $Out
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

function Get-BlobBytes([string]$Rev, [string]$GitPath) {
    $o = Join-Path $tmp ("blob-" + [guid]::NewGuid().ToString("N") + ".bin")
    $e = Join-Path $tmp ("err-" + [guid]::NewGuid().ToString("N") + ".txt")
    $p = Start-Process -FilePath "git" `
        -ArgumentList @("--no-optional-locks", "-C", $root, "cat-file", "blob", "${Rev}:${GitPath}") `
        -NoNewWindow -Wait -PassThru -RedirectStandardOutput $o -RedirectStandardError $e
    if ($p.ExitCode -ne 0) { throw "git cat-file blob ${Rev}:${GitPath} exit=$($p.ExitCode)" }
    $bytes = [System.IO.File]::ReadAllBytes($o)
    Remove-Item $o, $e -Force
    return , $bytes
}
function Get-Sha256Hex([byte[]]$Bytes) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { (($sha.ComputeHash($Bytes) | ForEach-Object { $_.ToString("x2") }) -join "") } finally { $sha.Dispose() }
}
function Get-FoldLf([byte[]]$Bytes) {
    $lf = [System.Collections.Generic.List[byte]]::new()
    for ($i = 0; $i -lt $Bytes.Length; $i++) {
        if ($Bytes[$i] -eq 0x0D) {
            if ($i + 1 -lt $Bytes.Length -and $Bytes[$i + 1] -eq 0x0A) { $i++ }
            [void]$lf.Add(0x0A)
        } else { [void]$lf.Add($Bytes[$i]) }
    }
    return [byte[]]$lf.ToArray()
}
function Get-ToCrlf([byte[]]$Bytes) {
    $crlf = [System.Collections.Generic.List[byte]]::new()
    foreach ($b in (Get-FoldLf $Bytes)) {
        if ($b -eq 0x0A) { [void]$crlf.Add(0x0D) }
        [void]$crlf.Add($b)
    }
    return [byte[]]$crlf.ToArray()
}

$manifestBytes = Get-BlobBytes $pinned "migrations/MANIFEST.sha256"
$manifestText = [System.Text.Encoding]::UTF8.GetString($manifestBytes)
$recorded = [ordered]@{}
foreach ($line in ($manifestText -split "`n")) {
    $t = $line.TrimEnd("`r")
    if ([string]::IsNullOrWhiteSpace($t)) { continue }
    if ($t -notmatch '^([0-9a-f]{64})  ([0-9]{3}_.+\.sql)$') { throw "unparsable manifest line '$t'" }
    if ($recorded.Contains($Matches[2])) { throw "duplicate manifest entry $($Matches[2])" }
    $recorded[$Matches[2]] = $Matches[1]
}
$names = @($recorded.Keys | Sort-Object)
$baselineRevision = $names.Count

$nl = [string][char]10
$q = [string][char]34
$sb = [System.Text.StringBuilder]::new()
[void]$sb.Append('{' + $nl)
[void]$sb.Append('  ' + $q + 'schema_version' + $q + ': ' + $q + 'plan-0014.wave5b.pinned-baseline-history.v1' + $q + ',' + $nl)
[void]$sb.Append('  ' + $q + 'pinned_revision' + $q + ': ' + $q + $pinned + $q + ',' + $nl)
[void]$sb.Append('  ' + $q + 'baseline_revision' + $q + ': ' + $baselineRevision + ',' + $nl)
[void]$sb.Append('  ' + $q + 'digest_rule' + $q + ': ' + $q + 'SHA-256 of the exact bytes returned by git cat-file blob for <pinned_revision>:<path>; never a Git object id and never working-tree bytes' + $q + ',' + $nl)
[void]$sb.Append('  ' + $q + 'entries' + $q + ': [' + $nl)
$items = [System.Collections.Generic.List[string]]::new()
foreach ($name in $names) {
    $bytes = Get-BlobBytes $pinned "migrations/$name"
    $raw = Get-Sha256Hex $bytes
    $lf = Get-Sha256Hex (Get-FoldLf $bytes)
    $crlf = Get-Sha256Hex (Get-ToCrlf $bytes)
    $rec = $recorded[$name]
    $form = if ($rec -eq $lf) { "lf" } elseif ($rec -eq $crlf) { "crlf" } elseif ($rec -eq $raw) { "raw" } else { throw "recorded digest for $name matches no line-ending form" }
    $rev = ($name -split '_')[0]
    $item = "    {`n" +
        "      `"path`": `"migrations/$name`",`n" +
        "      `"kind`": `"baseline-sql`",`n" +
        "      `"revision`": `"$rev`",`n" +
        "      `"blob_sha256`": `"$raw`",`n" +
        "      `"historical_manifest_digest`": `"$rec`",`n" +
        "      `"historical_manifest_form`": `"$form`"`n" +
        "    }"
    $items.Add($item)
}
$items.Add("    {`n" +
    "      `"path`": `"migrations/MANIFEST.sha256`",`n" +
    "      `"kind`": `"baseline-manifest`",`n" +
    "      `"blob_sha256`": `"$(Get-Sha256Hex $manifestBytes)`"`n" +
    "    }")
[void]$sb.Append(($items -join ",`n"))
[void]$sb.Append("`n  ]`n}`n")
[System.IO.File]::WriteAllText($out, $sb.ToString(), [System.Text.UTF8Encoding]::new($false))
Write-Output "wrote $out"
Write-Output ("entries=" + ($items.Count))
Write-Output ("forms: lf=" + (@($names | Where-Object { $true }).Count))
