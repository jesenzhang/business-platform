$ErrorActionPreference = "Stop"

Write-Host "Running local Runtime Governance repair contracts..."
cargo test -p governance-worker --all-features --test repair_worker
if ($LASTEXITCODE -ne 0) {
    throw "Governance repair worker contract failed"
}

cargo test -p governance-worker --all-features --test sqlite_governance
if ($LASTEXITCODE -ne 0) {
    throw "SQLite Governance scan/repair E2E failed"
}

# The processing SQLite contract exercises the same BEGIN IMMEDIATE single
# writer profile used by local Governance persistence.
cargo test -p document-processing-sqlite --all-features --tests
if ($LASTEXITCODE -ne 0) {
    throw "SQLite processing durability contract failed"
}

Write-Host "Local Governance repair E2E: PASS"
