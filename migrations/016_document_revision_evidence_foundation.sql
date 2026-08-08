-- PLAN-0008: immutable document revisions, lifecycle/deletion state, links,
-- exact processing bindings, and non-authoritative processing evidence.

ALTER TABLE documents
    ADD COLUMN current_revision_id UUID,
    ADD COLUMN deletion_state VARCHAR(30) NOT NULL DEFAULT 'present',
    ADD COLUMN pre_trash_lifecycle VARCHAR(30) NOT NULL DEFAULT 'active';

CREATE TABLE document_revisions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    document_id UUID NOT NULL,
    revision_no BIGINT NOT NULL CHECK (revision_no > 0),
    parent_revision_id UUID,
    source_object_ref VARCHAR(1024) NOT NULL,
    sha256 CHAR(64),
    content_type VARCHAR(200) NOT NULL,
    size_bytes BIGINT CHECK (size_bytes IS NULL OR size_bytes >= 0),
    original_filename VARCHAR(500) NOT NULL,
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    change_reason VARCHAR(500),
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, document_id, id),
    UNIQUE (tenant_id, document_id, revision_no),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id),
    FOREIGN KEY (tenant_id, document_id, parent_revision_id)
        REFERENCES document_revisions (tenant_id, document_id, id)
);

-- Existing object references are retained for compatibility and reconciliation;
-- all new writes use the revision-scoped `.../revisions/{uuid}/source` key.
INSERT INTO document_revisions
    (id, tenant_id, document_id, revision_no, source_object_ref, content_type,
     size_bytes, original_filename, created_by, created_at, change_reason)
SELECT uuid_generate_v4(), d.tenant_id, d.id, d.content_revision, d.object_key,
       d.content_type, d.size_bytes, d.original_filename, d.created_by,
       d.created_at, 'migration backfill: PLAN-0008 R1'
FROM documents d
WHERE NOT EXISTS (
    SELECT 1 FROM document_revisions r
    WHERE r.tenant_id = d.tenant_id AND r.document_id = d.id
);

UPDATE documents d
SET current_revision_id = r.id
FROM document_revisions r
WHERE r.tenant_id = d.tenant_id
  AND r.document_id = d.id
  AND r.revision_no = d.content_revision
  AND d.current_revision_id IS NULL;

ALTER TABLE documents
    ADD CONSTRAINT documents_deletion_state_check
        CHECK (deletion_state IN ('present', 'trashed', 'pending_purge', 'purged')),
    ADD CONSTRAINT documents_pre_trash_lifecycle_check
        CHECK (pre_trash_lifecycle IN ('active', 'archived'));

ALTER TABLE documents
    ADD CONSTRAINT documents_current_revision_fk
        FOREIGN KEY (tenant_id, id, current_revision_id)
        REFERENCES document_revisions (tenant_id, document_id, id);

CREATE INDEX idx_document_revisions_history
    ON document_revisions (tenant_id, document_id, revision_no DESC, id DESC);

CREATE TABLE document_links (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    document_id UUID NOT NULL,
    resource_kind VARCHAR(40) NOT NULL CHECK (resource_kind IN
        ('contract', 'project', 'customer', 'party', 'legal_matter',
         'finance_record', 'assurance_case', 'employee', 'performance_review')),
    resource_id UUID NOT NULL,
    role VARCHAR(40) NOT NULL CHECK (role IN
        ('main_contract', 'signed_copy', 'appendix', 'amendment', 'quotation',
         'invoice', 'evidence', 'other')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    UNIQUE (tenant_id, document_id, resource_kind, resource_id, role),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)
);

CREATE TABLE document_purge_holds (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    document_id UUID NOT NULL,
    hold_kind VARCHAR(40) NOT NULL,
    reason VARCHAR(500) NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)
);

ALTER TABLE document_processing_jobs
    ADD COLUMN document_revision_id UUID;

UPDATE document_processing_jobs j
SET document_revision_id = d.current_revision_id
FROM documents d
WHERE d.tenant_id = j.tenant_id
  AND d.id = j.document_id
  AND j.document_revision_id IS NULL;

ALTER TABLE document_processing_jobs
    ADD CONSTRAINT processing_job_revision_fk
        FOREIGN KEY (tenant_id, document_id, document_revision_id)
        REFERENCES document_revisions (tenant_id, document_id, id);

CREATE INDEX idx_processing_jobs_revision
    ON document_processing_jobs (tenant_id, document_revision_id, created_at DESC);

CREATE TABLE document_processing_runs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    document_revision_id UUID NOT NULL,
    pipeline_version VARCHAR(100) NOT NULL,
    parser_name VARCHAR(100) NOT NULL,
    parser_version VARCHAR(100) NOT NULL,
    model_provider VARCHAR(100),
    model_name VARCHAR(100),
    status VARCHAR(30) NOT NULL,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_code VARCHAR(100),
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, id),
    FOREIGN KEY (tenant_id, document_revision_id)
        REFERENCES document_revisions (tenant_id, id)
);

CREATE TABLE document_processing_artifacts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    processing_run_id UUID NOT NULL,
    kind VARCHAR(40) NOT NULL,
    storage_ref VARCHAR(1024) NOT NULL,
    checksum CHAR(64) NOT NULL,
    schema_version VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, id),
    FOREIGN KEY (tenant_id, processing_run_id)
        REFERENCES document_processing_runs (tenant_id, id)
);

CREATE TABLE document_processing_evidence (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    document_revision_id UUID NOT NULL,
    processing_run_id UUID NOT NULL,
    artifact_id UUID NOT NULL,
    location_json JSONB NOT NULL,
    source_checksum CHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, id),
    FOREIGN KEY (tenant_id, document_revision_id)
        REFERENCES document_revisions (tenant_id, id),
    FOREIGN KEY (tenant_id, processing_run_id)
        REFERENCES document_processing_runs (tenant_id, id),
    FOREIGN KEY (tenant_id, artifact_id)
        REFERENCES document_processing_artifacts (tenant_id, id)
);

CREATE INDEX idx_document_links_resource
    ON document_links (tenant_id, resource_kind, resource_id);
CREATE INDEX idx_processing_runs_revision
    ON document_processing_runs (tenant_id, document_revision_id, created_at DESC);
CREATE INDEX idx_processing_evidence_revision
    ON document_processing_evidence (tenant_id, document_revision_id, created_at DESC);
