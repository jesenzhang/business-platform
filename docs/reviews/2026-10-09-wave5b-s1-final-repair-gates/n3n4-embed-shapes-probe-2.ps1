$ErrorActionPreference = 'Stop'
$repo = 'F:\Workspace\business-platform'
$exe = Join-Path $repo 'target/debug/module-release-tool.exe'
$base = Join-Path $repo 'target/wave5b-n1-probe/embed2'
if (Test-Path $base) { Remove-Item -LiteralPath $base -Recurse -Force }
New-Item -ItemType Directory -Force -Path $base | Out-Null
$q = [string][char]34
$a = [string][char]39
$bs = [string][char]92
$rust = [ordered]@{
    byte_char_raw_string = 'const B: u8 = b' + $a + $bs + 'n' + $a + '; const R: &str = r#' + $q + 'sqlx::migrate!(' + $bs + $q + 'hide' + $bs + $q + ')#' + $q + '; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    unterminated_char    = 'const B: char = ' + $a + $bs + ' ; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    stray_apostrophe     = 'const B: char = ' + $a + $a + '; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    lifetime_chain       = 'fn f<' + $a + "a>(x: &'a str) -> &'a str { static M: Migrator = sqlx::migrate!(" + $q + '../../migrations' + $q + '); x }'
    quote_char_then_call = 'const QUOTE: char = ' + $a + $bs + $q + $a + '; use sqlx::migrate as embed; static M: Migrator = embed!(' + $q + '../../migrations' + $q + ');'
    raw_hash_two         = 'const R: &str = r##"' + $bs + $q + 'sqlx::migrate!(' + $bs + $q + 'x' + $bs + $q + ')"##; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    c_string             = 'const C: &core::ffi::CStr = c"sqlx::migrate!(""x"")"; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
}
foreach ($name in $rust.Keys) {
    $dir = Join-Path $base $name
    foreach ($d in @('crates/runtime-migration/src', 'crates/second/src', 'migrations')) {
        New-Item -ItemType Directory -Force -Path (Join-Path $dir $d) | Out-Null
    }
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/runtime-migration/Cargo.toml'), "[package]`n", [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/second/Cargo.toml'), "[package]`n", [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/runtime-migration/src/lib.rs'), ('pub static MIGRATOR: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');' + "`n"), [System.Text.UTF8Encoding]::new($false))
    $body = $rust[$name] + "`n"
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/second/src/lib.rs'), $body, [System.Text.UTF8Encoding]::new($false))
    $out = & $exe catalog-check --repo $dir --only baseline-embed 2>&1 | ForEach-Object { $_.ToString() }
    $msg = ((($out | Where-Object { $_ -match 'refused|baseline_embed_points' }) -join ' | ') -replace '\s+', ' ')
    if ($msg.Length -gt 150) { $msg = $msg.Substring(0, 150) }
    '{0,-22} exit={1}  {2}' -f $name, $LASTEXITCODE, $msg
    if ($name -in @('byte_char_raw_string', 'unterminated_char', 'stray_apostrophe')) { '   file: ' + $body.Trim() }
}