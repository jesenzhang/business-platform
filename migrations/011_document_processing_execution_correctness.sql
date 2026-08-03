-- PLAN-0004 Revision 1: execution correctness constraints and audit trail.
ALTER TABLE document_ai_tasks
    ADD COLUMN cancel_requested_at TIMESTAMPTZ;

ALTER TABLE document_processing_jobs
    ADD CONSTRAINT document_processing_jobs_tenant_id_id_key
    UNIQUE (tenant_id, id);

ALTER TABLE document_processing_steps
    ADD CONSTRAINT document_processing_steps_job_tenant_fk
    FOREIGN KEY (tenant_id, job_id)
    REFERENCES document_processing_jobs (tenant_id, id)
    ON DELETE CASCADE;

ALTER TABLE document_ai_tasks
    ADD CONSTRAINT document_ai_tasks_job_tenant_fk
    FOREIGN KEY (tenant_id, job_id)
    REFERENCES document_processing_jobs (tenant_id, id)
    ON DELETE CASCADE;

ALTER TABLE document_extraction_candidates
    ADD CONSTRAINT document_extraction_candidates_job_tenant_fk
    FOREIGN KEY (tenant_id, job_id)
    REFERENCES document_processing_jobs (tenant_id, id)
    ON DELETE CASCADE;

ALTER TABLE document_extraction_candidates
    ADD CONSTRAINT document_extraction_candidates_tenant_id_key
    UNIQUE (tenant_id, id);

ALTER TABLE document_extraction_reviews
    ADD CONSTRAINT document_extraction_reviews_candidate_tenant_fk
    FOREIGN KEY (tenant_id, candidate_id)
    REFERENCES document_extraction_candidates (tenant_id, id)
    ON DELETE CASCADE;

ALTER TABLE document_ai_tasks
    ADD CONSTRAINT document_ai_tasks_attempt_key
    UNIQUE (tenant_id, job_id, step_kind, attempt_count);

ALTER TABLE document_processing_jobs
    ADD CONSTRAINT document_processing_jobs_lease_consistency_ck
    CHECK (
        (status = 'running'
            AND lease_owner IS NOT NULL
            AND lease_token IS NOT NULL
            AND lease_expires_at IS NOT NULL
            AND fence_version > 0)
        OR (status <> 'running'
            AND lease_owner IS NULL
            AND lease_token IS NULL
            AND lease_expires_at IS NULL)
    );

ALTER TABLE document_processing_jobs
    ADD CONSTRAINT document_processing_jobs_state_consistency_ck
    CHECK (
        (status <> 'waiting_for_ai' OR current_step = 'extract_fields')
        AND (status <> 'waiting_for_review' OR current_step = 'await_review')
        AND NOT (status IN ('queued', 'running') AND current_step = 'await_review')
        AND (status NOT IN ('succeeded', 'rejected')
            OR current_step = 'await_review')
        AND attempt_count <= max_attempts
        AND updated_at >= created_at
    );

ALTER TABLE document_ai_tasks
    ADD CONSTRAINT document_ai_tasks_lease_consistency_ck
    CHECK (
        (status = 'running'
            AND lease_owner IS NOT NULL
            AND lease_token IS NOT NULL
            AND lease_expires_at IS NOT NULL
            AND fence_version > 0)
        OR (status <> 'running'
            AND lease_owner IS NULL
            AND lease_token IS NULL
            AND lease_expires_at IS NULL)
    );

ALTER TABLE document_ai_tasks
    ADD CONSTRAINT document_ai_tasks_state_consistency_ck
    CHECK (attempt_count <= max_attempts AND updated_at >= created_at);

CREATE TABLE document_processing_audit_events (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    job_id UUID NOT NULL,
    action VARCHAR(80) NOT NULL,
    actor_id UUID,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (tenant_id, job_id)
        REFERENCES document_processing_jobs (tenant_id, id)
        ON DELETE CASCADE
);

CREATE INDEX idx_processing_audit_job
    ON document_processing_audit_events (tenant_id, job_id, occurred_at DESC);
