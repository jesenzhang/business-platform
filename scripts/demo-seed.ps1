$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$apiUrl = "http://localhost:3000"
$token = if ($env:BUSINESS_API_DEV_TOKEN) { $env:BUSINESS_API_DEV_TOKEN } else { "dev-only-secret" }
$headers = @{ Authorization = "Bearer $token"; Accept = "application/json" }
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("business-platform-demo-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tempRoot | Out-Null

function Get-Data($response) {
    if ($null -ne $response.data) { return $response.data }
    return $response
}

function Upload-Document([string]$name, [string]$content, [string]$contentType) {
    $path = Join-Path $tempRoot $name
    [System.IO.File]::WriteAllText($path, $content, [System.Text.UTF8Encoding]::new($false))
    $key = "demo-$name"
    $form = @{ file = Get-Item -LiteralPath $path }
    return Get-Data (Invoke-RestMethod -Method Post -Uri "$apiUrl/api/v1/documents/upload" -Headers ($headers + @{ "Idempotency-Key" = $key }) -Form $form)
}

function Start-Processing($document) {
    $body = @{ content_revision = $document.content_revision } | ConvertTo-Json
    return Get-Data (Invoke-RestMethod -Method Post -Uri "$apiUrl/api/v1/documents/$($document.id)/processing-jobs" -Headers ($headers + @{ "Idempotency-Key" = "demo-job-$($document.id)"; "Content-Type" = "application/json" }) -Body $body)
}

function Wait-ForCandidate([string]$jobId) {
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        $job = Get-Data (Invoke-RestMethod -Method Get -Uri "$apiUrl/api/v1/processing-jobs/$jobId" -Headers $headers)
        if ($job.candidate_available -or $job.status -in @("waiting_for_review", "succeeded")) { return $job }
        if ($job.status -eq "failed") { return $job }
        Start-Sleep -Seconds 2
    }
    throw "Timed out waiting for processing job $jobId"
}

try {
    $acceptedDocument = Upload-Document "completed.txt" "Accepted demo document`nInvoice number: DEMO-001`n" "text/plain"
    $acceptedJob = Start-Processing $acceptedDocument
    $acceptedJob = Wait-ForCandidate $acceptedJob.job_id
    if ($acceptedJob.candidate_available) {
        $candidate = Get-Data (Invoke-RestMethod -Method Get -Uri "$apiUrl/api/v1/processing-jobs/$($acceptedJob.job_id)/candidate" -Headers $headers)
        $review = @{ decision = "accepted"; candidate_version = $candidate.version; comment = "Deterministic demo review" } | ConvertTo-Json
        Invoke-RestMethod -Method Post -Uri "$apiUrl/api/v1/processing-jobs/$($acceptedJob.job_id)/review" -Headers ($headers + @{ "Idempotency-Key" = "demo-review-$($acceptedJob.job_id)"; "Content-Type" = "application/json" }) -Body $review | Out-Null
    }

    $processingDocument = Upload-Document "processing.txt" "Processing demo document`nThis job intentionally remains awaiting review.`n" "text/plain"
    $processingJob = Start-Processing $processingDocument

    $failedDocument = Upload-Document "failed.pdf" "This deterministic PDF fixture is intentionally unsupported by the text extractor." "application/pdf"
    $failedJob = Start-Processing $failedDocument
    $failedJob = Wait-ForCandidate $failedJob.job_id

    Write-Output "Seeded deterministic demo data through REST:"
    Write-Output "  accepted document:  $($acceptedDocument.id) / job $($acceptedJob.job_id)"
    Write-Output "  processing document: $($processingDocument.id) / job $($processingJob.job_id)"
    Write-Output "  failed document:     $($failedDocument.id) / job $($failedJob.job_id)"
    Write-Output "The dashboard, audit pages, and MCP overview now use the same tenant-scoped facts."
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
