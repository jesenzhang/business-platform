-- PLAN-0005 Runtime Governance SQLite profile.
CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT,
    trace_id TEXT,
    created_at TEXT NOT NULL,
    operation_id TEXT,
    actor_type TEXT NOT NULL DEFAULT 'user',
    actor_id TEXT,
    correlation_id TEXT,
    causation_id TEXT,
    reason TEXT,
    result TEXT NOT NULL DEFAULT 'succeeded',
    failure_code TEXT,
    before_hash TEXT,
    after_hash TEXT,
    changed_fields TEXT NOT NULL DEFAULT '[]',
    schema_version TEXT NOT NULL DEFAULT 'audit.v1',
    previous_hash TEXT,
    record_hash TEXT,
    occurred_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_tenant_occurred_id
    ON audit_events (tenant_id, occurred_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor_time
    ON audit_events (tenant_id, actor_type, actor_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_resource_time
    ON audit_events (tenant_id, resource_type, resource_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_operation ON audit_events (operation_id);
CREATE INDEX IF NOT EXISTS idx_audit_trace ON audit_events (trace_id);

CREATE TABLE data_integrity_scan_runs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT,
    scope TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    rule_count INTEGER NOT NULL DEFAULT 0,
    finding_count INTEGER NOT NULL DEFAULT 0,
    failure_code TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE data_integrity_findings (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    rule_version INTEGER NOT NULL,
    bounded_context TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    severity TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    detected_state TEXT NOT NULL,
    expected_state TEXT NOT NULL,
    status TEXT NOT NULL,
    repairability TEXT NOT NULL,
    first_detected_at TEXT NOT NULL,
    last_detected_at TEXT NOT NULL,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    resolved_at TEXT,
    resolution_reason TEXT,
    version INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, rule_id, rule_version, resource_type, resource_id, fingerprint)
);
CREATE INDEX idx_integrity_findings_tenant_status
    ON data_integrity_findings (tenant_id, status, last_detected_at DESC, id DESC);

CREATE TABLE data_repair_plans (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    repair_type TEXT NOT NULL,
    repair_version INTEGER NOT NULL,
    risk_level TEXT NOT NULL,
    status TEXT NOT NULL,
    preview TEXT NOT NULL,
    created_by TEXT NOT NULL,
    approved_by TEXT,
    approval_note TEXT,
    idempotency_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, idempotency_key)
);
CREATE TABLE data_repair_runs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    plan_id TEXT,
    status TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    approval_note TEXT,
    worker_id TEXT,
    lease_token TEXT,
    fence_version INTEGER NOT NULL DEFAULT 0,
    lease_expires_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    checkpoint TEXT,
    next_attempt_at TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command TEXT NOT NULL DEFAULT '{}',
    version INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, idempotency_key)
);
CREATE TABLE data_repair_steps (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    repair_run_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    status TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    checkpoint TEXT,
    lease_owner TEXT,
    lease_token TEXT,
    fence_version INTEGER NOT NULL DEFAULT 0,
    lease_expires_at TEXT,
    next_attempt_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_repair_steps_claim
    ON data_repair_steps (status, next_attempt_at, lease_expires_at, created_at);
CREATE TABLE data_repair_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    repair_run_id TEXT NOT NULL,
    repair_step_id TEXT NOT NULL,
    finding_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    repair_type TEXT NOT NULL,
    repair_version INTEGER NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    before_hash TEXT NOT NULL,
    after_hash TEXT NOT NULL,
    before_snapshot TEXT NOT NULL DEFAULT '{}',
    after_snapshot TEXT NOT NULL DEFAULT '{}',
    rows_affected INTEGER NOT NULL DEFAULT 0,
    result TEXT NOT NULL,
    failure_code TEXT,
    trace_id TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    previous_hash TEXT,
    record_hash TEXT
);
CREATE INDEX idx_repair_events_tenant_time
    ON data_repair_events (tenant_id, finished_at DESC, id DESC);
