use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{Response, Url};
use serde::Deserialize;

use crate::auth::{
    resolve_credential, resolve_optional_credential, CredentialKind, CredentialRefresher,
    CredentialStore, ResolvedCredential,
};
use crate::providers::{
    client_for_policy, client_for_policy_without_redirects, normalize_base_url, EndpointPolicy,
};
use crate::{
    AbortSignal, Api, FailurePhase, ModelCatalog, ModelSpec, ProviderError, ProviderErrorKind,
    ProviderId,
};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_MODEL_CATALOG_BYTES: usize = 512 * 1024;
const MAX_MODEL_COUNT: usize = 1_024;
const MAX_MODEL_ID_CHARS: usize = 256;
const MAX_MODEL_NAME_CHARS: usize = 512;
const MAX_SOURCE_IDENTITY_CHARS: usize = 2_048;

/// The authentication contract for a provider profile.
///
/// Profiles never contain a credential value. A required credential is resolved
/// from the configured [`CredentialStore`] or the provider's environment
/// variable at connection/refresh time.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthRequirement {
    /// The endpoint must not receive an authentication header.
    None,
    /// Use a configured credential when one is available, but allow an empty
    /// credential for local or otherwise unauthenticated endpoints.
    Optional,
    /// Fail before network I/O when no credential is available.
    #[default]
    Required,
}

impl AuthRequirement {
    pub fn requires_credential(self) -> bool {
        matches!(self, Self::Required)
    }
}

/// Configuration for a remote model catalog endpoint.
///
/// The endpoint may be an absolute URL or a path relative to the provider's
/// base URL. Relative paths are the preferred form because the provider
/// profile remains the authority for endpoint security policy.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RemoteModelSource {
    pub endpoint: String,
}

impl RemoteModelSource {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// The conventional OpenAI-compatible model-list endpoint.
    pub fn models() -> Self {
        Self::new("models")
    }

    /// Return the non-secret identity used to bind stored facts to this
    /// configured source.
    pub fn identity(&self) -> &str {
        self.endpoint.trim()
    }
}

/// A provider profile is the immutable configuration snapshot used to build a
/// provider connection and to interpret a model catalog.
///
/// `Models` stores complete profile values and publishes a replacement
/// snapshot after refresh. Callers can therefore hold a returned profile while
/// another task refreshes the provider without observing a partially-updated
/// catalog.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderProfile {
    pub provider_id: ProviderId,
    pub api: Api,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub endpoint_policy: EndpointPolicy,
    #[serde(default)]
    pub auth: AuthRequirement,
    #[serde(default)]
    pub catalog: ModelCatalog,
    /// Curated/local facts have precedence over remote partial records. This
    /// is kept separate from the visible overlay so a refresh cannot erase a
    /// curated model that the remote endpoint omits.
    #[serde(default, skip_serializing_if = "ModelCatalog::is_empty")]
    pub curated_catalog: ModelCatalog,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_model_source: Option<RemoteModelSource>,
}

impl ProviderProfile {
    pub fn new(provider_id: ProviderId, api: Api, catalog: ModelCatalog) -> Self {
        Self {
            provider_id,
            api,
            base_url: None,
            endpoint_policy: EndpointPolicy::default(),
            auth: AuthRequirement::default(),
            curated_catalog: catalog.clone(),
            catalog,
            remote_model_source: None,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn with_endpoint_policy(mut self, endpoint_policy: EndpointPolicy) -> Self {
        self.endpoint_policy = endpoint_policy;
        self
    }

    pub fn with_auth(mut self, auth: AuthRequirement) -> Self {
        self.auth = auth;
        self
    }

    pub fn with_remote_model_source(mut self, source: RemoteModelSource) -> Self {
        self.remote_model_source = Some(source);
        self
    }

    pub fn with_curated_catalog(mut self, catalog: ModelCatalog) -> Self {
        self.curated_catalog = catalog.clone();
        for model in catalog.list() {
            self.catalog.insert(model.clone());
        }
        self
    }

    pub fn id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn auth_requirement(&self) -> AuthRequirement {
        self.auth
    }

    pub fn models(&self) -> &ModelCatalog {
        &self.catalog
    }

    pub fn model(&self, id: &str) -> Option<&ModelSpec> {
        self.catalog.get(self.provider_id.as_str(), id)
    }

    pub(crate) fn normalize_curated_catalog(mut self) -> Self {
        // Profiles serialized before the curated/remote split have only a
        // visible catalog. Treat that legacy catalog as curated on publish so
        // its compatibility facts remain authoritative.
        if self.curated_catalog.list().is_empty() && !self.catalog.list().is_empty() {
            self.curated_catalog = self.catalog.clone();
            return self;
        }
        // Public catalog mutation predates the split as well. Carry explicit
        // compatibility/known-capability edits into the curated side without
        // promoting every newly discovered unknown remote record.
        for model in self.catalog.list() {
            let Some(mut curated) = self
                .curated_catalog
                .get(self.provider_id.as_str(), &model.id)
                .cloned()
            else {
                continue;
            };
            curated.name = model.name.clone().or(curated.name);
            curated.context_window = model.context_window.or(curated.context_window);
            curated.max_output_tokens = model.max_output_tokens.or(curated.max_output_tokens);
            curated.cost = model.cost.or(curated.cost);
            curated.openai_completions_compatibility = model
                .openai_completions_compatibility
                .or(curated.openai_completions_compatibility);
            if matches!(
                model.capability_knowledge,
                crate::CapabilityKnowledge::Known
            ) {
                curated.capabilities = model.capabilities;
                curated.capability_knowledge = model.capability_knowledge.clone();
            }
            self.curated_catalog.insert(curated);
        }
        self
    }

    /// Validate all fields that affect connection or publication semantics.
    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.provider_id.as_str().trim().is_empty() {
            return Err(profile_invalid("provider profile id must not be empty"));
        }
        if self.provider_id.as_str().chars().count() > MAX_SOURCE_IDENTITY_CHARS {
            return Err(profile_invalid(
                "provider profile id exceeds the length limit",
            ));
        }
        if let Some(base_url) = &self.base_url {
            if base_url.chars().count() > MAX_SOURCE_IDENTITY_CHARS {
                return Err(profile_invalid(
                    "provider base URL exceeds the length limit",
                ));
            }
            normalize_base_url(base_url, self.endpoint_policy)?;
        }
        if let Some(source) = &self.remote_model_source {
            validate_source_endpoint(source, self.endpoint_policy)?;
        }
        validate_catalog(&self.catalog, self)?;
        validate_catalog(&self.curated_catalog, self)?;
        Ok(())
    }
}

/// A replaceable remote catalog adapter. The default implementation is
/// [`HttpModelCatalogSource`]; tests and applications may provide a source for
/// a different response shape without changing `Models` publication logic.
#[async_trait]
pub trait ModelCatalogSource: Send + Sync {
    async fn fetch_models(
        &self,
        profile: &ProviderProfile,
        api_key: Option<&str>,
        abort: Option<&AbortSignal>,
    ) -> Result<ModelCatalog, ProviderError>;

    /// Fetch catalog facts plus freshness metadata. Existing source
    /// implementations only need to provide [`Self::fetch_models`]; the
    /// default wrapper records a local check time and no cache validators.
    async fn fetch_snapshot(
        &self,
        profile: &ProviderProfile,
        api_key: Option<&str>,
        abort: Option<&AbortSignal>,
    ) -> Result<RemoteCatalogSnapshot, ProviderError> {
        Ok(RemoteCatalogSnapshot {
            catalog: self.fetch_models(profile, api_key, abort).await?,
            checked_at: unix_timestamp_seconds(),
            etag: None,
            last_modified: None,
        })
    }

    async fn fetch_models_with_credential(
        &self,
        profile: &ProviderProfile,
        credential: Option<&ResolvedCredential>,
        abort: Option<&AbortSignal>,
    ) -> Result<ModelCatalog, ProviderError> {
        self.fetch_models(profile, credential.map(ResolvedCredential::token), abort)
            .await
    }

    async fn fetch_snapshot_with_credential(
        &self,
        profile: &ProviderProfile,
        credential: Option<&ResolvedCredential>,
        abort: Option<&AbortSignal>,
    ) -> Result<RemoteCatalogSnapshot, ProviderError> {
        Ok(RemoteCatalogSnapshot {
            catalog: self
                .fetch_models_with_credential(profile, credential, abort)
                .await?,
            checked_at: unix_timestamp_seconds(),
            etag: None,
            last_modified: None,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RemoteCatalogSnapshot {
    pub catalog: ModelCatalog,
    pub checked_at: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// HTTP implementation for OpenAI-compatible and simple JSON model catalogs.
#[derive(Clone, Debug)]
pub struct HttpModelCatalogSource {
    request_timeout: Duration,
}

impl Default for HttpModelCatalogSource {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

impl HttpModelCatalogSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    fn endpoint(&self, profile: &ProviderProfile) -> Result<Url, ProviderError> {
        let source = profile
            .remote_model_source
            .as_ref()
            .ok_or_else(|| profile_invalid("provider has no remote model source"))?;
        let endpoint = source.endpoint.trim();
        if endpoint.is_empty() {
            return Err(profile_invalid(
                "remote model source endpoint must not be empty",
            ));
        }
        if let Ok(url) = Url::parse(endpoint) {
            if url.scheme() != "http" && url.scheme() != "https" {
                return Err(profile_invalid(
                    "remote model source URL must use HTTP or HTTPS",
                ));
            }
            // Validate the absolute endpoint with the profile policy while
            // preserving its endpoint path (normalizing a base URL appends a
            // slash, which would turn `/models` into `/models/`).
            normalize_base_url(endpoint, profile.endpoint_policy)?;
            return Ok(url);
        }

        let base = default_or_configured_base_url(profile)?;
        if endpoint.starts_with("//") {
            return Err(profile_invalid(
                "remote model source endpoint must not replace the profile host",
            ));
        }
        if endpoint.contains('?') || endpoint.contains('#') {
            return Err(profile_invalid(
                "remote model source endpoint must not contain a query or fragment",
            ));
        }
        let joined = base
            .join(endpoint)
            .map_err(|_| profile_invalid("invalid remote model source endpoint"))?;
        if joined.host_str() != base.host_str()
            || joined.port_or_known_default() != base.port_or_known_default()
        {
            return Err(profile_invalid(
                "remote model source endpoint must use the profile host",
            ));
        }
        Ok(joined)
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        abort: Option<&AbortSignal>,
    ) -> Result<Response, ProviderError> {
        if abort.is_some_and(AbortSignal::is_aborted) {
            return Err(aborted(FailurePhase::BeforeDispatch));
        }
        let send = request.send();
        match abort {
            Some(abort) => tokio::select! {
                result = send => result.map_err(transport_error),
                _ = abort.cancelled() => Err(aborted(FailurePhase::Unknown)),
            },
            None => send.await.map_err(transport_error),
        }
    }

    async fn body(
        &self,
        response: Response,
        abort: Option<&AbortSignal>,
    ) -> Result<Vec<u8>, ProviderError> {
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        loop {
            let next = stream.next();
            let chunk = match abort {
                Some(abort) => tokio::select! {
                    value = next => value,
                    _ = abort.cancelled() => {
                        return Err(aborted(FailurePhase::DuringStream));
                    }
                },
                None => next.await,
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::StreamInterrupted,
                    FailurePhase::DuringStream,
                    format!("remote model catalog body interrupted: {error}"),
                )
            })?;
            if body.len().saturating_add(chunk.len()) > MAX_MODEL_CATALOG_BYTES {
                return Err(ProviderError::new(
                    ProviderErrorKind::Protocol,
                    FailurePhase::DuringStream,
                    "remote model catalog exceeds the response limit",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

#[async_trait]
impl ModelCatalogSource for HttpModelCatalogSource {
    async fn fetch_models(
        &self,
        profile: &ProviderProfile,
        api_key: Option<&str>,
        abort: Option<&AbortSignal>,
    ) -> Result<ModelCatalog, ProviderError> {
        self.fetch_models_with_auth(profile, api_key, None, abort)
            .await
    }

    async fn fetch_models_with_credential(
        &self,
        profile: &ProviderProfile,
        credential: Option<&ResolvedCredential>,
        abort: Option<&AbortSignal>,
    ) -> Result<ModelCatalog, ProviderError> {
        self.fetch_models_with_auth(
            profile,
            credential.map(ResolvedCredential::token),
            credential.map(ResolvedCredential::kind),
            abort,
        )
        .await
    }
}

impl HttpModelCatalogSource {
    async fn fetch_models_with_auth(
        &self,
        profile: &ProviderProfile,
        api_key: Option<&str>,
        credential_kind: Option<CredentialKind>,
        abort: Option<&AbortSignal>,
    ) -> Result<ModelCatalog, ProviderError> {
        profile.validate()?;
        let endpoint = self.endpoint(profile)?;
        let client = if api_key.is_some_and(|value| !value.is_empty()) {
            client_for_policy_without_redirects(profile.endpoint_policy)?
        } else {
            client_for_policy(profile.endpoint_policy)?
        };
        let mut request = client
            .get(endpoint)
            .timeout(self.request_timeout)
            .header("accept", "application/json");
        if let Some(api_key) = api_key.filter(|value| !value.is_empty()) {
            if matches!(credential_kind, Some(CredentialKind::OAuth)) {
                request = request.bearer_auth(api_key);
                if matches!(profile.api, Api::AnthropicMessages) {
                    request = request.header("anthropic-version", "2023-06-01");
                }
            } else if matches!(profile.api, Api::AnthropicMessages) {
                request = request.header("x-api-key", api_key);
                request = request.header("anthropic-version", "2023-06-01");
            } else {
                request = request.bearer_auth(api_key);
            }
        }
        let response = self.send(request, abort).await?;
        let status = response.status();
        let body = self.body(response, abort).await?;
        if !status.is_success() {
            let kind = match status.as_u16() {
                401 | 403 => ProviderErrorKind::Authentication,
                408 => ProviderErrorKind::Timeout,
                429 => ProviderErrorKind::RateLimit,
                400..=499 => ProviderErrorKind::InvalidRequest,
                500..=599 => ProviderErrorKind::Unavailable,
                _ => ProviderErrorKind::Other,
            };
            return Err(ProviderError::new(
                kind,
                FailurePhase::AfterDispatch,
                format!("remote model catalog request returned HTTP {status}"),
            )
            .with_status(status.as_u16()));
        }
        parse_catalog(&body, profile)
    }
}

/// Merge a remote partial catalog over curated/local facts.
///
/// Curated records win field-by-field and remain visible even when the remote
/// endpoint omits them. A newly discovered remote model is retained as
/// capability-unknown; compatibility is never copied from an unrelated first
/// catalog entry.
pub(crate) fn merge_remote_catalog(
    profile: &ProviderProfile,
    remote: ModelCatalog,
) -> Result<ModelCatalog, ProviderError> {
    merge_catalog(profile, remote, true)
}

/// Merge a previously validated stored catalog. Stored records already carry
/// the facts published by the earlier process, so their metadata is retained
/// for models that are not overridden by the current profile's curated side.
pub(crate) fn merge_stored_catalog(
    profile: &ProviderProfile,
    stored: ModelCatalog,
) -> Result<ModelCatalog, ProviderError> {
    merge_catalog(profile, stored, false)
}

fn merge_catalog(
    profile: &ProviderProfile,
    remote: ModelCatalog,
    force_unknown_for_new: bool,
) -> Result<ModelCatalog, ProviderError> {
    if remote.list().len() > MAX_MODEL_COUNT {
        return Err(profile_invalid(
            "remote model catalog contains too many models",
        ));
    }
    let mut merged = ModelCatalog::default();
    for mut model in remote.into_models() {
        if let Some(curated) = profile
            .curated_catalog
            .get(profile.provider_id.as_str(), &model.id)
        {
            model.name = curated.name.clone().or(model.name);
            model.capabilities = curated.capabilities;
            model.capability_knowledge = curated.capability_knowledge.clone();
            model.context_window = curated.context_window.or(model.context_window);
            model.max_output_tokens = curated.max_output_tokens.or(model.max_output_tokens);
            model.cost = curated.cost.or(model.cost);
            model.openai_completions_compatibility = curated
                .openai_completions_compatibility
                .or(model.openai_completions_compatibility);
        } else if force_unknown_for_new {
            // A remote listing is a discovery hint, not a capability
            // assertion. Callers may replace this with a curated known spec.
            model.capability_knowledge = crate::CapabilityKnowledge::Unknown;
        }
        merged.insert(model);
    }
    for curated in profile.curated_catalog.list() {
        merged.insert(curated.clone());
    }
    let mut candidate = profile.clone();
    candidate.catalog = merged.clone();
    candidate.validate()?;
    Ok(merged)
}

/// Resolve the source identity without turning any profile configuration into
/// persisted authority. Relative sources include the currently registered
/// base URL so a catalog cannot silently move to a different endpoint after a
/// restart.
pub(crate) fn source_identity(profile: &ProviderProfile) -> Result<String, ProviderError> {
    let source = profile
        .remote_model_source
        .as_ref()
        .ok_or_else(|| profile_invalid("provider has no remote model source"))?;
    let endpoint = source.identity();
    if let Ok(url) = Url::parse(endpoint) {
        return Ok(url.to_string());
    }
    let base = default_or_configured_base_url(profile)?;
    base.join(endpoint)
        .map(|url| url.to_string())
        .map_err(|_| profile_invalid("invalid remote model source endpoint"))
}

fn parse_catalog(body: &[u8], profile: &ProviderProfile) -> Result<ModelCatalog, ProviderError> {
    let response = serde_json::from_slice::<RemoteCatalogResponse>(body).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::Serialization,
            FailurePhase::AfterDispatch,
            "invalid remote model catalog response",
        )
    })?;
    let records = match response {
        RemoteCatalogResponse::Envelope { data, models } => data.or(models).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Serialization,
                FailurePhase::AfterDispatch,
                "remote model catalog response has no model list",
            )
        })?,
        RemoteCatalogResponse::Array(models) => models,
    };
    if records.len() > MAX_MODEL_COUNT {
        return Err(ProviderError::new(
            ProviderErrorKind::Protocol,
            FailurePhase::AfterDispatch,
            "remote model catalog contains too many models",
        ));
    }
    let mut catalog = ModelCatalog::default();
    for record in records {
        if record.id.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Protocol,
                FailurePhase::AfterDispatch,
                "remote model catalog contains an empty model id",
            ));
        }
        if record.id.chars().count() > MAX_MODEL_ID_CHARS {
            return Err(ProviderError::new(
                ProviderErrorKind::Protocol,
                FailurePhase::AfterDispatch,
                "remote model catalog model id exceeds the length limit",
            ));
        }
        for value in [&record.name, &record.owned_by] {
            if value
                .as_deref()
                .is_some_and(|value| value.chars().count() > MAX_MODEL_NAME_CHARS)
            {
                return Err(ProviderError::new(
                    ProviderErrorKind::Protocol,
                    FailurePhase::AfterDispatch,
                    "remote model catalog model name exceeds the length limit",
                ));
            }
        }
        let mut model =
            ModelSpec::custom(record.id, profile.provider_id.clone(), profile.api.clone());
        model.name = record.name.or(record.owned_by);
        model.context_window = record.context_window;
        model.max_output_tokens = record.max_output_tokens;
        catalog.insert(model);
    }
    Ok(catalog)
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RemoteCatalogResponse {
    Envelope {
        #[serde(default)]
        data: Option<Vec<RemoteModelRecord>>,
        #[serde(default)]
        models: Option<Vec<RemoteModelRecord>>,
    },
    Array(Vec<RemoteModelRecord>),
}

#[derive(Debug, Deserialize)]
struct RemoteModelRecord {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    owned_by: Option<String>,
    #[serde(default)]
    context_window: Option<u32>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
}

fn default_or_configured_base_url(profile: &ProviderProfile) -> Result<Url, ProviderError> {
    let base_url = profile.base_url.as_deref().unwrap_or(match profile.api {
        Api::OpenAiCompletions | Api::OpenAiResponses | Api::OpenAiImages => {
            "https://api.openai.com/v1"
        }
        Api::AnthropicMessages => "https://api.anthropic.com/v1",
        Api::Custom(_) => {
            return Err(profile_invalid(
                "custom provider requires a base URL for a relative model source",
            ));
        }
    });
    normalize_base_url(base_url, profile.endpoint_policy)
}

fn validate_source_endpoint(
    source: &RemoteModelSource,
    policy: EndpointPolicy,
) -> Result<(), ProviderError> {
    if source.endpoint.trim().is_empty() {
        return Err(profile_invalid(
            "remote model source endpoint must not be empty",
        ));
    }
    if source.endpoint.chars().count() > MAX_SOURCE_IDENTITY_CHARS {
        return Err(profile_invalid(
            "remote model source endpoint exceeds the length limit",
        ));
    }
    if source.endpoint.contains('?') || source.endpoint.contains('#') {
        return Err(profile_invalid(
            "remote model source endpoint must not contain a query or fragment",
        ));
    }
    if source.endpoint.trim_start().starts_with("//") {
        return Err(profile_invalid(
            "remote model source endpoint must not replace the profile host",
        ));
    }
    if let Ok(url) = Url::parse(source.endpoint.trim()) {
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(profile_invalid(
                "remote model source URL must use HTTP or HTTPS",
            ));
        }
        normalize_base_url(source.endpoint.trim(), policy)?;
    }
    Ok(())
}

fn validate_catalog(
    catalog: &ModelCatalog,
    profile: &ProviderProfile,
) -> Result<(), ProviderError> {
    if catalog.list().len() > MAX_MODEL_COUNT {
        return Err(profile_invalid("provider profile contains too many models"));
    }
    for model in catalog.list() {
        if model.id.trim().is_empty() {
            return Err(profile_invalid(
                "provider profile contains a model with an empty id",
            ));
        }
        if model.id.chars().count() > MAX_MODEL_ID_CHARS {
            return Err(profile_invalid(
                "provider profile model id exceeds the length limit",
            ));
        }
        if model
            .name
            .as_deref()
            .is_some_and(|value| value.chars().count() > MAX_MODEL_NAME_CHARS)
        {
            return Err(profile_invalid(
                "provider profile model name exceeds the length limit",
            ));
        }
        if model.provider != profile.provider_id {
            return Err(profile_invalid(format!(
                "model {} belongs to {}, not provider profile {}",
                model.id, model.provider, profile.provider_id
            )));
        }
        if model.api != profile.api {
            return Err(profile_invalid(format!(
                "model {} uses API {:?}, not provider profile API {:?}",
                model.id, model.api, profile.api
            )));
        }
    }
    Ok(())
}

fn unix_timestamp_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn profile_invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn aborted(phase: FailurePhase) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Aborted,
        phase,
        "remote model catalog refresh aborted",
    )
}

fn transport_error(error: reqwest::Error) -> ProviderError {
    let kind = if error.is_timeout() {
        ProviderErrorKind::Timeout
    } else if error.is_connect() {
        ProviderErrorKind::Unavailable
    } else {
        ProviderErrorKind::Other
    };
    let phase = if error.is_connect() {
        FailurePhase::BeforeDispatch
    } else {
        FailurePhase::Unknown
    };
    ProviderError::new(
        kind,
        phase,
        format!("remote model catalog transport failed: {error}"),
    )
}

pub(crate) async fn profile_credential(
    profile: &ProviderProfile,
    store: &dyn CredentialStore,
    refresher: Option<std::sync::Arc<dyn CredentialRefresher>>,
) -> Result<Option<ResolvedCredential>, ProviderError> {
    match profile.auth {
        AuthRequirement::None => Ok(None),
        AuthRequirement::Optional => {
            resolve_optional_credential(&profile.provider_id, Some(store), refresher).await
        }
        AuthRequirement::Required => Ok(Some(
            resolve_credential(&profile.provider_id, Some(store), refresher).await?,
        )),
    }
}
