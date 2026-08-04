-- PLAN-0005 Revision 1: sequence-based audit chain metadata.
-- Historical rows are deliberately marked chain_version=0. They receive a
-- deterministic tenant-local sequence for ordering, but are not claimed as
-- hash-protected history. New adapter appends start the version-1 chain at the
-- next sequence and use an explicit genesis (previous_hash IS NULL).

ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS stream_sequence BIGINT;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS recorded_at TIMESTAMPTZ;
ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS chain_version SMALLINT;

WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY tenant_id
               ORDER BY occurred_at ASC NULLS LAST, id ASC
           ) AS sequence
    FROM audit_events
)
UPDATE audit_events AS events
SET stream_sequence = ranked.sequence,
    recorded_at = COALESCE(events.created_at, events.occurred_at, NOW()),
    chain_version = 0
FROM ranked
WHERE events.id = ranked.id
  AND (events.stream_sequence IS NULL
       OR events.recorded_at IS NULL
       OR events.chain_version IS NULL);

UPDATE audit_events
SET recorded_at = COALESCE(recorded_at, created_at, occurred_at, NOW()),
    chain_version = COALESCE(chain_version, 0),
    stream_sequence = COALESCE(stream_sequence, 1);

ALTER TABLE audit_events ALTER COLUMN recorded_at SET DEFAULT NOW();
ALTER TABLE audit_events ALTER COLUMN recorded_at SET NOT NULL;
ALTER TABLE audit_events ALTER COLUMN chain_version SET DEFAULT 0;
ALTER TABLE audit_events ALTER COLUMN chain_version SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS ux_audit_tenant_stream_sequence
    ON audit_events (tenant_id, stream_sequence)
    WHERE chain_version = 1;
CREATE INDEX IF NOT EXISTS idx_audit_tenant_chain_sequence
    ON audit_events (tenant_id, chain_version, stream_sequence);

-- A resolved finding is an episode, not a permanent tombstone.  Reopening is
-- explicit and retains the superseded resolution for audit and operator
-- review when the same rule version detects the issue again.
ALTER TABLE data_integrity_findings
    ADD COLUMN IF NOT EXISTS reopened_at TIMESTAMPTZ;
ALTER TABLE data_integrity_findings
    ADD COLUMN IF NOT EXISTS reopen_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE data_integrity_findings
    ADD COLUMN IF NOT EXISTS previous_resolution TEXT;
