-- PLAN-0008 compatibility bridge. The Document catalog owns the revision row;
-- this catalog only adds the exact binding to the durable processing job.
ALTER TABLE document_processing_jobs ADD COLUMN document_revision_id TEXT;

CREATE INDEX IF NOT EXISTS idx_processing_jobs_revision
    ON document_processing_jobs (tenant_id, document_revision_id, created_at DESC);
