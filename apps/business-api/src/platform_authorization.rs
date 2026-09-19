//! Platform authorization layer (PLAN-0013 Stage 7).
//!
//! Design (locked by `docs/reviews/PLAN-0013-PREFLIGHT.md` §5):
//! - Authentication stays first and unchanged in trust: this middleware
//!   only runs for requests that already carry an [`AuthenticatedPrincipal`]
//!   produced by [`crate::auth::auth_middleware`].
//! - The caller is resolved through the Identity context
//!   (`ResolveAuthenticatedUser`). The resolve is a write-on-read path; a
//!   store Unavailable failure fails the request closed with a retryable
//!   5xx, a principal conflict/validation fails with 401.
//! - The resolved subject plus the server-trusted compat grants become the
//!   policy [`AuthorizationContext`] attached to the request. Handlers do
//!   not implement authorization rules; they call the Policy `Authorize`
//!   use case through [`authorize_governance`].
//! - Token `roles` have no input path here at all, and `x-*` request
//!   headers remain inert.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use shared_kernel::error::AppError;
use uuid::Uuid;

use identity::application::{
    ResolveAuthenticatedUserCommand, ResolveCallerError, TenantAccessChecker, TenantAccessReason,
};
use organization::ports::{OrganizationQueryPort, OrganizationStoreError};
use policy::{
    application::{AuthorizationContext, ManagementCompatGrant},
    domain::DecisionReason,
    ports::{OrganizationScopePort, PolicyStoreError, SubjectStatus, SubjectStatusPort},
};

use crate::api_error::ApiError;
use crate::auth::{AuthenticatedPrincipal, AuthenticationType, ManagementPermission};
use crate::state::AppState;

/// Fixed server-side issuer namespace for the development static-token
/// principal (preflight §5). It is a constant, never configuration, so a
/// dev-auth caller can never widen it.
pub const DEV_AUTH_ISSUER: &str = "urn:business-api:dev-auth";

/// Map the server-trusted management-permission set onto the bounded
/// compat-grant vocabulary. Unknown strings cannot enter this set (the
/// authentication boundary already dropped them), and the mapping is
/// total, so the bridge can never widen past the seven governance keys.
#[must_use]
pub fn compat_grants_for(principal: &AuthenticatedPrincipal) -> BTreeSet<ManagementCompatGrant> {
    principal
        .management_permissions()
        .iter()
        .filter_map(|permission| ManagementCompatGrant::parse(permission.as_str()))
        .collect()
}

/// The middleware of the locked §5 flow: resolve the authenticated subject
/// through the Identity context and attach the policy
/// [`AuthorizationContext`]. It performs no permission decision — the
/// `Authorize` use case owns every authorization rule.
pub async fn platform_authorization_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(principal) = request
        .extensions()
        .get::<AuthenticatedPrincipal>()
        .cloned()
    else {
        // `auth_middleware` runs strictly before this layer and always
        // inserts the principal; absence is a wiring bug, never a client
        // error to recover from, so it stays fail-closed.
        tracing::error!("platform authorization ran without an authenticated principal");
        return Err(ApiError::from(AppError::Unauthorized(
            "authentication required".to_owned(),
        )));
    };
    let Some(access) = state.access.as_ref() else {
        // Absent services are a misconfiguration, not an outage: fail the
        // platform check closed with 403 (never a bypass).
        tracing::error!("platform authorization services are not configured; rejecting request");
        return Err(ApiError::from(AppError::Forbidden(
            "platform authorization unavailable".to_string(),
        )));
    };
    let issuer = match principal.authentication_type() {
        AuthenticationType::Oidc => access.oidc_issuer.clone(),
        AuthenticationType::DevelopmentStaticToken => DEV_AUTH_ISSUER.to_owned(),
    };
    match access
        .resolve
        .execute(ResolveAuthenticatedUserCommand {
            issuer,
            subject: principal.subject().to_owned(),
            // The authenticated user id is the server-trusted claim value
            // (OIDC `user_id` claim or the dev-auth server config). It is
            // stable per external subject, so repeated requests converge on
            // the same platform user.
            claimed_user_id: Some(principal.user_id()),
        })
        .await
    {
        Ok(resolved) => {
            let grants = if access.compat_enabled {
                compat_grants_for(&principal)
            } else {
                BTreeSet::new()
            };
            request.extensions_mut().insert(
                AuthorizationContext::new(resolved.user.user_id(), principal.tenant_id())
                    .with_compat_grants(grants),
            );
            Ok(next.run(request).await)
        }
        Err(error) => {
            // Never echo the resolver error text to the client.
            tracing::debug!(%error, "platform identity resolution rejected the request");
            match error {
                ResolveCallerError::Validation(_) | ResolveCallerError::PrincipalMismatch => {
                    crate::metrics::record_auth_failure("identity_resolution_failed");
                    Err(ApiError::from(AppError::Unauthorized(
                        "authentication failed".to_owned(),
                    )))
                }
                ResolveCallerError::Unavailable | ResolveCallerError::Failed => {
                    Err(ApiError::service_unavailable("identity"))
                }
            }
        }
    }
}

/// Governance-route guard: delegates the decision to the Policy `Authorize`
/// use case, records the bounded metric pair (§10), and maps the outcome:
/// allow → proceed, `DenyInternal` (or an evaluation error) → retryable
/// 503, every other deny reason → 403 with the pre-PLAN-0013 message.
pub async fn authorize_governance(
    state: &AppState,
    context: &AuthorizationContext,
    permission: ManagementPermission,
) -> Result<(), ApiError> {
    authorize_permission(state, context, permission.as_str()).await
}

/// Permission-key guard shared by every management route (PLAN-0013 §9):
/// identical evaluation, metrics, and error mapping as
/// [`authorize_governance`], but driven by a catalog permission key instead
/// of the fixed `ManagementPermission` enum. The IAM management surface
/// uses this with its catalog keys (`identity.*`, `organization.*`,
/// `policy.*`); governance routes keep calling
/// [`authorize_governance`], which delegates here with
/// `ManagementPermission::as_str()` — one shared decision path, no
/// behavior fork.
pub async fn authorize_permission(
    state: &AppState,
    context: &AuthorizationContext,
    permission: &str,
) -> Result<(), ApiError> {
    let Some(access) = state.access.as_ref() else {
        tracing::error!("platform authorization services are not configured; rejecting request");
        return Err(ApiError::from(AppError::Forbidden(
            "platform authorization unavailable".to_string(),
        )));
    };
    let started = Instant::now();
    let outcome = access.authorize.check(context, permission, None).await;
    crate::metrics::record_authorization_duration(started.elapsed());
    let (allowed, reason) = match &outcome {
        Ok(decision) => (decision.allowed, decision.reason),
        // An evaluation that could not complete is never an allow; treat it
        // like `DenyInternal` (retryable, fail closed).
        Err(error) => {
            tracing::debug!(%error, "policy evaluation failed");
            (false, DecisionReason::DenyInternal)
        }
    };
    crate::metrics::record_authorization_decision(allowed, reason);
    if allowed {
        return Ok(());
    }
    if reason == DecisionReason::DenyInternal {
        return Err(ApiError::service_unavailable("policy"));
    }
    Err(ApiError::from(AppError::Forbidden(
        "management permission required".to_string(),
    )))
}

/// Composition-root bridge implementing the organization
/// `TenantMembershipReader` port over the Identity context's `TenantAccess`
/// use case (organization ports contract: "implemented in the composition
/// root over the Identity context (never by reading identity tables)").
/// Only an Active user with an Active tenant membership counts as a member;
/// suspension and disable both answer "not a member".
pub struct IdentityTenantMembershipBridge {
    checker: Arc<TenantAccessChecker>,
}

impl IdentityTenantMembershipBridge {
    #[must_use]
    pub fn new(checker: Arc<TenantAccessChecker>) -> Self {
        Self { checker }
    }
}

#[async_trait::async_trait]
impl organization::ports::TenantMembershipReader for IdentityTenantMembershipBridge {
    async fn is_active_member(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, OrganizationStoreError> {
        self.checker
            .check(tenant_id, user_id)
            .await
            .map(|access| access.reason == TenantAccessReason::Active)
            .map_err(|error| match error {
                identity::application::TenantAccessError::Unavailable => {
                    OrganizationStoreError::Unavailable
                }
                identity::application::TenantAccessError::Failed => OrganizationStoreError::Failed,
            })
    }
}

/// Composition-root bridge implementing the policy `SubjectStatusPort` over
/// the Identity context's `TenantAccess` use case (policy ports contract:
/// the composition root implements this over the owning context's query
/// ports — never by reading identity tables).
pub struct IdentitySubjectStatusBridge {
    checker: Arc<TenantAccessChecker>,
}

impl IdentitySubjectStatusBridge {
    #[must_use]
    pub fn new(checker: Arc<TenantAccessChecker>) -> Self {
        Self { checker }
    }
}

#[async_trait::async_trait]
impl SubjectStatusPort for IdentitySubjectStatusBridge {
    async fn subject_status(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<SubjectStatus, PolicyStoreError> {
        self.checker
            .check(tenant_id, user_id)
            .await
            .map(|access| match access.reason {
                TenantAccessReason::Active => SubjectStatus::Active,
                // Identity collapses "unknown user" into "no membership";
                // both deny, and `DenyNoMembership` is the honest label.
                TenantAccessReason::NoMembership => SubjectStatus::NoMembership,
                TenantAccessReason::MembershipSuspended => SubjectStatus::MembershipSuspended,
                TenantAccessReason::UserDisabled => SubjectStatus::UserDisabled,
            })
            .map_err(|error| match error {
                identity::application::TenantAccessError::Unavailable => {
                    PolicyStoreError::Unavailable
                }
                identity::application::TenantAccessError::Failed => PolicyStoreError::Failed,
            })
    }
}

/// Maximum parent-chain depth for trusted subtree walks. The organization
/// domain forbids cycles; the bound is defense in depth and fails closed
/// (no grant) rather than erroring wide.
const MAX_ORG_WALK_DEPTH: u32 = 64;

/// Composition-root bridge implementing the policy `OrganizationScopePort`
/// over the Organization context's query port. Scope resolution is the
/// only scope expansion and reads only stored tree data.
pub struct OrganizationScopeBridge {
    units: Arc<dyn OrganizationQueryPort>,
}

impl OrganizationScopeBridge {
    #[must_use]
    pub fn new(units: Arc<dyn OrganizationQueryPort>) -> Self {
        Self { units }
    }

    async fn get_unit(
        &self,
        tenant_id: Uuid,
        unit_id: Uuid,
    ) -> Result<Option<organization::domain::OrganizationUnit>, PolicyStoreError> {
        self.units
            .get_unit(tenant_id, unit_id)
            .await
            .map_err(map_org_error)
    }
}

fn map_org_error(error: OrganizationStoreError) -> PolicyStoreError {
    match error {
        OrganizationStoreError::Unavailable => PolicyStoreError::Unavailable,
        _ => PolicyStoreError::Failed,
    }
}

#[async_trait::async_trait]
impl OrganizationScopePort for OrganizationScopeBridge {
    async fn unit_is_active(
        &self,
        tenant_id: Uuid,
        org_unit_id: Uuid,
    ) -> Result<bool, PolicyStoreError> {
        Ok(self
            .get_unit(tenant_id, org_unit_id)
            .await?
            .is_some_and(|unit| unit.is_active()))
    }

    async fn subtree_contains(
        &self,
        tenant_id: Uuid,
        ancestor: Uuid,
        candidate: Uuid,
    ) -> Result<bool, PolicyStoreError> {
        if ancestor == candidate {
            return self.unit_is_active(tenant_id, candidate).await;
        }
        // Walk from the candidate to the root through trusted parent links.
        let mut current = candidate;
        for _ in 0..MAX_ORG_WALK_DEPTH {
            let Some(unit) = self.get_unit(tenant_id, current).await? else {
                return Ok(false);
            };
            let Some(parent) = unit.parent_id() else {
                return Ok(false);
            };
            if parent == ancestor {
                return Ok(true);
            }
            current = parent;
        }
        // Bounded walk exhausted: fail closed (no expansion).
        Ok(false)
    }
}
