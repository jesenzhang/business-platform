//! Platform Identity bounded context (PLAN-0013).
//!
//! Owns `PlatformUser`, `ExternalIdentity`, and `TenantMembership`: who a
//! caller is inside the platform and whether they may act inside a tenant.
//! Authentication stays external (ADR-0024); this context maps an already
//! authenticated external subject to durable platform identity state. It owns
//! no credentials and never signs or validates tokens.
//!
//! DDD layering: `domain` (entities + invariants), `application` (use cases
//! over ports), `ports` (persistence contracts). Adapters live in
//! `identity-postgres` / `identity-sqlite`.

pub mod application;
pub mod domain;
pub mod ports;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
