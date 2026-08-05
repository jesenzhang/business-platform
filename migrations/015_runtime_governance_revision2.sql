-- PLAN-0005 Revision 2: durable repair failure facts and database guards.

-- Revision 1 used NULL for the first v1 predecessor. Its hashes therefore
-- cannot truthfully be represented as the explicit-genesis format below.
-- Preserve that evidence as legacy/unverified and begin a new tenant-local
-- v1 chain on the next append.
UPDATE audit_events
SET chain_version = 0,
    previous_hash = NULL,
    record_hash = NULL
WHERE chain_version = 1;

ALTER TABLE data_repair_runs
    ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 3,
    ADD COLUMN failure_code VARCHAR(128),
    ADD COLUMN last_error_category VARCHAR(64),
    ADD COLUMN finished_at TIMESTAMPTZ;

ALTER TABLE data_repair_steps
    ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 3,
    ADD COLUMN failure_code VARCHAR(128),
    ADD COLUMN last_error_category VARCHAR(64),
    ADD COLUMN finished_at TIMESTAMPTZ;

ALTER TABLE data_repair_runs
    ADD CONSTRAINT chk_repair_runs_attempt_count CHECK (attempt_count >= 0),
    ADD CONSTRAINT chk_repair_runs_max_attempts CHECK (max_attempts > 0),
    ADD CONSTRAINT chk_repair_runs_version CHECK (version >= 0),
    ADD CONSTRAINT chk_repair_runs_lease_complete CHECK (
        (worker_id IS NULL AND lease_token IS NULL AND lease_expires_at IS NULL)
        OR
        (worker_id IS NOT NULL AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)
    ),
    ADD CONSTRAINT chk_repair_runs_terminal_lease CHECK (
        status NOT IN ('succeeded', 'failed', 'cancelled', 'needs_manual_review')
        OR (worker_id IS NULL AND lease_token IS NULL AND lease_expires_at IS NULL)
    );

ALTER TABLE data_repair_steps
    ADD CONSTRAINT chk_repair_steps_attempt_count CHECK (attempt_count >= 0),
    ADD CONSTRAINT chk_repair_steps_max_attempts CHECK (max_attempts > 0),
    ADD CONSTRAINT chk_repair_steps_fence_version CHECK (fence_version >= 0),
    ADD CONSTRAINT chk_repair_steps_lease_complete CHECK (
        (lease_owner IS NULL AND lease_token IS NULL AND lease_expires_at IS NULL)
        OR
        (lease_owner IS NOT NULL AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)
    ),
    ADD CONSTRAINT chk_repair_steps_terminal_lease CHECK (
        status NOT IN ('succeeded', 'failed', 'cancelled', 'needs_manual_review')
        OR (lease_owner IS NULL AND lease_token IS NULL AND lease_expires_at IS NULL)
    );
