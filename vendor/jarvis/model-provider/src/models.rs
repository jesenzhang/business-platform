use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::auth::{
    env_var_name, has_configured_credential, resolve_credential, resolve_optional_credential,
    Credential, CredentialKind, CredentialRefresher, CredentialStore, MemoryCredentialStore,
    ResolvedCredential,
};
use crate::factory::{ProviderConfig, ProviderFactory};
use crate::profiles::{
    merge_remote_catalog, merge_stored_catalog, profile_credential, source_identity,
    HttpModelCatalogSource, ModelCatalogSource, ProviderProfile,
};
use crate::providers::EndpointPolicy;
use crate::{
    AbortSignal, Api, Completion, CompletionRequest, FailurePhase, ImageGenerationProvider,
    ImageGenerationRequest, InMemoryModelsStore, ModelCatalog, ModelProvider, ModelSpec,
    ModelsStore, ModelsStoreError, ProviderError, ProviderErrorKind, ProviderId, ProviderStream,
    RequestOptions, StoreDisposition, StoredModelCatalog,
};

/// Unified catalog + auth + transport factory for other agent systems.
pub struct Models {
    catalog: ModelCatalog,
    store: Arc<dyn CredentialStore>,
    refresher: Option<Arc<dyn CredentialRefresher>>,
    profiles: Arc<RwLock<HashMap<String, Arc<ProviderProfile>>>>,
    model_source: Arc<dyn ModelCatalogSource>,
    model_store: Arc<dyn ModelsStore>,
    generations: Arc<RwLock<HashMap<String, GenerationState>>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct GenerationState {
    operation: u64,
    persisted_revision: u64,
}

#[derive(Clone, Copy, Debug)]
struct OperationToken {
    operation: u64,
    persisted_revision: u64,
}

impl Default for Models {
    fn default() -> Self {
        Self::new()
    }
}

impl Models {
    pub fn new() -> Self {
        Self::with_model_source(Arc::new(HttpModelCatalogSource::default()))
    }

    pub fn with_model_source(model_source: Arc<dyn ModelCatalogSource>) -> Self {
        Self {
            catalog: ModelCatalog::builtin(),
            store: Arc::new(MemoryCredentialStore::new()),
            refresher: None,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            model_source,
            model_store: Arc::new(InMemoryModelsStore::new()),
            generations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_source<S>(source: S) -> Self
    where
        S: ModelCatalogSource + 'static,
    {
        Self::with_model_source(Arc::new(source))
    }

    pub fn with_credential_store(mut self, store: Arc<dyn CredentialStore>) -> Self {
        self.store = store;
        self
    }

    pub fn with_credential_refresher(mut self, refresher: Arc<dyn CredentialRefresher>) -> Self {
        self.refresher = Some(refresher);
        self
    }

    /// Publish a new in-memory credential. The store owns the atomic
    /// replacement; this value is never part of a profile or catalog.
    pub fn set_credential(&self, credential: ResolvedCredential) -> Result<(), ProviderError> {
        self.store.set_resolved(credential)
    }

    pub fn clear_credential(&self, provider: impl AsRef<str>) -> Result<(), ProviderError> {
        let provider = ProviderId::new(provider.as_ref()).map_err(invalid_model)?;
        self.store.clear(&provider);
        Ok(())
    }

    /// Locally revoke the credential by removing it from the configured
    /// store. Remote provider-side revocation remains application-owned.
    pub fn revoke_credential(&self, provider: impl AsRef<str>) -> Result<(), ProviderError> {
        let provider = ProviderId::new(provider.as_ref()).map_err(invalid_model)?;
        self.store.revoke(&provider);
        Ok(())
    }

    pub fn with_models_store(mut self, store: Arc<dyn ModelsStore>) -> Self {
        self.model_store = store;
        self
    }

    pub fn with_store<S>(self, store: S) -> Self
    where
        S: ModelsStore + 'static,
    {
        self.with_models_store(Arc::new(store))
    }

    pub fn catalog(&self) -> &ModelCatalog {
        &self.catalog
    }

    pub fn catalog_mut(&mut self) -> &mut ModelCatalog {
        &mut self.catalog
    }

    pub fn with_api_key(
        self,
        provider: impl AsRef<str>,
        api_key: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let provider = ProviderId::new(provider.as_ref()).map_err(|message| {
            ProviderError::new(
                crate::ProviderErrorKind::InvalidRequest,
                crate::FailurePhase::BeforeDispatch,
                message,
            )
        })?;
        self.store.set(Credential {
            provider,
            api_key: api_key.into(),
        });
        Ok(self)
    }

    pub fn get(&self, provider: &str, id: &str) -> Option<ModelSpec> {
        if let Some(profile) = self.profile(provider) {
            return profile.model(id).cloned();
        }
        self.catalog.get(provider, id).cloned()
    }

    /// Legacy borrowed view of the built-in catalog.
    ///
    /// Use [`Self::list_snapshot`] when profiles may be registered; a
    /// profile-backed view must be owned because refresh publishes a new
    /// immutable snapshot.
    #[deprecated(note = "use list_snapshot for unified profile discovery")]
    pub fn list(&self) -> &[ModelSpec] {
        self.catalog.list()
    }

    /// Legacy borrowed view of built-in provider models.
    #[deprecated(note = "use list_provider_snapshot for unified profile discovery")]
    pub fn list_provider(&self, provider: &str) -> Vec<&ModelSpec> {
        self.catalog.list_provider(provider)
    }

    /// Return the visible model overlay used by profile-aware discovery.
    /// Registered profiles replace the built-in provider slice; their remote
    /// additions and curated records are then exposed in one deterministic
    /// owned snapshot.
    pub fn list_snapshot(&self) -> Vec<ModelSpec> {
        let profiles = self.profiles();
        let registered = profiles
            .iter()
            .map(|profile| profile.provider_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut visible = ModelCatalog::new(
            self.catalog
                .list()
                .iter()
                .filter(|model| !registered.contains(model.provider.as_str()))
                .cloned(),
        );
        for profile in profiles {
            for model in profile.catalog.list() {
                visible.insert(model.clone());
            }
        }
        visible.into_models()
    }

    pub fn list_provider_snapshot(&self, provider: &str) -> Vec<ModelSpec> {
        self.list_snapshot()
            .into_iter()
            .filter(|model| model.provider.as_str() == provider)
            .collect()
    }

    /// Return the last successfully published provider profile.
    ///
    /// The returned value is a complete snapshot. It remains valid while an
    /// asynchronous refresh is in flight and is never mutated in place.
    pub fn profile(&self, provider: &str) -> Option<ProviderProfile> {
        self.profile_snapshot(provider)
            .map(|profile| (*profile).clone())
    }

    /// Return an immutable, reference-counted last-known profile snapshot.
    pub fn profile_snapshot(&self, provider: &str) -> Option<Arc<ProviderProfile>> {
        self.profiles.read().ok()?.get(provider).cloned()
    }

    pub fn profiles(&self) -> Vec<ProviderProfile> {
        let Ok(profiles) = self.profiles.read() else {
            return Vec::new();
        };
        let mut profiles = profiles
            .values()
            .map(|profile| (**profile).clone())
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.provider_id.as_str().cmp(right.provider_id.as_str()));
        profiles
    }

    /// Validate and atomically publish one complete provider profile.
    pub fn publish_profile(&self, profile: ProviderProfile) -> Result<(), ProviderError> {
        let profile = profile.normalize_curated_catalog();
        profile.validate()?;
        let provider = profile.provider_id.to_string();
        self.begin_operation(&provider)?;
        let mut profiles = self.profiles.write().map_err(|_| profile_lock_error())?;
        profiles.insert(provider, Arc::new(profile));
        Ok(())
    }

    pub fn register_profile(&self, profile: ProviderProfile) -> Result<(), ProviderError> {
        self.publish_profile(profile)
    }

    pub fn with_profile(self, profile: ProviderProfile) -> Result<Self, ProviderError> {
        self.publish_profile(profile)?;
        Ok(self)
    }

    pub fn profile_model(&self, provider: &str, model: &str) -> Result<ModelSpec, ProviderError> {
        self.profile_snapshot(provider)
            .and_then(|profile| profile.model(model).cloned())
            .ok_or_else(|| invalid_model(format!("unknown model {provider}/{model}")))
    }

    pub fn profile_models(&self, provider: &str) -> Vec<ModelSpec> {
        self.profile_snapshot(provider)
            .map(|profile| profile.catalog.list().to_vec())
            .unwrap_or_default()
    }

    /// Remove a profile and supersede any refresh already in flight for it.
    pub fn remove_profile(&self, provider: &str) -> Option<ProviderProfile> {
        if self.begin_operation(provider).is_err() {
            return None;
        }
        self.profiles
            .write()
            .ok()?
            .remove(provider)
            .map(|profile| (*profile).clone())
    }

    /// Legacy borrowed availability view of built-in models.
    #[deprecated(note = "use available_snapshot for unified profile discovery")]
    pub fn available(&self) -> Vec<&ModelSpec> {
        self.catalog
            .list()
            .iter()
            .filter(|model| has_configured_credential(&model.provider, Some(self.store.as_ref())))
            .collect()
    }

    pub fn available_snapshot(&self) -> Vec<ModelSpec> {
        self.list_snapshot()
            .into_iter()
            .filter(|model| {
                self.profile_snapshot(model.provider.as_str()).map_or_else(
                    || has_configured_credential(&model.provider, Some(self.store.as_ref())),
                    |profile| match profile.auth_requirement() {
                        crate::AuthRequirement::None | crate::AuthRequirement::Optional => true,
                        crate::AuthRequirement::Required => {
                            has_configured_credential(&model.provider, Some(self.store.as_ref()))
                        }
                    },
                )
            })
            .collect()
    }

    pub fn connect(&self, model: &ModelSpec) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        self.connect_with_api(model, model.api.clone())
    }

    /// Connect a standalone image-generation model. This is intentionally a
    /// separate seam from [`Self::connect`] because image generation has a
    /// different request and response contract than conversational models.
    pub fn connect_image(
        &self,
        model: &ModelSpec,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        if let Some(profile) = self.profile_snapshot(model.provider.as_str()) {
            return self.connect_image_profile_model(&profile, model);
        }
        self.connect_image_at(
            model,
            None,
            EndpointPolicy::SecureOrLoopback,
            crate::AuthRequirement::Required,
        )
    }

    pub fn connect_image_profile(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        let profile = self
            .profile_snapshot(provider)
            .ok_or_else(|| invalid_model(format!("unknown provider profile {provider}")))?;
        let model = profile
            .model(model)
            .ok_or_else(|| invalid_model(format!("unknown model {provider}/{model}")))?;
        self.connect_image_profile_model(&profile, model)
    }

    pub fn connect_image_id(
        &self,
        provider: &str,
        id: &str,
    ) -> Result<(ModelSpec, Arc<dyn ImageGenerationProvider>), ProviderError> {
        let model = self
            .get(provider, id)
            .ok_or_else(|| invalid_model(format!("unknown model {provider}/{id}")))?;
        let connection = self.connect_image(&model)?;
        Ok((model, connection))
    }

    /// Connect using an explicit API override for static catalog models.
    /// Matching registered profiles remain authoritative; the override must
    /// equal the profile API or the request is rejected.
    pub fn connect_with_api(
        &self,
        model: &ModelSpec,
        api: Api,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        if let Some(profile) = self.profile_snapshot(model.provider.as_str()) {
            if api != profile.api {
                return Err(profile_api_conflict(&profile, &api));
            }
            return self.connect_profile_model(&profile, model);
        }
        self.connect_with_api_at(model, api, None)
    }

    /// Build a provider connection from a published profile and one of its
    /// compatibility-bearing model specs.
    pub fn connect_profile(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let profile = self
            .profile_snapshot(provider)
            .ok_or_else(|| invalid_model(format!("unknown provider profile {provider}")))?;
        let model = profile
            .model(model)
            .ok_or_else(|| invalid_model(format!("unknown model {provider}/{model}")))?;
        self.connect_profile_model(&profile, model)
    }

    pub fn connect_profile_default(
        &self,
        provider: &str,
    ) -> Result<(ModelSpec, Arc<dyn ModelProvider>), ProviderError> {
        let profile = self
            .profile_snapshot(provider)
            .ok_or_else(|| invalid_model(format!("unknown provider profile {provider}")))?;
        let model =
            profile.catalog.list().first().ok_or_else(|| {
                invalid_model(format!("provider profile {provider} has no models"))
            })?;
        let provider = self.connect_profile_model(&profile, model)?;
        Ok((model.clone(), provider))
    }

    pub fn connect_profile_id(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<(ModelSpec, Arc<dyn ModelProvider>), ProviderError> {
        let profile = self
            .profile_snapshot(provider)
            .ok_or_else(|| invalid_model(format!("unknown provider profile {provider}")))?;
        let model_spec = profile
            .model(model)
            .ok_or_else(|| invalid_model(format!("unknown model {provider}/{model}")))?;
        let connection = self.connect_profile_model(&profile, model_spec)?;
        Ok((model_spec.clone(), connection))
    }

    fn connect_profile_model(
        &self,
        profile: &ProviderProfile,
        model: &ModelSpec,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        profile.validate()?;
        if model.provider != profile.provider_id {
            return Err(invalid_model(format!(
                "model belongs to {}, not provider profile {}",
                model.provider, profile.provider_id
            )));
        }
        if model.api != profile.api {
            return Err(profile_api_conflict(profile, &model.api));
        }
        if profile.auth_requirement().requires_credential()
            && !has_configured_credential(&profile.provider_id, Some(self.store.as_ref()))
        {
            return Err(missing_credential(&profile.provider_id));
        }
        let connection = CredentialAuthProvider {
            provider_id: profile.provider_id.clone(),
            api: profile.api.clone(),
            base_url: profile.base_url.clone(),
            endpoint_policy: profile.endpoint_policy,
            request_timeout: std::time::Duration::from_secs(120),
            compatibility: model.openai_completions_compatibility.unwrap_or_default(),
            auth: profile.auth_requirement(),
            store: Arc::clone(&self.store),
            refresher: self.refresher.clone(),
        };
        connection.build_provider(None)?;
        Ok(Arc::new(connection))
    }

    fn connect_image_profile_model(
        &self,
        profile: &ProviderProfile,
        model: &ModelSpec,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        profile.validate()?;
        if profile.api != Api::OpenAiImages {
            return Err(unsupported_image_api(profile.api.clone()));
        }
        if model.provider != profile.provider_id {
            return Err(invalid_model(format!(
                "model belongs to {}, not provider profile {}",
                model.provider, profile.provider_id
            )));
        }
        if model.api != profile.api {
            return Err(profile_api_conflict(profile, &model.api));
        }
        self.connect_image_at(
            model,
            profile.base_url.clone(),
            profile.endpoint_policy,
            profile.auth_requirement(),
        )
    }

    fn connect_image_at(
        &self,
        model: &ModelSpec,
        base_url: Option<String>,
        endpoint_policy: EndpointPolicy,
        auth: crate::AuthRequirement,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        if model.api != Api::OpenAiImages {
            return Err(unsupported_image_api(model.api.clone()));
        }
        if auth.requires_credential()
            && !has_configured_credential(&model.provider, Some(self.store.as_ref()))
        {
            return Err(missing_credential(&model.provider));
        }
        let connection = CredentialAuthImageProvider {
            provider_id: model.provider.clone(),
            api: Api::OpenAiImages,
            base_url,
            endpoint_policy,
            request_timeout: std::time::Duration::from_secs(120),
            auth,
            store: Arc::clone(&self.store),
            refresher: self.refresher.clone(),
        };
        connection.build_provider(None)?;
        Ok(Arc::new(connection))
    }

    /// Refresh one profile without holding the profile lock over network I/O.
    ///
    /// This compatibility wrapper returns the published (or unchanged static)
    /// profile. Use [`Self::refresh_with_outcome`] when callers need to
    /// distinguish abort, supersession, skip, and failure states.
    pub async fn refresh(
        &self,
        provider: impl AsRef<str>,
        abort: Option<AbortSignal>,
    ) -> Result<ProviderProfile, ProviderError> {
        let outcome = self.refresh_with_outcome(provider, abort).await;
        match outcome.status {
            RefreshStatus::Published | RefreshStatus::SkippedNoSource => outcome
                .profile
                .ok_or_else(|| invalid_model("refresh completed without a profile snapshot")),
            _ => Err(outcome
                .error
                .unwrap_or_else(|| refresh_status_error(outcome.status))),
        }
    }

    pub async fn refresh_with_abort(
        &self,
        provider: impl AsRef<str>,
        abort: &AbortSignal,
    ) -> Result<ProviderProfile, ProviderError> {
        self.refresh(provider, Some(abort.clone())).await
    }

    /// Refresh and return a structured lifecycle result. Network, parse,
    /// validation, persistence, and publication failures all leave the
    /// visible profile unchanged.
    pub async fn refresh_with_outcome(
        &self,
        provider: impl AsRef<str>,
        abort: Option<AbortSignal>,
    ) -> RefreshOutcome {
        let provider = provider.as_ref().to_owned();
        let Some(snapshot) = self.profile_snapshot(&provider) else {
            return RefreshOutcome::failed(provider, 0, invalid_model("unknown provider profile"));
        };
        if let Err(error) = snapshot.validate() {
            return RefreshOutcome::failed(provider, 0, error);
        }
        let operation = match self.begin_operation(&provider) {
            Ok(operation) => operation,
            Err(error) => return RefreshOutcome::failed(provider, 0, error),
        };
        let generation = operation.persisted_revision;
        if snapshot.remote_model_source.is_none() {
            return RefreshOutcome {
                provider,
                generation,
                status: RefreshStatus::SkippedNoSource,
                profile: Some((*snapshot).clone()),
                error: None,
            };
        }
        if abort.as_ref().is_some_and(AbortSignal::is_aborted) {
            return RefreshOutcome::failed_with_status(
                provider,
                generation,
                RefreshStatus::Aborted,
                aborted_refresh(FailurePhase::BeforeDispatch),
            );
        }
        let credential = match profile_credential(
            &snapshot,
            self.store.as_ref(),
            self.refresher.clone(),
        )
        .await
        {
            Ok(credential) => credential,
            Err(error) => {
                return RefreshOutcome::failed(provider, generation, error);
            }
        };
        let fetched = match self
            .model_source
            .fetch_snapshot_with_credential(&snapshot, credential.as_ref(), abort.as_ref())
            .await
        {
            Ok(fetched) => fetched,
            Err(error) => {
                let status = if error.kind == ProviderErrorKind::Aborted {
                    RefreshStatus::Aborted
                } else {
                    RefreshStatus::Failed
                };
                return RefreshOutcome::failed_with_status(provider, generation, status, error);
            }
        };
        let mut refreshed = (*snapshot).clone();
        refreshed.catalog = match merge_remote_catalog(&snapshot, fetched.catalog) {
            Ok(catalog) => catalog,
            Err(error) => return RefreshOutcome::failed(provider, generation, error),
        };
        if let Err(error) = refreshed.validate() {
            return RefreshOutcome::failed(provider, generation, error);
        }

        // Check the start operation before staging. The store itself also
        // rejects older persisted revisions for the race that can occur
        // during its async write.
        if self.current_operation(&provider) != Some(operation.operation) {
            return RefreshOutcome::superseded(provider, generation);
        }
        let mut stored = StoredModelCatalog::new(
            refreshed.provider_id.clone(),
            refreshed.api.clone(),
            refreshed.catalog.clone(),
            fetched.checked_at,
            match source_identity(&refreshed) {
                Ok(identity) => identity,
                Err(error) => return RefreshOutcome::failed(provider, generation, error),
            },
        );
        stored.etag = fetched.etag;
        stored.last_modified = fetched.last_modified;
        stored.generation = generation;
        if let Err(error) = stored.validate() {
            return RefreshOutcome::failed(provider, generation, models_store_error(error));
        }
        let disposition = match self.model_store.store(stored).await {
            Ok(disposition) => disposition,
            Err(error) => {
                return RefreshOutcome::failed(provider, generation, models_store_error(error));
            }
        };
        if !matches!(disposition, StoreDisposition::Stored)
            || self.current_operation(&provider) != Some(operation.operation)
        {
            return RefreshOutcome::superseded(provider, generation);
        }

        // Hold an operation-order read guard while publishing. A new refresh
        // start cannot advance the operation between this check and the Arc
        // swap.
        let operation_guard = match self.generations.read() {
            Ok(guard) => guard,
            Err(_) => return RefreshOutcome::failed(provider, generation, profile_lock_error()),
        };
        if operation_guard
            .get(&provider)
            .is_none_or(|state| state.operation != operation.operation)
        {
            return RefreshOutcome::superseded(provider, generation);
        }
        let mut profiles = match self.profiles.write() {
            Ok(profiles) => profiles,
            Err(_) => return RefreshOutcome::failed(provider, generation, profile_lock_error()),
        };
        let Some(current) = profiles.get(&provider) else {
            return RefreshOutcome::failed(
                provider,
                generation,
                invalid_model("provider profile was removed during refresh"),
            );
        };
        if !Arc::ptr_eq(current, &snapshot) {
            return RefreshOutcome::superseded(provider, generation);
        }
        let refreshed = Arc::new(refreshed);
        profiles.insert(provider.clone(), refreshed.clone());
        drop(profiles);
        drop(operation_guard);
        RefreshOutcome {
            provider,
            generation,
            status: RefreshStatus::Published,
            profile: Some((*refreshed).clone()),
            error: None,
        }
    }

    /// Restore the last stored catalog for a registered profile without any
    /// network access. Configuration authority remains the current profile.
    pub async fn restore(
        &self,
        provider: impl AsRef<str>,
    ) -> Result<ProviderProfile, ProviderError> {
        let outcome = self.restore_with_outcome(provider).await;
        match outcome.status {
            RestoreStatus::Restored
            | RestoreStatus::SkippedMissing
            | RestoreStatus::SkippedNoSource => outcome
                .profile
                .ok_or_else(|| invalid_model("restore completed without a profile snapshot")),
            _ => Err(outcome
                .error
                .unwrap_or_else(|| restore_status_error(outcome.status))),
        }
    }

    pub async fn restore_with_outcome(&self, provider: impl AsRef<str>) -> RestoreOutcome {
        let provider = provider.as_ref().to_owned();
        let Some(snapshot) = self.profile_snapshot(&provider) else {
            return RestoreOutcome::failed(provider, 0, invalid_model("unknown provider profile"));
        };
        let operation = match self.begin_operation(&provider) {
            Ok(operation) => operation,
            Err(error) => return RestoreOutcome::failed(provider, 0, error),
        };
        let mut generation = operation.persisted_revision;
        if snapshot.remote_model_source.is_none() {
            return RestoreOutcome {
                provider,
                generation,
                status: RestoreStatus::SkippedNoSource,
                profile: Some((*snapshot).clone()),
                error: None,
            };
        }
        let entry = match self.model_store.load(&snapshot.provider_id).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                return RestoreOutcome {
                    provider,
                    generation,
                    status: RestoreStatus::SkippedMissing,
                    profile: Some((*snapshot).clone()),
                    error: None,
                };
            }
            Err(error) => {
                return RestoreOutcome::failed(provider, generation, models_store_error(error));
            }
        };
        if let Err(error) = entry.validate() {
            return RestoreOutcome::failed(provider, generation, models_store_error(error));
        }
        generation = generation.max(entry.generation);
        if let Err(error) = self.raise_persisted_floor(&provider, entry.generation) {
            return RestoreOutcome::failed(provider, generation, error);
        }
        let source_identity = match source_identity(&snapshot) {
            Ok(identity) => identity,
            Err(error) => return RestoreOutcome::failed(provider, generation, error),
        };
        if entry.provider_id != snapshot.provider_id
            || entry.api != snapshot.api
            || entry.source_identity != source_identity
        {
            return RestoreOutcome::failed_with_status(
                provider,
                generation,
                RestoreStatus::Incompatible,
                invalid_model("stored model catalog does not match the registered profile"),
            );
        }
        let mut restored = (*snapshot).clone();
        restored.catalog = match merge_stored_catalog(&snapshot, entry.models) {
            Ok(catalog) => catalog,
            Err(error) => return RestoreOutcome::failed(provider, generation, error),
        };
        if let Err(error) = restored.validate() {
            return RestoreOutcome::failed(provider, generation, error);
        }
        let operation_guard = match self.generations.read() {
            Ok(guard) => guard,
            Err(_) => return RestoreOutcome::failed(provider, generation, profile_lock_error()),
        };
        if operation_guard
            .get(&provider)
            .is_none_or(|state| state.operation != operation.operation)
        {
            return RestoreOutcome::superseded(provider, generation);
        }
        let mut profiles = match self.profiles.write() {
            Ok(profiles) => profiles,
            Err(_) => return RestoreOutcome::failed(provider, generation, profile_lock_error()),
        };
        let Some(current) = profiles.get(&provider) else {
            return RestoreOutcome::failed(
                provider,
                generation,
                invalid_model("provider profile was removed during restore"),
            );
        };
        if !Arc::ptr_eq(current, &snapshot) {
            return RestoreOutcome::superseded(provider, generation);
        }
        let restored = Arc::new(restored);
        profiles.insert(provider.clone(), restored.clone());
        RestoreOutcome {
            provider,
            generation,
            status: RestoreStatus::Restored,
            profile: Some((*restored).clone()),
            error: None,
        }
    }

    fn begin_operation(&self, provider: &str) -> Result<OperationToken, ProviderError> {
        let mut generations = self.generations.write().map_err(|_| profile_lock_error())?;
        let state = generations.entry(provider.to_owned()).or_default();
        let operation = state
            .operation
            .checked_add(1)
            .ok_or_else(generation_overflow)?;
        let persisted_revision = if state.persisted_revision >= operation {
            state
                .persisted_revision
                .checked_add(1)
                .ok_or_else(generation_overflow)?
        } else {
            operation
        };
        state.operation = operation;
        state.persisted_revision = persisted_revision;
        Ok(OperationToken {
            operation,
            persisted_revision,
        })
    }

    fn current_operation(&self, provider: &str) -> Option<u64> {
        self.generations
            .read()
            .ok()
            .and_then(|generations| generations.get(provider).map(|state| state.operation))
    }

    fn raise_persisted_floor(
        &self,
        provider: &str,
        persisted_revision: u64,
    ) -> Result<(), ProviderError> {
        let mut generations = self.generations.write().map_err(|_| profile_lock_error())?;
        let state = generations
            .get_mut(provider)
            .ok_or_else(profile_lock_error)?;
        state.persisted_revision = state.persisted_revision.max(persisted_revision);
        Ok(())
    }

    fn connect_with_api_at(
        &self,
        model: &ModelSpec,
        api: Api,
        base_url: Option<String>,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        if !has_configured_credential(&model.provider, Some(self.store.as_ref())) {
            return Err(missing_credential(&model.provider));
        }
        let connection = CredentialAuthProvider {
            provider_id: model.provider.clone(),
            api,
            base_url,
            endpoint_policy: EndpointPolicy::SecureOrLoopback,
            request_timeout: std::time::Duration::from_secs(120),
            compatibility: model.openai_completions_compatibility.unwrap_or_default(),
            auth: crate::AuthRequirement::Required,
            store: Arc::clone(&self.store),
            refresher: self.refresher.clone(),
        };
        connection.build_provider(None)?;
        Ok(Arc::new(connection))
    }

    pub fn connect_id(
        &self,
        provider: &str,
        id: &str,
    ) -> Result<(ModelSpec, Arc<dyn ModelProvider>), ProviderError> {
        let model = self.get(provider, id).ok_or_else(|| {
            ProviderError::new(
                crate::ProviderErrorKind::InvalidRequest,
                crate::FailurePhase::BeforeDispatch,
                format!("unknown model {provider}/{id}"),
            )
        })?;
        if self.profile_snapshot(provider).is_some() {
            return self.connect_profile_id(provider, id);
        }
        let connection = self.connect(&model)?;
        Ok((model, connection))
    }
}

struct CredentialAuthProvider {
    provider_id: ProviderId,
    api: Api,
    base_url: Option<String>,
    endpoint_policy: EndpointPolicy,
    request_timeout: std::time::Duration,
    compatibility: crate::OpenAiCompletionsCompatibility,
    auth: crate::AuthRequirement,
    store: Arc<dyn CredentialStore>,
    refresher: Option<Arc<dyn CredentialRefresher>>,
}

impl CredentialAuthProvider {
    fn build_provider(
        &self,
        credential: Option<&ResolvedCredential>,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let config = ProviderConfig {
            provider_id: self.provider_id.clone(),
            api: self.api.clone(),
            api_key: credential
                .map(ResolvedCredential::token)
                .unwrap_or_default()
                .to_owned(),
            base_url: self.base_url.clone(),
            endpoint_policy: self.endpoint_policy,
            request_timeout: self.request_timeout,
        };
        let credential_kind = credential
            .map(ResolvedCredential::kind)
            .unwrap_or(CredentialKind::ApiKey);
        if matches!(&config.api, Api::OpenAiCompletions) {
            ProviderFactory::build_with_openai_completions_compatibility_and_credential(
                config,
                self.compatibility,
                credential_kind,
            )
        } else {
            ProviderFactory::build_with_credential(config, credential_kind)
        }
    }

    async fn resolve_for_request(&self) -> Result<Option<ResolvedCredential>, ProviderError> {
        match self.auth {
            crate::AuthRequirement::None => Ok(None),
            crate::AuthRequirement::Optional => {
                resolve_optional_credential(
                    &self.provider_id,
                    Some(self.store.as_ref()),
                    self.refresher.clone(),
                )
                .await
            }
            crate::AuthRequirement::Required => Ok(Some(
                resolve_credential(
                    &self.provider_id,
                    Some(self.store.as_ref()),
                    self.refresher.clone(),
                )
                .await?,
            )),
        }
    }

    fn validate_options(&self, options: &RequestOptions) -> Result<(), ProviderError> {
        if self.auth != crate::AuthRequirement::None {
            return Ok(());
        }
        if options.headers.iter().any(|(name, _)| {
            name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
        }) {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                FailurePhase::BeforeDispatch,
                "unauthenticated provider profiles reject credential headers",
            ));
        }
        Ok(())
    }
}

struct CredentialAuthImageProvider {
    provider_id: ProviderId,
    api: Api,
    base_url: Option<String>,
    endpoint_policy: EndpointPolicy,
    request_timeout: std::time::Duration,
    auth: crate::AuthRequirement,
    store: Arc<dyn CredentialStore>,
    refresher: Option<Arc<dyn CredentialRefresher>>,
}

impl CredentialAuthImageProvider {
    fn build_provider(
        &self,
        credential: Option<&ResolvedCredential>,
    ) -> Result<Arc<dyn ImageGenerationProvider>, ProviderError> {
        let config = ProviderConfig {
            provider_id: self.provider_id.clone(),
            api: self.api.clone(),
            api_key: credential
                .map(ResolvedCredential::token)
                .unwrap_or_default()
                .to_owned(),
            base_url: self.base_url.clone(),
            endpoint_policy: self.endpoint_policy,
            request_timeout: self.request_timeout,
        };
        let credential_kind = credential
            .map(ResolvedCredential::kind)
            .unwrap_or(CredentialKind::ApiKey);
        ProviderFactory::build_image_generator_with_credential(config, credential_kind)
    }

    async fn resolve_for_request(&self) -> Result<Option<ResolvedCredential>, ProviderError> {
        match self.auth {
            crate::AuthRequirement::None => Ok(None),
            crate::AuthRequirement::Optional => {
                resolve_optional_credential(
                    &self.provider_id,
                    Some(self.store.as_ref()),
                    self.refresher.clone(),
                )
                .await
            }
            crate::AuthRequirement::Required => Ok(Some(
                resolve_credential(
                    &self.provider_id,
                    Some(self.store.as_ref()),
                    self.refresher.clone(),
                )
                .await?,
            )),
        }
    }

    fn validate_options(&self, options: &RequestOptions) -> Result<(), ProviderError> {
        if self.auth != crate::AuthRequirement::None {
            return Ok(());
        }
        if options.headers.iter().any(|(name, _)| {
            name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
        }) {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                FailurePhase::BeforeDispatch,
                "unauthenticated image profiles reject credential headers",
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl ImageGenerationProvider for CredentialAuthImageProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        &self.api
    }

    async fn generate_with(
        &self,
        request: ImageGenerationRequest,
        options: RequestOptions,
    ) -> Result<crate::ImageGenerationResponse, ProviderError> {
        self.validate_options(&options)?;
        self.validate_request(&request)?;
        let credential = self.resolve_for_request().await?;
        self.build_provider(credential.as_ref())?
            .generate_with(request, options)
            .await
    }
}

#[async_trait]
impl ModelProvider for CredentialAuthProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        &self.api
    }

    fn capabilities(&self) -> crate::ProviderCapabilities {
        crate::ProviderCapabilities {
            streaming: true,
            reasoning: true,
            tools: true,
            tool_streaming: true,
            vision: true,
        }
    }

    fn validate_request(&self, request: &CompletionRequest) -> Result<(), ProviderError> {
        self.build_provider(None)?.validate_request(request)
    }

    async fn stream_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<ProviderStream, ProviderError> {
        self.validate_options(&options)?;
        let credential = self.resolve_for_request().await?;
        self.build_provider(credential.as_ref())?
            .stream_with(request, options)
            .await
    }

    async fn complete_with(
        &self,
        request: CompletionRequest,
        options: RequestOptions,
    ) -> Result<Completion, ProviderError> {
        self.validate_options(&options)?;
        let credential = self.resolve_for_request().await?;
        self.build_provider(credential.as_ref())?
            .complete_with(request, options)
            .await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshStatus {
    Published,
    SkippedNoSource,
    Superseded,
    Aborted,
    Failed,
}

#[derive(Clone, Debug)]
pub struct RefreshOutcome {
    pub provider: String,
    pub generation: u64,
    pub status: RefreshStatus,
    pub profile: Option<ProviderProfile>,
    pub error: Option<ProviderError>,
}

impl RefreshOutcome {
    fn failed(provider: String, generation: u64, error: ProviderError) -> Self {
        Self::failed_with_status(provider, generation, RefreshStatus::Failed, error)
    }

    fn failed_with_status(
        provider: String,
        generation: u64,
        status: RefreshStatus,
        error: ProviderError,
    ) -> Self {
        Self {
            provider,
            generation,
            status,
            profile: None,
            error: Some(error),
        }
    }

    fn superseded(provider: String, generation: u64) -> Self {
        Self {
            provider,
            generation,
            status: RefreshStatus::Superseded,
            profile: None,
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreStatus {
    Restored,
    SkippedMissing,
    SkippedNoSource,
    Incompatible,
    Superseded,
    Failed,
}

#[derive(Clone, Debug)]
pub struct RestoreOutcome {
    pub provider: String,
    pub generation: u64,
    pub status: RestoreStatus,
    pub profile: Option<ProviderProfile>,
    pub error: Option<ProviderError>,
}

impl RestoreOutcome {
    fn failed(provider: String, generation: u64, error: ProviderError) -> Self {
        Self::failed_with_status(provider, generation, RestoreStatus::Failed, error)
    }

    fn failed_with_status(
        provider: String,
        generation: u64,
        status: RestoreStatus,
        error: ProviderError,
    ) -> Self {
        Self {
            provider,
            generation,
            status,
            profile: None,
            error: Some(error),
        }
    }

    fn superseded(provider: String, generation: u64) -> Self {
        Self {
            provider,
            generation,
            status: RestoreStatus::Superseded,
            profile: None,
            error: None,
        }
    }
}

fn invalid_model(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn missing_credential(provider: &ProviderId) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Authentication,
        FailurePhase::BeforeDispatch,
        format!(
            "missing credential for {provider}; set {} or store a credential",
            env_var_name(provider.as_str())
        ),
    )
}

fn profile_lock_error() -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Other,
        FailurePhase::BeforeDispatch,
        "provider profile store is unavailable",
    )
}

fn generation_overflow() -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Other,
        FailurePhase::BeforeDispatch,
        "provider model generation exhausted",
    )
}

fn profile_api_conflict(profile: &ProviderProfile, api: &Api) -> ProviderError {
    invalid_model(format!(
        "explicit API override {api:?} conflicts with registered profile {} API {:?}",
        profile.provider_id, profile.api
    ))
}

fn unsupported_image_api(api: Api) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unsupported,
        FailurePhase::BeforeDispatch,
        format!("standalone image generation requires Api::OpenAiImages, got {api:?}"),
    )
}

fn aborted_refresh(phase: FailurePhase) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Aborted,
        phase,
        "provider model catalog refresh aborted",
    )
}

fn models_store_error(error: ModelsStoreError) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Other,
        FailurePhase::BeforeDispatch,
        format!("model catalog store failed: {error}"),
    )
}

fn refresh_status_error(status: RefreshStatus) -> ProviderError {
    let message = match status {
        RefreshStatus::Superseded => "model catalog refresh was superseded",
        RefreshStatus::Aborted => "model catalog refresh was aborted",
        RefreshStatus::Failed => "model catalog refresh failed",
        RefreshStatus::Published | RefreshStatus::SkippedNoSource => {
            "model catalog refresh did not produce an error"
        }
    };
    ProviderError::new(
        if matches!(status, RefreshStatus::Aborted) {
            ProviderErrorKind::Aborted
        } else {
            ProviderErrorKind::Unavailable
        },
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn restore_status_error(status: RestoreStatus) -> ProviderError {
    let message = match status {
        RestoreStatus::Incompatible => "stored model catalog is incompatible with the profile",
        RestoreStatus::Superseded => "model catalog restore was superseded",
        RestoreStatus::Failed => "model catalog restore failed",
        RestoreStatus::Restored
        | RestoreStatus::SkippedMissing
        | RestoreStatus::SkippedNoSource => "model catalog restore did not produce an error",
    };
    ProviderError::new(
        ProviderErrorKind::Unavailable,
        FailurePhase::BeforeDispatch,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Api, AuthRequirement, CompletionRequest, DataRetentionPolicy, HttpModelCatalogSource,
        MaxOutputTokensField, Message, ModelCatalogSource, ModelSpec,
        OpenAiCompletionsCompatibility, OpenAiSystemRole, OpenAiThinkingDialect, ProviderId,
        ProviderProfile, ReasoningConfig, RemoteModelSource, RequestOptions,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::{oneshot, Notify};

    async fn fixture() -> (String, oneshot::Receiver<String>) {
        fixture_with_response(
            r#"{"choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#
                .into(),
            "application/json",
        )
        .await
    }

    /// Bind a loopback-free RFC1918 fixture address for tests that exercise
    /// the `TrustedPrivateHttp` endpoint policy, which rejects loopback.
    async fn private_fixture_listener() -> TcpListener {
        for variable in ["COMPUTERNAME", "HOSTNAME"] {
            let Some(hostname) = std::env::var_os(variable) else {
                continue;
            };
            let hostname = hostname.to_string_lossy().into_owned();
            let Ok(addresses) =
                std::net::ToSocketAddrs::to_socket_addrs(&(hostname.as_str(), 0u16))
            else {
                continue;
            };
            for address in addresses {
                if let std::net::IpAddr::V4(ip) = address.ip() {
                    let [first, second, _, _] = ip.octets();
                    if first == 10
                        || (first == 172 && (16..=31).contains(&second))
                        || (first == 192 && second == 168)
                    {
                        return TcpListener::bind(std::net::SocketAddr::new(
                            std::net::IpAddr::V4(ip),
                            0,
                        ))
                        .await
                        .unwrap();
                    }
                }
            }
        }
        panic!("TrustedPrivateHttp test requires one local RFC1918 interface");
    }

    async fn fixture_with_response(
        response_body: String,
        content_type: &'static str,
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base_url = format!("http://{address}/v1");
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            loop {
                let count = socket.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                let Some(header_start) =
                    request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let header_end = header_start + 4;
                let content_length = String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                while request.len() < header_end + content_length {
                    let count = socket.read(&mut chunk).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                }
                let body = String::from_utf8_lossy(
                    &request
                        [header_end..header_end + content_length.min(request.len() - header_end)],
                )
                .into_owned();
                let _ = sender.send(body);
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                return;
            }
        });
        (base_url, receiver)
    }

    fn model_with_compatibility(
        compatibility: Option<OpenAiCompletionsCompatibility>,
    ) -> ModelSpec {
        let mut model = ModelSpec::custom(
            "gateway-model",
            ProviderId::new("openai-compatible").unwrap(),
            Api::OpenAiCompletions,
        );
        model.openai_completions_compatibility = compatibility;
        model
    }

    fn request(model: ModelSpec, reasoning: Option<ReasoningConfig>) -> CompletionRequest {
        CompletionRequest {
            model,
            messages: vec![Message::system("You are concise."), Message::user("hello")],
            tools: Vec::new(),
            temperature: None,
            max_output_tokens: Some(64),
            top_p: None,
            tool_choice: None,
            reasoning,
            output_constraint: None,
            retention: DataRetentionPolicy::Ephemeral,
            continuation: None,
        }
    }

    struct StaticSource {
        catalog: ModelCatalog,
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    #[async_trait]
    impl ModelCatalogSource for StaticSource {
        async fn fetch_models(
            &self,
            _profile: &ProviderProfile,
            _api_key: Option<&str>,
            _abort: Option<&AbortSignal>,
        ) -> Result<ModelCatalog, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(ProviderError::new(
                    ProviderErrorKind::Unavailable,
                    FailurePhase::BeforeDispatch,
                    "test source unavailable",
                ))
            } else {
                Ok(self.catalog.clone())
            }
        }
    }

    struct BlockingSource {
        started: Arc<Notify>,
        release: Arc<Notify>,
        catalog: ModelCatalog,
    }

    #[async_trait]
    impl ModelCatalogSource for BlockingSource {
        async fn fetch_models(
            &self,
            _profile: &ProviderProfile,
            _api_key: Option<&str>,
            abort: Option<&AbortSignal>,
        ) -> Result<ModelCatalog, ProviderError> {
            self.started.notify_one();
            if let Some(abort) = abort {
                tokio::select! {
                    _ = self.release.notified() => {}
                    _ = abort.cancelled() => {
                        return Err(ProviderError::new(
                            ProviderErrorKind::Aborted,
                            FailurePhase::DuringStream,
                            "test source aborted",
                        ));
                    }
                }
            } else {
                self.release.notified().await;
            }
            Ok(self.catalog.clone())
        }
    }

    struct SequencedSource {
        calls: Arc<AtomicUsize>,
        old_started: Arc<Notify>,
        release_old: Arc<Notify>,
        old_catalog: ModelCatalog,
        new_catalog: ModelCatalog,
    }

    #[async_trait]
    impl ModelCatalogSource for SequencedSource {
        async fn fetch_models(
            &self,
            _profile: &ProviderProfile,
            _api_key: Option<&str>,
            _abort: Option<&AbortSignal>,
        ) -> Result<ModelCatalog, ProviderError> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.old_started.notify_one();
                self.release_old.notified().await;
                Ok(self.old_catalog.clone())
            } else {
                Ok(self.new_catalog.clone())
            }
        }
    }

    struct FailingModelsStore;

    #[async_trait]
    impl ModelsStore for FailingModelsStore {
        async fn load(
            &self,
            _provider_id: &ProviderId,
        ) -> Result<Option<StoredModelCatalog>, ModelsStoreError> {
            Err(ModelsStoreError::new("test store failure"))
        }

        async fn store(
            &self,
            _entry: StoredModelCatalog,
        ) -> Result<StoreDisposition, ModelsStoreError> {
            Err(ModelsStoreError::new("test store failure"))
        }

        async fn remove(&self, _provider_id: &ProviderId) -> Result<(), ModelsStoreError> {
            Err(ModelsStoreError::new("test store failure"))
        }
    }

    fn profile_for_refresh() -> ProviderProfile {
        let provider = ProviderId::new("openai-compatible").unwrap();
        let model = ModelSpec::custom("known-model", provider.clone(), Api::OpenAiCompletions);
        ProviderProfile::new(provider, Api::OpenAiCompletions, ModelCatalog::new([model]))
            .with_auth(AuthRequirement::None)
            .with_remote_model_source(RemoteModelSource::models())
    }

    #[test]
    fn published_profile_is_a_synchronous_last_known_snapshot() {
        let profile = profile_for_refresh();
        let models = Models::new();
        models.publish_profile(profile.clone()).unwrap();

        let snapshot = models.profile_snapshot("openai-compatible").unwrap();
        assert_eq!(snapshot.catalog.list()[0].id, "known-model");
        assert_eq!(models.profile("openai-compatible"), Some(profile));
    }

    #[tokio::test]
    async fn refresh_publishes_a_complete_catalog_and_preserves_compatibility() {
        let provider = ProviderId::new("openai-compatible").unwrap();
        let mut profile = profile_for_refresh();
        let known = profile
            .catalog
            .get(provider.as_str(), "known-model")
            .unwrap();
        let mut known = known.clone();
        known.openai_completions_compatibility = Some(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            thinking_dialect: OpenAiThinkingDialect::Together,
            supports_reasoning_effort: false,
        });
        profile.catalog.insert(known);

        let mut discovered =
            ModelSpec::custom("known-model", provider.clone(), Api::OpenAiCompletions);
        discovered.name = Some("Known remote model".into());
        let new_model = ModelSpec::custom("new-model", provider.clone(), Api::OpenAiCompletions);
        let source = StaticSource {
            catalog: ModelCatalog::new([discovered, new_model]),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        };
        let calls = Arc::clone(&source.calls);
        let models = Models::with_source(source).with_profile(profile).unwrap();

        let refreshed = models.refresh("openai-compatible", None).await.unwrap();
        assert_eq!(refreshed.catalog.list().len(), 2);
        assert_eq!(
            refreshed
                .model("known-model")
                .unwrap()
                .openai_completions_compatibility
                .unwrap()
                .system_role,
            OpenAiSystemRole::Developer
        );
        assert!(models
            .profile("openai-compatible")
            .unwrap()
            .model("new-model")
            .is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_refresh_keeps_the_previous_profile() {
        let source = StaticSource {
            catalog: ModelCatalog::default(),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: true,
        };
        let models = Models::with_source(source)
            .with_profile(profile_for_refresh())
            .unwrap();
        let before = models.profile_snapshot("openai-compatible").unwrap();

        let error = models.refresh("openai-compatible", None).await.unwrap_err();

        assert_eq!(error.kind, ProviderErrorKind::Unavailable);
        let after = models.profile_snapshot("openai-compatible").unwrap();
        assert!(Arc::ptr_eq(&before, &after));
    }

    #[tokio::test]
    async fn aborted_refresh_does_not_call_the_source_or_publish() {
        let source = StaticSource {
            catalog: ModelCatalog::default(),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        };
        let calls = Arc::clone(&source.calls);
        let models = Models::with_source(source)
            .with_profile(profile_for_refresh())
            .unwrap();
        let before = models.profile_snapshot("openai-compatible").unwrap();
        let abort = AbortSignal::new();
        abort.abort();

        let error = models
            .refresh("openai-compatible", Some(abort))
            .await
            .unwrap_err();

        assert_eq!(error.kind, ProviderErrorKind::Aborted);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(Arc::ptr_eq(
            &before,
            &models.profile_snapshot("openai-compatible").unwrap()
        ));
    }

    #[tokio::test]
    async fn superseded_refresh_cannot_clobber_a_new_profile() {
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let source = BlockingSource {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
            catalog: ModelCatalog::default(),
        };
        let models = Arc::new(
            Models::with_source(source)
                .with_profile(profile_for_refresh())
                .unwrap(),
        );
        let refresh_models = Arc::clone(&models);
        let refresh =
            tokio::spawn(async move { refresh_models.refresh("openai-compatible", None).await });
        started.notified().await;

        let replacement = profile_for_refresh();
        models.publish_profile(replacement.clone()).unwrap();
        release.notify_one();
        let error = refresh.await.unwrap().unwrap_err();

        assert_eq!(error.kind, ProviderErrorKind::Unavailable);
        assert_eq!(models.profile("openai-compatible"), Some(replacement));
    }

    #[tokio::test]
    async fn required_profile_auth_fails_before_refresh_source() {
        let source = StaticSource {
            catalog: ModelCatalog::default(),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        };
        let calls = Arc::clone(&source.calls);
        let profile = profile_for_refresh();
        let profile = profile.with_auth(AuthRequirement::Required);
        let models = Models::with_source(source).with_profile(profile).unwrap();

        let error = models.refresh("openai-compatible", None).await.unwrap_err();

        assert_eq!(error.kind, ProviderErrorKind::Authentication);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn models_connection_applies_combined_openai_compatibility() {
        let (base_url, request_receiver) = fixture().await;
        let mut models = Models::new()
            .with_api_key("openai-compatible", "secret")
            .unwrap();
        let model = model_with_compatibility(Some(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            thinking_dialect: OpenAiThinkingDialect::Together,
            supports_reasoning_effort: false,
        }));
        models.catalog_mut().insert(model);
        let model = models
            .catalog()
            .get("openai-compatible", "gateway-model")
            .unwrap();
        let provider = models
            .connect_with_api_at(model, Api::OpenAiCompletions, Some(base_url))
            .unwrap();

        provider
            .complete(request(
                (*model).clone(),
                Some(ReasoningConfig::enabled(None)),
            ))
            .await
            .unwrap();

        let body: serde_json::Value =
            serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
        assert_eq!(body["messages"][0]["role"], "developer");
        assert_eq!(body["max_completion_tokens"], 64);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["reasoning"]["enabled"], true);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[tokio::test]
    async fn models_connection_without_compatibility_keeps_default_payload() {
        let (base_url, request_receiver) = fixture().await;
        let mut models = Models::new()
            .with_api_key("openai-compatible", "secret")
            .unwrap();
        let model = model_with_compatibility(None);
        models.catalog_mut().insert(model);
        let model = models
            .catalog()
            .get("openai-compatible", "gateway-model")
            .unwrap();
        let provider = models
            .connect_with_api_at(model, Api::OpenAiCompletions, Some(base_url))
            .unwrap();

        provider
            .complete(request((*model).clone(), None))
            .await
            .unwrap();

        let body: serde_json::Value =
            serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["max_tokens"], 64);
        assert!(body.get("max_completion_tokens").is_none());
    }

    #[tokio::test]
    async fn models_connection_ignores_chat_compatibility_for_responses() {
        let (base_url, request_receiver) = fixture_with_response(
            r#"{"id":"resp_1","error":null,"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}"#
                .into(),
            "application/json",
        )
        .await;
        let mut models = Models::new().with_api_key("openai", "secret").unwrap();
        let mut model = ModelSpec::custom(
            "responses-model",
            ProviderId::new("openai").unwrap(),
            Api::OpenAiResponses,
        );
        model.openai_completions_compatibility = Some(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            supports_reasoning_effort: false,
        });
        models.catalog_mut().insert(model);
        let model = models.catalog().get("openai", "responses-model").unwrap();
        let provider = models
            .connect_with_api_at(model, Api::OpenAiResponses, Some(base_url))
            .unwrap();

        provider
            .complete(request((*model).clone(), None))
            .await
            .unwrap();

        let body: serde_json::Value =
            serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
        assert_eq!(body["max_output_tokens"], 64);
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("enable_thinking").is_none());
        assert_eq!(body["instructions"], "You are concise.");
    }

    #[tokio::test]
    async fn models_connection_ignores_chat_compatibility_for_anthropic() {
        let (base_url, request_receiver) = fixture_with_response(
            r#"{"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#
                .into(),
            "application/json",
        )
        .await;
        let mut models = Models::new().with_api_key("anthropic", "secret").unwrap();
        let mut model = ModelSpec::custom(
            "anthropic-model",
            ProviderId::new("anthropic").unwrap(),
            Api::AnthropicMessages,
        );
        model.openai_completions_compatibility = Some(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            supports_reasoning_effort: false,
        });
        models.catalog_mut().insert(model);
        let model = models
            .catalog()
            .get("anthropic", "anthropic-model")
            .unwrap();
        let provider = models
            .connect_with_api_at(model, Api::AnthropicMessages, Some(base_url))
            .unwrap();

        provider
            .complete(request((*model).clone(), None))
            .await
            .unwrap();

        let body: serde_json::Value =
            serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
        assert_eq!(body["max_tokens"], 64);
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("enable_thinking").is_none());
        assert!(body.get("reasoning").is_none());
        assert_eq!(body["system"], "You are concise.");
    }

    #[tokio::test]
    async fn http_refresh_reads_remote_models_and_keeps_known_compatibility() {
        let (base_url, request_receiver) = fixture_with_response(
            r#"{"data":[{"id":"known-model","name":"Remote name"},{"id":"new-model"}]}"#.into(),
            "application/json",
        )
        .await;
        let provider = ProviderId::new("openai-compatible").unwrap();
        let mut profile = profile_for_refresh().with_base_url(base_url);
        let mut known = profile
            .catalog
            .get(provider.as_str(), "known-model")
            .unwrap()
            .clone();
        known.openai_completions_compatibility = Some(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            thinking_dialect: OpenAiThinkingDialect::Qwen,
            supports_reasoning_effort: false,
        });
        profile.catalog.insert(known);
        let models = Models::new().with_profile(profile).unwrap();

        let refreshed = models.refresh("openai-compatible", None).await.unwrap();

        assert_eq!(refreshed.catalog.list().len(), 2);
        assert_eq!(
            refreshed
                .model("known-model")
                .unwrap()
                .openai_completions_compatibility
                .unwrap()
                .thinking_dialect,
            OpenAiThinkingDialect::Qwen
        );
        let request = request_receiver.await.unwrap();
        assert!(request.is_empty());
    }

    #[tokio::test]
    async fn profile_connection_uses_profile_endpoint_and_auth_contract() {
        let listener = private_fixture_listener().await;
        let address = listener.local_addr().unwrap();
        let base_url = format!("http://{address}/v1");
        let (request_sender, request_receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            loop {
                let count = socket.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                let Some(header_start) =
                    request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let header_end = header_start + 4;
                let content_length = String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                while request.len() < header_end + content_length {
                    let count = socket.read(&mut chunk).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                }
                let body_text = String::from_utf8_lossy(
                    &request
                        [header_end..header_end + content_length.min(request.len() - header_end)],
                )
                .into_owned();
                let body = "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}";
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                let _ = request_sender.send(body_text);
                return;
            }
        });
        let profile = profile_for_refresh()
            .with_base_url(base_url)
            .with_endpoint_policy(crate::providers::EndpointPolicy::TrustedPrivateHttp);
        let models = Models::new().with_profile(profile).unwrap();
        let mut model = models.get("openai-compatible", "known-model").unwrap();
        model.openai_completions_compatibility = Some(OpenAiCompletionsCompatibility {
            system_role: OpenAiSystemRole::Developer,
            max_output_tokens_field: MaxOutputTokensField::MaxCompletionTokens,
            thinking_dialect: OpenAiThinkingDialect::Together,
            supports_reasoning_effort: false,
        });
        let connected = models.connect(&model).unwrap();

        let credential_error = connected
            .complete_with(
                request(model.clone(), None),
                RequestOptions {
                    abort: None,
                    headers: vec![("Authorization".into(), "Bearer caller-secret".into())],
                },
            )
            .await
            .unwrap_err();
        assert_eq!(credential_error.kind, ProviderErrorKind::InvalidRequest);
        assert_eq!(credential_error.phase, FailurePhase::BeforeDispatch);

        connected
            .complete(request(model, Some(ReasoningConfig::enabled(None))))
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_str(&request_receiver.await.unwrap()).unwrap();
        assert_eq!(body["messages"][0]["role"], "developer");
        assert_eq!(body["reasoning"]["enabled"], true);
        assert!(body.get("authorization").is_none());
    }

    #[test]
    fn generic_profile_connection_retains_trusted_private_http_policy() {
        let provider = ProviderId::new("private-profile").unwrap();
        let model = ModelSpec::custom("private-model", provider.clone(), Api::OpenAiCompletions);
        let profile = ProviderProfile::new(
            provider,
            Api::OpenAiCompletions,
            ModelCatalog::new([model.clone()]),
        )
        .with_auth(AuthRequirement::None)
        .with_base_url("http://192.168.1.10/v1")
        .with_endpoint_policy(crate::providers::EndpointPolicy::TrustedPrivateHttp);
        let models = Models::new().with_profile(profile).unwrap();

        let connected = models.connect(&model).unwrap();

        assert_eq!(connected.api(), &Api::OpenAiCompletions);
    }

    #[test]
    fn connect_with_api_rejects_profile_api_conflicts() {
        let profile = profile_for_refresh();
        let model = profile.model("known-model").unwrap().clone();
        let models = Models::new().with_profile(profile).unwrap();

        let error = match models.connect_with_api(&model, Api::OpenAiResponses) {
            Ok(_) => panic!("profile API conflict unexpectedly connected"),
            Err(error) => error,
        };

        assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
        assert_eq!(error.phase, FailurePhase::BeforeDispatch);
        assert!(error.message.contains("conflicts with registered profile"));
    }

    #[tokio::test]
    async fn unauthenticated_profiles_reject_request_credentials_for_all_transports() {
        for (provider_name, api) in [
            ("none-openai", Api::OpenAiCompletions),
            ("none-responses", Api::OpenAiResponses),
            ("none-anthropic", Api::AnthropicMessages),
        ] {
            let provider = ProviderId::new(provider_name).unwrap();
            let model = ModelSpec::custom("local-model", provider.clone(), api.clone());
            let profile =
                ProviderProfile::new(provider.clone(), api, ModelCatalog::new([model.clone()]))
                    .with_auth(AuthRequirement::None);
            let models = Models::new().with_profile(profile).unwrap();
            let connection = models
                .connect_profile(provider_name, "local-model")
                .unwrap();
            let error = connection
                .complete_with(
                    CompletionRequest::new(model, vec![Message::user("hello")]),
                    RequestOptions {
                        abort: None,
                        headers: vec![("Authorization".into(), "Bearer caller-secret".into())],
                    },
                )
                .await
                .unwrap_err();
            assert_eq!(error.kind, ProviderErrorKind::InvalidRequest);
            assert_eq!(error.phase, FailurePhase::BeforeDispatch);
        }
    }

    #[tokio::test]
    async fn shared_store_restores_offline_without_calling_the_source() {
        let provider = ProviderId::new("offline-provider").unwrap();
        let discovered =
            ModelSpec::custom("discovered-model", provider.clone(), Api::OpenAiCompletions);
        let shared = Arc::new(InMemoryModelsStore::new());
        let first_source = StaticSource {
            catalog: ModelCatalog::new([discovered.clone()]),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        };
        let first_calls = Arc::clone(&first_source.calls);
        let profile = ProviderProfile::new(
            provider.clone(),
            Api::OpenAiCompletions,
            ModelCatalog::default(),
        )
        .with_auth(AuthRequirement::None)
        .with_remote_model_source(RemoteModelSource::models());
        let first = Models::with_source(first_source)
            .with_models_store(shared.clone())
            .with_profile(profile.clone())
            .unwrap();
        first.refresh("offline-provider", None).await.unwrap();
        let first_generation = shared.read(&provider).await.unwrap().unwrap().generation;
        first.refresh("offline-provider", None).await.unwrap();
        let second_generation = shared.read(&provider).await.unwrap().unwrap().generation;
        first.refresh("offline-provider", None).await.unwrap();
        let persisted_generation = shared.read(&provider).await.unwrap().unwrap().generation;
        assert_eq!(first_calls.load(Ordering::SeqCst), 3);
        assert!(first_generation < second_generation);
        assert!(second_generation < persisted_generation);

        let second_source = StaticSource {
            catalog: ModelCatalog::new([discovered]),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        };
        let second_calls = Arc::clone(&second_source.calls);
        let second = Models::with_source(second_source)
            .with_models_store(shared.clone())
            .with_profile(profile)
            .unwrap();
        let restored = second.restore_with_outcome("offline-provider").await;
        assert_eq!(restored.status, RestoreStatus::Restored);
        assert_eq!(restored.generation, persisted_generation);
        assert!(second
            .profile_model("offline-provider", "discovered-model")
            .is_ok());
        assert_eq!(second_calls.load(Ordering::SeqCst), 0);

        let refreshed = second.refresh_with_outcome("offline-provider", None).await;
        assert_eq!(refreshed.status, RefreshStatus::Published);
        assert!(refreshed.generation > persisted_generation);
        assert_eq!(second_calls.load(Ordering::SeqCst), 1);
        let after_refresh = shared.read(&provider).await.unwrap().unwrap();
        assert_eq!(after_refresh.generation, refreshed.generation);
    }

    #[tokio::test]
    async fn equal_cross_process_revisions_cannot_overwrite_newer_stored_facts() {
        let provider = ProviderId::new("cross-process-provider").unwrap();
        let profile = ProviderProfile::new(
            provider.clone(),
            Api::OpenAiCompletions,
            ModelCatalog::default(),
        )
        .with_auth(AuthRequirement::None)
        .with_remote_model_source(RemoteModelSource::models());
        let shared = Arc::new(InMemoryModelsStore::new());
        let seed_model = ModelSpec::custom("seed-model", provider.clone(), Api::OpenAiCompletions);
        let seed = Models::with_source(StaticSource {
            catalog: ModelCatalog::new([seed_model]),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        })
        .with_models_store(shared.clone())
        .with_profile(profile.clone())
        .unwrap();
        seed.refresh("cross-process-provider", None).await.unwrap();
        let persisted_generation = shared.read(&provider).await.unwrap().unwrap().generation;

        let old_started = Arc::new(Notify::new());
        let release_old = Arc::new(Notify::new());
        let old_model = ModelSpec::custom("old-model", provider.clone(), Api::OpenAiCompletions);
        let old_models = Arc::new(
            Models::with_source(BlockingSource {
                started: Arc::clone(&old_started),
                release: Arc::clone(&release_old),
                catalog: ModelCatalog::new([old_model]),
            })
            .with_models_store(shared.clone())
            .with_profile(profile.clone())
            .unwrap(),
        );
        let new_model = ModelSpec::custom("new-model", provider.clone(), Api::OpenAiCompletions);
        let new_models = Models::with_source(StaticSource {
            catalog: ModelCatalog::new([new_model]),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        })
        .with_models_store(shared.clone())
        .with_profile(profile)
        .unwrap();
        let old_restore = old_models
            .restore_with_outcome("cross-process-provider")
            .await;
        let new_restore = new_models
            .restore_with_outcome("cross-process-provider")
            .await;
        assert_eq!(old_restore.status, RestoreStatus::Restored);
        assert_eq!(new_restore.status, RestoreStatus::Restored);
        assert_eq!(old_restore.generation, persisted_generation);
        assert_eq!(new_restore.generation, persisted_generation);

        let older_models = Arc::clone(&old_models);
        let older = tokio::spawn(async move {
            older_models
                .refresh_with_outcome("cross-process-provider", None)
                .await
        });
        old_started.notified().await;
        let newer = new_models
            .refresh_with_outcome("cross-process-provider", None)
            .await;
        assert_eq!(newer.status, RefreshStatus::Published);
        assert!(newer.generation > persisted_generation);
        release_old.notify_one();
        let older = older.await.unwrap();
        assert_eq!(older.status, RefreshStatus::Superseded);

        let stored = shared
            .read(&provider)
            .await
            .unwrap()
            .expect("newer cross-process refresh should remain in the store");
        assert!(stored
            .models
            .get("cross-process-provider", "new-model")
            .is_some());
        assert!(stored
            .models
            .get("cross-process-provider", "old-model")
            .is_none());
    }

    #[tokio::test]
    async fn store_failure_keeps_the_visible_profile_unchanged() {
        let before_profile = profile_for_refresh();
        let source = StaticSource {
            catalog: ModelCatalog::new([ModelSpec::custom(
                "new-model",
                before_profile.provider_id.clone(),
                before_profile.api.clone(),
            )]),
            calls: Arc::new(AtomicUsize::new(0)),
            fail: false,
        };
        let models = Models::with_source(source)
            .with_models_store(Arc::new(FailingModelsStore))
            .with_profile(before_profile)
            .unwrap();
        let before = models.profile_snapshot("openai-compatible").unwrap();
        let outcome = models.refresh_with_outcome("openai-compatible", None).await;
        assert_eq!(outcome.status, RefreshStatus::Failed);
        let after = models.profile_snapshot("openai-compatible").unwrap();
        assert!(Arc::ptr_eq(&before, &after));
    }

    #[tokio::test]
    async fn newer_started_refresh_supersedes_a_late_older_refresh() {
        let provider = ProviderId::new("generation-provider").unwrap();
        let old_model = ModelSpec::custom("old-model", provider.clone(), Api::OpenAiCompletions);
        let new_model = ModelSpec::custom("new-model", provider.clone(), Api::OpenAiCompletions);
        let source = SequencedSource {
            calls: Arc::new(AtomicUsize::new(0)),
            old_started: Arc::new(Notify::new()),
            release_old: Arc::new(Notify::new()),
            old_catalog: ModelCatalog::new([old_model]),
            new_catalog: ModelCatalog::new([new_model]),
        };
        let old_started = Arc::clone(&source.old_started);
        let release_old = Arc::clone(&source.release_old);
        let shared = Arc::new(InMemoryModelsStore::new());
        let models = Arc::new(
            Models::with_source(source)
                .with_models_store(shared.clone())
                .with_profile(
                    ProviderProfile::new(
                        provider.clone(),
                        Api::OpenAiCompletions,
                        ModelCatalog::default(),
                    )
                    .with_auth(AuthRequirement::None)
                    .with_remote_model_source(RemoteModelSource::models()),
                )
                .unwrap(),
        );
        let older_models = Arc::clone(&models);
        let older = tokio::spawn(async move {
            older_models
                .refresh_with_outcome("generation-provider", None)
                .await
        });
        old_started.notified().await;
        let newer = models
            .refresh_with_outcome("generation-provider", None)
            .await;
        assert_eq!(newer.status, RefreshStatus::Published);
        release_old.notify_one();
        let older = older.await.unwrap();
        assert_eq!(older.status, RefreshStatus::Superseded);
        assert!(models
            .profile_model("generation-provider", "new-model")
            .is_ok());
        assert!(models
            .profile_model("generation-provider", "old-model")
            .is_err());
        let stored = shared
            .read(&provider)
            .await
            .unwrap()
            .expect("newer refresh should remain in the store");
        assert!(stored
            .models
            .get("generation-provider", "new-model")
            .is_some());
        assert!(stored
            .models
            .get("generation-provider", "old-model")
            .is_none());
    }

    #[tokio::test]
    async fn trusted_private_http_catalog_rejects_redirects() {
        let redirect_listener = private_fixture_listener().await;
        let redirect_address = redirect_listener.local_addr().unwrap();
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target_url = format!("http://{target_address}/final");
        tokio::spawn(async move {
            let (mut socket, _) = redirect_listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            let response = format!(
                "HTTP/1.1 302 Found\r\nlocation: {target_url}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        let provider = ProviderId::new("trusted-catalog").unwrap();
        let profile =
            ProviderProfile::new(provider, Api::OpenAiCompletions, ModelCatalog::default())
                .with_auth(AuthRequirement::None)
                .with_endpoint_policy(crate::providers::EndpointPolicy::TrustedPrivateHttp)
                .with_remote_model_source(RemoteModelSource::new(format!(
                    "http://{redirect_address}/models"
                )));
        let error = HttpModelCatalogSource::new()
            .fetch_models(&profile, None, None)
            .await
            .unwrap_err();
        assert_eq!(error.http_status, Some(302));
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(100),
            target_listener.accept()
        )
        .await
        .is_err());
    }
}
