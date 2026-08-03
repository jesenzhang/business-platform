-- Stable tenant-scoped keyset pagination for Document read models.
CREATE INDEX idx_documents_tenant_created_id
    ON documents (tenant_id, created_at DESC, id DESC);
