-- Durable document processing MVP: persistent job/step/AI task/candidate/review state.
CREATE UNIQUE INDEX IF NOT EXISTS documents_tenant_id_id_unique
    ON documents (tenant_id, id);

CREATE TABLE document_processing_jobs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    document_id UUID NOT NULL,
    content_revision BIGINT NOT NULL CHECK (content_revision > 0),
    request_key VARCHAR(200) NOT NULL,
    status VARCHAR(40) NOT NULL CHECK (status IN ('queued', 'running', 'waiting_for_ai', 'waiting_for_review', 'succeeded', 'failed', 'cancelled', 'rejected')),
    current_step VARCHAR(40) NOT NULL CHECK (current_step IN ('validate_source', 'detect_type', 'extract_text', 'extract_fields', 'validate_candidate', 'await_review')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts >= 1),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    cancel_requested_at TIMESTAMPTZ,
    failure_code VARCHAR(80),
    failure_message TEXT,
    lease_owner VARCHAR(200),
    lease_token VARCHAR(200),
    lease_expires_at TIMESTAMPTZ,
    fence_version BIGINT NOT NULL DEFAULT 0 CHECK (fence_version >= 0),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, document_id, request_key),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)
);

CREATE TABLE document_processing_steps (
    job_id UUID NOT NULL REFERENCES document_processing_jobs(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    step_kind VARCHAR(40) NOT NULL,
    status VARCHAR(20) NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'skipped')),
    attempt_number INTEGER NOT NULL CHECK (attempt_number >= 0),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    checkpoint_json JSONB,
    failure_code VARCHAR(80),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (job_id, step_kind, attempt_number)
);

CREATE TABLE document_ai_tasks (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    job_id UUID NOT NULL REFERENCES document_processing_jobs(id) ON DELETE CASCADE,
    step_kind VARCHAR(40) NOT NULL,
    status VARCHAR(20) NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    input_artifact_id VARCHAR(500),
    output_candidate_id UUID,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts >= 1),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    lease_owner VARCHAR(200),
    lease_token VARCHAR(200),
    lease_expires_at TIMESTAMPTZ,
    fence_version BIGINT NOT NULL DEFAULT 0 CHECK (fence_version >= 0),
    failure_code VARCHAR(80),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE document_extraction_candidates (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    job_id UUID NOT NULL REFERENCES document_processing_jobs(id) ON DELETE CASCADE,
    schema_version VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL,
    evidence JSONB NOT NULL,
    provider VARCHAR(100) NOT NULL,
    model VARCHAR(100) NOT NULL,
    prompt_version VARCHAR(100) NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, job_id)
);

CREATE TABLE document_extraction_reviews (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    candidate_id UUID NOT NULL REFERENCES document_extraction_candidates(id) ON DELETE CASCADE,
    reviewer_id UUID NOT NULL,
    decision VARCHAR(20) NOT NULL CHECK (decision IN ('accepted', 'edited', 'rejected')),
    patch JSONB,
    comment TEXT,
    candidate_version BIGINT NOT NULL CHECK (candidate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, candidate_id)
);

CREATE INDEX idx_processing_jobs_claim
    ON document_processing_jobs (status, next_attempt_at, lease_expires_at, created_at);
CREATE INDEX idx_processing_jobs_document
    ON document_processing_jobs (tenant_id, document_id, created_at DESC);
CREATE INDEX idx_processing_ai_tasks_claim
    ON document_ai_tasks (status, next_attempt_at, lease_expires_at);
CREATE INDEX idx_processing_steps_job
    ON document_processing_steps (job_id, step_kind, attempt_number);
CREATE INDEX idx_processing_candidates_job
    ON document_extraction_candidates (tenant_id, job_id);
CREATE INDEX idx_processing_reviews_candidate
    ON document_extraction_reviews (tenant_id, candidate_id);
