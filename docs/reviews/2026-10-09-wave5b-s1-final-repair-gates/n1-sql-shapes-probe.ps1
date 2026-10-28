$ErrorActionPreference = 'Stop'
$repo = 'F:\Workspace\business-platform'
$exe = Join-Path $repo 'target/debug/module-release-tool.exe'
$base = Join-Path $repo 'target/wave5b-n1-probe/probes'
if (Test-Path $base) { Remove-Item -LiteralPath $base -Recurse -Force }
New-Item -ItemType Directory -Force -Path $base | Out-Null
$catalog = ((@(
'{',
'  "schema_version": "plan-0014.wave5b.catalog.v1",',
'  "namespace": "policy",',
'  "owner": "policy",',
'  "ledger": "platform_module_migrations",',
'  "sql_schema": "public",',
'  "depends_on": ["platform-baseline@19"]',
'}') -join "`n") + "`n")

$sql = [ordered]@{
    control_foreign_alias        = 'INSERT INTO roles SELECT r.* FROM roles r, platform_users p;'
    same_owner_plain             = 'INSERT INTO roles SELECT r.* FROM roles r, role_permissions rp;'
    foreign_after_alias_columns  = 'INSERT INTO roles (role_id) SELECT r.id FROM roles AS r(id), platform_users p;'
    same_owner_alias_columns     = 'INSERT INTO roles (role_id) SELECT r.id FROM roles AS r(role_id), role_permissions rp;'
    foreign_after_join_on        = 'INSERT INTO roles (role_id) SELECT r.role_id FROM roles r JOIN role_permissions rp ON rp.role_id = r.role_id, platform_users p;'
    same_owner_join_on           = 'INSERT INTO roles (role_id) SELECT r.role_id FROM roles r JOIN role_permissions rp ON rp.role_id = r.role_id, permission_definitions q;'
    delete_foreign_alias_columns = 'DELETE FROM roles USING roles AS r(id), platform_users p;'
    delete_same_owner_columns    = 'DELETE FROM roles USING roles AS r(role_id), role_permissions rp;'
    inheritance_star             = 'INSERT INTO roles SELECT r.* FROM roles * r, platform_users p;'
    inheritance_star_same_owner  = 'INSERT INTO roles SELECT r.* FROM roles * r, role_permissions rp;'
    join_using_columns           = 'INSERT INTO roles (role_id) SELECT a.id FROM roles a JOIN role_permissions USING (role_id);'
    index_using_btree            = 'CREATE UNIQUE INDEX wave5b_ix ON roles USING btree (role_id);'
    table_function               = 'INSERT INTO roles (role_id) SELECT x FROM generate_series(1, 3) g;'
    join_on_subquery             = 'INSERT INTO roles (role_id) SELECT r.id FROM roles r JOIN role_permissions rp ON rp.role_id IN (SELECT id FROM platform_users);'
    where_subquery               = 'INSERT INTO roles (role_id) SELECT 1 WHERE EXISTS (SELECT 1 FROM platform_users);'
    cross_join_list              = 'INSERT INTO roles (role_id) SELECT r.id FROM roles r CROSS JOIN role_permissions rp, platform_users p;'
    natural_join_list            = 'INSERT INTO roles (role_id) SELECT r.id FROM roles r NATURAL JOIN platform_users;'
    where_subquery_after_join    = 'INSERT INTO roles (role_id) SELECT r.id FROM roles r JOIN role_permissions rp ON rp.role_id = r.role_id WHERE EXISTS (SELECT 1 FROM platform_users);'
    alias_column_list_join       = 'INSERT INTO roles (role_id) SELECT r.role_id FROM roles r(id) JOIN role_permissions rp(role_id) ON rp.role_id = r.role_id, platform_users p;'
    qualified_after_join_on      = 'INSERT INTO roles (role_id) SELECT r.role_id FROM roles r JOIN role_permissions rp ON rp.role_id = r.role_id, public.platform_users p;'
}
foreach ($name in $sql.Keys) {
    $dir = Join-Path $base "sql-$name"
    $policy = Join-Path $dir 'migrations/modules/policy'
    New-Item -ItemType Directory -Force -Path $policy | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $policy 'catalog.json'), $catalog, [System.Text.UTF8Encoding]::new($false))
    $sqlBytes = [System.Text.Encoding]::UTF8.GetBytes((($sql[$name]) + "`n"))
    [System.IO.File]::WriteAllBytes((Join-Path $policy '001_probe.sql'), $sqlBytes)
    $digest = (([System.Security.Cryptography.SHA256]::Create().ComputeHash($sqlBytes) | ForEach-Object { $_.ToString('x2') }) -join '')
    [System.IO.File]::WriteAllText((Join-Path $policy 'MANIFEST.sha256'), ("$digest" + '  001_probe.sql' + "`n"), [System.Text.UTF8Encoding]::new($false))
    $out = & $exe catalog-check --repo $dir --only module-catalog 2>&1 | ForEach-Object { $_.ToString() }
    $msg = ((($out | Where-Object { $_ -match 'refused|status=' }) -join ' ') -replace '\s+', ' ')
    if ($msg.Length -gt 158) { $msg = $msg.Substring(0, 158) }
    ('{0,-32} exit={1}  {2}' -f $name, $LASTEXITCODE, $msg)
}