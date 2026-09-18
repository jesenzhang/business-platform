//! `ExternalIdentity` — the stable (issuer, subject) → `PlatformUser` link.
//!
//! Invariant: one `(issuer, subject)` pair points to exactly one user,
//! forever. Uniqueness is enforced by a database unique index (adapters) and
//! the domain rejects malformed links; a same-key re-link attempt to a
//! different user must fail closed (adapter conflict → application error).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::IdentityDomainError;

/// Field cap for the external issuer URL (OIDC `iss` values are URLs; the
/// cap bounds storage and index sizes).
pub const MAX_EXTERNAL_ISSUER_LEN: usize = 2_048;

/// Field cap for the external subject identifier.
pub const MAX_EXTERNAL_SUBJECT_LEN: usize = 1_024;

fn validate_component(value: &str, max_len: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !trimmed.contains('\0') && value.len() <= max_len
}

/// A persisted external identity link. Fields are private; adapters restore
/// state only through [`ExternalIdentity::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentity {
    link_id: Uuid,
    issuer: String,
    subject: String,
    user_id: Uuid,
    linked_at: DateTime<Utc>,
}

impl ExternalIdentity {
    /// Link `issuer`+`subject` to a platform user.
    pub fn link(
        external_identity_id: Uuid,
        issuer: &str,
        subject: &str,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Self, IdentityDomainError> {
        Self::validate(issuer, subject, user_id)?;
        if external_identity_id.is_nil() {
            return Err(IdentityDomainError::InvalidIdentity);
        }
        Ok(Self {
            link_id: external_identity_id,
            issuer: issuer.to_string(),
            subject: subject.to_string(),
            user_id,
            linked_at: now,
        })
    }

    /// Restore a persisted link after full validation.
    pub fn rehydrate(
        external_identity_id: Uuid,
        issuer: String,
        subject: String,
        user_id: Uuid,
        linked_at: DateTime<Utc>,
    ) -> Result<Self, IdentityDomainError> {
        if external_identity_id.is_nil() {
            return Err(IdentityDomainError::InvalidIdentity);
        }
        Self::validate(&issuer, &subject, user_id)?;
        Ok(Self {
            link_id: external_identity_id,
            issuer,
            subject,
            user_id,
            linked_at,
        })
    }

    fn validate(issuer: &str, subject: &str, user_id: Uuid) -> Result<(), IdentityDomainError> {
        if !validate_component(issuer, MAX_EXTERNAL_ISSUER_LEN) {
            return Err(IdentityDomainError::InvalidIssuer);
        }
        if !validate_component(subject, MAX_EXTERNAL_SUBJECT_LEN) {
            return Err(IdentityDomainError::InvalidSubject);
        }
        if user_id.is_nil() {
            return Err(IdentityDomainError::InvalidExternalIdentityLink);
        }
        Ok(())
    }

    #[must_use]
    pub const fn external_identity_id(&self) -> Uuid {
        self.link_id
    }

    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub const fn user_id(&self) -> Uuid {
        self.user_id
    }

    #[must_use]
    pub const fn linked_at(&self) -> DateTime<Utc> {
        self.linked_at
    }

    /// True when this link is exactly the `(issuer, subject)` key the caller
    /// authenticated with.
    #[must_use]
    pub fn matches(&self, issuer: &str, subject: &str) -> bool {
        self.issuer == issuer && self.subject == subject
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(100, 0)
            .single()
            .unwrap_or_else(|| unreachable!())
    }

    fn id(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    #[test]
    fn link_accepts_well_formed_key() {
        let link = ExternalIdentity::link(id(1), "https://idp.example", "user-1", id(2), now())
            .unwrap_or_else(|_| unreachable!());
        assert!(link.matches("https://idp.example", "user-1"));
        assert!(!link.matches("https://other.example", "user-1"));
        assert!(!link.matches("https://idp.example", "user-2"));
    }

    #[test]
    fn rejects_blank_nul_and_oversized_components() {
        assert_eq!(
            ExternalIdentity::link(id(1), "  ", "user-1", id(2), now()),
            Err(IdentityDomainError::InvalidIssuer)
        );
        assert_eq!(
            ExternalIdentity::link(id(1), "https://idp.example", "", id(2), now()),
            Err(IdentityDomainError::InvalidSubject)
        );
        assert_eq!(
            ExternalIdentity::link(id(1), "https://idp.example", "bad\0subject", id(2), now()),
            Err(IdentityDomainError::InvalidSubject)
        );
        let huge_subject = "s".repeat(MAX_EXTERNAL_SUBJECT_LEN + 1);
        assert_eq!(
            ExternalIdentity::link(id(1), "https://idp.example", &huge_subject, id(2), now()),
            Err(IdentityDomainError::InvalidSubject)
        );
    }

    #[test]
    fn rejects_nil_identities() {
        assert_eq!(
            ExternalIdentity::link(id(1), "https://idp.example", "user-1", Uuid::nil(), now()),
            Err(IdentityDomainError::InvalidExternalIdentityLink)
        );
        assert_eq!(
            ExternalIdentity::link(Uuid::nil(), "https://idp.example", "user-1", id(2), now()),
            Err(IdentityDomainError::InvalidIdentity)
        );
    }
}
