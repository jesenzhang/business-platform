PRAGMA journal_mode = WAL;

CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    original_filename TEXT NOT NULL CHECK(length(trim(original_filename)) > 0),
    content_type TEXT NOT NULL CHECK(length(trim(content_type)) > 0),
    object_key TEXT NOT NULL CHECK(length(trim(object_key)) > 0),
    status TEXT NOT NULL CHECK(status IN ('active', 'archived', 'deleted')),
    version INTEGER NOT NULL CHECK(version > 0),
    size_bytes INTEGER CHECK(size_bytes IS NULL OR size_bytes >= 0),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_documents_tenant_cursor
    ON documents (tenant_id, created_at DESC, id DESC);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE outbox_events (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    published INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE document_idempotency (
    tenant_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    fingerprint_version INTEGER NOT NULL CHECK(fingerprint_version > 0),
    document_id TEXT NOT NULL REFERENCES documents(id),
    PRIMARY KEY (tenant_id, idempotency_key)
);
