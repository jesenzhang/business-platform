-- PLAN-0004 Revision 1: single-process SQLite execution constraints and audit.
ALTER TABLE document_ai_tasks ADD COLUMN cancel_requested_at TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS document_processing_jobs_tenant_id_id_key
    ON document_processing_jobs (tenant_id, id);
CREATE UNIQUE INDEX IF NOT EXISTS document_ai_tasks_attempt_key
    ON document_ai_tasks (tenant_id, job_id, step_kind, attempt_count);
CREATE UNIQUE INDEX IF NOT EXISTS document_extraction_candidates_tenant_id_id_key
    ON document_extraction_candidates (tenant_id, id);

CREATE TABLE document_processing_audit_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    action TEXT NOT NULL,
    actor_id TEXT,
    details TEXT NOT NULL DEFAULT '{}',
    occurred_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id, job_id)
        REFERENCES document_processing_jobs (tenant_id, id)
        ON DELETE CASCADE
);

CREATE INDEX idx_processing_audit_job
    ON document_processing_audit_events (tenant_id, job_id, occurred_at DESC);

CREATE TRIGGER document_processing_steps_tenant_guard
BEFORE INSERT ON document_processing_steps
BEGIN
    SELECT RAISE(ABORT, 'processing step tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_processing_steps_tenant_guard_update
BEFORE UPDATE OF tenant_id, job_id ON document_processing_steps
BEGIN
    SELECT RAISE(ABORT, 'processing step tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_ai_tasks_tenant_guard
BEFORE INSERT ON document_ai_tasks
BEGIN
    SELECT RAISE(ABORT, 'AI task tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_ai_tasks_tenant_guard_update
BEFORE UPDATE OF tenant_id, job_id ON document_ai_tasks
BEGIN
    SELECT RAISE(ABORT, 'AI task tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_ai_tasks_state_guard
BEFORE INSERT ON document_ai_tasks
BEGIN
    SELECT RAISE(ABORT, 'invalid AI task state')
    WHERE (NEW.status = 'running' AND
           (NEW.lease_owner IS NULL OR NEW.lease_token IS NULL OR NEW.lease_expires_at IS NULL OR NEW.fence_version <= 0))
       OR (NEW.status <> 'running' AND
           (NEW.lease_owner IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
       OR NEW.attempt_count > NEW.max_attempts
       OR NEW.updated_at < NEW.created_at;
END;

CREATE TRIGGER document_ai_tasks_state_guard_update
BEFORE UPDATE ON document_ai_tasks
BEGIN
    SELECT RAISE(ABORT, 'invalid AI task state')
    WHERE (NEW.status = 'running' AND
           (NEW.lease_owner IS NULL OR NEW.lease_token IS NULL OR NEW.lease_expires_at IS NULL OR NEW.fence_version <= 0))
       OR (NEW.status <> 'running' AND
           (NEW.lease_owner IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
       OR NEW.attempt_count > NEW.max_attempts
       OR NEW.updated_at < NEW.created_at;
END;

CREATE TRIGGER document_extraction_candidates_tenant_guard
BEFORE INSERT ON document_extraction_candidates
BEGIN
    SELECT RAISE(ABORT, 'candidate tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_extraction_candidates_tenant_guard_update
BEFORE UPDATE OF tenant_id, job_id ON document_extraction_candidates
BEGIN
    SELECT RAISE(ABORT, 'candidate tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_extraction_reviews_tenant_guard
BEFORE INSERT ON document_extraction_reviews
BEGIN
    SELECT RAISE(ABORT, 'review tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_extraction_candidates
        WHERE id = NEW.candidate_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_extraction_reviews_tenant_guard_update
BEFORE UPDATE OF tenant_id, candidate_id ON document_extraction_reviews
BEGIN
    SELECT RAISE(ABORT, 'review tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_extraction_candidates
        WHERE id = NEW.candidate_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_processing_audit_tenant_guard
BEFORE INSERT ON document_processing_audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_processing_audit_tenant_guard_update
BEFORE UPDATE OF tenant_id, job_id ON document_processing_audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit tenant mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM document_processing_jobs
        WHERE id = NEW.job_id AND tenant_id = NEW.tenant_id
    );
END;

CREATE TRIGGER document_processing_jobs_state_guard
BEFORE INSERT ON document_processing_jobs
BEGIN
    SELECT RAISE(ABORT, 'invalid processing job state')
    WHERE (NEW.status = 'running' AND
           (NEW.lease_owner IS NULL OR NEW.lease_token IS NULL OR NEW.lease_expires_at IS NULL OR NEW.fence_version <= 0))
       OR (NEW.status <> 'running' AND
           (NEW.lease_owner IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
       OR (NEW.status = 'waiting_for_ai' AND NEW.current_step <> 'extract_fields')
       OR (NEW.status = 'waiting_for_review' AND NEW.current_step <> 'await_review')
       OR (NEW.status IN ('queued', 'running') AND NEW.current_step = 'await_review')
       OR (NEW.status IN ('succeeded', 'rejected') AND NEW.current_step <> 'await_review')
       OR NEW.attempt_count > NEW.max_attempts
       OR NEW.updated_at < NEW.created_at;
END;

CREATE TRIGGER document_processing_jobs_state_guard_update
BEFORE UPDATE ON document_processing_jobs
BEGIN
    SELECT RAISE(ABORT, 'invalid processing job state')
    WHERE (NEW.status = 'running' AND
           (NEW.lease_owner IS NULL OR NEW.lease_token IS NULL OR NEW.lease_expires_at IS NULL OR NEW.fence_version <= 0))
       OR (NEW.status <> 'running' AND
           (NEW.lease_owner IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_expires_at IS NOT NULL))
       OR (NEW.status = 'waiting_for_ai' AND NEW.current_step <> 'extract_fields')
       OR (NEW.status = 'waiting_for_review' AND NEW.current_step <> 'await_review')
       OR (NEW.status IN ('queued', 'running') AND NEW.current_step = 'await_review')
       OR (NEW.status IN ('succeeded', 'rejected') AND NEW.current_step <> 'await_review')
       OR NEW.attempt_count > NEW.max_attempts
       OR NEW.updated_at < NEW.created_at;
END;
