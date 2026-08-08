$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

docker compose -f deploy/demo/docker-compose.yml down --volumes --remove-orphans
if ($LASTEXITCODE -ne 0) {
    throw "Demo stack failed to reset"
}
Write-Output "Demo containers and only the demo PostgreSQL/MinIO volumes were removed."
