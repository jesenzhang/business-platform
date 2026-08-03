[CmdletBinding()]
param(
    [int]$Port = 0
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$isolated = Join-Path ([IO.Path]::GetTempPath()) ("business-platform-processing-test-" + [Guid]::NewGuid().ToString("N"))
$storageDirectory = Join-Path $isolated "objects"
$databasePath = Join-Path $isolated "processing.db"
$logDirectory = Join-Path $isolated "logs"
$databaseUrl = "sqlite:///$($databasePath.Replace('\', '/'))?mode=rwc"
$targetDirectory = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $repo "target"
} else {
    $env:CARGO_TARGET_DIR
}
$debugDirectory = Join-Path $targetDirectory "debug"
$api = $null
$worker = $null
$crashObserved = $false

if ($Port -eq 0) {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $Port = $listener.LocalEndpoint.Port
    $listener.Stop()
}

New-Item -ItemType Directory -Force -Path $storageDirectory, $logDirectory | Out-Null

function Stop-ProcessTree([System.Diagnostics.Process]$Process) {
    if ($null -eq $Process) {
        return
    }
    try {
        $rootId = $Process.Id
        $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $rootId")
        foreach ($child in $children) {
            Stop-ProcessTree (Get-Process -Id $child.ProcessId -ErrorAction SilentlyContinue)
        }
        if (-not $Process.HasExited) {
            Stop-Process -Id $rootId -Force -ErrorAction SilentlyContinue
        }
    } catch {
        # Cleanup is best effort for already-exited cargo/binary children.
    }
}

function Start-LocalProcesses {
    $apiLog = Join-Path $logDirectory "business-api.log"
    $workerLog = Join-Path $logDirectory "business-worker.log"
    $apiErrorLog = Join-Path $logDirectory "business-api.error.log"
    $workerErrorLog = Join-Path $logDirectory "business-worker.error.log"
    $script:api = Start-Process (Join-Path $debugDirectory "business-api.exe") -WorkingDirectory $repo -RedirectStandardOutput $apiLog -RedirectStandardError $apiErrorLog -PassThru -WindowStyle Hidden
    $script:worker = Start-Process (Join-Path $debugDirectory "business-worker.exe") -WorkingDirectory $repo -RedirectStandardOutput $workerLog -RedirectStandardError $workerErrorLog -PassThru -WindowStyle Hidden
}

function Wait-Ready([string]$BaseUrl) {
    for ($attempt = 0; $attempt -lt 90; $attempt++) {
        if ($api.HasExited -or $worker.HasExited) {
            throw "local API or worker exited during startup"
        }
        try {
            $ready = Invoke-RestMethod -Method Get -Uri "$BaseUrl/health/ready" -TimeoutSec 2
            if ($ready.status -eq "ready") {
                return
            }
        } catch {
            # Cargo may still be compiling or the API may be applying startup work.
        }
        Start-Sleep -Milliseconds 500
    }
    throw "local API did not become ready"
}

function Wait-WorkerReady {
    for ($attempt = 0; $attempt -lt 180; $attempt++) {
        if ($worker.HasExited) {
            throw "local worker exited during startup"
        }
        $stdout = if (Test-Path -LiteralPath (Join-Path $logDirectory "business-worker.log")) {
            Get-Content -LiteralPath (Join-Path $logDirectory "business-worker.log") -Raw
        } else { "" }
        $stderr = if (Test-Path -LiteralPath (Join-Path $logDirectory "business-worker.error.log")) {
            Get-Content -LiteralPath (Join-Path $logDirectory "business-worker.error.log") -Raw
        } else { "" }
        if ($stdout -match "business-worker ready" -or $stderr -match "business-worker ready") {
            return
        }
        Start-Sleep -Milliseconds 500
    }
    throw "local worker did not become ready"
}

function Get-Job([string]$BaseUrl, [hashtable]$Headers, [string]$JobId) {
    (Invoke-RestMethod -Method Get -Uri "$BaseUrl/api/v1/processing-jobs/$JobId" -Headers $Headers -TimeoutSec 5).data
}

Push-Location $repo
try {
    # The script owns only this unique temporary directory and never opens or
    # removes a caller-supplied database.
    $env:MIGRATION__DATABASE__URL = $databaseUrl
    cargo run -p migration --quiet -- --backend sqlite up
    if ($LASTEXITCODE -ne 0) { throw "SQLite migration failed" }
    cargo build --quiet -p business-api -p business-worker
    if ($LASTEXITCODE -ne 0) { throw "local API/worker build failed" }

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
    $env:BUSINESS_WORKER__TEST_STEP_DELAY_MILLIS = "1000"
    $env:RUST_LOG = "info"

    Start-LocalProcesses
    $baseUrl = "http://127.0.0.1:$Port"
    Wait-Ready $baseUrl
    Wait-WorkerReady

    $tenant = [Guid]::NewGuid().ToString()
    $user = [Guid]::NewGuid().ToString()
    $headers = @{
        Authorization = "Bearer local-processing-only"
        "X-Tenant-Id" = $tenant
        "X-User-Id" = $user
    }
    $documentHeaders = $headers.Clone()
    $documentHeaders["Idempotency-Key"] = "local-document-$([Guid]::NewGuid().ToString('N'))"
    $sourceText = "Local processing title`n" + ("durable recovery padding`n" * 250000)
    $sourceBytes = [Text.Encoding]::UTF8.GetBytes($sourceText)
    $documentBody = @{
        original_filename = "processing.txt"
        content_type = "text/plain"
        object_key = "incoming/processing.txt"
        size_bytes = $sourceBytes.Length
    } | ConvertTo-Json -Compress
    $document = (Invoke-RestMethod -Method Post -Uri "$baseUrl/api/v1/documents" -Headers $documentHeaders -ContentType "application/json" -Body $documentBody -TimeoutSec 5).data
    $documentId = $document.id
    $objectPath = Join-Path $storageDirectory ("tenants/$tenant/documents/$documentId/v1/incoming/processing.txt")
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $objectPath) | Out-Null
    [IO.File]::WriteAllText($objectPath, $sourceText, [Text.UTF8Encoding]::new($false))

    $jobHeaders = $headers.Clone()
    $jobHeaders["Idempotency-Key"] = "local-job-$([Guid]::NewGuid().ToString('N'))"
    $jobBody = @{ content_revision = 1 } | ConvertTo-Json -Compress
    $job = (Invoke-RestMethod -Method Post -Uri "$baseUrl/api/v1/documents/$documentId/processing-jobs" -Headers $jobHeaders -ContentType "application/json" -Body $jobBody -TimeoutSec 5).data
    $jobId = $job.job_id

    $current = $null
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        $current = Get-Job $baseUrl $headers $jobId
        if ($current.status -eq "running") {
            $crashObserved = $true
            Stop-ProcessTree $worker
            Stop-ProcessTree $api
            $worker = $null
            $api = $null
            Start-LocalProcesses
            Wait-Ready $baseUrl
            Wait-WorkerReady
            break
        }
        if ($current.status -in @("waiting_for_review", "failed", "cancelled", "rejected")) {
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $crashObserved) {
        Write-Output "Local process E2E diagnostic: latest job status was $($current.status)"
        if (Test-Path -LiteralPath (Join-Path $logDirectory "business-worker.error.log")) {
            Get-Content -LiteralPath (Join-Path $logDirectory "business-worker.error.log")
        }
        if (Test-Path -LiteralPath (Join-Path $logDirectory "business-worker.log")) {
            Get-Content -LiteralPath (Join-Path $logDirectory "business-worker.log")
        }
        throw "did not observe a running job before worker crash"
    }

    $current = $null
    for ($attempt = 0; $attempt -lt 90; $attempt++) {
        $current = Get-Job $baseUrl $headers $jobId
        if ($current.status -eq "waiting_for_review") { break }
        if ($current.status -in @("failed", "cancelled", "rejected")) {
            throw "processing job reached unexpected status: $($current.status)"
        }
        Start-Sleep -Milliseconds 500
    }
    if ($null -eq $current -or $current.status -ne "waiting_for_review") {
        throw "processing job did not reach waiting_for_review"
    }

    $candidate = (Invoke-RestMethod -Method Get -Uri "$baseUrl/api/v1/processing-jobs/$jobId/candidate" -Headers $headers -TimeoutSec 5).data
    if ($candidate.payload.title -ne "Local processing title") {
        throw "deterministic candidate title mismatch"
    }
    $reviewBody = @{
        decision = "accepted"
        candidate_version = $candidate.version
    } | ConvertTo-Json -Compress
    $reviewHeaders = $headers.Clone()
    $reviewHeaders["Idempotency-Key"] = "local-review-$([Guid]::NewGuid().ToString('N'))"
    Invoke-RestMethod -Method Post -Uri "$baseUrl/api/v1/processing-jobs/$jobId/review" -Headers $reviewHeaders -ContentType "application/json" -Body $reviewBody -TimeoutSec 5 | Out-Null
    $current = Get-Job $baseUrl $headers $jobId
    if ($current.status -ne "succeeded") {
        throw "accepted review did not complete the job"
    }

    # Restart both owned processes and prove the terminal state remains durable.
    Stop-ProcessTree $worker
    Stop-ProcessTree $api
    $worker = $null
    $api = $null
    Start-LocalProcesses
    Wait-Ready $baseUrl
    Wait-WorkerReady
    $afterRestart = Get-Job $baseUrl $headers $jobId
    if ($afterRestart.status -ne "succeeded") {
        throw "job state was not durable across process restart"
    }

    Write-Output "Local document processing process E2E: PASS"
    Write-Output "SQLite running-step crash recovery: PASS"
    Write-Output "SQLite database and object storage were isolated under: $isolated"
} finally {
    Stop-ProcessTree $worker
    Stop-ProcessTree $api
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
    Remove-Item Env:BUSINESS_WORKER__TEST_STEP_DELAY_MILLIS -ErrorAction SilentlyContinue
    Remove-Item Env:RUST_LOG -ErrorAction SilentlyContinue
    Pop-Location
    if (Test-Path -LiteralPath $isolated) {
        Remove-Item -LiteralPath $isolated -Recurse -Force
    }
}
