-- PLAN-0013 policy foundation (SQLite, local single-process only).
-- Mirrors the policy tables of
-- migrations/019_identity_authorization_foundation.sql, including the
-- catalog and immutable system-role seeds (ids are UUIDv5 over
-- "policy-system-role:<stable-key>" in the URL namespace, identical to the
-- PostgreSQL root migration and crates/policy/src/catalog.rs consumers).
-- User and org-unit references are bare TEXT ids (no cross-context FK).

CREATE TABLE IF NOT EXISTS permission_definitions (
    stable_key TEXT PRIMARY KEY
        CHECK (length(stable_key) <= 128 AND instr(stable_key, '.') > 0),
    description TEXT NOT NULL CHECK (length(trim(description)) > 0),
    reserved INTEGER NOT NULL DEFAULT 0 CHECK (reserved IN (0, 1)),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1))
);

CREATE TABLE IF NOT EXISTS roles (
    role_id TEXT PRIMARY KEY,
    tenant_id TEXT,
    stable_key TEXT NOT NULL CHECK (length(stable_key) <= 128),
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    status TEXT NOT NULL CHECK (status IN ('active', 'disabled')),
    system INTEGER NOT NULL CHECK (system IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK ((system = 1 AND tenant_id IS NULL) OR (system = 0 AND tenant_id IS NOT NULL)),
    -- The system. namespace belongs to global roles only.
    CHECK (tenant_id IS NULL OR stable_key NOT LIKE 'system.%'),
    CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_roles_system_key
    ON roles (stable_key) WHERE tenant_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS ux_roles_tenant_key
    ON roles (tenant_id, stable_key) WHERE tenant_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS role_permissions (
    role_id TEXT NOT NULL REFERENCES roles(role_id),
    permission_key TEXT NOT NULL REFERENCES permission_definitions(stable_key),
    PRIMARY KEY (role_id, permission_key)
);

CREATE TABLE IF NOT EXISTS role_bindings (
    binding_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL REFERENCES roles(role_id),
    scope_kind TEXT NOT NULL
        CHECK (scope_kind IN ('tenant', 'organization_unit', 'resource_type', 'resource')),
    scope_org_unit_id TEXT,
    scope_include_subtree INTEGER NOT NULL DEFAULT 0 CHECK (scope_include_subtree IN (0, 1)),
    scope_resource_kind TEXT,
    scope_resource_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked')),
    effective_at TEXT NOT NULL,
    expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (expires_at IS NULL OR expires_at > effective_at),
    CHECK (updated_at >= created_at),
    CHECK (
        (scope_kind = 'tenant'
            AND scope_org_unit_id IS NULL AND scope_resource_kind IS NULL
            AND scope_resource_id IS NULL)
        OR (scope_kind = 'organization_unit'
            AND scope_org_unit_id IS NOT NULL AND scope_resource_kind IS NULL
            AND scope_resource_id IS NULL)
        OR (scope_kind = 'resource_type'
            AND scope_org_unit_id IS NULL AND scope_resource_kind IS NOT NULL
            AND scope_resource_id IS NULL)
        OR (scope_kind = 'resource'
            AND scope_org_unit_id IS NULL AND scope_resource_kind IS NOT NULL
            AND scope_resource_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS ix_role_bindings_tenant_user
    ON role_bindings (tenant_id, user_id, status);
CREATE INDEX IF NOT EXISTS ix_role_bindings_tenant_role
    ON role_bindings (tenant_id, role_id);
CREATE INDEX IF NOT EXISTS ix_role_permissions_permission_key
    ON role_permissions (permission_key, role_id);

CREATE TABLE IF NOT EXISTS policy_idempotency (
    tenant_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL CHECK (length(request_fingerprint) = 64),
    result_kind TEXT NOT NULL CHECK (result_kind IN ('role', 'binding')),
    result_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, operation, idempotency_key)
);

-- Seeds (mirror crates/policy/src/catalog.rs; replays are harmless) --------

INSERT INTO permission_definitions (stable_key, description, reserved, active) VALUES
    ('audit.read', 'Read the unified audit trail', 0, 1),
    ('integrity.read', 'Read integrity scan results', 0, 1),
    ('integrity.scan', 'Run integrity scans', 0, 1),
    ('repair.dry-run', 'Create and preview repair dry-runs', 0, 1),
    ('repair.execute', 'Execute approved repairs', 0, 1),
    ('repair.approve', 'Approve repair executions', 0, 1),
    ('repair.cancel', 'Cancel pending repairs', 0, 1),
    ('identity.read', 'Read users, memberships, and external identities', 0, 1),
    ('identity.user.manage', 'Disable or enable platform users', 0, 1),
    ('identity.membership.update', 'Create, suspend, and reactivate tenant memberships', 0, 1),
    ('organization.read', 'Read the organization tree and memberships', 0, 1),
    ('organization.manage', 'Create, move, and staff organization units', 0, 1),
    ('policy.role.read', 'Read roles, permissions, and bindings', 0, 1),
    ('policy.role.manage', 'Create roles and change role permissions', 0, 1),
    ('policy.binding.read', 'Read role bindings', 0, 1),
    ('policy.binding.manage', 'Bind and revoke roles', 0, 1),
    ('policy.explain', 'Explain authorization decisions', 0, 1),
    ('document.read', 'Read documents (reserved: document context)', 1, 1),
    ('document.review', 'Review documents (reserved: document context)', 1, 1),
    ('contract.read', 'Read contracts (reserved: contract context)', 1, 1),
    ('contract.create', 'Create contracts (reserved: contract context)', 1, 1),
    ('contract.update', 'Update contracts (reserved: contract context)', 1, 1),
    ('contract.review', 'Review contracts (reserved: contract context)', 1, 1),
    ('contract.archive', 'Archive contracts (reserved: contract context)', 1, 1)
ON CONFLICT (stable_key) DO NOTHING;

INSERT INTO roles
    (role_id, tenant_id, stable_key, display_name, status, system, created_at, updated_at, version)
VALUES
    ('2e8307f4-74ef-5912-9778-2f5d31ce6005', NULL, 'system.bootstrap-admin',
     'Bootstrap Admin', 'active', 1, strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'), 1),
    ('1c5342ac-8f77-57d0-a53b-8ee43bbe650e', NULL, 'system.platform-admin',
     'Platform Admin', 'active', 1, strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'), 1)
ON CONFLICT (role_id) DO NOTHING;

INSERT INTO role_permissions (role_id, permission_key)
SELECT r.role_id, p.stable_key
FROM roles r
CROSS JOIN permission_definitions p
WHERE r.tenant_id IS NULL
  AND r.stable_key IN ('system.bootstrap-admin', 'system.platform-admin')
  AND p.reserved = 0
ON CONFLICT (role_id, permission_key) DO NOTHING;
