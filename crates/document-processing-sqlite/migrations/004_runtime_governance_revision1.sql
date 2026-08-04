-- PLAN-0005 Revision 1 SQLite audit chain metadata.
-- chain_version=0 is an explicit legacy/unverified boundary. New writes use
-- chain_version=1 and a tenant-local append sequence.

ALTER TABLE audit_events ADD COLUMN stream_sequence INTEGER;
ALTER TABLE audit_events ADD COLUMN recorded_at TEXT;
ALTER TABLE audit_events ADD COLUMN chain_version INTEGER;

UPDATE audit_events AS events
SET stream_sequence = (
        SELECT COUNT(*)
        FROM audit_events AS prior
        WHERE prior.tenant_id = events.tenant_id
          AND (
              COALESCE(prior.occurred_at, '') < COALESCE(events.occurred_at, '')
              OR (
                  COALESCE(prior.occurred_at, '') = COALESCE(events.occurred_at, '')
                  AND prior.id <= events.id
              )
          )
    ),
    recorded_at = COALESCE(events.created_at, events.occurred_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    chain_version = 0
WHERE events.stream_sequence IS NULL
   OR events.recorded_at IS NULL
   OR events.chain_version IS NULL;

UPDATE audit_events
SET stream_sequence = COALESCE(stream_sequence, 1),
    recorded_at = COALESCE(recorded_at, occurred_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    chain_version = COALESCE(chain_version, 0);

CREATE UNIQUE INDEX IF NOT EXISTS ux_audit_tenant_stream_sequence
    ON audit_events (tenant_id, stream_sequence);
CREATE INDEX IF NOT EXISTS idx_audit_tenant_chain_sequence
    ON audit_events (tenant_id, chain_version, stream_sequence);

ALTER TABLE data_integrity_findings ADD COLUMN reopened_at TEXT;
ALTER TABLE data_integrity_findings ADD COLUMN reopen_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE data_integrity_findings ADD COLUMN previous_resolution TEXT;
