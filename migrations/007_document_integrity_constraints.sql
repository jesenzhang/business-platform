-- Enforce Document metadata invariants without rewriting historical migrations.
-- Existing invalid data is an operational error: fail fast instead of silently
-- mutating business records during schema upgrade.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM documents
        WHERE size_bytes < 0
           OR version <= 0
           OR status NOT IN ('active', 'archived', 'deleted')
           OR length(trim(original_filename)) = 0
           OR length(trim(content_type)) = 0
           OR length(trim(object_key)) = 0
    ) THEN
        RAISE EXCEPTION 'documents contains rows that violate integrity constraints';
    END IF;
END
$$;

ALTER TABLE documents
    ADD CONSTRAINT documents_size_bytes_nonnegative_check
        CHECK (size_bytes IS NULL OR size_bytes >= 0),
    ADD CONSTRAINT documents_version_positive_check
        CHECK (version > 0),
    ADD CONSTRAINT documents_status_check
        CHECK (status IN ('active', 'archived', 'deleted')),
    ADD CONSTRAINT documents_original_filename_nonblank_check
        CHECK (length(trim(original_filename)) > 0),
    ADD CONSTRAINT documents_content_type_nonblank_check
        CHECK (length(trim(content_type)) > 0),
    ADD CONSTRAINT documents_object_key_nonblank_check
        CHECK (length(trim(object_key)) > 0);

ALTER TABLE document_idempotency
    ADD COLUMN fingerprint_version SMALLINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT document_idempotency_fingerprint_version_check
        CHECK (fingerprint_version > 0);
