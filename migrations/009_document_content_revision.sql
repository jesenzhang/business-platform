-- Gate 0: separate file-content revision from aggregate state version.
ALTER TABLE documents
    ADD COLUMN content_revision BIGINT NOT NULL DEFAULT 1;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM documents
        WHERE content_revision <= 0
    ) THEN
        RAISE EXCEPTION 'documents contains an invalid content revision';
    END IF;
END
$$;

ALTER TABLE documents
    ADD CONSTRAINT documents_content_revision_positive_check
        CHECK (content_revision > 0);
