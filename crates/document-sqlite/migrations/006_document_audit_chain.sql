-- Audit remains the semantic owner. The Document SQLite bootstrap owns the
-- shared audit_events table in this profile. The adapter conditionally adds
-- the three chain columns before this SQL, in the same transaction, because
-- historical Processing migration 004 may already have installed them.
-- Preserve all existing v1 rows and hashes. Never manufacture verified history.
WITH legacy AS MATERIALIZED (
    SELECT events.id,
        (SELECT COALESCE(MAX(prior.stream_sequence), 0) FROM audit_events AS prior
         WHERE prior.tenant_id = events.tenant_id)
        + ROW_NUMBER() OVER (PARTITION BY tenant_id ORDER BY id) AS sequence
    FROM audit_events AS events
    WHERE stream_sequence IS NULL AND chain_version IS NULL AND recorded_at IS NULL
)
UPDATE audit_events
SET stream_sequence = (SELECT sequence FROM legacy WHERE legacy.id = audit_events.id),
    recorded_at = COALESCE(created_at, occurred_at),
    chain_version = 0,
    previous_hash = NULL,
    record_hash = NULL
WHERE stream_sequence IS NULL AND chain_version IS NULL AND recorded_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS ux_audit_tenant_stream_sequence
    ON audit_events (tenant_id, stream_sequence);
CREATE INDEX IF NOT EXISTS idx_audit_tenant_chain_sequence
    ON audit_events (tenant_id, chain_version, stream_sequence);
