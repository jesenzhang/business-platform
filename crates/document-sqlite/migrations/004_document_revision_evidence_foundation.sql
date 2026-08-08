-- PLAN-0008 SQLite parity. SQLite is local single-process only; legacy rows
-- are backfilled with the document UUID as their deterministic R1 identity.

ALTER TABLE documents ADD COLUMN current_revision_id TEXT;
ALTER TABLE documents ADD COLUMN deletion_state TEXT NOT NULL DEFAULT 'present';
ALTER TABLE documents ADD COLUMN pre_trash_lifecycle TEXT NOT NULL DEFAULT 'active';

CREATE TABLE IF NOT EXISTS document_revisions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    revision_no INTEGER NOT NULL CHECK (revision_no > 0),
    parent_revision_id TEXT,
    source_object_ref TEXT NOT NULL,
    sha256 TEXT,
    content_type TEXT NOT NULL,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    original_filename TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    change_reason TEXT,
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, document_id, id),
    UNIQUE (tenant_id, document_id, revision_no),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id),
    FOREIGN KEY (tenant_id, document_id, parent_revision_id)
        REFERENCES document_revisions (tenant_id, document_id, id)
);

INSERT OR IGNORE INTO document_revisions
    (id, tenant_id, document_id, revision_no, source_object_ref, content_type,
     size_bytes, original_filename, created_by, created_at, change_reason)
SELECT id, tenant_id, id, content_revision, object_key, content_type,
       size_bytes, original_filename, created_by, created_at,
       'migration backfill: PLAN-0008 R1'
FROM documents;

UPDATE documents
SET current_revision_id = id
WHERE current_revision_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_document_revisions_history
    ON document_revisions (tenant_id, document_id, revision_no DESC, id DESC);

CREATE TABLE IF NOT EXISTS document_links (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    resource_kind TEXT NOT NULL CHECK (resource_kind IN
        ('contract', 'project', 'customer', 'party', 'legal_matter',
         'finance_record', 'assurance_case', 'employee', 'performance_review')),
    resource_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN
        ('main_contract', 'signed_copy', 'appendix', 'amendment', 'quotation',
         'invoice', 'evidence', 'other')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT NOT NULL,
    UNIQUE (tenant_id, document_id, resource_kind, resource_id, role),
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)
);

CREATE TABLE IF NOT EXISTS document_purge_holds (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    hold_kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents (tenant_id, id)
);

CREATE TABLE IF NOT EXISTS document_processing_runs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_revision_id TEXT NOT NULL,
    pipeline_version TEXT NOT NULL,
    parser_name TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    model_provider TEXT,
    model_name TEXT,
    status TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    failure_code TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id, document_revision_id)
        REFERENCES document_revisions (tenant_id, id)
);

CREATE TABLE IF NOT EXISTS document_processing_artifacts (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    processing_run_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    storage_ref TEXT NOT NULL,
    checksum TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id, processing_run_id)
        REFERENCES document_processing_runs (tenant_id, id)
);

CREATE TABLE IF NOT EXISTS document_processing_evidence (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_revision_id TEXT NOT NULL,
    processing_run_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    location_json TEXT NOT NULL,
    source_checksum TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id, document_revision_id)
        REFERENCES document_revisions (tenant_id, id),
    FOREIGN KEY (tenant_id, processing_run_id)
        REFERENCES document_processing_runs (tenant_id, id),
    FOREIGN KEY (tenant_id, artifact_id)
        REFERENCES document_processing_artifacts (tenant_id, id)
);
