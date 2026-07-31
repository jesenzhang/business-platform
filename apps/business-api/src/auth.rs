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

/// Configuration for the authentication middleware.
///
/// Derived from the API process configuration at startup.
#[derive(Clone)]
pub struct AuthMiddlewareConfig {
    /// When true, accept the static `dev_secret` token (development only).
    pub dev_auth_enabled: bool,
    /// The static development token. Must be `None` in production.
    pub dev_secret: Option<String>,
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

    let tenant_context = TenantContext::new(tenant_id, user_id);
    request.extensions_mut().insert(tenant_context);
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
