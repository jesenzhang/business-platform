-- Persist review idempotency at the Document Intelligence transaction boundary.
ALTER TABLE document_extraction_reviews
    ADD COLUMN idempotency_key VARCHAR(255) NOT NULL DEFAULT '';

ALTER TABLE document_extraction_reviews
    ADD COLUMN request_fingerprint CHAR(64) NOT NULL DEFAULT '0000000000000000000000000000000000000000000000000000000000000000';

-- Existing reviews predate the idempotency contract. Give each one an
-- immutable, non-reusable legacy key so the new uniqueness constraint is safe.
UPDATE document_extraction_reviews
SET idempotency_key = 'legacy-review-' || id::text,
    request_fingerprint = substr(replace(id::text, '-', '') || repeat('0', 64), 1, 64)
WHERE idempotency_key = '';

ALTER TABLE document_extraction_reviews
    ALTER COLUMN idempotency_key DROP DEFAULT;

ALTER TABLE document_extraction_reviews
    ALTER COLUMN request_fingerprint DROP DEFAULT;

ALTER TABLE document_extraction_reviews
    ADD CONSTRAINT document_extraction_reviews_idempotency_key_check
        CHECK (length(btrim(idempotency_key)) BETWEEN 1 AND 255),
    ADD CONSTRAINT document_extraction_reviews_fingerprint_check
        CHECK (request_fingerprint ~ '^[0-9a-fA-F]{64}$');

CREATE UNIQUE INDEX document_extraction_reviews_tenant_idempotency_key
    ON document_extraction_reviews (tenant_id, idempotency_key);
