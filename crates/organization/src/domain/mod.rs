//! Organization domain: entities, invariants, and domain errors.

mod error;
mod membership;
mod unit;
mod version;

pub use error::OrganizationDomainError;
pub use membership::{
    OrganizationMembership, OrganizationMembershipStatus, OrganizationMembershipType,
    RehydrateOrganizationMembership,
};
pub use unit::{
    validate_tree_placement, OrganizationUnit, OrganizationUnitStatus, OrganizationUnitType,
    RehydrateOrganizationUnit, MAX_UNIT_DEPTH, MAX_UNIT_NAME_LEN,
};
pub use version::AggregateVersion;
