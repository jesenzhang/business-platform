-- Persist review idempotency at the Document Intelligence transaction boundary.
ALTER TABLE document_extraction_reviews
    ADD COLUMN idempotency_key TEXT NOT NULL DEFAULT '';

ALTER TABLE document_extraction_reviews
    ADD COLUMN request_fingerprint TEXT NOT NULL DEFAULT '';

-- Existing reviews predate the idempotency contract. Give each one an
-- immutable, non-reusable legacy key so the new uniqueness constraint is safe.
UPDATE document_extraction_reviews
SET idempotency_key = 'legacy-review-' || id,
    request_fingerprint = substr(replace(id, '-', '') || printf('%064d', 0), 1, 64)
WHERE idempotency_key = '';

CREATE UNIQUE INDEX document_extraction_reviews_tenant_idempotency_key
    ON document_extraction_reviews (tenant_id, idempotency_key);
