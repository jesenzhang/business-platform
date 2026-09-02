//! OIDC/JWT validation for the production authentication boundary
//! (PLAN-0012 M3).
//!
//! The validator fetches signing keys from the configured issuer's JWKS
//! (OIDC discovery by default, with an explicit `auth.jwks_url` override),
//! caches them with a TTL, refreshes on unknown `kid`, and validates the
//! signature, `exp`, `iss`, and (when configured) `aud` claims.
//!
//! Fail-closed semantics:
//! - only ES256/RS256 are accepted (the `alg=none` and algorithm-confusion
//!   families are rejected before any key lookup);
//! - any validation or key-availability failure maps to `AuthError::InvalidToken`
//!   and therefore a generic `401`, without echoing provider errors;
//! - missing tenant/user context maps to `AuthError::MissingTenant`, also `401`.
//!
//! A temporary JWKS outage rejects requests; it never bypasses validation and
//! never affects liveness/readiness endpoints, which sit outside this
//! middleware.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use tokio::sync::RwLock;

use crate::auth::{AuthError, AuthenticatedPrincipal, AuthenticationType, ManagementPermission};

/// Allowed signature algorithms. Anything else (including `none`) is rejected
/// before key lookup.
const ALLOWED_ALGORITHMS: &[Algorithm] = &[Algorithm::ES256, Algorithm::RS256];
/// JWKS cache lifetime. Key rotation on unknown `kid` triggers an immediate
/// refresh regardless of this TTL.
const JWKS_CACHE_TTL: Duration = Duration::from_secs(600);
/// Discovery/JWKS HTTP timeout.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Clock leeway for `exp`/`nbf` validation.
const CLOCK_LEEWAY_SECONDS: u64 = 60;

struct JwksCache {
    keys: HashMap<String, Jwk>,
    fetched_at: Option<Instant>,
}

impl JwksCache {
    fn fresh(&self) -> bool {
        self.fetched_at
            .is_some_and(|at| at.elapsed() < JWKS_CACHE_TTL)
    }
}

/// OIDC token validator backed by a remote JWKS endpoint.
pub struct OidcValidator {
    client: reqwest::Client,
    issuer_url: String,
    audience: Option<String>,
    jwks_url: Option<String>,
    /// When true, every endpoint this validator contacts (issuer discovery and
    /// JWKS, explicit or discovered) must use `https://`. Config validation
    /// enforces this for production at startup; the validator re-checks every
    /// URL it actually fetches as defense in depth.
    require_https: bool,
    cache: RwLock<JwksCache>,
}

/// Validated claim set mapped onto the request principal.
struct TokenClaims {
    subject: String,
    tenant_id: uuid::Uuid,
    user_id: uuid::Uuid,
    roles: Vec<String>,
    permissions: Vec<ManagementPermission>,
}

impl OidcValidator {
    pub fn new(issuer_url: String, audience: Option<String>, jwks_url: Option<String>) -> Self {
        Self::with_transport_policy(issuer_url, audience, jwks_url, false)
    }

    /// Build a validator with an explicit transport policy. Production
    /// composition must pass `require_https = true` so identity material is
    /// never fetched over plaintext HTTP.
    #[must_use]
    pub fn with_transport_policy(
        issuer_url: String,
        audience: Option<String>,
        jwks_url: Option<String>,
        require_https: bool,
    ) -> Self {
        // Redirects are never followed: discovery and JWKS endpoints must be
        // reachable at their configured locations. A redirect response is
        // treated as an outage (fail closed) instead of silently widening the
        // trust boundary to whatever target the server points at.
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self {
            client,
            issuer_url,
            audience,
            jwks_url,
            require_https,
            cache: RwLock::new(JwksCache {
                keys: HashMap::new(),
                fetched_at: None,
            }),
        }
    }

    /// Validate a bearer token and map its claims onto an
    /// [`AuthenticatedPrincipal`].
    pub async fn validate(&self, token: &str) -> Result<AuthenticatedPrincipal, AuthError> {
        let header = jsonwebtoken::decode_header(token).map_err(|error| {
            tracing::debug!(%error, "OIDC token header is malformed");
            AuthError::InvalidToken
        })?;
        if !ALLOWED_ALGORITHMS.contains(&header.alg) {
            tracing::debug!(alg = ?header.alg, "OIDC token algorithm not allowed");
            return Err(AuthError::InvalidToken);
        }
        let Some(kid) = header.kid.clone() else {
            tracing::debug!("OIDC token header has no kid");
            return Err(AuthError::InvalidToken);
        };

        let jwk = self.jwk_for(&kid).await?;
        let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|error| {
            tracing::debug!(%error, "JWK for kid is not usable as a decoding key");
            AuthError::InvalidToken
        })?;
        let claims = self.decode_claims(token, header.alg, &decoding_key)?;
        let principal = Self::principal_from_claims(claims)?;
        Ok(principal)
    }

    /// Resolve the JWK for `kid`, refreshing the cache when the key is unknown
    /// or the cache is stale.
    async fn jwk_for(&self, kid: &str) -> Result<Jwk, AuthError> {
        {
            let cache = self.cache.read().await;
            if cache.fresh() {
                if let Some(jwk) = cache.keys.get(kid) {
                    return Ok(jwk.clone());
                }
                // Fall through to a forced refresh below: the cached snapshot
                // is fresh but does not contain this key (rotation in flight).
            }
        }
        let mut cache = self.cache.write().await;
        // Another task may have refreshed while we waited for the write lock.
        if cache.fresh() {
            if let Some(jwk) = cache.keys.get(kid) {
                return Ok(jwk.clone());
            }
        }
        let jwks = self.fetch_jwks().await?;
        let mut keys = HashMap::new();
        for jwk in jwks.keys {
            let kid_for_key = jwk.common.key_id.clone().unwrap_or_default();
            if !kid_for_key.is_empty() {
                keys.insert(kid_for_key, jwk);
            }
        }
        let resolved = keys.get(kid).cloned();
        *cache = JwksCache {
            keys,
            fetched_at: Some(Instant::now()),
        };
        resolved.ok_or_else(|| {
            tracing::debug!("OIDC token kid is not present in the issuer JWKS");
            AuthError::InvalidToken
        })
    }

    /// Defense-in-depth transport check applied to every URL actually
    /// fetched, independent of startup config validation.
    fn enforce_https(&self, url: &str) -> Result<(), AuthError> {
        if self.require_https && !url.trim().to_ascii_lowercase().starts_with("https://") {
            tracing::debug!("OIDC transport endpoint must use https under the production policy");
            return Err(AuthError::InvalidToken);
        }
        Ok(())
    }

    async fn fetch_jwks(&self) -> Result<JwkSet, AuthError> {
        let url = match self.jwks_url.as_deref() {
            Some(url) => url.to_owned(),
            None => self.discover_jwks_url().await?,
        };
        self.enforce_https(&url)?;
        let response = self.client.get(&url).send().await.map_err(|error| {
            tracing::debug!(%error, "JWKS fetch failed");
            AuthError::InvalidToken
        })?;
        if !response.status().is_success() {
            tracing::debug!(status = %response.status(), "JWKS endpoint returned an error status");
            return Err(AuthError::InvalidToken);
        }
        response.json::<JwkSet>().await.map_err(|error| {
            tracing::debug!(%error, "JWKS payload is not a valid JWK set");
            AuthError::InvalidToken
        })
    }

    /// Resolve `jwks_uri` through OIDC discovery
    /// (`{issuer}/.well-known/openid-configuration`).
    async fn discover_jwks_url(&self) -> Result<String, AuthError> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.issuer_url.trim_end_matches('/')
        );
        self.enforce_https(&url)?;
        let response = self.client.get(&url).send().await.map_err(|error| {
            tracing::debug!(%error, "OIDC discovery fetch failed");
            AuthError::InvalidToken
        })?;
        if !response.status().is_success() {
            tracing::debug!(status = %response.status(), "OIDC discovery returned an error status");
            return Err(AuthError::InvalidToken);
        }
        let document: serde_json::Value = response.json().await.map_err(|error| {
            tracing::debug!(%error, "OIDC discovery payload is not valid JSON");
            AuthError::InvalidToken
        })?;
        let jwks_uri = document
            .get("jwks_uri")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                tracing::debug!("OIDC discovery document has no jwks_uri");
                AuthError::InvalidToken
            })?;
        Ok(jwks_uri.to_owned())
    }

    /// Verify signature, `exp`, `iss` and (when configured) `aud`, then decode
    /// the claim set.
    fn decode_claims(
        &self,
        token: &str,
        algorithm: Algorithm,
        key: &DecodingKey,
    ) -> Result<TokenClaims, AuthError> {
        let mut validation = Validation::new(algorithm);
        validation.leeway = CLOCK_LEEWAY_SECONDS;
        validation.set_issuer(&[self.issuer_url.as_str()]);
        match self.audience.as_deref() {
            Some(audience) => validation.set_audience(&[audience]),
            None => validation.validate_aud = false,
        }
        let data = jsonwebtoken::decode::<serde_json::Value>(token, key, &validation)
            .inspect_err(|error| tracing::debug!(%error, "OIDC token validation failed"))
            .map_err(|_| AuthError::InvalidToken)?;
        parse_claims(&data.claims).ok_or_else(|| {
            tracing::debug!("OIDC token claims do not carry the required tenant context");
            AuthError::MissingTenant
        })
    }

    /// Map validated claims onto the principal. Unknown permission strings are
    /// ignored (an unrecognized grant is not a granted permission).
    fn principal_from_claims(claims: TokenClaims) -> Result<AuthenticatedPrincipal, AuthError> {
        let roles: std::collections::BTreeSet<String> = claims
            .roles
            .into_iter()
            .filter(|r| !r.trim().is_empty())
            .collect();
        let permissions: std::collections::BTreeSet<ManagementPermission> =
            claims.permissions.into_iter().collect();
        AuthenticatedPrincipal::new(
            claims.tenant_id,
            claims.user_id,
            claims.subject,
            roles,
            permissions,
            AuthenticationType::Oidc,
        )
        .inspect_err(|error| tracing::debug!(?error, "OIDC claims failed principal construction"))
    }
}

/// Extract and validate the tenant/user context from the verified claim set.
///
/// Required claims: `sub` (non-empty), `tenant_id` (UUID). `user_id` falls
/// back to `sub` when it parses as a UUID.
fn parse_claims(claims: &serde_json::Value) -> Option<TokenClaims> {
    let object = claims.as_object()?;
    let subject = object
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())?
        .to_owned();
    let tenant_id = object
        .get("tenant_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .filter(|value| !value.is_nil())?;
    let user_id = object
        .get("user_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .or_else(|| uuid::Uuid::parse_str(&subject).ok())
        .filter(|value| !value.is_nil())?;
    let roles = string_array(object.get("roles"));
    let permissions = string_array(object.get("management_permissions"))
        .into_iter()
        .filter_map(|value| ManagementPermission::from_str(&value).ok())
        .collect();
    Some(TokenClaims {
        subject,
        tenant_id,
        user_id,
        roles,
        permissions,
    })
}

fn string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
