-- PLAN-0013 organization foundation (SQLite, local single-process only).
-- Mirrors the organization tables of
-- migrations/019_identity_authorization_foundation.sql; user references
-- are bare TEXT ids (no cross-context FK, no ordering dependency on the
-- identity catalog).

CREATE TABLE IF NOT EXISTS organization_units (
    unit_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    parent_id TEXT REFERENCES organization_units(unit_id),
    unit_type TEXT NOT NULL CHECK (unit_type IN ('company', 'department', 'team')),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    status TEXT NOT NULL CHECK (status IN ('active', 'disabled')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (updated_at >= created_at),
    CHECK (parent_id IS NULL OR parent_id <> unit_id)
);

CREATE INDEX IF NOT EXISTS ix_organization_units_tenant_parent
    ON organization_units (tenant_id, parent_id);

CREATE TABLE IF NOT EXISTS organization_members (
    membership_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    unit_id TEXT NOT NULL REFERENCES organization_units(unit_id),
    membership_type TEXT NOT NULL CHECK (membership_type IN ('member', 'leader')),
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    joined_at TEXT NOT NULL,
    deactivated_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (tenant_id, unit_id, user_id, membership_type),
    CHECK (deactivated_at IS NULL OR deactivated_at >= joined_at)
);

CREATE INDEX IF NOT EXISTS ix_organization_members_tenant_user
    ON organization_members (tenant_id, user_id);

CREATE TABLE IF NOT EXISTS organization_idempotency (
    tenant_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL CHECK (length(request_fingerprint) = 64),
    result_kind TEXT NOT NULL CHECK (result_kind IN ('unit', 'member')),
    result_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, operation, idempotency_key)
);
