//! Policy bounded context for internal authorization (PLAN-0013).
//!
//! Owns the generic authorization definitions and bindings:
//! `PermissionDefinition` (catalog), `RoleDefinition`, `RolePermission`
//! (set), `RoleBinding` + `ResourceScope`, and the `Authorize` /
//! `ExplainDecision` evaluator. Decisions are **default DENY**: unknown
//! permission, missing membership, revoked/expired binding, disabled role,
//! or any store failure all deny.
//!
//! Hard boundaries (PLAN-0013 locks):
//! - Authentication is not authorization: this crate consumes an already
//!   validated subject; OIDC token `roles` have no input path here at all.
//! - Policy is not a domain invariant: business state rules stay with the
//!   owning context; a `PolicyDecision` never mutates business data.
//! - No policy DSL, no ABAC expression language, no `ReBAC` graph — the
//!   bounded [`domain::ResourceScope`] enum is the entire scope model.
//!
//! Adapters live in `policy-postgres` / `policy-sqlite`; this crate stays
//! pure Rust (domain + application + ports + fakes).

pub mod application;
pub mod catalog;
pub mod domain;
pub mod ports;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
