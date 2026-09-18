//! Reusable behavior contracts for the PLAN-0013 identity/organization/
//! policy persistence adapters (the `document-persistence-contracts`
//! pattern). Each adapter test target — `PostgreSQL` and `SQLite` — runs the
//! same suite against its own adapters, so "`PostgreSQL` is the production
//! authority, `SQLite` is local" can never become two dialects of behavior.
//!
//! The suites pin exactly what the port modules promise (see the binding
//! `Adapter contract` sections in `identity::ports`, `organization::ports`,
//! and `policy::ports`): unique-key match-and-continue, fail-closed
//! principal mismatch, idempotent replay vs. conflict, optimistic versioning,
//! same-value convergence with no version bump, tenant isolation, keyset
//! pagination, store-side caps, and every status/visibility/revocation rule.
//! They deliberately do not pin anything the ports leave open (audit action
//! strings and row layout are adapter- and migration-owned).

// The contract suites deliberately express each scenario as one linear
// script; the line and argument budgets would only fragment them.
#![allow(clippy::too_many_lines, clippy::too_many_arguments)]

mod identity_contract;
mod organization_contract;
mod policy_contract;

// `IdentityContractPorts` is part of the suite's public surface: adapter
// test targets (identity-postgres / identity-sqlite) must be able to name
// and construct it; only `verify_identity_contract` was re-exported before.
pub use identity_contract::{verify_identity_contract, IdentityContractPorts};
// `OrganizationContractPorts` is part of the suite's public surface: adapter
// test targets (organization-postgres / organization-sqlite) must be able to
// name and construct it.
pub use organization_contract::{verify_organization_contract, OrganizationContractPorts};
// `PolicyContractPorts` is part of the suite's public surface: adapter
// test targets (policy-postgres / policy-sqlite) must be able to name and
// construct it.
pub use policy_contract::{verify_policy_contract, PolicyContractPorts};

/// Whole-second wall clock. `PostgreSQL` `TIMESTAMPTZ` truncates below one
/// microsecond, and the suites compare full records across replay — so all
/// command timestamps must start at whole-second precision to round-trip
/// losslessly on both backends.
#[must_use]
pub(crate) fn second_now() -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    now - chrono::TimeDelta::nanoseconds(i64::from(now.timestamp_subsec_nanos()))
}

/// Assertion helper: contract failures surface as human-readable strings so
/// a failing adapter test names the violated rule directly.
pub(crate) fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string())
    }
}
