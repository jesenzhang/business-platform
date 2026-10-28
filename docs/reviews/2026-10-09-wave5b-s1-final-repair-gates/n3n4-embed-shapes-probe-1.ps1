$ErrorActionPreference = 'Stop'
$repo = 'F:\Workspace\business-platform'
$exe = Join-Path $repo 'target/debug/module-release-tool.exe'
$base = Join-Path $repo 'target/wave5b-n1-probe/embed'
if (Test-Path $base) { Remove-Item -LiteralPath $base -Recurse -Force }
New-Item -ItemType Directory -Force -Path $base | Out-Null
$q = [char]34
$bs = [char]92
$rust = [ordered]@{
    control_plain      = 'use sqlx::migrate as embed; static M: Migrator = embed!(' + $q + '../../migrations' + $q + ');'
    raw_alias          = 'use sqlx::migrate as r#embed; static M: Migrator = r#embed!(' + $q + '../../migrations' + $q + ');'
    raw_macro_import   = 'use sqlx::r#migrate as embed; static M: Migrator = embed!(' + $q + '../../migrations' + $q + ');'
    raw_direct_call    = 'static M: Migrator = sqlx::r#migrate!(' + $q + '../../migrations' + $q + ');'
    raw_path_segment   = 'use sqlx::r#type::migrate; static M: Migrator = migrate!(' + $q + '../../migrations' + $q + ');'
    raw_use_ident      = 'let r#use = 1; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    double_quote_char  = 'const QUOTE: char = ' + $q + $bs + $q + $q + '; use sqlx::migrate as embed; static M: Migrator = embed!(' + $q + '../../migrations' + $q + ');'
    escaped_chars      = 'const A: char = ' + $q + $bs + $bs + $q + '; const B: char = ' + $q + $bs + $q + $q + '; const C: char = ' + $q + $bs + 'u{1F600}' + $q + '; fn f<' + $q + 'a, ' + $q + 'b>(x: &' + $q + 'a str) -> &' + $q + 'a str { static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + '); x }'
    apostrophe_comment = '// it' + $q + 's a comment with an apostrophe' + [char]10 + 'const S: &str = "a // not a comment"; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    byte_and_raw       = 'const B: u8 = b' + $q + $bs + 'n' + $q + '; const R: &str = r#' + $q + 'sqlx::migrate!("hide")#' + $q + '; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    unterminated_quote = 'const B: char = ' + $q + $bs + '; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
    half_use_tail      = 'use sqlx::migrate as embed extra; static M: Migrator = embed!(' + $q + '../../migrations' + $q + ');'
    quoted_pseudo      = 'const S: &str = ' + $q + 'sqlx::migrate!(&quot;../../migrations&quot;)' + $q + '; static M: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');'
}
foreach ($name in $rust.Keys) {
    $dir = Join-Path $base $name
    foreach ($d in @('crates/runtime-migration/src', 'crates/second/src', 'migrations')) {
        New-Item -ItemType Directory -Force -Path (Join-Path $dir $d) | Out-Null
    }
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/runtime-migration/Cargo.toml'), "[package]`n", [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/second/Cargo.toml'), "[package]`n", [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/runtime-migration/src/lib.rs'), ('pub static MIGRATOR: Migrator = sqlx::migrate!(' + $q + '../../migrations' + $q + ');' + "`n"), [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $dir 'crates/second/src/lib.rs'), ($rust[$name] + "`n"), [System.Text.UTF8Encoding]::new($false))
    $out = & $exe catalog-check --repo $dir --only baseline-embed 2>&1 | ForEach-Object { $_.ToString() }
    $msg = ((($out | Where-Object { $_ -match 'refused|baseline_embed_points' }) -join ' | ') -replace '\s+', ' ')
    if ($msg.Length -gt 170) { $msg = $msg.Substring(0, 170) }
    '{0,-20} exit={1}  {2}' -f $name, $LASTEXITCODE, $msg
}