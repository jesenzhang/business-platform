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
Assert-NotContains "apps/business-worker/src" @(
    "ProcessingJobCommandPort",
    "ProcessingStepStore",
    "CandidateStore",
    "AiTaskPort",
    "FixedPipelineRunner"
) "business-worker must use the execution unit of work for writes"
Assert-NotContains "apps/ai-worker/src" @(
    "CandidateStore",
    "ProcessingJobCommandPort",
    "ProcessingStepStore",
    "FixedPipelineRunner"
) "ai-worker must use the execution unit of work for writes"
Assert-NotContains "apps/business-api/src" @(
    "ProcessingJobCommandPort",
    "ProcessingStepStore",
    "CandidateStore",
    "AiTaskPort"
) "business-api must not compose legacy processing write ports"
Assert-NotContains "apps/governance-worker/src" @(
    "execute_sql",
    "repair_table",
    "DELETE FROM audit_events",
    "UPDATE audit_events"
) "governance worker must use typed repair ports"

# PLAN-0007 external access and client boundaries.
Assert-NotContains "crates/public-api-contracts/Cargo.toml" @(
    "axum", "sqlx", "reqwest", "object-storage", "document-postgres", "document-processing-postgres"
) "public API contracts must remain transport and infrastructure neutral"
Assert-NotContains "apps/business-cli/Cargo.toml" @(
    "sqlx", "object-storage", "document-postgres", "document-processing-postgres"
) "business CLI must use the typed Business API client"
Assert-NotContains "apps/agent-adapter/Cargo.toml" @(
    "sqlx", "object-storage", "document-postgres", "document-processing-postgres"
) "MCP adapter must use the typed Business API client"
Assert-NotContains "apps/agent-adapter/src" @(
    "sqlx::", "PgPool", "S3Client", "STORAGE_SECRET_KEY", "execute_sql"
) "MCP adapter must not access persistence or object storage"
Assert-NotContains "apps/business-cli/src" @(
    "sqlx::", "PgPool", "S3Client", "STORAGE_SECRET_KEY", "execute_sql"
) "business CLI must not access persistence or object storage"
Assert-NotContains "apps/business-console/src" @(
    "postgres", "sqlx", "object_key", "storage_key", "internal_path"
) "business console must remain a replaceable REST client"

# PLAN-0010 Business Module Isolation and Semantic Contract boundaries.
foreach ($contractCrate in @(
    "crates/business-module-contracts",
    "crates/business-application-compiler",
    "crates/semantic-contract"
)) {
    Assert-NotContains (Join-Path $contractCrate "Cargo.toml") @(
        "wren", "python", "lancedb", "datafusion", "sqlglot", "clickhouse",
        "text-to-sql", "axum", "sqlx", "reqwest", "aws", "aws-sdk", "aws-config", "object-storage",
        "messaging", "ai-application", "customer = ", "contract = ",
        "finance = ", "project = "
    ) "PLAN-0010 pure contract/compiler dependency violation"
    Assert-NotContains (Join-Path $contractCrate "src") @(
        "WrenAI", "wren-ai", "lancedb", "DataFusion", "SQLGlot", "ClickHouse",
        "text-to-sql", "run_sql", "execute_sql", "AWS", "aws_sdk", "contract_management",
        "legacy-contract", "C Project", "plan-0009", "DATABASE_URL",
        "storage_key", "signed_url", "credential"
    ) "PLAN-0010 generic platform/C-specific boundary violation"
}

# PLAN-0011 Business Application Packaging must remain a pure, business-neutral
# declaration/compiler foundation. Test fixtures are intentionally outside this
# source scan; production code must not know fixture or concrete module names.
$businessApplicationCompilerCargo = Get-Content -Raw (Join-Path $root "crates/business-application-compiler/Cargo.toml")
if ($businessApplicationCompilerCargo -match 'path\s*=\s*"\.\./\.\./(apps|crates)/(?!business-module-contracts)') {
    throw "business-application-compiler must not depend on implementation crates"
}
$businessApplicationCompilerSource = @(Get-ChildItem (Join-Path $root "crates/business-application-compiler/src") -Recurse -File | ForEach-Object {
    Get-Content -Raw $_.FullName
}) -join [Environment]::NewLine
foreach ($forbiddenCompilerToken in @(
    "axum", "sqlx", "reqwest", "object-storage", "messaging", "ai-provider",
    "WrenAI", "wren-ai", "Twenty", "Odoo", "Frappe", "ERPNext",
    "module-a", "module-b", "module-extension",
    "PurgeBusinessData", "DeleteData", "DropBusinessFacts", "PurgeOperation",
    "DatasetDefinition", "MetricDefinition", "DimensionDefinition",
    "RelationshipDefinition", "LineageDefinition"
)) {
    if ($businessApplicationCompilerSource -match [regex]::Escape($forbiddenCompilerToken)) {
        throw "PLAN-0011 generic compiler boundary violation: production source contains '$forbiddenCompilerToken'"
    }
}
if ($businessApplicationCompilerSource -match '(?im)^\s*pub\s+(struct|enum)\s+(Dataset|Metric|Dimension|Relationship|Lineage)') {
    throw "PLAN-0011 compiler must not define a second semantic authority"
}

$semanticCompilerCargo = Get-Content -Raw (Join-Path $root "crates/semantic-contract/Cargo.toml")
if ($semanticCompilerCargo -notmatch 'business-module-contracts\s*=\s*\{\s*workspace\s*=\s*true\s*\}') {
    throw "semantic-contract must depend on the generic business-module-contracts seam"
}
$moduleContractCargo = Get-Content -Raw (Join-Path $root "crates/business-module-contracts/Cargo.toml")
if ($moduleContractCargo -match 'path\s*=\s*"\.\./\.\./(crates|apps)/') {
    throw "business-module-contracts must not depend on workspace implementation crates"
}

$requiredManifestFields = @(
    "module_id", "module_version", "manifest_schema_version",
    "owned_bounded_contexts", "required_platform_capabilities",
    "optional_platform_capabilities", "published_commands",
    "published_queries", "published_events", "resource_kinds",
    "data_classification", "migration_namespace", "semantic_contributions",
    "ui_contributions", "agent_tool_contributions", "dependencies",
    "compatibility"
)
$moduleManifestSource = @(Get-ChildItem (Join-Path $root "crates/business-module-contracts/src") -Recurse -File | ForEach-Object {
    Get-Content -Raw $_.FullName
}) -join [Environment]::NewLine
foreach ($field in $requiredManifestFields) {
    if ($moduleManifestSource -notmatch [regex]::Escape($field)) {
        throw "Business Module Manifest field is missing: $field"
    }
}

$semanticSource = @(Get-ChildItem (Join-Path $root "crates/semantic-contract/src") -Recurse -File | ForEach-Object {
    Get-Content -Raw $_.FullName
}) -join [Environment]::NewLine
foreach ($semanticKind in @(
    "DatasetDefinition", "ProjectionDefinition", "FieldDefinition",
    "RelationshipDefinition", "MeasureDefinition", "MetricDefinition",
    "DimensionDefinition", "TimeDimensionDefinition",
    "FilterPolicyDefinition", "LineageDefinition", "SemanticCompiler"
)) {
    if ($semanticSource -notmatch [regex]::Escape($semanticKind)) {
        throw "Semantic Contract type or compiler is missing: $semanticKind"
    }
}
if ($semanticSource -notmatch "canonical_json" -or $semanticSource -notmatch "Sha256") {
    throw "Semantic compiler must produce canonical JSON and a SHA-256 digest"
}

$platformCoreFiles = Get-ChildItem (Join-Path $root "crates") -Recurse -File |
    Where-Object { $_.FullName -notmatch "legacy-migration-rehearsal" }
foreach ($file in $platformCoreFiles) {
    $content = Get-Content -Raw $file.FullName
    foreach ($pattern in @(
        "contract_management", "legacy-contract", "C Project", "plan-0009"
    )) {
        if ($content -match [regex]::Escape($pattern)) {
            throw "C-specific Platform Core symbol '$pattern' found in $($file.FullName)"
        }
    }
}
if (-not (Test-Path (Join-Path $root "openapi.json"))) {
    throw "PLAN-0007 public OpenAPI contract is missing"
}
& (Join-Path $PSScriptRoot "check-openapi.ps1")
if ($LASTEXITCODE -ne 0) {
    throw "OpenAPI contract fitness failed"
}
$processingPorts = Get-Content -Raw (Join-Path $root "crates/document-processing/src/ports.rs")
foreach ($requiredProcessingPort in @("ProcessingExecutionUnitOfWork", "ExecutionFence", "TextArtifactReference")) {
    if ($processingPorts -notmatch [regex]::Escape($requiredProcessingPort)) {
        throw "Processing execution port missing: $requiredProcessingPort"
    }
}

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
    $gitExitCode = $LASTEXITCODE
    if ($gitExitCode -gt 1) {
        throw "Legacy document query scan failed for: $legacySymbol"
    }
    if ($gitExitCode -eq 0 -and $matches) {
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

    function Get-BytesSha256([byte[]]$Bytes) {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            return (($sha.ComputeHash($Bytes) | ForEach-Object { $_.ToString("x2") }) -join "")
        } finally {
            $sha.Dispose()
        }
    }

    function Get-NormalizedMigrationHashes([string]$Path) {
        $bytes = [System.IO.File]::ReadAllBytes($Path)
        $lf = [System.Collections.Generic.List[byte]]::new()
        for ($index = 0; $index -lt $bytes.Length; $index++) {
            if ($bytes[$index] -eq 0x0D) {
                if ($index + 1 -lt $bytes.Length -and $bytes[$index + 1] -eq 0x0A) {
                    $index++
                }
                [void]$lf.Add(0x0A)
            } else {
                [void]$lf.Add($bytes[$index])
            }
        }
        $crlf = [System.Collections.Generic.List[byte]]::new()
        foreach ($byte in $lf) {
            if ($byte -eq 0x0A) {
                [void]$crlf.Add(0x0D)
            }
            [void]$crlf.Add($byte)
        }
        return @(
            (Get-BytesSha256 ([byte[]]$lf.ToArray())),
            (Get-BytesSha256 ([byte[]]$crlf.ToArray()))
        )
    }

    foreach ($file in $sqlFiles) {
        if (-not $entries.ContainsKey($file.Name)) {
            throw "Migration missing from manifest: $($file.Name)"
        }
        $actual = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $normalized = Get-NormalizedMigrationHashes $file.FullName
        if ($entries[$file.Name] -notin @($actual) + $normalized) {
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
    "docs/architecture/RUNTIME_AUDIT_ARCHITECTURE.md"
    "docs/architecture/DATA_INTEGRITY_AND_REPAIR_ARCHITECTURE.md"
    "docs/architecture/AUDIT_RETENTION_AND_TAMPER_EVIDENCE.md"
    "docs/standards/AUDIT_EVENT_STANDARD.md"
    "docs/standards/DATA_INTEGRITY_RULE_STANDARD.md"
    "docs/standards/CONTROLLED_REPAIR_STANDARD.md"
    "docs/adr/ADR-0013-unified-runtime-audit-model.md"
    "docs/adr/ADR-0014-data-integrity-finding-lifecycle.md"
    "docs/adr/ADR-0015-controlled-repair-and-approval.md"
    "docs/adr/ADR-0016-repair-ledger-and-verification.md"
)) {
    if (-not (Test-Path (Join-Path $root $required))) {
        throw "Required architecture entry missing: $required"
    }
}

Write-Output "Architecture fitness: PASS"
exit 0
