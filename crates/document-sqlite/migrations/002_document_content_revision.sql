-- Gate 0: separate file-content revision from aggregate state version.
ALTER TABLE documents
    ADD COLUMN content_revision INTEGER NOT NULL DEFAULT 1
    CHECK (content_revision > 0);

CREATE UNIQUE INDEX IF NOT EXISTS documents_tenant_id_id_unique
    ON documents (tenant_id, id);
