-- PLAN-0012 T4.5 correlation propagation. The enqueue request id (bounded,
-- non-secret, from the API request-correlation header) is stored on the job
-- and inherited by AI tasks so audit rows and worker logs join one chain.
ALTER TABLE document_processing_jobs ADD COLUMN correlation_id TEXT;

ALTER TABLE document_ai_tasks ADD COLUMN correlation_id TEXT;
