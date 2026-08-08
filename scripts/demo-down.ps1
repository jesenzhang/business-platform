$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

docker compose -f deploy/demo/docker-compose.yml down --remove-orphans
if ($LASTEXITCODE -ne 0) {
    throw "Demo stack failed to stop"
}
Write-Output "Demo containers stopped; named demo volumes were preserved."
