-- Document metadata table for the first vertical slice (WP-10).
CREATE TABLE documents (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    original_filename VARCHAR(500) NOT NULL,
    content_type VARCHAR(200) NOT NULL,
    object_key VARCHAR(1024) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 1,
    size_bytes BIGINT,
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_documents_tenant ON documents (tenant_id, created_at DESC);
CREATE INDEX idx_documents_status ON documents (tenant_id, status);
