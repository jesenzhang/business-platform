//! Consumer of the **policy module release candidate**.
//!
//! This crate lives outside the platform workspace and depends only on the
//! packed artifact directory. It resolves, compiles and calls the module's
//! published authorisation contract without seeing `crates/policy`, any
//! `*-sqlite` / `*-postgres` adapter or any `apps/*` crate.
//!
//! The contract exercised here is the `authorize` published query
//! ([`policy::application::Authorize`]), evaluated against the module's own
//! `testing` feature ports. Authorisation semantics matter for this proof: a
//! released contract must deny by default, so the assertions below check
//! fail-closed behaviour rather than a happy path only.

use std::sync::Arc;

use policy::application::{AuthorizationContext, Authorize, PolicyApplicationError};
use policy::domain::PolicyDecision;
use policy::testing::FakePolicyPorts;

/// Builds the published use case from the module's in-memory fake ports.
#[must_use]
pub fn authorize(ports: &FakePolicyPorts) -> Authorize {
    Authorize::new(
        Arc::clone(&ports.query),
        Arc::clone(&ports.subject),
        Arc::clone(&ports.org),
    )
}

/// Calls the published contract once, without a concrete resource.
pub async fn check(
    authorize: &Authorize,
    context: &AuthorizationContext,
    permission: &str,
) -> Result<PolicyDecision, PolicyApplicationError> {
    authorize.check(context, permission, None).await
}
