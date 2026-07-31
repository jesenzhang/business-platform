-- Upgrade outbox_events for multi-worker reliable delivery
ALTER TABLE outbox_events
    ADD COLUMN IF NOT EXISTS status VARCHAR(30) NOT NULL DEFAULT 'pending',
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 5,
    ADD COLUMN IF NOT EXISTS available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS claimed_by VARCHAR(100),
    ADD COLUMN IF NOT EXISTS lease_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS published_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error TEXT;

-- Drop old index, create new ones for claim queries
DROP INDEX IF EXISTS idx_outbox_unpublished;

-- Index for claiming: find available events ordered deterministically
CREATE INDEX idx_outbox_claimable
    ON outbox_events (available_at, event_id)
    WHERE status IN ('pending', 'retry_scheduled');

-- Index for lease recovery: find expired leases
CREATE INDEX idx_outbox_lease_recovery
    ON outbox_events (lease_until)
    WHERE status = 'processing' AND lease_until IS NOT NULL;
