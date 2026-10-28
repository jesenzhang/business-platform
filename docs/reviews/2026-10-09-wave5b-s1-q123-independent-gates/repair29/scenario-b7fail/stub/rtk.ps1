$dir = $env:WAVE5B_REPAIR_RTK_STUB_DIR
if ([string]::IsNullOrWhiteSpace($dir)) { Write-Error ('stub without capture dir'); exit 99 }
$serial = @(Get-ChildItem -LiteralPath $dir -Filter ('call-*.txt') -ErrorAction SilentlyContinue).Count
[System.IO.File]::WriteAllLines(
    (Join-Path $dir ('call-{0:D3}.txt' -f $serial)),
    [string[]]$args)
switch ($env:WAVE5B_REPAIR_RTK_STUB_MODE) {
    'always-7' { exit 7 }
    'unknown-check-1' {
        if ($args -contains 'everything') { exit 1 }
        exit 0
    }
    default { exit 0 }
}