-- Additive unified audit columns for the document-management SQLite profile.
ALTER TABLE audit_events ADD COLUMN operation_id TEXT;
ALTER TABLE audit_events ADD COLUMN actor_type TEXT NOT NULL DEFAULT 'user';
ALTER TABLE audit_events ADD COLUMN actor_id TEXT;
ALTER TABLE audit_events ADD COLUMN correlation_id TEXT;
ALTER TABLE audit_events ADD COLUMN causation_id TEXT;
ALTER TABLE audit_events ADD COLUMN trace_id TEXT;
ALTER TABLE audit_events ADD COLUMN reason TEXT;
ALTER TABLE audit_events ADD COLUMN result TEXT NOT NULL DEFAULT 'succeeded';
ALTER TABLE audit_events ADD COLUMN failure_code TEXT;
ALTER TABLE audit_events ADD COLUMN before_hash TEXT;
ALTER TABLE audit_events ADD COLUMN after_hash TEXT;
ALTER TABLE audit_events ADD COLUMN changed_fields TEXT NOT NULL DEFAULT '[]';
ALTER TABLE audit_events ADD COLUMN schema_version TEXT NOT NULL DEFAULT 'audit.v1';
ALTER TABLE audit_events ADD COLUMN previous_hash TEXT;
ALTER TABLE audit_events ADD COLUMN record_hash TEXT;
ALTER TABLE audit_events ADD COLUMN occurred_at TEXT;
UPDATE audit_events SET occurred_at = created_at WHERE occurred_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_audit_tenant_occurred_id
    ON audit_events (tenant_id, occurred_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_audit_resource_time
    ON audit_events (tenant_id, resource_type, resource_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor_time
    ON audit_events (tenant_id, actor_type, actor_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_operation ON audit_events (operation_id);
CREATE INDEX IF NOT EXISTS idx_audit_trace ON audit_events (trace_id);
