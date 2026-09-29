//! Consumer of the **identity module release candidate**.
//!
//! This crate lives outside the platform workspace and depends only on the
//! packed artifact directory. It exists to answer one question with a compiler
//! instead of a document: can a consumer that never sees `crates/identity`, any
//! `*-sqlite` / `*-postgres` adapter or any `apps/*` crate resolve, compile and
//! actually call a published contract of the released module?
//!
//! What is used here is the module's own published surface:
//! [`identity::application::TenantAccessChecker`], the `check-tenant-access`
//! published query, driven through the module's own `testing` feature ports. No
//! concrete database adapter is involved on either side.

use std::sync::Arc;

use identity::application::{TenantAccess, TenantAccessChecker, TenantAccessError};
use identity::testing::FakeIdentityStores;
use uuid::Uuid;

/// Builds the published use case from the module's in-memory fake ports.
///
/// The ports come from the module crate itself (`testing` feature), which is
/// what lets this consumer stay free of any concrete adapter.
#[must_use]
pub fn tenant_access_checker(stores: &FakeIdentityStores) -> TenantAccessChecker {
    TenantAccessChecker::new(Arc::clone(&stores.query))
}

/// Calls the published contract once.
pub async fn check_tenant_access(
    checker: &TenantAccessChecker,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<TenantAccess, TenantAccessError> {
    checker.check(tenant_id, user_id).await
}
