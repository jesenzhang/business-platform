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
# ADR-0023 / PLAN-0012: jarvis-model-provider is scoped solely to the ai-worker
# composition root, integrated as a vendored path dependency under vendor/.
# ai-worker must reference it via the vendored path; no other crate may declare
# it, and nothing may reintroduce it as a git dependency.
$aiWorkerCargo = Get-Content -Raw (Join-Path $root "apps/ai-worker/Cargo.toml")
if ($aiWorkerCargo -notmatch 'jarvis-model-provider\s*=\s*\{\s*path\s*=\s*"\.\./\.\./vendor/jarvis/model-provider"\s*\}') {
    throw "ai-worker must declare jarvis-model-provider as the vendored path dependency under vendor/jarvis/model-provider"
}
if ($aiWorkerCargo -match "jarvis-rs") {
    throw "ai-worker must not reference the private upstream jarvis-rs git dependency; use the vendored path"
}
if (-not (Test-Path (Join-Path $root "vendor/jarvis/model-provider/Cargo.toml"))) {
    throw "vendored model-provider crate missing: vendor/jarvis/model-provider/Cargo.toml"
}
foreach ($cratePath in @("crates/document-processing", "crates/ai-application", "apps/business-api", "crates/public-api-contracts", "crates/business-module-contracts", "crates/business-application-compiler", "crates/semantic-contract")) {
    $cargoPath = Join-Path $root "$cratePath/Cargo.toml"
    if (Test-Path $cargoPath) {
        if ((Get-Content -Raw $cargoPath) -match 'jarvis-model-provider|vendor/jarvis|jarvis-rs') {
            throw "model-provider must not be a dependency of $cratePath"
        }
    }
}
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

# `workspace = true` can hide an internal path dependency from a raw
# Cargo.toml regex. Resolve the three generic crates through Cargo metadata and
# enforce their complete internal-dependency allowlist. External registry
# dependencies have no `path` and are not part of this boundary check.
$genericDependencyAllowlist = @{
    "business-module-contracts" = @()
    "business-application-compiler" = @("business-module-contracts")
    "semantic-contract" = @("business-module-contracts")
}
$workspaceMetadata = cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw "Cargo metadata failed while validating generic crate dependencies"
}
foreach ($packageName in $genericDependencyAllowlist.Keys) {
    $package = @($workspaceMetadata.packages | Where-Object { $_.name -eq $packageName }) | Select-Object -First 1
    if ($null -eq $package) {
        throw "Missing generic crate from Cargo metadata: $packageName"
    }
    foreach ($dependency in @($package.dependencies | Where-Object { $_.path })) {
        if ($genericDependencyAllowlist[$packageName] -notcontains $dependency.name) {
            throw "$packageName has forbidden internal dependency '$($dependency.name)'"
        }
    }
}

$genericProductionSourcePaths = @(
    "crates/business-module-contracts/src",
    "crates/business-application-compiler/src",
    "crates/semantic-contract/src"
)
$genericProductionSourceFiles = foreach ($sourcePath in $genericProductionSourcePaths) {
    Get-ChildItem (Join-Path $root $sourcePath) -Recurse -File |
        Where-Object { $_.FullName -notmatch "\\(tests|fixtures)\\" }
}
$genericProductionSource = @($genericProductionSourceFiles | ForEach-Object {
    $content = Get-Content -Raw $_.FullName
    [regex]::Replace($content, '(?s)\r?\n#\[cfg\(test\)\].*$', '')
}) -join [Environment]::NewLine
foreach ($forbiddenGenericToken in @(
    "axum", "sqlx", "reqwest", "object-storage", "messaging", "ai-provider",
    "WrenAI", "wren-ai", "Twenty", "Odoo", "Frappe", "ERPNext",
    "module-a", "module-b", "module-extension",
    "contract_management", "legacy-contract", "C Project", "plan-0009",
    "PurgeBusinessData", "DeleteData", "DropBusinessFacts", "PurgeOperation"
)) {
    if ($genericProductionSource -match [regex]::Escape($forbiddenGenericToken)) {
        throw "PLAN-0011 generic production boundary violation: source contains '$forbiddenGenericToken'"
    }
}

$businessApplicationCompilerSource = @(Get-ChildItem (Join-Path $root "crates/business-application-compiler/src") -Recurse -File |
    Where-Object { $_.FullName -notmatch "\\(tests|fixtures)\\" } |
    ForEach-Object {
    $content = Get-Content -Raw $_.FullName
    [regex]::Replace($content, '(?s)\r?\n#\[cfg\(test\)\].*$', '')
}) -join [Environment]::NewLine
foreach ($forbiddenCompilerToken in @(
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

# PLAN-0014 Wave 2: the canonical module release identity contract must stay
# pinned to the SAAS_MODULE_STANDARD release identity surface, and the generic
# compiler must expose the canonical contract digest seam used to assemble it.
$requiredReleaseIdentityMembers = @(
    "ModuleReleaseIdentity", "MODULE_RELEASE_IDENTITY_SCHEMA_VERSION",
    "module_id", "module_version", "manifest_schema_version",
    "source_revision", "contract_digest", "package_digest",
    "migration_revision", "build_toolchain",
    "ModuleReleaseBundle", "crate_versions"
)
foreach ($member in $requiredReleaseIdentityMembers) {
    if ($moduleManifestSource -notmatch [regex]::Escape($member)) {
        throw "Module release identity member is missing: $member"
    }
}
if ($businessApplicationCompilerSource -notmatch 'pub fn contract_digest') {
    throw "business-application-compiler must expose the canonical contract digest seam"
}
if ($businessApplicationCompilerSource -notmatch 'pub fn package_fingerprint') {
    throw "business-application-compiler must expose the compiled package fingerprint seam"
}
if ($businessApplicationCompilerSource -notmatch 'pub fn verify_release_identity') {
    throw "business-application-compiler must expose the release identity consistency verification seam"
}

# PLAN-0014 Wave 5A boundaries. The release-candidate tool assembles and verifies
# local artifacts; it must do that by reusing the pure seams above, never by
# widening them, reaching into business crates / adapters / applications, or
# letting the host environment into a digest.
$releaseToolCargo = Get-Content -Raw (Join-Path $root "tools/module-release-tool/Cargo.toml")
foreach ($forbidden in @(
    "apps/", "runtime-migration", "identity-sqlite", "policy-sqlite",
    "document", "sqlx", "axum", "tokio", "reqwest", "tracing", "object-storage"
)) {
    if ($releaseToolCargo -match [regex]::Escape($forbidden)) {
        throw "module-release-tool must not depend on '$forbidden'"
    }
}
foreach ($required in @("business-module-contracts", "business-application-compiler")) {
    if ($releaseToolCargo -notmatch [regex]::Escape($required)) {
        throw "module-release-tool must reuse the '$required' seam instead of re-deriving digests"
    }
}
Assert-NotContains "tools/module-release-tool/src" @(
    "SystemTime",
    "Instant::now",
    "chrono",
    "COMPUTERNAME",
    "gethostname",
    "localtime"
) "module release tool host-dependency violation (a release digest may not depend on the clock, the host or a subprocess)"
# Deliberate, narrowed exception (PLAN-0014 Wave 5A repair 1, review F1): source
# provenance has to be answered by Git, not by re-implementing `.git/HEAD`,
# `refs/` and `packed-refs`, because a hand-written reader silently disagrees with
# Git for a linked worktree, a detached HEAD or a ref store it does not know.
# So exactly one file may spawn anything, it may spawn exactly one program, and it
# may only pass arguments as an array — never a shell command line.
foreach ($toolSource in Get-ChildItem -Path (Join-Path $root "tools/module-release-tool/src") -Filter '*.rs' -File) {
    $text = Get-Content -Raw $toolSource.FullName
    if (($text -match 'std::process::Command' -or $text -match 'Command::new\(') -and $toolSource.Name -ne 'git.rs') {
        throw "only tools/module-release-tool/src/git.rs may spawn a process; '$($toolSource.Name)' does"
    }
    if ($text -match 'std::fs::remove_dir_all' -and $toolSource.Name -ne 'tempdir.rs') {
        throw "only the tempdir helper may delete a directory tree; '$($toolSource.Name)' does"
    }
}
$gitSource = Get-Content -Raw (Join-Path $root "tools/module-release-tool/src/git.rs")
foreach ($spawned in @('Command::new("cmd")', 'Command::new("powershell")', 'Command::new("pwsh")', 'Command::new("sh")')) {
    if ($gitSource -match [regex]::Escape($spawned)) {
        throw "provenance must not invoke a shell: '$spawned'"
    }
}
if ($gitSource -notmatch 'Command::new\("git"\)') {
    throw 'provenance may only spawn git, as Command::new("git")'
}
if ($gitSource -notmatch 'args\(' -or $gitSource -notmatch '--no-optional-locks') {
    throw "the git invocation must pass an argument array and --no-optional-locks (read-only, no lock files)"
}
if ($gitSource -notmatch 'fn blob' -or $gitSource -notmatch 'cat-file') {
    throw "provenance must read committed content through git (`cat-file blob`), not through the working tree"
}
# The wave must not have made the pure crates impure in the direction of this
# repository: they may not learn the tool's name, spawn processes or read time.
foreach ($pureCrate in @("crates/business-module-contracts/src", "crates/business-application-compiler/src")) {
    Assert-NotContains $pureCrate @(
        "module-release-tool",
        "std::process",
        "SystemTime",
        "Instant::now"
    ) "PLAN-0014 Wave 5A purity violation in $pureCrate"
}
# The consumer templates are deliberately outside the workspace member set, and
# may only ever point at an artifact directory substituted by the script.
$workspaceCargo = Get-Content -Raw (Join-Path $root "Cargo.toml")
if ($workspaceCargo -notmatch 'tools/module-release-tool/consumers') {
    throw "the workspace must exclude tools/module-release-tool/consumers from its member set"
}
foreach ($pilotModule in @("identity", "policy")) {
    $consumerManifestPath = Join-Path $root "tools/module-release-tool/consumers/$pilotModule/Cargo.toml"
    if (-not (Test-Path $consumerManifestPath)) {
        throw "Missing Wave 5A consumer template for '$pilotModule'"
    }
    $consumerManifest = Get-Content -Raw $consumerManifestPath
    if ($consumerManifest -notmatch '(?m)^\[workspace\]') {
        throw "Wave 5A consumer '$pilotModule' must carry its own empty [workspace] table"
    }
    if ($consumerManifest -match '\.\./') {
        throw "Wave 5A consumer '$pilotModule' must not use a relative path into the workspace"
    }
    $pathLines = @($consumerManifest -split "`n" | Where-Object { $_ -match 'path\s*=' })
    if ($pathLines.Count -ne 1 -or $pathLines[0] -notmatch '__ARTIFACT_CRATE_DIR__') {
        throw "Wave 5A consumer '$pilotModule' must depend on exactly the artifact directory placeholder"
    }
    $consumerTest = Join-Path $root "tools/module-release-tool/consumers/$pilotModule/tests/public_contract.rs"
    if (-not (Test-Path $consumerTest)) {
        throw "Wave 5A consumer '$pilotModule' must actually call a published contract"
    }
}
# Consumer isolation is decided by the resolved dependency graph, in exactly one
# place: `module-release guard` (tools/module-release-tool/src/isolation.rs), fed
# by `cargo metadata`. A PowerShell-side approximation — searching a manifest or a
# lock file for a path — was the Wave 5A defect (review F4): it missed a
# forward-slash path, a relative path and any transitive path dependency. The
# orchestration script therefore has to delegate, and may not keep its own rule.
$wave5aScriptFiles = @(
    "scripts/plan0014-wave5a.ps1",
    "scripts/lib/plan0014-wave5a-lib.ps1",
    "scripts/test-plan0014-wave5a.ps1"
)
foreach ($wave5aScriptFile in $wave5aScriptFiles) {
    $wave5aScriptPath = Join-Path $root $wave5aScriptFile
    if (-not (Test-Path $wave5aScriptPath)) {
        throw "Missing Wave 5A orchestration file: $wave5aScriptFile"
    }
}
$wave5aScript = ''
foreach ($wave5aScriptFile in @("scripts/plan0014-wave5a.ps1", "scripts/lib/plan0014-wave5a-lib.ps1")) {
    $wave5aScript += Get-Content -Raw (Join-Path $root $wave5aScriptFile)
}
if ($wave5aScript -notmatch "'metadata'" -or $wave5aScript -notmatch "'--format-version'" -or $wave5aScript -notmatch "'--locked'" -or $wave5aScript -notmatch "'--offline'") {
    throw "the Wave 5A orchestration must resolve the consumer graph with cargo metadata (--format-version 1 --locked --offline)"
}
if ($wave5aScript -notmatch "'guard'" -or $wave5aScript -notmatch "'--metadata'") {
    throw "the Wave 5A orchestration must judge isolation through 'module-release guard --metadata …'"
}
foreach ($ownRule in @('-like\s*"\$\{?repo', '\.StartsWith\(', 'Select-String.*Cargo\.lock')) {
    if ($wave5aScript -match $ownRule) {
        throw "the Wave 5A orchestration re-implements path containment ('/$ownRule'); isolation belongs in tools/module-release-tool/src/isolation.rs"
    }
}
# Pilot declarations: one file per module, ids that agree with the file name, a
# module version of its own (never the platform version), and persistence that
# agrees with the migration sets it declares.
$platformVersion = $null
if ($workspaceCargo -match '(?ms)\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
    $platformVersion = $Matches[1]
}
if (-not $platformVersion) {
    throw "Architecture fitness could not read the platform version from the workspace manifest"
}
foreach ($pilotModule in @("identity", "policy")) {
    $specPath = Join-Path $root "tools/module-release-tool/specs/$pilotModule.json"
    if (-not (Test-Path $specPath)) {
        throw "Missing Wave 5A release declaration: tools/module-release-tool/specs/$pilotModule.json"
    }
    $declaration = Get-Content -Raw $specPath | ConvertFrom-Json
    if ($declaration.module_id -ne $pilotModule) {
        throw "Release declaration '$pilotModule' declares module_id '$($declaration.module_id)'"
    }
    if ($declaration.module_version -eq $platformVersion) {
        throw "Release declaration '$pilotModule' must carry its own version, not the platform version"
    }
    if ([bool]$declaration.persistent -ne ($declaration.migration_sets.Count -gt 0)) {
        throw "Release declaration '$pilotModule' claims persistence without migration sets, or the reverse"
    }
    if ($declaration.persistent -and -not $declaration.migration_namespace) {
        throw "Release declaration '$pilotModule' is persistent but declares no migration namespace"
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
Assert-MigrationManifest "crates/identity-sqlite/migrations" "crates/identity-sqlite/migrations/MANIFEST.sha256"
Assert-MigrationManifest "crates/organization-sqlite/migrations" "crates/organization-sqlite/migrations/MANIFEST.sha256"
Assert-MigrationManifest "crates/policy-sqlite/migrations" "crates/policy-sqlite/migrations/MANIFEST.sha256"

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

# PLAN-0013 §15 source scans (regex, case-sensitive unless noted).
function Assert-SourceScan {
    param(
        [string]$Label,
        [string[]]$Directories,
        [string[]]$Patterns,
        [string[]]$PathExcludes = @()
    )
    foreach ($directory in $Directories) {
        $target = Join-Path $root $directory
        if (-not (Test-Path $target)) {
            continue
        }
        foreach ($file in Get-ChildItem $target -Recurse -Include "*.rs" -File) {
            $normalized = $file.FullName.Replace("\", "/")
            $skip = $false
            foreach ($exclude in $PathExcludes) {
                if ($normalized -match $exclude) {
                    $skip = $true
                    break
                }
            }
            if ($skip) {
                continue
            }
            $content = Get-Content -Raw $file.FullName
            foreach ($pattern in $Patterns) {
                if ($content -match $pattern) {
                    throw "${Label}: $($file.FullName) matches forbidden pattern '$pattern'"
                }
            }
        }
    }
}

# Rule 1+4: business modules and HTTP handlers never touch the RoleBinding
# table or parse role strings — RoleBinding is read only through Policy ports.
# Adding a business-module crate requires extending this directory list;
# an omitted crate is never scanned (silent fail-open). Keep it in sync
# with the IAM forbidden-business-dependency list in
# crates/architecture-check/src/lib.rs.
Assert-SourceScan -Label "IAM role-authority scan" `
    -Directories @(
        "crates/contract", "crates/customer", "crates/finance",
        "crates/project", "crates/approval", "crates/workflow",
        "crates/agent-integration", "crates/notification",
        "crates/document", "crates/document-sqlite",
        "crates/document-postgres", "crates/document-persistence-contracts",
        "crates/document-processing", "crates/document-processing-sqlite",
        "crates/document-processing-postgres", "crates/document-processing-contracts",
        "apps/business-api/src/routes"
    ) `
    -Patterns @("role_bindings", "roles\(\)\s*\.\s*contains")

# Rule 4 (preflight §11): no authorization-server markers. Signing keys and
# JWT minting belong to the external IdP, never to this repository; OIDC
# validation (decode/verify) stays allowed. Tests may mint tokens in-process.
Assert-SourceScan -Label "Authorization-server scan" `
    -Directories @("crates", "apps") `
    -Patterns @("SigningKey", "jsonwebtoken::encode\b") `
    -PathExcludes @("/tests/", "/oidc", "oidc_", "/vendor/")

# Rule 5 (preflight §11): no policy DSL — bounded enum evaluation only.
Assert-SourceScan -Label "Policy DSL scan" `
    -Directories @("crates/policy/src") `
    -Patterns @("parse_expression", "evaluate_expression", "Expression\s*\{")

Write-Output "Architecture fitness: PASS"
exit 0
