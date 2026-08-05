-- PLAN-0005 Revision 2: durable repair failure facts and SQLite guards.

UPDATE audit_events
SET chain_version = 0,
    previous_hash = NULL,
    record_hash = NULL
WHERE chain_version = 1;

ALTER TABLE data_repair_runs
    ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts > 0);
ALTER TABLE data_repair_runs ADD COLUMN failure_code TEXT;
ALTER TABLE data_repair_runs ADD COLUMN last_error_category TEXT;
ALTER TABLE data_repair_runs ADD COLUMN finished_at TEXT;

ALTER TABLE data_repair_steps
    ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts > 0);
ALTER TABLE data_repair_steps ADD COLUMN failure_code TEXT;
ALTER TABLE data_repair_steps ADD COLUMN last_error_category TEXT;
ALTER TABLE data_repair_steps ADD COLUMN finished_at TEXT;

CREATE TRIGGER validate_repair_run_revision2_insert
BEFORE INSERT ON data_repair_runs
WHEN NEW.attempt_count < 0
  OR NEW.version < 0
  OR ((NEW.worker_id IS NULL) <> (NEW.lease_token IS NULL))
  OR ((NEW.worker_id IS NULL) <> (NEW.lease_expires_at IS NULL))
  OR (NEW.status IN ('succeeded','failed','cancelled','needs_manual_review')
      AND (NEW.worker_id IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'invalid repair run state');
END;

CREATE TRIGGER validate_repair_run_revision2_update
BEFORE UPDATE ON data_repair_runs
WHEN NEW.attempt_count < 0
  OR NEW.version < 0
  OR ((NEW.worker_id IS NULL) <> (NEW.lease_token IS NULL))
  OR ((NEW.worker_id IS NULL) <> (NEW.lease_expires_at IS NULL))
  OR (NEW.status IN ('succeeded','failed','cancelled','needs_manual_review')
      AND (NEW.worker_id IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'invalid repair run state');
END;

CREATE TRIGGER validate_repair_step_revision2_insert
BEFORE INSERT ON data_repair_steps
WHEN NEW.attempt_count < 0
  OR NEW.fence_version < 0
  OR ((NEW.lease_owner IS NULL) <> (NEW.lease_token IS NULL))
  OR ((NEW.lease_owner IS NULL) <> (NEW.lease_expires_at IS NULL))
  OR (NEW.status IN ('succeeded','failed','cancelled','needs_manual_review')
      AND (NEW.lease_owner IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'invalid repair step state');
END;

CREATE TRIGGER validate_repair_step_revision2_update
BEFORE UPDATE ON data_repair_steps
WHEN NEW.attempt_count < 0
  OR NEW.fence_version < 0
  OR ((NEW.lease_owner IS NULL) <> (NEW.lease_token IS NULL))
  OR ((NEW.lease_owner IS NULL) <> (NEW.lease_expires_at IS NULL))
  OR (NEW.status IN ('succeeded','failed','cancelled','needs_manual_review')
      AND (NEW.lease_owner IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'invalid repair step state');
END;
