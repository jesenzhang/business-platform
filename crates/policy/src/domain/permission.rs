//! `PermissionDefinition` — the catalog of stable business action
//! identifiers that roles may grant.
//!
//! A permission is a *declaration*, not a secret: it names an action the
//! platform recognizes ("audit.read", "contract.review"). Unknown keys
//! never authorize anything — evaluation fails closed — so publishing the
//! catalog is harmless. Reserved keys (e.g. `contract.*` before the
//! Contract context ships) are catalogued so bindings and UIs stay
//! consistent, while no implemented use case consumes them yet.

use super::error::PolicyDomainError;

/// Maximum length of a permission stable key.
pub const MAX_PERMISSION_KEY_LEN: usize = 128;

/// Stable, versionable business action identifier.
///
/// Grammar: 2..=3 dot-separated segments of `[a-z0-9][a-z0-9_-]*`, total
/// length ≤ [`MAX_PERMISSION_KEY_LEN`]. The strict grammar keeps keys
/// embeddable in audit actions and DB check constraints without quoting
/// (no spaces, no control characters, no injection shapes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PermissionKey(String);

impl PermissionKey {
    /// Parse and validate a permission stable key.
    pub fn parse(raw: &str) -> Result<Self, PolicyDomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_PERMISSION_KEY_LEN {
            return Err(PolicyDomainError::InvalidPermissionKey(raw.to_string()));
        }
        let segments: Vec<&str> = trimmed.split('.').collect();
        if segments.len() < 2 || segments.len() > 3 {
            return Err(PolicyDomainError::InvalidPermissionKey(raw.to_string()));
        }
        for segment in &segments {
            if segment.is_empty()
                || !segment.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                || !segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
            {
                return Err(PolicyDomainError::InvalidPermissionKey(raw.to_string()));
            }
        }
        Ok(Self(trimmed.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A catalogued permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDefinition {
    key: PermissionKey,
    description: String,
    /// Reserved keys belong to no implemented use case yet; binding them is
    /// legal but an allow can never produce a business operation until the
    /// owning context consumes the key.
    reserved: bool,
    /// Retired keys (catalog rows kept for audit resolution) evaluate as
    /// `DenyUnknownPermission`; they can never be re-granted.
    active: bool,
}

impl PermissionDefinition {
    pub fn new(
        key: PermissionKey,
        description: impl Into<String>,
        reserved: bool,
    ) -> Result<Self, PolicyDomainError> {
        let description = description.into();
        if description.trim().is_empty() || description.len() > 512 {
            return Err(PolicyDomainError::InvalidText);
        }
        Ok(Self {
            key,
            description,
            reserved,
            active: true,
        })
    }

    /// Restore a catalog row with its persisted active state (adapters and
    /// retirement rehearsal only; there is no runtime retire use case).
    #[must_use]
    pub const fn restored(
        key: PermissionKey,
        description: String,
        reserved: bool,
        active: bool,
    ) -> Self {
        Self {
            key,
            description,
            reserved,
            active,
        }
    }

    #[must_use]
    pub const fn key(&self) -> &PermissionKey {
        &self.key
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub const fn is_reserved(&self) -> bool {
        self.reserved
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_grammar_accepts_catalog_and_rejects_injection_shapes() {
        for good in [
            "audit.read",
            "policy.binding.manage",
            "repair.dry-run",
            "integrity.scan",
            "identity.re_review",
            "document.read-v2",
        ] {
            assert!(PermissionKey::parse(good).is_ok(), "{good} must parse");
        }
        for bad in [
            "",
            "read",
            "a.b.c.d",
            ".read",
            "audit.",
            "audit..read",
            "Audit.read",
            "audit.read; DROP",
            "audit read",
            "audit/read",
            "audit.read\u{0}",
            &format!("a.{}", "b".repeat(MAX_PERMISSION_KEY_LEN)),
        ] {
            assert!(
                PermissionKey::parse(bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
    }
}
