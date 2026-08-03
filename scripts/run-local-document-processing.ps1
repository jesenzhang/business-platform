[CmdletBinding()]
param(
    [string]$DataDirectory = "",
    [int]$Port = 3300,
    [switch]$KeepProcesses
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$ownedDirectory = $false
if ([string]::IsNullOrWhiteSpace($DataDirectory)) {
    $DataDirectory = Join-Path ([IO.Path]::GetTempPath()) ("business-platform-processing-" + [Guid]::NewGuid().ToString("N"))
    $ownedDirectory = $true
}
$DataDirectory = [IO.Path]::GetFullPath($DataDirectory)
New-Item -ItemType Directory -Force -Path $DataDirectory | Out-Null
$storageDirectory = Join-Path $DataDirectory "objects"
New-Item -ItemType Directory -Force -Path $storageDirectory | Out-Null
$databasePath = Join-Path $DataDirectory "document-processing.db"
$databaseUrl = "sqlite:///$($databasePath.Replace('\', '/'))?mode=rwc"
$logDirectory = Join-Path $DataDirectory "logs"
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null

function Stop-Child([System.Diagnostics.Process]$Process) {
    if ($null -ne $Process -and -not $Process.HasExited) {
        $Process.CloseMainWindow() | Out-Null
        if (-not $Process.WaitForExit(5000)) {
            $Process.Kill()
        }
    }
}

Push-Location $repo
try {
    $env:MIGRATION__DATABASE__URL = $databaseUrl
    cargo run -p migration --quiet -- --backend sqlite up
    if ($LASTEXITCODE -ne 0) { throw "SQLite migration failed" }

    $env:BUSINESS_API__ENV = "development"
    $env:BUSINESS_API__SERVER__HOST = "127.0.0.1"
    $env:BUSINESS_API__SERVER__PORT = "$Port"
    $env:BUSINESS_API__DATABASE__BACKEND = "sqlite"
    $env:BUSINESS_API__DATABASE__URL = $databaseUrl
    $env:BUSINESS_API__DATABASE__MAX_CONNECTIONS = "1"
    $env:BUSINESS_API__DATABASE__MIN_CONNECTIONS = "1"
    $env:BUSINESS_API__AUTH__DEV_AUTH_ENABLED = "true"
    $env:BUSINESS_API__AUTH__DEV_SECRET = "local-processing-only"
    $env:BUSINESS_WORKER__ENV = "development"
    $env:BUSINESS_WORKER__DATABASE__BACKEND = "sqlite"
    $env:BUSINESS_WORKER__DATABASE__URL = $databaseUrl
    $env:BUSINESS_WORKER__STORAGE__BACKEND = "local"
    $env:BUSINESS_WORKER__STORAGE__BASE_DIR = $storageDirectory
    $env:BUSINESS_WORKER__CONCURRENCY = "1"
    $env:BUSINESS_WORKER__AI_MODE = "inline"
    $env:RUST_LOG = "info"

    $apiLog = Join-Path $logDirectory "business-api.log"
    $workerLog = Join-Path $logDirectory "business-worker.log"
    $apiErrorLog = Join-Path $logDirectory "business-api.error.log"
    $workerErrorLog = Join-Path $logDirectory "business-worker.error.log"
    $api = Start-Process cargo -ArgumentList @("run", "-p", "business-api", "--quiet") -WorkingDirectory $repo -RedirectStandardOutput $apiLog -RedirectStandardError $apiErrorLog -PassThru -WindowStyle Hidden
    $worker = Start-Process cargo -ArgumentList @("run", "-p", "business-worker", "--quiet") -WorkingDirectory $repo -RedirectStandardOutput $workerLog -RedirectStandardError $workerErrorLog -PassThru -WindowStyle Hidden
    Write-Output "SQLite processing runtime started."
    Write-Output "database=$databasePath"
    Write-Output "storage=$storageDirectory"
    Write-Output "api=http://127.0.0.1:$Port"
    Write-Output "logs=$logDirectory"

    if (-not $KeepProcesses) {
        Write-Output "Press Ctrl+C to stop the local runtime."
        try {
            while (-not $api.HasExited -and -not $worker.HasExited) {
                Start-Sleep -Seconds 1
            }
        } finally {
            Stop-Child $api
            Stop-Child $worker
        }
    }
} finally {
    Remove-Item Env:MIGRATION__DATABASE__URL -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__ENV -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__SERVER__HOST -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__SERVER__PORT -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__DATABASE__BACKEND -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__DATABASE__URL -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__DATABASE__MAX_CONNECTIONS -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__DATABASE__MIN_CONNECTIONS -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__AUTH__DEV_AUTH_ENABLED -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_API__AUTH__DEV_SECRET -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__ENV -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__DATABASE__BACKEND -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__DATABASE__URL -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__STORAGE__BACKEND -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__STORAGE__BASE_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__CONCURRENCY -ErrorAction SilentlyContinue
    Remove-Item Env:BUSINESS_WORKER__AI_MODE -ErrorAction SilentlyContinue
    Remove-Item Env:RUST_LOG -ErrorAction SilentlyContinue
    Pop-Location
    if ($ownedDirectory -and (Test-Path -LiteralPath $DataDirectory)) {
        Remove-Item -LiteralPath $DataDirectory -Recurse -Force
    }
}
