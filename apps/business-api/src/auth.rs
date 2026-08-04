//! Authentication and tenant context extraction.
//!
//! Development mode: accepts a static dev token and extracts tenant from headers.
//! Production mode: validates JWT against OIDC issuer (skeleton for now).

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use shared_kernel::error::AppError;
use shared_kernel::tenant::TenantContext;
use std::collections::BTreeSet;
use std::str::FromStr;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPrincipal {
    pub tenant_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub permissions: BTreeSet<ManagementPermission>,
}

impl AuthenticatedPrincipal {
    #[must_use]
    pub fn has_permission(&self, permission: ManagementPermission) -> bool {
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
}

/// Middleware that extracts and validates authentication.
///
/// In development mode (when `dev_auth_enabled` is true):
/// - Accepts any Bearer token that equals the configured `dev_secret`
/// - Reads `tenant_id` from the `X-Tenant-Id` header
/// - Reads `user_id` from the `X-User-Id` header
///
/// In production mode:
/// - Would validate JWT against the OIDC issuer (not yet implemented)
/// - For now, rejects all requests (fail-closed)
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
        // Production OIDC/JWT validation is not implemented yet. Fail closed
        // rather than accidentally granting access.
        tracing::warn!("production authentication requested but OIDC validation is not yet implemented; rejecting request");
        Err(unauthorized(
            AuthError::InvalidToken,
            "authentication not available",
        ))
    }
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

/// Development-mode authentication: compare against the static dev secret and
/// resolve tenant context from headers.
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

    let tenant = header_str(request, "x-tenant-id");
    let user = header_str(request, "x-user-id");

    let (Some(tenant_id), Some(user_id)) = (tenant, user) else {
        return Err(unauthorized(
            AuthError::MissingTenant,
            "missing X-Tenant-Id or X-User-Id header",
        ));
    };

    let tenant_id: uuid::Uuid = tenant_id
        .parse()
        .map_err(|_| unauthorized(AuthError::MissingTenant, "invalid tenant context"))?;
    let user_id: uuid::Uuid = user_id
        .parse()
        .map_err(|_| unauthorized(AuthError::MissingTenant, "invalid user context"))?;
    let tenant_context = TenantContext::new(tenant_id.to_string(), user_id.to_string());
    request.extensions_mut().insert(tenant_context);
    request.extensions_mut().insert(AuthenticatedPrincipal {
        tenant_id,
        user_id,
        permissions: config.dev_permissions.clone(),
    });
    Ok(())
}

/// Read a header value as a trimmed, non-empty UTF-8 string.
fn header_str(request: &Request, name: &str) -> Option<String> {
    let value = request.headers().get(name)?.to_str().ok()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
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
        let config = AuthMiddlewareConfig {
            dev_auth_enabled: true,
            dev_secret: Some("secret".to_string()),
            dev_permissions: permissions,
        };
        let tenant = uuid::Uuid::new_v4();
        let user = uuid::Uuid::new_v4();
        let mut request = Request::builder()
            .header("x-tenant-id", tenant.to_string())
            .header("x-user-id", user.to_string())
            .header("x-management-permissions", "repair.execute,repair.approve")
            .body(Body::empty())
            .unwrap_or_else(|_| unreachable!());
        authenticate_dev(&config, "secret", &mut request).unwrap_or_else(|_| unreachable!());
        let principal = request
            .extensions()
            .get::<AuthenticatedPrincipal>()
            .unwrap_or_else(|| unreachable!());
        assert_eq!(principal.tenant_id, tenant);
        assert_eq!(principal.user_id, user);
        assert!(principal.has_permission(ManagementPermission::IntegrityRead));
        assert!(!principal.has_permission(ManagementPermission::RepairExecute));
        assert!(!principal.has_permission(ManagementPermission::RepairApprove));
    }
}
