//! Minimal Organization bounded context for authorization scoping
//! (PLAN-0013).
//!
//! Owns only what policy needs: an `OrganizationUnit` tree (same-tenant
//! parents, no cycles) and `OrganizationMembership` links. This is **not** an
//! HR system: no positions, jobs, salary, performance, or attendance.
//!
//! DDD layering: `domain` (entities + invariants), `application` (use cases
//! over ports), `ports` (persistence contracts). Adapters live in
//! `organization-postgres` / `organization-sqlite`.

pub mod application;
pub mod domain;
pub mod ports;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
