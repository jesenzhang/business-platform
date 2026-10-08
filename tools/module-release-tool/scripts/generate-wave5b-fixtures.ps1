<#
  Regenerates the shared Wave 5B-S1 catalog fixture tree and its EXPECTATIONS.json.
  The tree is the single source of expectations for both readers
  (scripts/test-plan0014-wave5b.ps1 and tests/wave5b_catalog.rs), so any
  change here must be followed by both readers plus the gate re-run.
#>
param([string]$Repo = (Join-Path (Get-Location) '.'))
$ErrorActionPreference = "Stop"
$root = (Resolve-Path $Repo).Path
$fx = Join-Path $root "tools/module-release-tool/tests/fixtures/wave5b/catalogs"
$nl = [string][char]10
function Remove-Tree([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    for ($attempt = 1; $attempt -le 6; $attempt++) {
        try {
            Get-ChildItem -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue |
                Sort-Object { $_.FullName.Length } -Descending | ForEach-Object {
                    try {
                        $_.Attributes = 'Normal'
                        $_.Delete()
                    } catch { }
                }
            [System.IO.Directory]::Delete($Path, $true)
            return
        } catch {
            if ($attempt -eq 6) { throw }
            Start-Sleep -Milliseconds (250 * $attempt)
        }
    }
}

if (Test-Path $fx) { Remove-Tree $fx }
New-Item -ItemType Directory -Force -Path $fx | Out-Null

function Write-Text([string]$Path, [string]$Text) {
    $dir = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $normalized = ($Text -replace "`r`n", "`n")
    [System.IO.File]::WriteAllText($Path, $normalized, [System.Text.UTF8Encoding]::new($false))
}

function Get-LfDigest([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $lf = [System.Collections.Generic.List[byte]]::new()
    for ($i = 0; $i -lt $bytes.Length; $i++) {
        if ($bytes[$i] -eq 0x0D) {
            if ($i + 1 -lt $bytes.Length -and $bytes[$i + 1] -eq 0x0A) { $i++ }
            [void]$lf.Add(0x0A)
        } else { [void]$lf.Add($bytes[$i]) }
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { (($sha.ComputeHash([byte[]]$lf.ToArray()) | ForEach-Object { $_.ToString("x2") }) -join "") } finally { $sha.Dispose() }
}

function Get-CrlfDigest([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $crlf = [System.Collections.Generic.List[byte]]::new()
    foreach ($b in $bytes) {
        if ($b -eq 0x0A) { [void]$crlf.Add(0x0D) }
        [void]$crlf.Add($b)
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { (($sha.ComputeHash([byte[]]$crlf.ToArray()) | ForEach-Object { $_.ToString("x2") }) -join "") } finally { $sha.Dispose() }
}

function Set-Manifest([string]$CaseDir) {
    $lines = foreach ($file in (Get-ChildItem $CaseDir -Filter "*.sql" -File | Sort-Object Name)) {
        "{0}  {1}" -f (Get-LfDigest $file.FullName), $file.Name
    }
    if (@($lines).Count -eq 0) {
        [System.IO.File]::WriteAllBytes((Join-Path $CaseDir "MANIFEST.sha256"), [byte[]]@())
    } else {
        Write-Text (Join-Path $CaseDir "MANIFEST.sha256") (($lines -join "`n") + "`n")
    }
}

function Catalog([string]$Namespace, [string]$Owner, $DependsOn) {
    $deps = if ($DependsOn -is [string]) { $DependsOn } else { ($DependsOn | ForEach-Object { '"' + $_ + '"' }) -join ', ' }
    return @"
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "$Namespace",
  "owner": "$Owner",
  "ledger": "platform_module_migrations",
  "sql_schema": "public",
  "depends_on": [$deps]
}
"@
}

$SQL_FOUNDATION = @"
-- Wave 5B-S1 shared fixture: a policy-owned table plus its unique index.
-- @bp-owned: wave5b_fixture_policy_t1
CREATE TABLE wave5b_fixture_policy_t1 (
    t1_id BIGINT PRIMARY KEY,
    stable_key TEXT NOT NULL
);

CREATE UNIQUE INDEX ux_wave5b_fixture_policy_t1_key
    ON wave5b_fixture_policy_t1 (stable_key);
"@

$SQL_REFERENCE = @"
-- Wave 5B-S1 shared fixture: a later revision may reference an earlier one.
-- @bp-owned: wave5b_fixture_policy_t2
CREATE TABLE wave5b_fixture_policy_t2 (
    t2_id BIGINT PRIMARY KEY,
    t1_id BIGINT NOT NULL REFERENCES wave5b_fixture_policy_t1 (t1_id)
);

INSERT INTO wave5b_fixture_policy_t2 (t2_id, t1_id)
VALUES (1, 1);

ALTER TABLE wave5b_fixture_policy_t2
    ADD CONSTRAINT fk_wave5b_fixture_policy_t2_t1 FOREIGN KEY (t1_id)
    REFERENCES wave5b_fixture_policy_t1 (t1_id);

COMMENT ON TABLE wave5b_fixture_policy_t2 IS 'Wave 5B-S1 fixture table';
"@

# --- positives -------------------------------------------------------------
$dir = Join-Path $fx "reserved/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Set-Manifest $dir

$dir = Join-Path $fx "validated/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "002_wave5b_fixture_reference.sql") $SQL_REFERENCE
Set-Manifest $dir

# --- catalog metadata (schema) ---------------------------------------------
$dir = Join-Path $fx "catalog-unknown-field/policy"
Write-Text (Join-Path $dir "catalog.json") (@'
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "policy",
  "owner": "policy",
  "ledger": "platform_module_migrations",
  "sql_schema": "public",
  "depends_on": ["platform-baseline@19"],
  "table_prefix": "wave5b_"
}
'@)
Set-Manifest $dir

$dir = Join-Path $fx "catalog-missing-field/policy"
Write-Text (Join-Path $dir "catalog.json") (@'
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "policy",
  "owner": "policy",
  "sql_schema": "public",
  "depends_on": ["platform-baseline@19"]
}
'@)
Set-Manifest $dir

$dir = Join-Path $fx "catalog-duplicate-field/policy"
Write-Text (Join-Path $dir "catalog.json") (@'
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "policy",
  "owner": "policy",
  "namespace": "contract",
  "ledger": "platform_module_migrations",
  "sql_schema": "public",
  "depends_on": ["platform-baseline@19"]
}
'@)
Set-Manifest $dir

$dir = Join-Path $fx "catalog-wrong-type/policy"
$wrongType = "{" + $nl + '  "schema_version": "plan-0014.wave5b.catalog.v1",' + $nl +
    '  "namespace": "policy",' + $nl + '  "owner": "policy",' + $nl +
    '  "ledger": "platform_module_migrations",' + $nl + '  "sql_schema": "public",' + $nl +
    '  "depends_on": "platform-baseline@19"' + $nl + "}"
Write-Text (Join-Path $dir "catalog.json") $wrongType
Set-Manifest $dir

$dir = Join-Path $fx "catalog-unknown-schema/policy"
Write-Text (Join-Path $dir "catalog.json") (@'
{
  "schema_version": "plan-0014.wave5b.catalog.v2",
  "namespace": "policy",
  "owner": "policy",
  "ledger": "platform_module_migrations",
  "sql_schema": "public",
  "depends_on": ["platform-baseline@19"]
}
'@)
Set-Manifest $dir

$dir = Join-Path $fx "catalog-owner-mismatch/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "identity-management" @("platform-baseline@19"))
Set-Manifest $dir

$dir = Join-Path $fx "catalog-directory-mismatch/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Set-Manifest $dir

$dir = Join-Path $fx "catalog-unknown-context/contract"
Write-Text (Join-Path $dir "catalog.json") (Catalog "contract" "contract" @("platform-baseline@19"))
Set-Manifest $dir

$dir = Join-Path $fx "catalog-bad-ledger/policy"
Write-Text (Join-Path $dir "catalog.json") (@'
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "policy",
  "owner": "policy",
  "ledger": "policy_module_migrations",
  "sql_schema": "public",
  "depends_on": ["platform-baseline@19"]
}
'@)
Set-Manifest $dir

$dir = Join-Path $fx "catalog-bad-sql-schema/policy"
Write-Text (Join-Path $dir "catalog.json") (@'
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "policy",
  "owner": "policy",
  "ledger": "platform_module_migrations",
  "sql_schema": "policy",
  "depends_on": ["platform-baseline@19"]
}
'@)
Set-Manifest $dir

$dir = Join-Path $fx "catalog-not-a-directory/policy.txt"
Write-Text (Join-Path $fx "catalog-not-a-directory/policy.txt") "not a directory"

# --- manifest and revision rules -------------------------------------------
$dir = Join-Path $fx "manifest-path-entry/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "MANIFEST.sha256") ("{0}  sub/../001_wave5b_fixture_foundation.sql`n" -f (Get-LfDigest (Join-Path $dir "001_wave5b_fixture_foundation.sql")))

$dir = Join-Path $fx "manifest-ghost-entry/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
$ghost = Get-LfDigest (Join-Path $dir "001_wave5b_fixture_foundation.sql")
Write-Text (Join-Path $dir "MANIFEST.sha256") (("{0}  001_wave5b_fixture_foundation.sql" -f $ghost) + "`n" + ("{0}  002_wave5b_fixture_absent.sql`n" -f $ghost))

$dir = Join-Path $fx "manifest-missing-entry/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "002_wave5b_fixture_reference.sql") $SQL_REFERENCE
Write-Text (Join-Path $dir "MANIFEST.sha256") ("{0}  001_wave5b_fixture_foundation.sql`n" -f (Get-LfDigest (Join-Path $dir "001_wave5b_fixture_foundation.sql")))

$dir = Join-Path $fx "manifest-duplicate-entry/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
$one = Get-LfDigest (Join-Path $dir "001_wave5b_fixture_foundation.sql")
Write-Text (Join-Path $dir "MANIFEST.sha256") (("{0}  001_wave5b_fixture_foundation.sql" -f $one) + "`n" + ("{0}  001_wave5b_fixture_foundation.sql`n" -f $one))

$dir = Join-Path $fx "revision-duplicate/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "001_wave5b_fixture_alias.sql") $SQL_FOUNDATION
Set-Manifest $dir

$dir = Join-Path $fx "revision-gap/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "003_wave5b_fixture_reference.sql") $SQL_REFERENCE
Set-Manifest $dir

$dir = Join-Path $fx "manifest-non-canonical-digest/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "MANIFEST.sha256") ("{0}  001_wave5b_fixture_foundation.sql`n" -f (Get-CrlfDigest (Join-Path $dir "001_wave5b_fixture_foundation.sql")))

$dir = Join-Path $fx "manifest-empty-not-zero-bytes/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "MANIFEST.sha256") "`n"

$dir = Join-Path $fx "manifest-unknown-digest/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Write-Text (Join-Path $dir "MANIFEST.sha256") ("{0}  001_wave5b_fixture_foundation.sql`n" -f ("0" * 64))

# --- dependency rules -------------------------------------------------------
foreach ($case in @(
        @{ name = "dependency-unknown-namespace"; deps = @("platform-baseline@19", "acme@1") },
        @{ name = "dependency-baseline-zero"; deps = @("platform-baseline@0") },
        @{ name = "dependency-baseline-unavailable"; deps = @("platform-baseline@20") },
        @{ name = "dependency-without-revision"; deps = @("platform-baseline") },
        @{ name = "dependency-self"; deps = @("policy@1") },
        @{ name = "dependency-duplicate"; deps = @("platform-baseline@19", "platform-baseline@19") }
    )) {
    $dir = Join-Path $fx ($case.name + "/policy")
    Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" $case.deps)
    Set-Manifest $dir
}

$dir = Join-Path $fx "dependency-cycle/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("policy@1"))
Write-Text (Join-Path $dir "001_wave5b_fixture_identity.sql") @"
-- @bp-owned: wave5b_fixture_identity_t1
CREATE TABLE wave5b_fixture_identity_t1 (t1_id BIGINT PRIMARY KEY);
"@
Set-Manifest $dir
$dir = Join-Path $fx "dependency-cycle/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("identity-management@1"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Set-Manifest $dir

# --- SQL owner rules --------------------------------------------------------
function SqlCase([string]$Case, [string]$Sql, [string]$Declaration) {
    $dir = Join-Path $fx (Join-Path $Case "policy")
    Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
    if ($Declaration) {
        Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") ($Declaration + "`n" + $Sql)
    } else {
        Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $Sql
    }
    Set-Manifest $dir
}

SqlCase "sql-references-other-owner" @"
CREATE TABLE wave5b_fixture_policy_t3 (
    t3_id BIGINT NOT NULL REFERENCES platform_users (t1_id)
);
"@ "-- @bp-owned: wave5b_fixture_policy_t3"

SqlCase "sql-creates-other-owner-table" "CREATE TABLE platform_users (id BIGINT PRIMARY KEY);" "-- @bp-owned: platform_users"

SqlCase "sql-references-platform-ledger" "DELETE FROM platform_module_migrations;" ""

SqlCase "sql-undeclared-new-table" "CREATE TABLE wave5b_fixture_policy_t4 (t4_id BIGINT);" ""

SqlCase "sql-declared-not-created" "DELETE FROM permission_definitions;" "-- @bp-owned: wave5b_fixture_policy_t5"

SqlCase "sql-dynamic-sql" @"
DO `$`$ BEGIN
    EXECUTE 'CREATE TABLE wave5b_fixture_policy_t6 (id BIGINT)';
END
`$`$;
"@ ""

SqlCase "sql-qualified-reference" "DELETE FROM public.wave5b_fixture_policy_t1;" ""

SqlCase "sql-unsupported-statement" "CREATE SEQUENCE wave5b_fixture_policy_s1;" "-- @bp-owned: wave5b_fixture_policy_s1"

$dir = Join-Path $fx "table-name-conflict/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_shared.sql") @"
-- @bp-owned: wave5b_fixture_shared_t1
CREATE TABLE wave5b_fixture_shared_t1 (id BIGINT PRIMARY KEY);
"@
Set-Manifest $dir
$dir = Join-Path $fx "table-name-conflict/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_shared.sql") @"
-- @bp-owned: wave5b_fixture_shared_t1
CREATE TABLE wave5b_fixture_shared_t1 (id BIGINT PRIMARY KEY);
"@
Set-Manifest $dir

# --- CRLF checkout positive: bytes are CRLF, the recorded digest is canonical LF
$dir = Join-Path $fx "crlf-checkout-validated/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
$sqlPath = Join-Path $dir "001_wave5b_fixture_foundation.sql"
$lfBytes = [System.IO.File]::ReadAllBytes($sqlPath)
$crlfBytes = [System.Collections.Generic.List[byte]]::new()
foreach ($byte in $lfBytes) {
    if ($byte -eq 0x0A) { [void]$crlfBytes.Add(0x0D) }
    [void]$crlfBytes.Add($byte)
}
[System.IO.File]::WriteAllBytes($sqlPath, $crlfBytes.ToArray())
Write-Text (Join-Path $dir "MANIFEST.sha256") ("{0}  001_wave5b_fixture_foundation.sql`n" -f (Get-LfDigest (Join-Path $fx "validated/policy/001_wave5b_fixture_foundation.sql")))

# --- path forms the manifest must never reach (review §3.8 path group) -------
foreach ($entry in @(
        @{ name = "manifest-absolute-entry"; text = "/etc/passwd" },
        @{ name = "manifest-parent-entry"; text = "../001_wave5b_fixture_foundation.sql" },
        @{ name = "manifest-backslash-entry"; text = "sub\001_wave5b_fixture_foundation.sql" },
        @{ name = "manifest-dot-entry"; text = "./001_wave5b_fixture_foundation.sql" }
    )) {
    $dir = Join-Path $fx ($entry.name + "/policy")
    Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
    Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
    Write-Text (Join-Path $dir "MANIFEST.sha256") ("{0}  {1}`n" -f (Get-LfDigest (Join-Path $dir "001_wave5b_fixture_foundation.sql")), $entry.text)
}

# --- required revision that the checked-out catalog cannot satisfy ------------
$dir = Join-Path $fx "dependency-revision-unavailable/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Set-Manifest $dir
$dir = Join-Path $fx "dependency-revision-unavailable/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19", "identity-management@1"))
Set-Manifest $dir

$dir = Join-Path $fx "dependency-same-target-twice/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") $SQL_FOUNDATION
Set-Manifest $dir
$dir = Join-Path $fx "dependency-same-target-twice/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19", "identity-management@1", "identity-management@2"))
Set-Manifest $dir

# --- DDL forms that move or copy a whole table (review §3.5) ------------------
$dir = Join-Path $fx "sql-renames-whole-table/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_foundation.sql") @"
-- @bp-owned: wave5b_fixture_policy_t7
CREATE TABLE wave5b_fixture_policy_t7 (t7_id BIGINT);

ALTER TABLE wave5b_fixture_policy_t7
    RENAME TO roles;
"@
Set-Manifest $dir

SqlCase "sql-likes-other-table" "CREATE TABLE wave5b_fixture_policy_t8 (LIKE platform_users INCLUDING ALL);" "-- @bp-owned: wave5b_fixture_policy_t8"

SqlCase "sql-partitions-of-host-table" "CREATE TABLE wave5b_fixture_policy_t9 () PARTITION OF roles;" "-- @bp-owned: wave5b_fixture_policy_t9"

# --- the host baseline survives even its own owner (schema-level destruction) --
# `roles` belongs to policy (§3.1), so this is the owner dropping its own host
# baseline table: the ordinary owner rule would allow it, the freeze rule does not.
$dir = Join-Path $fx "sql-drops-host-table/policy"
Write-Text (Join-Path $dir "catalog.json") (Catalog "policy" "policy" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_policy.sql") @"
DROP TABLE roles;
"@
Set-Manifest $dir

$dir = Join-Path $fx "sql-drops-own-revision-table/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_identity.sql") @"
-- @bp-owned: wave5b_fixture_identity_t9
CREATE TABLE wave5b_fixture_identity_t9 (t9_id BIGINT PRIMARY KEY);
"@
Write-Text (Join-Path $dir "002_wave5b_fixture_identity_drop.sql") @"
DROP TABLE wave5b_fixture_identity_t9;
"@
Set-Manifest $dir

# --- review R1: the 019 rename targets are reserved legacy objects -----------
# Every rename target of the pinned 019 migration must stay unclaimable: an
# `IF NOT EXISTS` create over a legacy table would otherwise authorise writes
# against an object the module does not own (the original B2 bypass).
foreach ($renamed in @(
        "v0_demo_role_permissions",
        "v0_demo_user_roles",
        "v0_demo_roles",
        "v0_demo_permissions"
    )) {
    SqlCase ("r1-rename-target-" + $renamed) @"
CREATE TABLE IF NOT EXISTS $renamed (id bigint);
DELETE FROM $renamed;
"@ ("-- @bp-owned: " + $renamed)
}

# Host baseline tables that belong to no module owner stay reserved too.
SqlCase "r1-reserved-documents" "CREATE TABLE IF NOT EXISTS documents (id bigint);" "-- @bp-owned: documents"
SqlCase "r1-reserved-sqlx-migrations" "CREATE TABLE IF NOT EXISTS _sqlx_migrations (version BIGINT);" "-- @bp-owned: _sqlx_migrations"

# Reserved does not mean unusable by the existing owner: identity-management
# still references its own baseline table platform_users (review R1, narrow IAM
# exception kept).
$dir = Join-Path $fx "r1-owned-host-table-reference/identity-management"
Write-Text (Join-Path $dir "catalog.json") (Catalog "identity-management" "identity-management" @("platform-baseline@19"))
Write-Text (Join-Path $dir "001_wave5b_fixture_identity.sql") @"
-- @bp-owned: wave5b_fixture_iam_t2
CREATE TABLE wave5b_fixture_iam_t2 (
    t2_id BIGINT PRIMARY KEY REFERENCES platform_users (t2_id)
);
"@
Set-Manifest $dir

# --- review R2: comma table lists and aliases are all owner checked ----------
# The two statements the repair review called out by name, kept verbatim.
SqlCase "r2-foreign-in-from-alias" "INSERT INTO roles SELECT r.* FROM roles r, platform_users p;" ""
SqlCase "r2-foreign-in-using" "DELETE FROM roles USING roles r, platform_users p;" ""

# Foreign reference in the first / middle / last list position, and the AS alias
# form, all refused; a qualified name behind a comma is refused by name.
$R2_HEAD = "-- @bp-owned: wave5b_fixture_policy_t1" + "`n" + "CREATE TABLE wave5b_fixture_policy_t1 (id BIGINT);"
SqlCase "r2-foreign-first-in-list" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT p.id FROM platform_users p, roles r;" $R2_HEAD
SqlCase "r2-foreign-middle-in-list" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT a.id FROM roles a, platform_users p, role_permissions b;" $R2_HEAD
SqlCase "r2-foreign-last-in-list" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT b.id FROM roles a, role_permissions b, platform_users p;" $R2_HEAD
SqlCase "r2-foreign-in-as-alias" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT r.id FROM roles AS r, platform_users AS p;" $R2_HEAD
SqlCase "r2-qualified-after-comma" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT x FROM roles r, public.platform_users p;" $R2_HEAD

# Same-owner multi-table list with aliases (bare and AS) must pass unchanged.
SqlCase "r2-same-owner-alias-list" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT r.id FROM roles r, role_permissions rp;" $R2_HEAD

# FROM / USING / commas inside comments and string literals are not references.
SqlCase "r2-comment-string-not-reference" @"
CREATE TABLE wave5b_fixture_policy_t1 (payload TEXT);

-- FROM platform_users, USING x
INSERT INTO wave5b_fixture_policy_t1 (payload) VALUES ('FROM platform_users, roles USING y');
"@ "-- @bp-owned: wave5b_fixture_policy_t1"

# --- review R6: statement-aware USING, INHERITS and index methods ------------
# The owned-table index with an access method is legal SQL and must pass.
SqlCase "r6-index-using-btree" "CREATE INDEX wave5b_ix ON roles USING btree (role_id);" ""
SqlCase "r6-index-unknown-method" "CREATE INDEX wave5b_ix ON roles USING mystery (role_id);" ""
SqlCase "r6-join-using-columns" "INSERT INTO wave5b_fixture_policy_t1 (id) SELECT a.id FROM roles a JOIN role_permissions USING (role_id);" $R2_HEAD
SqlCase "r6-inherits-parent" "CREATE TABLE wave5b_fixture_policy_t10 (id BIGINT) INHERITS (roles);" "-- @bp-owned: wave5b_fixture_policy_t10"
SqlCase "r6-attach-partition" "ALTER TABLE wave5b_fixture_policy_t1 ATTACH PARTITION platform_users DEFAULT;" $R2_HEAD

# --- expectations shared by both readers ------------------------------------
Write-Text (Join-Path $fx "EXPECTATIONS.json") @'
{
  "schema_version": "plan-0014.wave5b.catalog-fixtures.v1",
  "cases": [
    { "case": "reserved", "expect": "pass", "status": "RESERVED" },
    { "case": "validated", "expect": "pass", "status": "VALIDATED" },
    { "case": "crlf-checkout-validated", "expect": "pass", "status": "VALIDATED" },
    { "case": "catalog-unknown-field", "expect": "fail", "contains": "table_prefix" },
    { "case": "catalog-missing-field", "expect": "fail", "contains": "ledger" },
    { "case": "catalog-duplicate-field", "expect": "fail", "contains": "declared twice" },
    { "case": "catalog-wrong-type", "expect": "fail", "contains": "invalid type" },
    { "case": "catalog-unknown-schema", "expect": "fail", "contains": "plan-0014.wave5b.catalog.v2" },
    { "case": "catalog-owner-mismatch", "expect": "fail", "contains": "does not equal namespace" },
    { "case": "catalog-directory-mismatch", "expect": "fail", "contains": "does not equal the directory name" },
    { "case": "catalog-unknown-context", "expect": "fail", "contains": "authoritative contexts" },
    { "case": "catalog-bad-ledger", "expect": "fail", "contains": "platform_module_migrations" },
    { "case": "catalog-bad-sql-schema", "expect": "fail", "contains": "sql_schema" },
    { "case": "catalog-not-a-directory", "expect": "fail", "contains": "is not a namespace directory" },
    { "case": "manifest-path-entry", "expect": "fail", "contains": "plain 'NNN_lower_name.sql' basename" },
    { "case": "manifest-absolute-entry", "expect": "fail", "contains": "plain 'NNN_lower_name.sql' basename" },
    { "case": "manifest-parent-entry", "expect": "fail", "contains": "plain 'NNN_lower_name.sql' basename" },
    { "case": "manifest-backslash-entry", "expect": "fail", "contains": "plain 'NNN_lower_name.sql' basename" },
    { "case": "manifest-dot-entry", "expect": "fail", "contains": "plain 'NNN_lower_name.sql' basename" },
    { "case": "manifest-ghost-entry", "expect": "fail", "contains": "no such file is in" },
    { "case": "manifest-missing-entry", "expect": "fail", "contains": "absent from" },
    { "case": "manifest-duplicate-entry", "expect": "fail", "contains": "lists '001_wave5b_fixture_foundation.sql' twice" },
    { "case": "revision-duplicate", "expect": "fail", "contains": "revision 001 is used by more than one file" },
    { "case": "revision-gap", "expect": "fail", "contains": "leaves a gap" },
    { "case": "manifest-non-canonical-digest", "expect": "fail", "contains": "canonical LF digest" },
    { "case": "manifest-empty-not-zero-bytes", "expect": "fail", "contains": "blank line" },
    { "case": "manifest-unknown-digest", "expect": "fail", "contains": "canonical LF digest" },
    { "case": "dependency-unknown-namespace", "expect": "fail", "contains": "unknown namespace 'acme'" },
    { "case": "dependency-baseline-zero", "expect": "fail", "contains": "positive revision" },
    { "case": "dependency-baseline-unavailable", "expect": "fail", "contains": "pinned baseline provides 19" },
    { "case": "dependency-without-revision", "expect": "fail", "contains": "'<name>@<revision>'" },
    { "case": "dependency-self", "expect": "fail", "contains": "depends on itself" },
    { "case": "dependency-duplicate", "expect": "fail", "contains": "more than once" },
    { "case": "dependency-cycle", "expect": "fail", "contains": "form a cycle" },
    { "case": "dependency-revision-unavailable", "expect": "fail", "contains": "requires revision 1 of 'identity-management'" },
    { "case": "dependency-same-target-twice", "expect": "fail", "contains": "more than once" },
    { "case": "sql-renames-whole-table", "expect": "fail", "contains": "'RENAME TO' renames the whole table" },
    { "case": "sql-likes-other-table", "expect": "fail", "contains": "'LIKE <table>' copies another table's definition" },
    { "case": "sql-partitions-of-host-table", "expect": "fail", "contains": "'PARTITION OF' inherits a parent table" },
    { "case": "sql-drops-host-table", "expect": "fail", "contains": "may not destroy it, even as its owner" },
    { "case": "sql-drops-own-revision-table", "expect": "pass", "status": "VALIDATED" },
    { "case": "sql-references-other-owner", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "sql-creates-other-owner-table", "expect": "fail", "contains": "host baseline table owned by 'identity-management'" },
    { "case": "sql-references-platform-ledger", "expect": "fail", "contains": "platform ledger" },
    { "case": "sql-undeclared-new-table", "expect": "fail", "contains": "without an '@bp-owned: wave5b_fixture_policy_t4' declaration" },
    { "case": "sql-declared-not-created", "expect": "fail", "contains": "never creates it" },
    { "case": "sql-dynamic-sql", "expect": "fail", "contains": "dollar-quoted" },
    { "case": "sql-qualified-reference", "expect": "fail", "contains": "schema-qualified" },
    { "case": "sql-unsupported-statement", "expect": "fail", "contains": "is outside the supported SQL subset" },
    { "case": "table-name-conflict", "expect": "fail", "contains": "two namespaces may not create the same table" },
    { "case": "r1-rename-target-v0_demo_role_permissions", "expect": "fail", "contains": "already exists in the pinned host baseline" },
    { "case": "r1-rename-target-v0_demo_user_roles", "expect": "fail", "contains": "already exists in the pinned host baseline" },
    { "case": "r1-rename-target-v0_demo_roles", "expect": "fail", "contains": "already exists in the pinned host baseline" },
    { "case": "r1-rename-target-v0_demo_permissions", "expect": "fail", "contains": "already exists in the pinned host baseline" },
    { "case": "r1-reserved-documents", "expect": "fail", "contains": "already exists in the pinned host baseline" },
    { "case": "r1-reserved-sqlx-migrations", "expect": "fail", "contains": "already exists in the pinned host baseline" },
    { "case": "r1-owned-host-table-reference", "expect": "pass", "status": "VALIDATED" },
    { "case": "r2-foreign-in-from-alias", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "r2-foreign-in-using", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "r2-foreign-first-in-list", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "r2-foreign-middle-in-list", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "r2-foreign-last-in-list", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "r2-foreign-in-as-alias", "expect": "fail", "contains": "gives to owner 'identity-management'" },
    { "case": "r2-qualified-after-comma", "expect": "fail", "contains": "schema-qualified" },
    { "case": "r2-same-owner-alias-list", "expect": "pass", "status": "VALIDATED" },
    { "case": "r2-comment-string-not-reference", "expect": "pass", "status": "VALIDATED" },
    { "case": "r6-index-using-btree", "expect": "pass", "status": "VALIDATED" },
    { "case": "r6-index-unknown-method", "expect": "fail", "contains": "index access method 'mystery'" },
    { "case": "r6-join-using-columns", "expect": "fail", "contains": "outside the supported SQL subset" },
    { "case": "r6-inherits-parent", "expect": "fail", "contains": "'INHERITS (parent)'" },
    { "case": "r6-attach-partition", "expect": "fail", "contains": "gives to owner 'identity-management'" }
  ]
}
'@

Write-Output ("fixtures written under " + $fx)
Get-ChildItem -Recurse $fx -File | Measure-Object | ForEach-Object { "files=$($_.Count)" }
