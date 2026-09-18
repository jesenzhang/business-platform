-- PLAN-0013 identity-management foundation (SQLite, local single-process
-- only). Mirrors migrations/019_identity_authorization_foundation.sql for
-- the identity tables; cross-context references are bare TEXT ids.
-- audit_events is provided by the chained document-sqlite catalog.

CREATE TABLE IF NOT EXISTS platform_users (
    user_id TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('active', 'disabled')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (updated_at >= created_at)
);

CREATE TABLE IF NOT EXISTS external_identities (
    external_identity_id TEXT PRIMARY KEY,
    issuer TEXT NOT NULL CHECK (length(trim(issuer)) > 0),
    subject TEXT NOT NULL CHECK (length(trim(subject)) > 0),
    user_id TEXT NOT NULL REFERENCES platform_users(user_id),
    linked_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (issuer, subject),
    UNIQUE (user_id)
);

CREATE TABLE IF NOT EXISTS tenant_memberships (
    membership_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES platform_users(user_id),
    status TEXT NOT NULL CHECK (status IN ('active', 'suspended')),
    joined_at TEXT NOT NULL,
    suspended_at TEXT,
    source TEXT NOT NULL CHECK (source IN ('bootstrap', 'admin', 'migration')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (tenant_id, user_id),
    CHECK (suspended_at IS NULL OR suspended_at >= joined_at)
);

CREATE INDEX IF NOT EXISTS ix_tenant_memberships_tenant_joined
    ON tenant_memberships (tenant_id, joined_at DESC, membership_id DESC);
CREATE INDEX IF NOT EXISTS ix_tenant_memberships_tenant_status
    ON tenant_memberships (tenant_id, status);

CREATE TABLE IF NOT EXISTS identity_idempotency (
    tenant_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL CHECK (length(request_fingerprint) = 64),
    result_kind TEXT NOT NULL CHECK (result_kind IN ('membership', 'user')),
    result_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, operation, idempotency_key)
);

CREATE TABLE IF NOT EXISTS platform_bootstrap_executions (
    tenant_id TEXT NOT NULL,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    role_stable_key TEXT NOT NULL,
    config_version INTEGER NOT NULL CHECK (config_version >= 1),
    config_digest TEXT NOT NULL CHECK (length(config_digest) = 64),
    outcome TEXT NOT NULL CHECK (outcome IN ('executed', 'no_op', 'failed')),
    recorded_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, issuer, subject, config_digest)
);

CREATE INDEX IF NOT EXISTS ix_bootstrap_lookup
    ON platform_bootstrap_executions (tenant_id, issuer, subject, config_version DESC);
