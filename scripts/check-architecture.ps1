$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Push-Location $root
try {
    cargo run -p architecture-check -- check
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo metadata architecture fitness failed"
    }
} finally {
    Pop-Location
}

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
Assert-NotContains "crates/shared-kernel/Cargo.toml" @("config", "tracing") "shared-kernel runtime configuration dependency violation"
Assert-NotContains "crates/shared-kernel/src" @(
    "AppConfig",
    "DatabaseConfig",
    "StorageConfig",
    "MessagingConfig",
    "ServerConfig",
    "AuthConfig",
    "std::env"
) "shared-kernel process configuration violation"
Assert-NotContains "crates/document/Cargo.toml" @("axum", "sqlx", "aws-sdk", "object-storage", "messaging") "document core dependency violation"
Assert-NotContains "crates/document/src/domain" @("IntoResponse", "sqlx::", "FromRow") "document domain protocol/SQL violation"
Assert-NotContains "crates/document/src/domain" @("std::env") "document domain environment access violation"
Assert-NotContains "crates/document/src/application" @("std::env") "document application environment access violation"
Assert-NotContains "crates/document/src/application" @("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM", "sqlx::", "FromRow") "document application SQL violation"
Assert-NotContains "crates/document/src/query" @("sqlx::", "FromRow") "document query DTO adapter leak"
Assert-NotContains "crates/document/src/domain" @("#[derive(FromRow", "sqlx::FromRow") "document domain row-model violation"
Assert-NotContains "apps/migration/src" @("sqlx::migrate!") "migration app must use the shared runtime migration catalog"
Assert-NotContains "apps/business-api/src" @("expected_migration", "PostgresReadinessProbe::new(pool,") "readiness must derive compatibility from the shared migration catalog"

$aggregatePath = Join-Path $root "crates/document/src/domain/entity.rs"
$aggregateContent = Get-Content -Raw $aggregatePath
$aggregateMatch = [regex]::Match($aggregateContent, 'pub struct DocumentMetadata\s*\{(?<body>[\s\S]*?)\n\}')
if (-not $aggregateMatch.Success) {
    throw "Unable to locate DocumentMetadata for aggregate encapsulation fitness"
}
if ($aggregateMatch.Groups["body"].Value -match '(?m)^\s*pub\s+(?!fn\b)') {
    throw "DocumentMetadata exposes a public field; aggregate state must remain private"
}
$aggregateDeclaration = [regex]::Match($aggregateContent, '#\[derive\((?<derive>[^\]]+)\)\]\s*pub struct DocumentMetadata')
if ($aggregateDeclaration.Success -and $aggregateDeclaration.Groups["derive"].Value -match 'Serialize') {
    throw "DocumentMetadata must not implement serde::Serialize"
}

foreach ($adapterPath in @("crates/document-postgres/src", "crates/document-sqlite/src")) {
    $adapterFiles = Get-ChildItem (Join-Path $root $adapterPath) -Filter "*.rs" -File
    foreach ($file in $adapterFiles) {
        $adapterContent = Get-Content -Raw $file.FullName
        if ($adapterContent -match '(?<!for )(?<!Rehydrate)DocumentMetadata\s*\{') {
            throw "Adapter constructs DocumentMetadata directly; use rehydrate: $($file.FullName)"
        }
    }
}

foreach ($searchPath in @(
    "crates/document/src/query/search.rs",
    "crates/document-postgres/src/search_query.rs"
)) {
    if (Test-Path (Join-Path $root $searchPath)) {
        throw "Deferred Document Search adapter still exists: $searchPath"
    }
}

foreach ($legacyPath in @(
    "crates/document/src/application/get.rs",
    "crates/document/src/application/list.rs",
    "crates/document-postgres/src/repository.rs",
    "crates/document-sqlite/src/repository.rs"
)) {
    if (Test-Path (Join-Path $root $legacyPath)) {
        throw "Legacy aggregate query/repository path still exists: $legacyPath"
    }
}
foreach ($legacySymbol in @("DocumentQueryRepository", "ListDocumentsQuery", "DocumentPage", "QueryDocumentError")) {
    $matches = git -C $root grep -n -F -- $legacySymbol -- '*.rs' '*.toml' 2>$null
    if ($LASTEXITCODE -eq 0 -and $matches) {
        throw "Legacy document query symbol still exists: $legacySymbol"
    }
}

$routeContent = Get-Content -Raw (Join-Path $root "apps/business-api/src/routes/documents.rs")
foreach ($legacyCursorField in @("cursor_created_at", "cursor_id")) {
    if ($routeContent -match [regex]::Escape($legacyCursorField)) {
        throw "HTTP route still accepts legacy double cursor field '$legacyCursorField'"
    }
}
$responseMatch = [regex]::Match($routeContent, 'struct DocumentResponse\s*\{(?<body>[\s\S]*?)\n\}')
if ($responseMatch.Success -and $responseMatch.Groups["body"].Value -match 'object_key') {
    throw "Document HTTP response contains an internal object key"
}
$statePath = Join-Path $root "apps/business-api/src/state.rs"
$stateContent = Get-Content -Raw $statePath
$appStateMatch = [regex]::Match($stateContent, 'pub struct AppState\s*\{(?<body>[\s\S]*?)\n\}')
if (-not $appStateMatch.Success) {
    throw "Unable to locate AppState for architecture fitness"
}
foreach ($pattern in @("AppConfig", "DatabaseConfig", "SecretUrl", "PgPool", "SqlitePool")) {
    if ($appStateMatch.Groups["body"].Value -match [regex]::Escape($pattern)) {
        throw "HTTP application state infrastructure leak (AppState contains '$pattern')"
    }
}

$repositoryPath = Join-Path $root "crates/document/src/domain/repository.rs"
$repositoryContent = Get-Content -Raw $repositoryPath
foreach ($forbiddenMethod in @("list", "search", "page", "offset", "total", "dashboard", "report", "export")) {
    if ($repositoryContent -match "async\s+fn\s+$forbiddenMethod") {
        throw "Aggregate repository contains non-aggregate query method '$forbiddenMethod'"
    }
}

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

function Assert-MigrationManifest([string]$MigrationDirectory, [string]$ManifestPath) {
    $directory = Join-Path $root $MigrationDirectory
    $manifest = Join-Path $root $ManifestPath
    if (-not (Test-Path $manifest)) {
        throw "Migration manifest missing: $ManifestPath"
    }
    $entries = @{}
    foreach ($line in Get-Content $manifest) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line -notmatch '^([0-9a-f]{64})\s{2}(.+\.sql)$') {
            throw "Invalid migration manifest line: $line"
        }
        $entries[$Matches[2]] = $Matches[1]
    }
    $sqlFiles = @(Get-ChildItem $directory -Filter "*.sql" -File | Sort-Object Name)
    if ($entries.Count -ne $sqlFiles.Count) {
        throw "Migration manifest does not cover exactly all SQL files: $ManifestPath"
    }

    function Get-NormalizedMigrationHash([string]$Path, [string]$NewLine) {
        $content = Get-Content -Raw -Encoding UTF8 $Path
        $normalized = $content -replace "`r`n", "`n" -replace "`r", "`n"
        if ($NewLine -eq "CRLF") {
            $normalized = $normalized -replace "`n", "`r`n"
        }
        $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($normalized)
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join "")
        } finally {
            $sha.Dispose()
        }
    }

    foreach ($file in $sqlFiles) {
        if (-not $entries.ContainsKey($file.Name)) {
            throw "Migration missing from manifest: $($file.Name)"
        }
        $actual = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $lf = Get-NormalizedMigrationHash $file.FullName "LF"
        $crlf = Get-NormalizedMigrationHash $file.FullName "CRLF"
        if ($entries[$file.Name] -notin @($actual, $lf, $crlf)) {
            throw "Migration hash mismatch: $($file.FullName)"
        }
    }
}

Assert-MigrationManifest "migrations" "migrations/MANIFEST.sha256"
Assert-MigrationManifest "crates/document-sqlite/migrations" "crates/document-sqlite/migrations/MANIFEST.sha256"
Assert-MigrationManifest "crates/document-processing-sqlite/migrations" "crates/document-processing-sqlite/migrations/MANIFEST.sha256"

foreach ($required in @(
    "docs/README.md",
    "docs/adr/README.md",
    "docs/architecture/ARCHITECTURE_STATUS.md",
    "docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md",
    "docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md",
    "docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md",
    "docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md",
    "docs/standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md"
)) {
    if (-not (Test-Path (Join-Path $root $required))) {
        throw "Required architecture entry missing: $required"
    }
}

Write-Output "Architecture fitness: PASS"
