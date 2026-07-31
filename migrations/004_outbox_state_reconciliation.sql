-- Reconcile legacy publication flags and make `status` the only queue authority.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM outbox_events
        WHERE status NOT IN ('pending', 'processing', 'published', 'retry_scheduled', 'failed')
    ) THEN
        RAISE EXCEPTION 'outbox_events contains an invalid status';
    END IF;
END
$$;

UPDATE outbox_events
SET status = 'published'
WHERE published = TRUE;

UPDATE outbox_events
SET status = 'pending'
WHERE published = FALSE
  AND status NOT IN ('processing', 'retry_scheduled', 'failed');

ALTER TABLE outbox_events
    ADD COLUMN IF NOT EXISTS claim_token UUID,
    ADD COLUMN IF NOT EXISTS claim_version BIGINT NOT NULL DEFAULT 0;

ALTER TABLE outbox_events
    DROP CONSTRAINT IF EXISTS outbox_status_check,
    DROP CONSTRAINT IF EXISTS outbox_attempt_count_check,
    DROP CONSTRAINT IF EXISTS outbox_max_attempts_check,
    DROP CONSTRAINT IF EXISTS outbox_published_compatibility_check;

ALTER TABLE outbox_events
    ADD CONSTRAINT outbox_status_check
        CHECK (status IN ('pending', 'processing', 'published', 'retry_scheduled', 'failed')),
    ADD CONSTRAINT outbox_attempt_count_check
        CHECK (attempt_count >= 0),
    ADD CONSTRAINT outbox_max_attempts_check
        CHECK (max_attempts > 0 AND attempt_count <= max_attempts),
    ADD CONSTRAINT outbox_published_compatibility_check
        CHECK (published = (status = 'published'));

CREATE INDEX IF NOT EXISTS idx_outbox_claim_fence
    ON outbox_events (event_id, claim_version, claim_token)
    WHERE status = 'processing';
