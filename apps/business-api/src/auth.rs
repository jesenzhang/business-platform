//! Authentication and tenant context extraction.
//!
//! Development mode: accepts a static dev token and uses a server-configured
//! fixed identity.
//! Production mode: validates JWT against the configured OIDC issuer via
//! [`crate::oidc::OidcValidator`] (JWKS signature, `exp`, `iss`, `aud`),
//! then maps the verified claims onto the request principal.

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use shared_kernel::error::AppError;
use shared_kernel::tenant::TenantContext;
use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::Arc;

use crate::api_error::ApiError;

/// Authentication error categories.
///
/// Kept for documentation and future structured logging; the middleware maps
/// every variant to a fail-closed `401 Unauthorized` response so clients never
/// receive details that could aid an attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// No `Authorization: Bearer <token>` header was supplied.
    MissingToken,
    /// The supplied token failed validation.
    InvalidToken,
    /// The token was valid but required tenant/user context was missing.
    MissingTenant,
    /// The configured trusted identity is invalid.
    InvalidPrincipal,
}

/// Management permissions are issued by the trusted authentication boundary,
/// never parsed from request-controlled headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ManagementPermission {
    AuditRead,
    IntegrityRead,
    IntegrityScan,
    RepairDryRun,
    RepairExecute,
    RepairApprove,
    RepairCancel,
}

impl ManagementPermission {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuditRead => "audit.read",
            Self::IntegrityRead => "integrity.read",
            Self::IntegrityScan => "integrity.scan",
            Self::RepairDryRun => "repair.dry-run",
            Self::RepairExecute => "repair.execute",
            Self::RepairApprove => "repair.approve",
            Self::RepairCancel => "repair.cancel",
        }
    }
}

impl FromStr for ManagementPermission {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "audit.read" => Ok(Self::AuditRead),
            "integrity.read" => Ok(Self::IntegrityRead),
            "integrity.scan" => Ok(Self::IntegrityScan),
            "repair.dry-run" => Ok(Self::RepairDryRun),
            "repair.execute" => Ok(Self::RepairExecute),
            "repair.approve" => Ok(Self::RepairApprove),
            "repair.cancel" => Ok(Self::RepairCancel),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationType {
    DevelopmentStaticToken,
    Oidc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPrincipal {
    tenant_id: uuid::Uuid,
    user_id: uuid::Uuid,
    subject: String,
    roles: BTreeSet<String>,
    permissions: BTreeSet<ManagementPermission>,
    authentication_type: AuthenticationType,
}

impl AuthenticatedPrincipal {
    pub fn new(
        tenant_id: uuid::Uuid,
        user_id: uuid::Uuid,
        subject: String,
        roles: BTreeSet<String>,
        permissions: BTreeSet<ManagementPermission>,
        authentication_type: AuthenticationType,
    ) -> Result<Self, AuthError> {
        if tenant_id.is_nil()
            || user_id.is_nil()
            || subject.trim().is_empty()
            || roles.iter().any(|role| role.trim().is_empty())
        {
            return Err(AuthError::InvalidPrincipal);
        }
        Ok(Self {
            tenant_id,
            user_id,
            subject,
            roles,
            permissions,
            authentication_type,
        })
    }

    #[must_use]
    pub fn tenant_id(&self) -> uuid::Uuid {
        self.tenant_id
    }

    #[must_use]
    pub fn user_id(&self) -> uuid::Uuid {
        self.user_id
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn roles(&self) -> &BTreeSet<String> {
        &self.roles
    }

    #[must_use]
    pub const fn authentication_type(&self) -> AuthenticationType {
        self.authentication_type
    }

    #[must_use]
    pub fn has_permission(&self, permission: &str) -> bool {
        ManagementPermission::from_str(permission)
            .is_ok_and(|permission| self.permissions.contains(&permission))
    }

    #[must_use]
    pub fn has_management_permission(&self, permission: ManagementPermission) -> bool {
        self.permissions.contains(&permission)
    }
}

/// Configuration for the authentication middleware.
///
/// Derived from the API process configuration at startup.
#[derive(Clone)]
pub struct AuthMiddlewareConfig {
    /// When true, accept the static `dev_secret` token (development only).
    pub dev_auth_enabled: bool,
    /// The static development token. Must be `None` in production.
    pub dev_secret: Option<String>,
    /// Server-side development grants. Client headers are ignored.
    pub dev_permissions: BTreeSet<ManagementPermission>,
    /// Server-side fixed development identity. Request headers never replace it.
    pub dev_tenant_id: Option<uuid::Uuid>,
    pub dev_user_id: Option<uuid::Uuid>,
    pub dev_subject: Option<String>,
    pub dev_roles: BTreeSet<String>,
    /// OIDC validator for production authentication. Required when
    /// `dev_auth_enabled` is false; absent together with dev auth disabled is
    /// a startup misconfiguration that keeps every request rejected.
    pub oidc: Option<Arc<crate::oidc::OidcValidator>>,
}

/// Middleware that extracts and validates authentication.
///
/// In development mode (when `dev_auth_enabled` is true):
/// - Accepts any Bearer token that equals the configured `dev_secret`
/// - Uses the server-configured tenant and user identity
/// - Ignores request-controlled tenant, user, and permission headers
///
/// In production mode:
/// - Validates the JWT signature against the issuer JWKS and enforces
///   `exp`/`iss`/`aud`; verified claims map onto the request principal
/// - Any validation failure is a fail-closed generic `401`
///
/// On success the resolved [`TenantContext`] is inserted into request
/// extensions so downstream handlers and extractors can access it.
pub async fn auth_middleware(
    State(config): State<AuthMiddlewareConfig>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = extract_bearer_token(&request)
        .ok_or_else(|| unauthorized(AuthError::MissingToken, "missing bearer token"))?;

    if config.dev_auth_enabled {
        authenticate_dev(&config, &token, &mut request)?;
        // Do not let a request-controlled compatibility header cross the
        // trusted authentication boundary. Management handlers use the
        // principal extension populated above; downstream middleware must not
        // have a second, ambiguous authorization source.
        request.headers_mut().remove("x-management-permissions");
        Ok(next.run(request).await)
    } else {
        authenticate_oidc(&config, &token, &mut request).await?;
        request.headers_mut().remove("x-management-permissions");
        Ok(next.run(request).await)
    }
}

/// Production-mode authentication: verify the JWT against the configured OIDC
/// issuer and map the verified claims onto the request principal.
///
/// Any failure is fail-closed: a generic `401` without issuer, claim, or JWKS
/// details. A JWKS outage rejects requests instead of bypassing validation.
async fn authenticate_oidc(
    config: &AuthMiddlewareConfig,
    token: &str,
    request: &mut Request,
) -> Result<(), ApiError> {
    let Some(validator) = &config.oidc else {
        tracing::error!("OIDC auth enabled but no validator is configured; rejecting request");
        return Err(unauthorized(
            AuthError::InvalidToken,
            "authentication misconfigured",
        ));
    };
    let principal = validator
        .validate(token)
        .await
        .map_err(|error| unauthorized(error, "invalid token"))?;
    let tenant_context = TenantContext::new(
        principal.tenant_id().to_string(),
        principal.user_id().to_string(),
    );
    request.extensions_mut().insert(tenant_context);
    request.extensions_mut().insert(principal);
    Ok(())
}

/// Extract the bearer token from the `Authorization` header, if present and
/// well-formed.
fn extract_bearer_token(request: &Request) -> Option<String> {
    let header_value = request.headers().get(header::AUTHORIZATION)?;
    let header_str = header_value.to_str().ok()?;
    let token = header_str.strip_prefix("Bearer ")?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_owned())
    }
}

/// Development-mode authentication against a server-configured fixed identity.
fn authenticate_dev(
    config: &AuthMiddlewareConfig,
    token: &str,
    request: &mut Request,
) -> Result<(), ApiError> {
    let expected = config
        .dev_secret
        .as_deref()
        .filter(|secret| !secret.is_empty());

    let Some(expected) = expected else {
        tracing::error!("dev auth enabled but no dev_secret configured; rejecting request");
        return Err(unauthorized(
            AuthError::InvalidToken,
            "authentication misconfigured",
        ));
    };

    // Constant-time comparison is not required for a development-only static
    // token, but we avoid leaking timing information beyond what is trivial.
    if token != expected {
        return Err(unauthorized(AuthError::InvalidToken, "invalid token"));
    }

    let (Some(tenant_id), Some(user_id), Some(subject)) = (
        config.dev_tenant_id,
        config.dev_user_id,
        config.dev_subject.clone(),
    ) else {
        return Err(unauthorized(
            AuthError::InvalidPrincipal,
            "authentication misconfigured",
        ));
    };

    let principal = AuthenticatedPrincipal::new(
        tenant_id,
        user_id,
        subject,
        config.dev_roles.clone(),
        config.dev_permissions.clone(),
        AuthenticationType::DevelopmentStaticToken,
    )
    .map_err(|error| unauthorized(error, "authentication misconfigured"))?;
    let tenant_context = TenantContext::new(tenant_id.to_string(), user_id.to_string());
    request.extensions_mut().insert(tenant_context);
    request.extensions_mut().insert(principal);
    Ok(())
}

/// Build a fail-closed `401 Unauthorized` error. The `kind` is logged for
/// operators but never exposed to the client.
fn unauthorized(kind: AuthError, public_message: &str) -> ApiError {
    tracing::debug!(?kind, "authentication rejected");
    ApiError::from(AppError::Unauthorized(public_message.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;

    #[test]
    fn client_permission_header_cannot_grant_access() {
        let mut permissions = BTreeSet::new();
        permissions.insert(ManagementPermission::IntegrityRead);
        let tenant = uuid::Uuid::new_v4();
        let user = uuid::Uuid::new_v4();
        let config = AuthMiddlewareConfig {
            dev_auth_enabled: true,
            dev_secret: Some("secret".to_string()),
            dev_permissions: permissions,
            dev_tenant_id: Some(tenant),
            dev_user_id: Some(user),
            dev_subject: Some("dev-user".to_string()),
            dev_roles: BTreeSet::new(),
            oidc: None,
        };
        let mut request = Request::builder()
            .header("x-tenant-id", uuid::Uuid::new_v4().to_string())
            .header("x-user-id", uuid::Uuid::new_v4().to_string())
            .header("x-management-permissions", "repair.execute,repair.approve")
            .body(Body::empty())
            .unwrap_or_else(|_| unreachable!());
        authenticate_dev(&config, "secret", &mut request).unwrap_or_else(|_| unreachable!());
        let principal = request
            .extensions()
            .get::<AuthenticatedPrincipal>()
            .unwrap_or_else(|| unreachable!());
        assert_eq!(principal.tenant_id(), tenant);
        assert_eq!(principal.user_id(), user);
        assert!(principal.has_permission("integrity.read"));
        assert!(!principal.has_permission("repair.execute"));
        assert!(!principal.has_permission("repair.approve"));
    }
}
