use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Api, ModelCatalog, ProviderId};

/// Non-secret catalog facts that may be retained between processes.
///
/// A stored entry deliberately contains no base URL, endpoint policy, auth
/// requirement, credential, or remote-source configuration. Those values are
/// supplied by the currently registered [`crate::ProviderProfile`] during
/// restore and are never reconstructed from this value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredModelCatalog {
    pub provider_id: ProviderId,
    pub api: Api,
    pub models: ModelCatalog,
    /// Unix timestamp in seconds at which the catalog was checked.
    pub checked_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// Stable identity of the configured remote source, normally its endpoint.
    pub source_identity: String,
    /// Provider-local persisted revision used to prevent stale concurrent
    /// refreshes from overwriting newer stored facts. A process that restores
    /// this value must use a strictly newer revision for its next refresh.
    #[serde(default)]
    pub generation: u64,
}

impl StoredModelCatalog {
    pub fn new(
        provider_id: ProviderId,
        api: Api,
        models: ModelCatalog,
        checked_at: u64,
        source_identity: impl Into<String>,
    ) -> Self {
        Self {
            provider_id,
            api,
            models,
            checked_at,
            etag: None,
            last_modified: None,
            source_identity: source_identity.into(),
            generation: 0,
        }
    }

    pub fn validate(&self) -> Result<(), ModelsStoreError> {
        if self.provider_id.as_str().trim().is_empty()
            || self.provider_id.as_str().chars().count() > 2_048
        {
            return Err(ModelsStoreError::new("stored provider id is invalid"));
        }
        if self.source_identity.chars().count() > 2_048 {
            return Err(ModelsStoreError::new(
                "stored source identity exceeds the length limit",
            ));
        }
        if self
            .etag
            .as_deref()
            .is_some_and(|value| value.chars().count() > 512)
            || self
                .last_modified
                .as_deref()
                .is_some_and(|value| value.chars().count() > 512)
        {
            return Err(ModelsStoreError::new(
                "stored freshness metadata exceeds the length limit",
            ));
        }
        if self.models.list().len() > 1_024 {
            return Err(ModelsStoreError::new(
                "stored catalog contains too many models",
            ));
        }
        for model in self.models.list() {
            if model.id.trim().is_empty() || model.id.chars().count() > 256 {
                return Err(ModelsStoreError::new("stored model id is invalid"));
            }
            if model
                .name
                .as_deref()
                .is_some_and(|value| value.chars().count() > 512)
            {
                return Err(ModelsStoreError::new(
                    "stored model name exceeds the length limit",
                ));
            }
            if model.provider != self.provider_id || model.api != self.api {
                return Err(ModelsStoreError::new(
                    "stored model does not match its provider identity",
                ));
            }
        }
        Ok(())
    }
}

/// A bounded, non-secret persistence failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelsStoreError {
    pub message: String,
}

impl ModelsStoreError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into().chars().take(2_048).collect(),
        }
    }
}

impl fmt::Display for ModelsStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ModelsStoreError {}

/// Result of a strictly generation-increasing store write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreDisposition {
    Stored,
    /// The incoming revision was equal to or older than the stored revision.
    SkippedOlderGeneration,
}

/// Async storage boundary for non-secret model catalog facts.
///
/// Implementations must treat `generation` as strictly increasing per
/// provider: an entry with an equal or older generation may not replace the
/// stored entry. This lets a slow refresh finish safely even when a newer
/// refresh has already staged its result, including when separate processes
/// independently derived the same next revision after restoring the same
/// generation. The in-process operation order used for publication is
/// separate from this persisted revision.
#[async_trait]
pub trait ModelsStore: Send + Sync {
    async fn load(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<StoredModelCatalog>, ModelsStoreError>;

    async fn store(&self, entry: StoredModelCatalog) -> Result<StoreDisposition, ModelsStoreError>;

    async fn remove(&self, provider_id: &ProviderId) -> Result<(), ModelsStoreError>;
}

/// In-memory generation-aware store useful for applications, tests, and
/// composition with a higher-level persistence owner.
#[derive(Clone, Default)]
pub struct InMemoryModelsStore {
    entries: Arc<Mutex<HashMap<String, StoredModelCatalog>>>,
}

impl InMemoryModelsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn read(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<StoredModelCatalog>, ModelsStoreError> {
        self.load(provider_id).await
    }

    pub async fn write(
        &self,
        entry: StoredModelCatalog,
    ) -> Result<StoreDisposition, ModelsStoreError> {
        self.store(entry).await
    }

    pub async fn delete(&self, provider_id: &ProviderId) -> Result<(), ModelsStoreError> {
        self.remove(provider_id).await
    }
}

#[async_trait]
impl ModelsStore for InMemoryModelsStore {
    async fn load(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<StoredModelCatalog>, ModelsStoreError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| ModelsStoreError::new("model store lock is unavailable"))?;
        Ok(entries.get(provider_id.as_str()).cloned())
    }

    async fn store(&self, entry: StoredModelCatalog) -> Result<StoreDisposition, ModelsStoreError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| ModelsStoreError::new("model store lock is unavailable"))?;
        if entries
            .get(entry.provider_id.as_str())
            .is_some_and(|current| current.generation >= entry.generation)
        {
            return Ok(StoreDisposition::SkippedOlderGeneration);
        }
        entries.insert(entry.provider_id.to_string(), entry);
        Ok(StoreDisposition::Stored)
    }

    async fn remove(&self, provider_id: &ProviderId) -> Result<(), ModelsStoreError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| ModelsStoreError::new("model store lock is unavailable"))?;
        entries.remove(provider_id.as_str());
        Ok(())
    }
}
