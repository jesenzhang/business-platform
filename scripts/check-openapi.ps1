$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$path = Join-Path $root "openapi.json"
if (-not (Test-Path -LiteralPath $path)) {
    throw "openapi.json is missing"
}

$document = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
if ($document.openapi -ne "3.1.0") {
    throw "OpenAPI document must use 3.1.0"
}
if ($document.info.version -ne "v1") {
    throw "Public API contract version must remain v1"
}

$requiredPaths = @(
    "/api/v1/documents/upload",
    "/api/v1/processing-jobs",
    "/api/v1/documents/{documentId}/processing-jobs",
    "/api/v1/operations/overview",
    "/api/v1/admin/integrity/findings",
    "/api/v1/admin/audit-events"
)
foreach ($requiredPath in $requiredPaths) {
    if ($null -eq $document.paths.$requiredPath) {
        throw "OpenAPI path missing: $requiredPath"
    }
}

$serialized = $document | ConvertTo-Json -Depth 100
foreach ($forbiddenField in @("object_key", "storage_key", "bucket", "internal_path", "password", "secret_key")) {
    if ($serialized.Contains("`"$forbiddenField`"")) {
        throw "OpenAPI exposes forbidden internal field: $forbiddenField"
    }
}
if ($null -eq $document.components.securitySchemes.bearerAuth) {
    throw "Bearer authentication scheme is missing"
}

Write-Output "OpenAPI contract: PASS"
