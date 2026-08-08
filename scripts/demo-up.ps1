$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$envFile = if (Test-Path -LiteralPath "deploy/demo/.env") { "deploy/demo/.env" } else { "deploy/demo/.env.example" }
docker compose -f deploy/demo/docker-compose.yml --env-file $envFile up -d --build
if ($LASTEXITCODE -ne 0) {
    throw "Demo stack failed to start"
}

$deadline = (Get-Date).AddMinutes(3)
do {
    try {
        $ready = Invoke-RestMethod -Uri "http://localhost:3000/health/ready" -TimeoutSec 5
        if ($ready.status -eq "ok" -or $ready.data.status -eq "ok") {
            break
        }
    } catch {
        # Services are expected to be unavailable while images start and migrations run.
    }
    Start-Sleep -Seconds 3
} while ((Get-Date) -lt $deadline)

if ((Get-Date) -ge $deadline) {
    throw "Business API did not become ready; inspect: docker compose -f deploy/demo/docker-compose.yml logs"
}

Write-Output "Business Platform demo is ready."
Write-Output "Web URL:       http://localhost:4173"
Write-Output "REST API URL:  http://localhost:3000"
Write-Output "OpenAPI:       http://localhost:3000/openapi.json"
Write-Output "MCP URL:       http://localhost:3100/mcp"
Write-Output "MCP token:     mcp-demo-token"
Write-Output "CLI example:   cargo run -p business-cli -- --api-url http://localhost:3000 --token dev-only-secret documents list --json"
