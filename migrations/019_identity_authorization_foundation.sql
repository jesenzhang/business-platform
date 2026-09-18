-- PLAN-0013 identity and authorization foundation.
-- Owners: identity-management (platform_users, external_identities,
-- tenant_memberships, identity_idempotency, platform_bootstrap_executions),
-- organization (organization_units, organization_members,
-- organization_idempotency), policy (permission_definitions, roles,
-- role_permissions, role_bindings, policy_idempotency).
-- Cross-context references are bare UUIDs by design: no cross-context
-- foreign keys (DATA_OWNERSHIP: other contexts never write owner rows).
-- Seeds mirror crates/policy/src/catalog.rs exactly (ON CONFLICT DO
-- NOTHING keeps replays harmless); system role ids are UUIDv5 over
-- "policy-system-role:<stable-key>" in the URL namespace.

-- identity-management ------------------------------------------------------

CREATE TABLE IF NOT EXISTS platform_users (
    user_id UUID PRIMARY KEY,
    status VARCHAR(16) NOT NULL CHECK (status IN ('active', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (updated_at >= created_at)
);

CREATE TABLE IF NOT EXISTS external_identities (
    external_identity_id UUID PRIMARY KEY,
    issuer VARCHAR(512) NOT NULL CHECK (length(btrim(issuer)) > 0),
    subject VARCHAR(1024) NOT NULL CHECK (length(btrim(subject)) > 0),
    user_id UUID NOT NULL REFERENCES platform_users(user_id),
    linked_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (issuer, subject),
    UNIQUE (user_id)
);

CREATE TABLE IF NOT EXISTS tenant_memberships (
    membership_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES platform_users(user_id),
    status VARCHAR(16) NOT NULL CHECK (status IN ('active', 'suspended')),
    joined_at TIMESTAMPTZ NOT NULL,
    suspended_at TIMESTAMPTZ,
    source VARCHAR(16) NOT NULL CHECK (source IN ('bootstrap', 'admin', 'migration')),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (tenant_id, user_id),
    CHECK (suspended_at IS NULL OR suspended_at >= joined_at)
);

CREATE INDEX IF NOT EXISTS ix_tenant_memberships_tenant_joined
    ON tenant_memberships (tenant_id, joined_at DESC, membership_id DESC);
CREATE INDEX IF NOT EXISTS ix_tenant_memberships_tenant_status
    ON tenant_memberships (tenant_id, status);

CREATE TABLE IF NOT EXISTS identity_idempotency (
    tenant_id UUID NOT NULL,
    operation VARCHAR(64) NOT NULL,
    idempotency_key VARCHAR(255) NOT NULL,
    request_fingerprint CHAR(64) NOT NULL CHECK (request_fingerprint ~ '^[0-9a-fA-F]{64}$'),
    result_kind VARCHAR(32) NOT NULL CHECK (result_kind IN ('membership', 'user')),
    result_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, operation, idempotency_key)
);

-- Bootstrap ledger: identity-owned audit of server-controlled bootstrap
-- executions. Uniqueness on the config digest makes re-runs no-ops even
-- after the produced binding was revoked; only a deliberate config
-- version bump re-arms bootstrap.
CREATE TABLE IF NOT EXISTS platform_bootstrap_executions (
    tenant_id UUID NOT NULL,
    issuer VARCHAR(512) NOT NULL,
    subject VARCHAR(1024) NOT NULL,
    role_stable_key VARCHAR(128) NOT NULL,
    config_version BIGINT NOT NULL CHECK (config_version >= 1),
    config_digest CHAR(64) NOT NULL CHECK (config_digest ~ '^[0-9a-fA-F]{64}$'),
    outcome VARCHAR(16) NOT NULL CHECK (outcome IN ('executed', 'no_op', 'failed')),
    recorded_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, issuer, subject, config_digest)
);

CREATE INDEX IF NOT EXISTS ix_bootstrap_lookup
    ON platform_bootstrap_executions (tenant_id, issuer, subject, config_version DESC);

-- organization --------------------------------------------------------------

CREATE TABLE IF NOT EXISTS organization_units (
    unit_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    parent_id UUID REFERENCES organization_units(unit_id),
    unit_type VARCHAR(16) NOT NULL CHECK (unit_type IN ('company', 'department', 'team')),
    name VARCHAR(256) NOT NULL CHECK (length(btrim(name)) > 0),
    status VARCHAR(16) NOT NULL CHECK (status IN ('active', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (updated_at >= created_at),
    CHECK (parent_id IS NULL OR parent_id <> unit_id)
);

CREATE INDEX IF NOT EXISTS ix_organization_units_tenant_parent
    ON organization_units (tenant_id, parent_id);

CREATE TABLE IF NOT EXISTS organization_members (
    membership_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    unit_id UUID NOT NULL REFERENCES organization_units(unit_id),
    membership_type VARCHAR(16) NOT NULL CHECK (membership_type IN ('member', 'leader')),
    status VARCHAR(16) NOT NULL CHECK (status IN ('active', 'inactive')),
    joined_at TIMESTAMPTZ NOT NULL,
    deactivated_at TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (tenant_id, unit_id, user_id, membership_type),
    CHECK (deactivated_at IS NULL OR deactivated_at >= joined_at)
);

CREATE INDEX IF NOT EXISTS ix_organization_members_tenant_user
    ON organization_members (tenant_id, user_id);

CREATE TABLE IF NOT EXISTS organization_idempotency (
    tenant_id UUID NOT NULL,
    operation VARCHAR(64) NOT NULL,
    idempotency_key VARCHAR(255) NOT NULL,
    request_fingerprint CHAR(64) NOT NULL CHECK (request_fingerprint ~ '^[0-9a-fA-F]{64}$'),
    result_kind VARCHAR(32) NOT NULL CHECK (result_kind IN ('unit', 'member')),
    result_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, operation, idempotency_key)
);

-- policy --------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS permission_definitions (
    stable_key VARCHAR(128) PRIMARY KEY
        CHECK (stable_key ~ '^[a-z0-9][a-z0-9_-]*(\.[a-z0-9][a-z0-9_-]*){1,2}$'),
    description VARCHAR(512) NOT NULL CHECK (length(btrim(description)) > 0),
    reserved BOOLEAN NOT NULL DEFAULT FALSE,
    active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS roles (
    role_id UUID PRIMARY KEY,
    tenant_id UUID,
    stable_key VARCHAR(128) NOT NULL
        CHECK (stable_key ~ '^[a-z0-9][a-z0-9_-]*(\.[a-z0-9][a-z0-9_-]*){0,2}$'),
    display_name VARCHAR(256) NOT NULL CHECK (length(btrim(display_name)) > 0),
    status VARCHAR(16) NOT NULL CHECK (status IN ('active', 'disabled')),
    system BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (system = (tenant_id IS NULL)),
    -- The system. namespace belongs to global roles only.
    CHECK (tenant_id IS NULL OR stable_key NOT LIKE 'system.%'),
    CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_roles_system_key
    ON roles (stable_key) WHERE tenant_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS ux_roles_tenant_key
    ON roles (tenant_id, stable_key) WHERE tenant_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS role_permissions (
    role_id UUID NOT NULL REFERENCES roles(role_id),
    permission_key VARCHAR(128) NOT NULL REFERENCES permission_definitions(stable_key),
    PRIMARY KEY (role_id, permission_key)
);

CREATE TABLE IF NOT EXISTS role_bindings (
    binding_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    user_id UUID NOT NULL,
    role_id UUID NOT NULL REFERENCES roles(role_id),
    scope_kind VARCHAR(24) NOT NULL
        CHECK (scope_kind IN ('tenant', 'organization_unit', 'resource_type', 'resource')),
    scope_org_unit_id UUID,
    scope_include_subtree BOOLEAN NOT NULL DEFAULT FALSE,
    scope_resource_kind VARCHAR(128),
    scope_resource_id UUID,
    status VARCHAR(16) NOT NULL CHECK (status IN ('active', 'revoked')),
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
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
    tenant_id UUID NOT NULL,
    operation VARCHAR(64) NOT NULL,
    idempotency_key VARCHAR(255) NOT NULL,
    request_fingerprint CHAR(64) NOT NULL CHECK (request_fingerprint ~ '^[0-9a-fA-F]{64}$'),
    result_kind VARCHAR(32) NOT NULL CHECK (result_kind IN ('role', 'binding')),
    result_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, operation, idempotency_key)
);

-- Seeds (mirror crates/policy/src/catalog.rs; replays are harmless) --------

INSERT INTO permission_definitions (stable_key, description, reserved, active) VALUES
    ('audit.read', 'Read the unified audit trail', FALSE, TRUE),
    ('integrity.read', 'Read integrity scan results', FALSE, TRUE),
    ('integrity.scan', 'Run integrity scans', FALSE, TRUE),
    ('repair.dry-run', 'Create and preview repair dry-runs', FALSE, TRUE),
    ('repair.execute', 'Execute approved repairs', FALSE, TRUE),
    ('repair.approve', 'Approve repair executions', FALSE, TRUE),
    ('repair.cancel', 'Cancel pending repairs', FALSE, TRUE),
    ('identity.read', 'Read users, memberships, and external identities', FALSE, TRUE),
    ('identity.user.manage', 'Disable or enable platform users', FALSE, TRUE),
    ('identity.membership.update', 'Create, suspend, and reactivate tenant memberships', FALSE, TRUE),
    ('organization.read', 'Read the organization tree and memberships', FALSE, TRUE),
    ('organization.manage', 'Create, move, and staff organization units', FALSE, TRUE),
    ('policy.role.read', 'Read roles, permissions, and bindings', FALSE, TRUE),
    ('policy.role.manage', 'Create roles and change role permissions', FALSE, TRUE),
    ('policy.binding.read', 'Read role bindings', FALSE, TRUE),
    ('policy.binding.manage', 'Bind and revoke roles', FALSE, TRUE),
    ('policy.explain', 'Explain authorization decisions', FALSE, TRUE),
    ('document.read', 'Read documents (reserved: document context)', TRUE, TRUE),
    ('document.review', 'Review documents (reserved: document context)', TRUE, TRUE),
    ('contract.read', 'Read contracts (reserved: contract context)', TRUE, TRUE),
    ('contract.create', 'Create contracts (reserved: contract context)', TRUE, TRUE),
    ('contract.update', 'Update contracts (reserved: contract context)', TRUE, TRUE),
    ('contract.review', 'Review contracts (reserved: contract context)', TRUE, TRUE),
    ('contract.archive', 'Archive contracts (reserved: contract context)', TRUE, TRUE)
ON CONFLICT (stable_key) DO NOTHING;

INSERT INTO roles
    (role_id, tenant_id, stable_key, display_name, status, system, created_at, updated_at, version)
VALUES
    ('2e8307f4-74ef-5912-9778-2f5d31ce6005', NULL, 'system.bootstrap-admin',
     'Bootstrap Admin', 'active', TRUE, now(), now(), 1),
    ('1c5342ac-8f77-57d0-a53b-8ee43bbe650e', NULL, 'system.platform-admin',
     'Platform Admin', 'active', TRUE, now(), now(), 1)
ON CONFLICT (role_id) DO NOTHING;

-- System roles grant every non-reserved catalog key (governance + IAM).
INSERT INTO role_permissions (role_id, permission_key)
SELECT r.role_id, p.stable_key
FROM roles r
CROSS JOIN permission_definitions p
WHERE r.tenant_id IS NULL
  AND r.stable_key IN ('system.bootstrap-admin', 'system.platform-admin')
  AND NOT p.reserved
ON CONFLICT (role_id, permission_key) DO NOTHING;
