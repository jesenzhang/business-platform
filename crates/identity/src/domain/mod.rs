//! Identity domain: entities, invariants, and domain errors.

mod error;
mod external_identity;
mod membership;
mod user;
mod version;

pub use error::IdentityDomainError;
pub use external_identity::{ExternalIdentity, MAX_EXTERNAL_ISSUER_LEN, MAX_EXTERNAL_SUBJECT_LEN};
pub use membership::{MembershipSource, MembershipStatus, TenantMembership};
pub use user::{PlatformUser, UserLifecycleStatus};
pub use version::AggregateVersion;
