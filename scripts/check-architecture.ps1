$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Assert-NotContains([string]$Path, [string[]]$Patterns, [string]$Message) {
    $target = Join-Path $root $Path
    if (-not (Test-Path $target)) {
        throw "Missing architecture target: $Path"
    }
    $files = if ((Get-Item $target).PSIsContainer) {
        Get-ChildItem $target -Recurse -File
    } else {
        @(Get-Item $target)
    }
    foreach ($file in $files) {
        $content = Get-Content -Raw $file.FullName
        foreach ($pattern in $Patterns) {
            if ($content -match [regex]::Escape($pattern)) {
                throw "$Message ($($file.FullName) contains '$pattern')"
            }
        }
    }
}

Assert-NotContains "crates/shared-kernel/Cargo.toml" @("axum", "sqlx", "reqwest", "aws-sdk") "shared-kernel dependency violation"
Assert-NotContains "crates/document/Cargo.toml" @("axum", "sqlx", "aws-sdk", "object-storage", "messaging") "document core dependency violation"
Assert-NotContains "crates/document/src/domain" @("IntoResponse", "sqlx::", "FromRow") "document domain protocol/SQL violation"

foreach ($path in @("crates/document/src/application", "crates/document/src/ports.rs")) {
    if (Test-Path (Join-Path $root $path)) {
        Assert-NotContains $path @("PgPool", "sqlx::Transaction", "aws_sdk_", "async_nats::") "document application adapter leak"
    }
}

$coreCrates = Get-ChildItem (Join-Path $root "crates") -Directory
foreach ($crate in $coreCrates) {
    $cargo = Join-Path $crate.FullName "Cargo.toml"
    if (Test-Path $cargo) {
        $content = Get-Content -Raw $cargo
        if ($content -match 'path\s*=\s*"\.\./\.\./apps/') {
            throw "Core crate depends on apps: $($crate.Name)"
        }
    }
}

$migrationVersions = @()
foreach ($file in Get-ChildItem (Join-Path $root "migrations") -Filter "*.sql" | Sort-Object Name) {
    if ($file.BaseName -match "^(\d+)_") {
        $migrationVersions += [int64]$Matches[1]
    } else {
        throw "Migration filename lacks numeric version: $($file.Name)"
    }
}
if (($migrationVersions | Sort-Object -Unique).Count -ne $migrationVersions.Count) {
    throw "Migration versions are not unique"
}
for ($i = 1; $i -lt $migrationVersions.Count; $i++) {
    if ($migrationVersions[$i] -le $migrationVersions[$i - 1]) {
        throw "Migration versions must increase strictly"
    }
}

foreach ($required in @(
    "docs/README.md",
    "docs/adr/README.md",
    "docs/architecture/ARCHITECTURE_STATUS.md",
    "docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md",
    "docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md"
)) {
    if (-not (Test-Path (Join-Path $root $required))) {
        throw "Required architecture entry missing: $required"
    }
}

Write-Output "Architecture fitness: PASS"
