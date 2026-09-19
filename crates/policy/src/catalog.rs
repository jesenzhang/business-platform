//! The versioned permission catalog and the two immutable system roles.
//!
//! This module is the single Rust-side source of truth for seed content.
//! The Stage-5 migration seeds PostgreSQL/SQLite from exactly these lists
//! (`ON CONFLICT DO NOTHING`); the fakes seed themselves from here too, so
//! catalog drift between code, migration, and tests is structurally
//! impossible.
//!
//! Reconciliation note (Stage 3): the preflight §4 list names nine IAM keys;
//! `identity.user.manage` is added as the tenth because the management
//! plane's disable/enable-user use case needs its own key — bundling it
//! under `identity.membership.update` would make user-level and
//! membership-level authority indistinguishable in audit. The governance
//! seven and reserved keys are unchanged from the lock.

use std::collections::BTreeSet;

/// The seven governance keys migrated from the fixed `ManagementPermission`
/// enum (compat bridge inputs and built-in seed; active).
pub const GOVERNANCE_PERMISSIONS: [(&str, &str); 7] = [
    ("audit.read", "Read the unified audit trail"),
    ("integrity.read", "Read integrity scan results"),
    ("integrity.scan", "Run integrity scans"),
    ("repair.dry-run", "Create and preview repair dry-runs"),
    ("repair.execute", "Execute approved repairs"),
    ("repair.approve", "Approve repair executions"),
    ("repair.cancel", "Cancel pending repairs"),
];

/// IAM keys required to operate PLAN-0013's management plane (active).
pub const IAM_PERMISSIONS: [(&str, &str); 10] = [
    (
        "identity.read",
        "Read users, memberships, and external identities",
    ),
    ("identity.user.manage", "Disable or enable platform users"),
    (
        "identity.membership.update",
        "Create, suspend, and reactivate tenant memberships",
    ),
    (
        "organization.read",
        "Read the organization tree and memberships",
    ),
    (
        "organization.manage",
        "Create, move, and staff organization units",
    ),
    ("policy.role.read", "Read roles, permissions, and bindings"),
    (
        "policy.role.manage",
        "Create roles and change role permissions",
    ),
    ("policy.binding.read", "Read role bindings"),
    ("policy.binding.manage", "Bind and revoke roles"),
    ("policy.explain", "Explain authorization decisions"),
];

/// Defined-but-reserved keys: catalogued now so bindings/UIs are
/// consistent, with no consuming use case yet (an allow therefore performs
/// no business operation). Lit up by later plans and the Stage-13 fixture.
pub const RESERVED_PERMISSIONS: [(&str, &str); 7] = [
    (
        "document.read",
        "Read documents (reserved: document context)",
    ),
    (
        "document.review",
        "Review documents (reserved: document context)",
    ),
    (
        "contract.read",
        "Read contracts (reserved: contract context)",
    ),
    (
        "contract.create",
        "Create contracts (reserved: contract context)",
    ),
    (
        "contract.update",
        "Update contracts (reserved: contract context)",
    ),
    (
        "contract.review",
        "Review contracts (reserved: contract context)",
    ),
    (
        "contract.archive",
        "Archive contracts (reserved: contract context)",
    ),
];

/// Stable keys of the two immutable system roles (migration-seeded only).
pub const SYSTEM_BOOTSTRAP_ADMIN_KEY: &str = "system.bootstrap-admin";
pub const SYSTEM_PLATFORM_ADMIN_KEY: &str = "system.platform-admin";

/// Every catalogued permission as `(key, description, reserved)`.
#[must_use]
pub fn catalog() -> Vec<(&'static str, &'static str, bool)> {
    GOVERNANCE_PERMISSIONS
        .iter()
        .map(|(key, description)| (*key, *description, false))
        .chain(IAM_PERMISSIONS.iter().map(|(key, d)| (*key, *d, false)))
        .chain(RESERVED_PERMISSIONS.iter().map(|(key, d)| (*key, *d, true)))
        .collect()
}

/// Permission keys granted by both system roles: governance + IAM (the
/// reserved keys are deliberately excluded — platform admins earn them
/// through the owning contexts later).
#[must_use]
pub fn system_role_permissions() -> BTreeSet<String> {
    GOVERNANCE_PERMISSIONS
        .iter()
        .chain(IAM_PERMISSIONS.iter())
        .map(|(key, _)| (*key).to_string())
        .collect()
}

/// True for the IAM-management permission keys — the keys that let a
/// principal change who holds authority. Binding such a role to oneself,
/// or adding such a permission to a role one holds, is blocked by the
/// self-escalation guards (the only exception is a bootstrap-sourced actor
/// — an active binding to an immutable system role).
#[must_use]
pub fn is_iam_management_permission(key: &str) -> bool {
    matches!(
        key,
        "identity.user.manage"
            | "identity.membership.update"
            | "organization.manage"
            | "policy.role.manage"
            | "policy.binding.manage"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PermissionKey;

    #[test]
    fn catalog_keys_all_parse_and_are_unique() {
        let mut seen = BTreeSet::new();
        for (key, description, _) in catalog() {
            assert!(
                PermissionKey::parse(key).is_ok(),
                "catalog key {key} must parse"
            );
            assert!(!description.is_empty(), "{key} needs a description");
            assert!(seen.insert(key), "duplicate catalog key {key}");
        }
        assert_eq!(seen.len(), 24);
    }

    #[test]
    fn iam_management_subset_is_within_the_iam_list() {
        for (key, _) in IAM_PERMISSIONS {
            let in_subset = is_iam_management_permission(key);
            let expected = matches!(
                key,
                "identity.user.manage"
                    | "identity.membership.update"
                    | "organization.manage"
                    | "policy.role.manage"
                    | "policy.binding.manage"
            );
            assert_eq!(in_subset, expected, "{key} classification");
        }
        // Governance and reserved keys are never management.
        for (key, _) in GOVERNANCE_PERMISSIONS
            .iter()
            .chain(RESERVED_PERMISSIONS.iter())
        {
            assert!(!is_iam_management_permission(key), "{key} misclassified");
        }
    }

    #[test]
    fn system_role_set_excludes_reserved() {
        let granted = system_role_permissions();
        assert_eq!(granted.len(), 17);
        for (key, _, reserved) in catalog() {
            assert_eq!(granted.contains(key), !reserved);
        }
    }
}
