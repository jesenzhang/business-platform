-- PLAN-0005 Runtime Governance: unified audit, integrity findings and repairs.
-- This is an additive migration; published migrations 001-011 remain immutable.

ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS operation_id UUID;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS actor_type VARCHAR(32) NOT NULL DEFAULT 'user';
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS actor_id UUID;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS correlation_id UUID;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS causation_id UUID;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS reason TEXT;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS result VARCHAR(32) NOT NULL DEFAULT 'succeeded';
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS failure_code VARCHAR(128);
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS before_hash VARCHAR(128);
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS after_hash VARCHAR(128);
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS changed_fields JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS schema_version VARCHAR(32) NOT NULL DEFAULT 'audit.v1';
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS previous_hash VARCHAR(128);
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS record_hash VARCHAR(128);
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ;
UPDATE audit_events SET occurred_at = created_at WHERE occurred_at IS NULL;
ALTER TABLE audit_events ALTER COLUMN occurred_at SET DEFAULT NOW();

CREATE INDEX IF NOT EXISTS idx_audit_tenant_occurred_id
    ON audit_events (tenant_id, occurred_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor_time
    ON audit_events (tenant_id, actor_type, actor_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_resource_time
    ON audit_events (tenant_id, resource_type, resource_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_operation ON audit_events (operation_id);
CREATE INDEX IF NOT EXISTS idx_audit_trace ON audit_events (trace_id);

CREATE TABLE data_integrity_scan_runs (
    id UUID PRIMARY KEY,
    tenant_id UUID,
    scope JSONB NOT NULL,
    status VARCHAR(32) NOT NULL,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    rule_count INTEGER NOT NULL DEFAULT 0,
    finding_count BIGINT NOT NULL DEFAULT 0,
    failure_code VARCHAR(128),
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE data_integrity_findings (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    rule_id VARCHAR(128) NOT NULL,
    rule_version INTEGER NOT NULL,
    bounded_context VARCHAR(128) NOT NULL,
    resource_type VARCHAR(128) NOT NULL,
    resource_id VARCHAR(256) NOT NULL,
    severity VARCHAR(32) NOT NULL,
    fingerprint VARCHAR(128) NOT NULL,
    detected_state JSONB NOT NULL,
    expected_state JSONB NOT NULL,
    status VARCHAR(32) NOT NULL,
    repairability VARCHAR(128) NOT NULL,
    first_detected_at TIMESTAMPTZ NOT NULL,
    last_detected_at TIMESTAMPTZ NOT NULL,
    occurrence_count BIGINT NOT NULL DEFAULT 1,
    resolved_at TIMESTAMPTZ,
    resolution_reason TEXT,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, rule_id, rule_version, resource_type, resource_id, fingerprint)
);
CREATE INDEX idx_integrity_findings_tenant_status
    ON data_integrity_findings (tenant_id, status, last_detected_at DESC, id DESC);

CREATE TABLE data_repair_plans (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    finding_id UUID NOT NULL REFERENCES data_integrity_findings(id),
    repair_type VARCHAR(128) NOT NULL,
    repair_version INTEGER NOT NULL,
    risk_level VARCHAR(32) NOT NULL,
    status VARCHAR(32) NOT NULL,
    preview JSONB NOT NULL,
    created_by UUID NOT NULL,
    approved_by UUID,
    approval_note TEXT,
    idempotency_key VARCHAR(256) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, idempotency_key)
);

CREATE TABLE data_repair_runs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    finding_id UUID NOT NULL REFERENCES data_integrity_findings(id),
    plan_id UUID REFERENCES data_repair_plans(id),
    status VARCHAR(32) NOT NULL,
    requested_by UUID NOT NULL,
    approved_by UUID,
    approval_note TEXT,
    worker_id VARCHAR(128),
    lease_token VARCHAR(256),
    fence_version BIGINT NOT NULL DEFAULT 0,
    lease_expires_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    checkpoint JSONB,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    idempotency_key VARCHAR(256) NOT NULL,
    command JSONB NOT NULL DEFAULT '{}'::jsonb,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, idempotency_key)
);

CREATE TABLE data_repair_steps (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    repair_run_id UUID NOT NULL REFERENCES data_repair_runs(id) ON DELETE CASCADE,
    finding_id UUID NOT NULL REFERENCES data_integrity_findings(id),
    status VARCHAR(32) NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    checkpoint JSONB,
    lease_owner VARCHAR(128),
    lease_token VARCHAR(256),
    fence_version BIGINT NOT NULL DEFAULT 0,
    lease_expires_at TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_repair_steps_claim
    ON data_repair_steps (status, next_attempt_at, lease_expires_at, created_at);

CREATE TABLE data_repair_events (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    repair_run_id UUID NOT NULL REFERENCES data_repair_runs(id),
    repair_step_id UUID NOT NULL REFERENCES data_repair_steps(id),
    finding_id UUID NOT NULL REFERENCES data_integrity_findings(id),
    rule_id VARCHAR(128) NOT NULL,
    repair_type VARCHAR(128) NOT NULL,
    repair_version INTEGER NOT NULL,
    actor_type VARCHAR(32) NOT NULL,
    actor_id UUID NOT NULL,
    reason TEXT NOT NULL,
    resource_type VARCHAR(128) NOT NULL,
    resource_id VARCHAR(256) NOT NULL,
    before_hash VARCHAR(128) NOT NULL,
    after_hash VARCHAR(128) NOT NULL,
    before_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    after_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    rows_affected INTEGER NOT NULL DEFAULT 0,
    result VARCHAR(32) NOT NULL,
    failure_code VARCHAR(128),
    trace_id VARCHAR(128),
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,
    previous_hash VARCHAR(128),
    record_hash VARCHAR(128)
);
CREATE INDEX idx_repair_events_tenant_time
    ON data_repair_events (tenant_id, finished_at DESC, id DESC);
