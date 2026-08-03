CREATE UNIQUE INDEX IF NOT EXISTS documents_tenant_id_id_unique
    ON documents (tenant_id, id);

CREATE TABLE document_processing_jobs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    content_revision INTEGER NOT NULL CHECK (content_revision > 0),
    request_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'waiting_for_ai', 'waiting_for_review', 'succeeded', 'failed', 'cancelled', 'rejected')),
    current_step TEXT NOT NULL CHECK (current_step IN ('validate_source', 'detect_type', 'extract_text', 'extract_fields', 'validate_candidate', 'await_review')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts >= 1),
    next_attempt_at TEXT NOT NULL,
    cancel_requested_at TEXT,
    failure_code TEXT,
    failure_message TEXT,
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TEXT,
    fence_version INTEGER NOT NULL DEFAULT 0 CHECK (fence_version >= 0),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, document_id, request_key),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)
);

CREATE TABLE document_processing_steps (
    job_id TEXT NOT NULL REFERENCES document_processing_jobs(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    step_kind TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'skipped')),
    attempt_number INTEGER NOT NULL CHECK (attempt_number >= 0),
    started_at TEXT,
    finished_at TEXT,
    checkpoint_json TEXT,
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (job_id, step_kind, attempt_number)
);

CREATE TABLE document_ai_tasks (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL REFERENCES document_processing_jobs(id) ON DELETE CASCADE,
    step_kind TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    input_artifact_id TEXT,
    output_candidate_id TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts >= 1),
    next_attempt_at TEXT NOT NULL,
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TEXT,
    fence_version INTEGER NOT NULL DEFAULT 0 CHECK (fence_version >= 0),
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE document_extraction_candidates (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    job_id TEXT NOT NULL REFERENCES document_processing_jobs(id) ON DELETE CASCADE,
    schema_version TEXT NOT NULL,
    payload TEXT NOT NULL,
    evidence TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id, job_id)
);

CREATE TABLE document_extraction_reviews (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL REFERENCES document_extraction_candidates(id) ON DELETE CASCADE,
    reviewer_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('accepted', 'edited', 'rejected')),
    patch TEXT,
    comment TEXT,
    candidate_version INTEGER NOT NULL CHECK (candidate_version > 0),
    created_at TEXT NOT NULL,
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
