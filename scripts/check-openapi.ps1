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
    "/api/v1/admin/audit-events",
    "/api/v1/admin/users",
    "/api/v1/admin/users/{userId}",
    "/api/v1/admin/tenant-memberships",
    "/api/v1/admin/tenant-memberships/{userId}/suspend",
    "/api/v1/admin/tenant-memberships/{userId}/reactivate",
    "/api/v1/admin/permissions",
    "/api/v1/admin/roles",
    "/api/v1/admin/roles/{roleId}",
    "/api/v1/admin/roles/{roleId}/permissions",
    "/api/v1/admin/role-bindings",
    "/api/v1/admin/role-bindings/{bindingId}/revoke",
    "/api/v1/admin/organization-units",
    "/api/v1/admin/organization-units/{unitId}",
    "/api/v1/admin/organization-units/{unitId}/move",
    "/api/v1/admin/organization-units/{unitId}/members",
    "/api/v1/admin/organization-units/{unitId}/members/{userId}",
    "/api/v1/admin/authorization/explain"
)
foreach ($requiredPath in $requiredPaths) {
    if ($null -eq $document.paths.$requiredPath) {
        throw "OpenAPI path missing: $requiredPath"
    }
}

# IAM management surface contract pins (PLAN-0013 Stage 8 review):
# every management operation documents its default-deny 403, and the
# binding view keeps its audit timestamps.
foreach ($op in $document.paths.PSObject.Properties | Where-Object {
    $_.Name -like "/api/v1/admin/users*" -or $_.Name -like "/api/v1/admin/tenant-memberships*" -or
    $_.Name -like "/api/v1/admin/permissions*" -or $_.Name -like "/api/v1/admin/roles*" -or
    $_.Name -like "/api/v1/admin/role-bindings*" -or $_.Name -like "/api/v1/admin/organization-units*" -or
    $_.Name -like "/api/v1/admin/authorization*"
}) {
    foreach ($method in $op.Value.PSObject.Properties) {
        if ($null -eq $method.Value.responses."403") {
            throw "IAM management operation missing 403: $($op.Name) $($method.Name)"
        }
    }
}
foreach ($timestampField in @("created_at", "updated_at")) {
    if ($document.components.schemas.RoleBindingView.required -notcontains $timestampField) {
        throw "RoleBindingView must require $timestampField"
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
