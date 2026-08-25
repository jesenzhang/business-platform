use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use async_trait::async_trait;

use crate::{FailurePhase, ProviderError, ProviderErrorKind, ProviderId};

/// Resolved provider credential. Secrets are redacted in Debug.
#[derive(Clone)]
pub struct Credential {
    pub provider: ProviderId,
    pub api_key: String,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Credential")
            .field("provider", &self.provider)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl Credential {
    pub fn api_key(provider: ProviderId, api_key: impl Into<String>) -> Self {
        Self {
            provider,
            api_key: api_key.into(),
        }
    }

    pub fn token(&self) -> &str {
        &self.api_key
    }
}

/// An externally-issued short-lived OAuth/WIF credential.
///
/// The crate owns its in-memory lifecycle only. It does not implement a
/// browser login or token exchange; an application supplies a
/// [`CredentialRefresher`] for the provider-specific exchange.
#[derive(Clone)]
pub struct OAuthCredential {
    pub provider: ProviderId,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<SystemTime>,
}

impl OAuthCredential {
    pub fn new(
        provider: ProviderId,
        access_token: impl Into<String>,
        refresh_token: Option<impl Into<String>>,
        expires_at: Option<SystemTime>,
    ) -> Self {
        Self {
            provider,
            access_token: access_token.into(),
            refresh_token: refresh_token.map(Into::into),
            expires_at,
        }
    }

    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at <= SystemTime::now())
    }
}

impl std::fmt::Debug for OAuthCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthCredential")
            .field("provider", &self.provider)
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialKind {
    ApiKey,
    OAuth,
}

/// A credential selected for one request. It intentionally has no serde
/// implementation so token material cannot enter profiles or catalog stores.
#[derive(Clone)]
pub enum ResolvedCredential {
    ApiKey(Credential),
    OAuth(OAuthCredential),
}

impl ResolvedCredential {
    pub fn provider(&self) -> &ProviderId {
        match self {
            Self::ApiKey(credential) => &credential.provider,
            Self::OAuth(credential) => &credential.provider,
        }
    }

    pub fn kind(&self) -> CredentialKind {
        match self {
            Self::ApiKey(_) => CredentialKind::ApiKey,
            Self::OAuth(_) => CredentialKind::OAuth,
        }
    }

    pub fn token(&self) -> &str {
        match self {
            Self::ApiKey(credential) => credential.token(),
            Self::OAuth(credential) => credential.access_token(),
        }
    }

    pub fn refresh_token(&self) -> Option<&str> {
        match self {
            Self::ApiKey(_) => None,
            Self::OAuth(credential) => credential.refresh_token(),
        }
    }

    pub fn expires_at(&self) -> Option<SystemTime> {
        match self {
            Self::ApiKey(_) => None,
            Self::OAuth(credential) => credential.expires_at(),
        }
    }

    pub fn is_expired(&self) -> bool {
        match self {
            Self::ApiKey(_) => false,
            Self::OAuth(credential) => credential.is_expired(),
        }
    }
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedCredential")
            .field("provider", self.provider())
            .field("kind", &self.kind())
            .field("token", &"[REDACTED]")
            .field("refresh_token", &self.refresh_token().map(|_| "[REDACTED]"))
            .field("expires_at", &self.expires_at())
            .finish()
    }
}

/// Application-owned provider-specific OAuth/WIF refresh operation.
#[async_trait]
pub trait CredentialRefresher: Send + Sync {
    async fn refresh(
        &self,
        provider: &ProviderId,
        refresh_token: Option<&str>,
    ) -> Result<OAuthCredential, ProviderError>;
}

#[async_trait]
pub trait CredentialStore: Send + Sync {
    fn get(&self, provider: &ProviderId) -> Option<Credential>;
    fn set(&self, credential: Credential);
    fn get_oauth(&self, _provider: &ProviderId) -> Option<OAuthCredential> {
        None
    }
    fn set_oauth(&self, _credential: OAuthCredential) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            FailurePhase::BeforeDispatch,
            "configured credential store does not support OAuth credentials",
        ))
    }
    fn remove(&self, provider: &ProviderId);

    /// Whether a local clear/revoke operation blocks environment fallback.
    ///
    /// Stores that support revocation should return `true` after `clear` or
    /// `revoke`, and return `false` after a new credential is set. The
    /// default preserves compatibility for legacy stores that only implement
    /// the API-key methods.
    fn is_revoked(&self, _provider: &ProviderId) -> bool {
        false
    }

    fn clear(&self, provider: &ProviderId) {
        self.remove(provider);
    }

    fn revoke(&self, provider: &ProviderId) {
        self.remove(provider);
    }

    fn get_resolved(&self, provider: &ProviderId) -> Option<ResolvedCredential> {
        self.get_oauth(provider)
            .map(ResolvedCredential::OAuth)
            .or_else(|| self.get(provider).map(ResolvedCredential::ApiKey))
    }

    fn set_resolved(&self, credential: ResolvedCredential) -> Result<(), ProviderError> {
        match credential {
            ResolvedCredential::ApiKey(credential) => {
                self.set(credential);
                Ok(())
            }
            ResolvedCredential::OAuth(credential) => self.set_oauth(credential),
        }
    }

    /// Resolve the current usable credential.
    ///
    /// A store that supports OAuth refresh must override this method and
    /// publish refreshed credentials atomically. The compatibility default
    /// refuses an expired or empty credential instead of performing an
    /// unsafe read-refresh-write sequence.
    async fn resolve(
        &self,
        provider: &ProviderId,
        refresher: Option<Arc<dyn CredentialRefresher>>,
    ) -> Result<Option<ResolvedCredential>, ProviderError> {
        let Some(current) = self.get_resolved(provider) else {
            return Ok(None);
        };
        if !current.token().trim().is_empty() && !current.is_expired() {
            return Ok(Some(current));
        }
        let _ = refresher;
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            FailurePhase::BeforeDispatch,
            format!("credential store for {provider} does not provide atomic OAuth refresh"),
        ))
    }
}

#[derive(Clone, Default)]
pub struct MemoryCredentialStore {
    inner: Arc<Mutex<CredentialState>>,
    refresh_locks: Arc<Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

#[derive(Clone)]
enum StoredCredential {
    ApiKey(Credential),
    OAuth(OAuthCredential),
}

#[derive(Default)]
struct CredentialState {
    entries: BTreeMap<String, StoredCredential>,
    epochs: BTreeMap<String, u64>,
    revoked: BTreeSet<String>,
}

impl MemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_api_key(&self, provider: ProviderId, api_key: impl Into<String>) {
        <Self as CredentialStore>::set(self, Credential::api_key(provider, api_key));
    }

    pub fn set_oauth(&self, credential: OAuthCredential) -> Result<(), ProviderError> {
        <Self as CredentialStore>::set_oauth(self, credential)
    }

    pub fn clear(&self, provider: &ProviderId) {
        <Self as CredentialStore>::clear(self, provider);
    }

    pub fn revoke(&self, provider: &ProviderId) {
        <Self as CredentialStore>::revoke(self, provider);
    }

    fn current(&self, provider: &ProviderId) -> Option<ResolvedCredential> {
        let inner = self.inner.lock().ok()?;
        match inner.entries.get(provider.as_str())? {
            StoredCredential::ApiKey(credential) => {
                Some(ResolvedCredential::ApiKey(credential.clone()))
            }
            StoredCredential::OAuth(credential) => {
                Some(ResolvedCredential::OAuth(credential.clone()))
            }
        }
    }

    fn current_with_epoch(&self, provider: &ProviderId) -> Option<(ResolvedCredential, u64)> {
        let inner = self.inner.lock().ok()?;
        let credential = match inner.entries.get(provider.as_str())? {
            StoredCredential::ApiKey(credential) => ResolvedCredential::ApiKey(credential.clone()),
            StoredCredential::OAuth(credential) => ResolvedCredential::OAuth(credential.clone()),
        };
        Some((
            credential,
            inner
                .epochs
                .get(provider.as_str())
                .copied()
                .unwrap_or_default(),
        ))
    }

    fn bump_epoch(inner: &mut CredentialState, provider: &ProviderId) -> u64 {
        let epoch = inner.epochs.entry(provider.to_string()).or_default();
        *epoch = epoch.saturating_add(1);
        *epoch
    }

    fn publish_refresh_if_current(
        &self,
        provider: &ProviderId,
        expected_epoch: u64,
        credential: OAuthCredential,
    ) -> Result<bool, ProviderError> {
        let mut inner = self.inner.lock().map_err(|_| auth_lock_error())?;
        if inner
            .epochs
            .get(provider.as_str())
            .copied()
            .unwrap_or_default()
            != expected_epoch
        {
            return Ok(false);
        }
        Self::bump_epoch(&mut inner, provider);
        inner.revoked.remove(provider.as_str());
        inner
            .entries
            .insert(provider.to_string(), StoredCredential::OAuth(credential));
        Ok(true)
    }

    fn refresh_lock(
        &self,
        provider: &ProviderId,
    ) -> Result<Arc<tokio::sync::Mutex<()>>, ProviderError> {
        let mut locks = self.refresh_locks.lock().map_err(|_| auth_lock_error())?;
        Ok(Arc::clone(
            locks
                .entry(provider.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        ))
    }
}

#[async_trait]
impl CredentialStore for MemoryCredentialStore {
    fn get(&self, provider: &ProviderId) -> Option<Credential> {
        self.inner
            .lock()
            .ok()?
            .entries
            .get(provider.as_str())
            .and_then(|credential| match credential {
                StoredCredential::ApiKey(credential) => Some(credential.clone()),
                StoredCredential::OAuth(_) => None,
            })
    }

    fn get_oauth(&self, provider: &ProviderId) -> Option<OAuthCredential> {
        self.inner
            .lock()
            .ok()?
            .entries
            .get(provider.as_str())
            .and_then(|credential| match credential {
                StoredCredential::ApiKey(_) => None,
                StoredCredential::OAuth(credential) => Some(credential.clone()),
            })
    }

    fn set(&self, credential: Credential) {
        if let Ok(mut inner) = self.inner.lock() {
            Self::bump_epoch(&mut inner, &credential.provider);
            inner.revoked.remove(credential.provider.as_str());
            inner.entries.insert(
                credential.provider.to_string(),
                StoredCredential::ApiKey(credential),
            );
        }
    }

    fn set_oauth(&self, credential: OAuthCredential) -> Result<(), ProviderError> {
        if let Ok(mut inner) = self.inner.lock() {
            Self::bump_epoch(&mut inner, &credential.provider);
            inner.revoked.remove(credential.provider.as_str());
            inner.entries.insert(
                credential.provider.to_string(),
                StoredCredential::OAuth(credential),
            );
            return Ok(());
        }
        Err(auth_lock_error())
    }

    fn remove(&self, provider: &ProviderId) {
        if let Ok(mut inner) = self.inner.lock() {
            Self::bump_epoch(&mut inner, provider);
            inner.entries.remove(provider.as_str());
            inner.revoked.insert(provider.to_string());
        }
    }

    fn is_revoked(&self, provider: &ProviderId) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|inner| inner.revoked.contains(provider.as_str()))
    }

    fn get_resolved(&self, provider: &ProviderId) -> Option<ResolvedCredential> {
        self.current(provider)
    }

    async fn resolve(
        &self,
        provider: &ProviderId,
        refresher: Option<Arc<dyn CredentialRefresher>>,
    ) -> Result<Option<ResolvedCredential>, ProviderError> {
        let Some((current, _)) = self.current_with_epoch(provider) else {
            return Ok(None);
        };
        if !current.token().trim().is_empty() && !current.is_expired() {
            return Ok(Some(current));
        }

        let lock = self.refresh_lock(provider)?;
        let result = {
            let _guard = lock.lock().await;
            let Some((current, epoch)) = self.current_with_epoch(provider) else {
                return Ok(None);
            };
            if !current.token().trim().is_empty() && !current.is_expired() {
                Some(Ok(Some(current)))
            } else {
                let refreshed =
                    refresh_stored_credential(provider, current, refresher.clone()).await?;
                let ResolvedCredential::OAuth(ref refreshed_credential) = refreshed else {
                    return Err(auth_error("credential refresh returned an API key"));
                };
                if self.publish_refresh_if_current(provider, epoch, refreshed_credential.clone())? {
                    Some(Ok(Some(refreshed)))
                } else {
                    None
                }
            }
        };
        match result {
            Some(result) => result,
            None => self.resolve(provider, refresher).await,
        }
    }
}

async fn refresh_stored_credential(
    provider: &ProviderId,
    current: ResolvedCredential,
    refresher: Option<Arc<dyn CredentialRefresher>>,
) -> Result<ResolvedCredential, ProviderError> {
    let ResolvedCredential::OAuth(current) = current else {
        return Err(auth_error(format!(
            "credential for {provider} has no usable token"
        )));
    };
    let Some(refresher) = refresher else {
        return Err(auth_error(format!(
            "OAuth credential for {provider} is expired and no refresh strategy is configured"
        )));
    };
    let access_token = current.access_token().to_owned();
    let refresh_token = current.refresh_token();
    let refreshed = refresher
        .refresh(provider, refresh_token)
        .await
        .map_err(|error| {
            let mut secrets = vec![access_token.as_str()];
            if let Some(refresh_token) = refresh_token {
                secrets.push(refresh_token);
            }
            redact_refresh_error(error, &secrets)
        })?;
    if refreshed.provider != *provider {
        return Err(auth_error(format!(
            "credential refresh returned the wrong provider for {provider}"
        )));
    }
    if refreshed.access_token().trim().is_empty() {
        return Err(auth_error(format!(
            "credential refresh returned an empty access token for {provider}"
        )));
    }
    if refreshed.is_expired() {
        return Err(auth_error(format!(
            "credential refresh returned an already expired token for {provider}"
        )));
    }
    Ok(ResolvedCredential::OAuth(refreshed))
}

fn redact_refresh_error(error: ProviderError, secrets: &[&str]) -> ProviderError {
    let mut message = error.message;
    for secret in secrets {
        if !secret.is_empty() {
            message = message.replace(secret, "[REDACTED]");
        }
    }
    let mut redacted = ProviderError::new(error.kind, error.phase, message);
    if let Some(status) = error.http_status {
        redacted = redacted.with_status(status);
    }
    if let Some(retry_after) = error.retry_after {
        redacted = redacted.with_retry_after(retry_after);
    }
    redacted
}

fn auth_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Authentication,
        FailurePhase::BeforeDispatch,
        message,
    )
}

fn auth_lock_error() -> ProviderError {
    auth_error("credential store lock is unavailable")
}

pub fn env_var_name(provider: &str) -> String {
    match provider {
        "openai" | "openai-compatible" => "OPENAI_API_KEY".into(),
        "anthropic" => "ANTHROPIC_API_KEY".into(),
        other => format!("{}_API_KEY", other.to_ascii_uppercase().replace('-', "_")),
    }
}

pub fn env_api_key(provider: &str) -> Option<String> {
    std::env::var(env_var_name(provider))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn resolve_api_key(
    provider: &ProviderId,
    store: Option<&dyn CredentialStore>,
) -> Result<String, ProviderError> {
    if store.is_some_and(|store| store.is_revoked(provider)) {
        return Err(missing_credential_error(provider));
    }
    if let Some(credential) = store.and_then(|store| store.get_resolved(provider)) {
        if !credential.token().trim().is_empty() && !credential.is_expired() {
            return Ok(credential.token().to_owned());
        }
    }
    env_api_key(provider.as_str()).ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::Authentication,
            FailurePhase::BeforeDispatch,
            format!(
                "missing API key for {}; set {} or store a credential",
                provider,
                env_var_name(provider.as_str())
            ),
        )
    })
}

pub async fn resolve_credential(
    provider: &ProviderId,
    store: Option<&dyn CredentialStore>,
    refresher: Option<Arc<dyn CredentialRefresher>>,
) -> Result<ResolvedCredential, ProviderError> {
    if let Some(store) = store {
        if store.is_revoked(provider) {
            return Err(missing_credential_error(provider));
        }
        match store.resolve(provider, refresher).await {
            Ok(Some(credential)) => return Ok(credential),
            Ok(None) => {}
            Err(error) => {
                if let Some(credential) = environment_credential(provider) {
                    return Ok(credential);
                }
                return Err(error);
            }
        }
    }
    environment_credential(provider).ok_or_else(|| missing_credential_error(provider))
}

pub async fn resolve_optional_credential(
    provider: &ProviderId,
    store: Option<&dyn CredentialStore>,
    refresher: Option<Arc<dyn CredentialRefresher>>,
) -> Result<Option<ResolvedCredential>, ProviderError> {
    if let Some(store) = store {
        if store.is_revoked(provider) {
            return Ok(None);
        }
        match store.resolve(provider, refresher).await {
            Ok(Some(credential)) => return Ok(Some(credential)),
            Ok(None) => {}
            Err(error) => {
                if let Some(credential) = environment_credential(provider) {
                    return Ok(Some(credential));
                }
                return Err(error);
            }
        }
    }
    Ok(environment_credential(provider))
}

fn environment_credential(provider: &ProviderId) -> Option<ResolvedCredential> {
    env_api_key(provider.as_str())
        .map(|api_key| ResolvedCredential::ApiKey(Credential::api_key(provider.clone(), api_key)))
}

fn missing_credential_error(provider: &ProviderId) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Authentication,
        FailurePhase::BeforeDispatch,
        format!(
            "missing API key for {}; set {} or store a credential",
            provider,
            env_var_name(provider.as_str())
        ),
    )
}

pub fn has_configured_credential(
    provider: &ProviderId,
    store: Option<&dyn CredentialStore>,
) -> bool {
    if store.is_some_and(|store| store.is_revoked(provider)) {
        return false;
    }
    store
        .and_then(|store| store.get_resolved(provider))
        .is_some_and(|credential| {
            !credential.token().trim().is_empty()
                || credential
                    .refresh_token()
                    .is_some_and(|value| !value.trim().is_empty())
        })
        || env_api_key(provider.as_str()).is_some()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    use async_trait::async_trait;
    use tokio::sync::Notify;

    use super::*;

    struct CountingRefresher {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl CredentialRefresher for CountingRefresher {
        async fn refresh(
            &self,
            provider: &ProviderId,
            _refresh_token: Option<&str>,
        ) -> Result<OAuthCredential, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(OAuthCredential::new(
                provider.clone(),
                "fresh-access-token",
                Some("fresh-refresh-token"),
                None,
            ))
        }
    }

    struct FailingRefresher;

    #[async_trait]
    impl CredentialRefresher for FailingRefresher {
        async fn refresh(
            &self,
            _provider: &ProviderId,
            refresh_token: Option<&str>,
        ) -> Result<OAuthCredential, ProviderError> {
            Err(ProviderError::new(
                ProviderErrorKind::Authentication,
                FailurePhase::BeforeDispatch,
                format!("refresh failed for {}", refresh_token.unwrap_or("missing")),
            ))
        }
    }

    struct LegacyOauthStore {
        credential: Mutex<Option<OAuthCredential>>,
    }

    #[async_trait]
    impl CredentialStore for LegacyOauthStore {
        fn get(&self, _provider: &ProviderId) -> Option<Credential> {
            None
        }

        fn set(&self, _credential: Credential) {}

        fn get_oauth(&self, _provider: &ProviderId) -> Option<OAuthCredential> {
            self.credential.lock().ok()?.clone()
        }

        fn remove(&self, _provider: &ProviderId) {
            if let Ok(mut credential) = self.credential.lock() {
                *credential = None;
            }
        }
    }

    struct BlockingRefresher {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    #[async_trait]
    impl CredentialRefresher for BlockingRefresher {
        async fn refresh(
            &self,
            provider: &ProviderId,
            _refresh_token: Option<&str>,
        ) -> Result<OAuthCredential, ProviderError> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(OAuthCredential::new(
                provider.clone(),
                "late-access-token",
                None::<String>,
                None,
            ))
        }
    }

    #[tokio::test]
    async fn expired_oauth_refresh_is_deduplicated_and_published_atomically() {
        let provider = ProviderId::new("anthropic").unwrap();
        let store = MemoryCredentialStore::new();
        store
            .set_oauth(OAuthCredential::new(
                provider.clone(),
                "expired-access-token",
                Some("old-refresh-token"),
                Some(SystemTime::UNIX_EPOCH),
            ))
            .unwrap();
        let refresher_state = Arc::new(CountingRefresher {
            calls: AtomicUsize::new(0),
        });
        let refresher: Arc<dyn CredentialRefresher> = refresher_state.clone();

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let store = store.clone();
            let refresher = Arc::clone(&refresher);
            let provider = provider.clone();
            tasks.push(tokio::spawn(async move {
                store.resolve(&provider, Some(refresher)).await
            }));
        }
        for task in tasks {
            let resolved = task.await.unwrap().unwrap().unwrap();
            assert_eq!(resolved.token(), "fresh-access-token");
            assert_eq!(resolved.kind(), CredentialKind::OAuth);
        }

        assert_eq!(refresher_state.calls.load(Ordering::SeqCst), 1);
        let stored = store.get_oauth(&provider).unwrap();
        assert_eq!(stored.access_token(), "fresh-access-token");
        assert_eq!(stored.refresh_token(), Some("fresh-refresh-token"));
    }

    #[tokio::test]
    async fn oauth_without_refresh_token_can_use_an_application_refresh_strategy() {
        let provider = ProviderId::new("openai").unwrap();
        let store = MemoryCredentialStore::new();
        store
            .set_oauth(OAuthCredential::new(
                provider.clone(),
                "expired-workload-token",
                None::<String>,
                Some(SystemTime::UNIX_EPOCH),
            ))
            .unwrap();

        let resolved = store
            .resolve(
                &provider,
                Some(Arc::new(CountingRefresher {
                    calls: AtomicUsize::new(0),
                })),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(resolved.token(), "fresh-access-token");
    }

    #[tokio::test]
    async fn revoke_during_refresh_cannot_republish_the_revoked_credential() {
        let provider = ProviderId::new("anthropic").unwrap();
        let store = MemoryCredentialStore::new();
        store
            .set_oauth(OAuthCredential::new(
                provider.clone(),
                "expired-access-token",
                Some("refresh-token"),
                Some(SystemTime::UNIX_EPOCH),
            ))
            .unwrap();
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let refresher: Arc<dyn CredentialRefresher> = Arc::new(BlockingRefresher {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        });
        let resolving = {
            let store = store.clone();
            let provider = provider.clone();
            let refresher = Arc::clone(&refresher);
            tokio::spawn(async move { store.resolve(&provider, Some(refresher)).await })
        };

        started.notified().await;
        store.revoke(&provider);
        release.notify_one();

        assert!(resolving.await.unwrap().unwrap().is_none());
        assert!(store.get_oauth(&provider).is_none());
    }

    #[tokio::test]
    async fn failed_oauth_refresh_keeps_old_credential_and_redacts_refresh_token() {
        let provider = ProviderId::new("openai").unwrap();
        let store = MemoryCredentialStore::new();
        store
            .set_oauth(OAuthCredential::new(
                provider.clone(),
                "expired-access-token",
                Some("secret-refresh-token"),
                Some(SystemTime::UNIX_EPOCH),
            ))
            .unwrap();

        let error = store
            .resolve(&provider, Some(Arc::new(FailingRefresher)))
            .await
            .unwrap_err();

        assert!(!error.message.contains("secret-refresh-token"));
        let stored = store.get_oauth(&provider).unwrap();
        assert_eq!(stored.access_token(), "expired-access-token");
        assert_eq!(stored.refresh_token(), Some("secret-refresh-token"));
    }

    #[tokio::test]
    async fn api_key_environment_fallback_survives_oauth_refresh_failure() {
        let provider = ProviderId::new("m7-fallback").unwrap();
        let variable = env_var_name(provider.as_str());
        let previous = std::env::var(&variable).ok();
        std::env::set_var(&variable, "fallback-api-key");

        let store = MemoryCredentialStore::new();
        store
            .set_oauth(OAuthCredential::new(
                provider.clone(),
                "expired-access-token",
                Some("refresh-token"),
                Some(SystemTime::UNIX_EPOCH),
            ))
            .unwrap();
        let resolved =
            resolve_credential(&provider, Some(&store), Some(Arc::new(FailingRefresher)))
                .await
                .unwrap();

        assert_eq!(resolved.kind(), CredentialKind::ApiKey);
        assert_eq!(resolved.token(), "fallback-api-key");
        match previous {
            Some(value) => std::env::set_var(variable, value),
            None => std::env::remove_var(variable),
        }
    }

    #[tokio::test]
    async fn revoke_blocks_environment_fallback_until_reauthentication() {
        let provider = ProviderId::new("m7-revocation-env").unwrap();
        let variable = env_var_name(provider.as_str());
        let previous = std::env::var(&variable).ok();
        std::env::set_var(&variable, "environment-api-key");

        let store = MemoryCredentialStore::new();
        store.set_api_key(provider.clone(), "stored-api-key");
        store.revoke(&provider);

        let error = resolve_credential(&provider, Some(&store), None)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Authentication);
        assert!(!has_configured_credential(&provider, Some(&store)));

        store.set_api_key(provider.clone(), "reauthenticated-api-key");
        let resolved = resolve_credential(&provider, Some(&store), None)
            .await
            .unwrap();
        assert_eq!(resolved.token(), "reauthenticated-api-key");

        match previous {
            Some(value) => std::env::set_var(variable, value),
            None => std::env::remove_var(variable),
        }
    }

    #[tokio::test]
    async fn legacy_store_default_does_not_perform_unsafe_oauth_refresh() {
        let provider = ProviderId::new("legacy-store").unwrap();
        let store = LegacyOauthStore {
            credential: Mutex::new(Some(OAuthCredential::new(
                provider.clone(),
                "expired-access-token",
                Some("refresh-token"),
                Some(SystemTime::UNIX_EPOCH),
            ))),
        };
        let refresher = Arc::new(CountingRefresher {
            calls: AtomicUsize::new(0),
        });

        let error = store
            .resolve(&provider, Some(refresher.clone()))
            .await
            .unwrap_err();

        assert_eq!(error.kind, ProviderErrorKind::Unsupported);
        assert_eq!(refresher.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            store.get_oauth(&provider).unwrap().access_token(),
            "expired-access-token"
        );
    }

    #[test]
    fn credential_revoke_and_reauth_are_atomic_store_operations() {
        let provider = ProviderId::new("openai").unwrap();
        let store = MemoryCredentialStore::new();
        store.set(Credential {
            provider: provider.clone(),
            api_key: "api-key-one".into(),
        });
        assert_eq!(store.get(&provider).unwrap().api_key, "api-key-one");

        store.revoke(&provider);
        assert!(store.get(&provider).is_none());

        store
            .set_oauth(OAuthCredential::new(
                provider.clone(),
                "oauth-access-token",
                Some("oauth-refresh-token"),
                None,
            ))
            .unwrap();
        assert_eq!(
            store.get_oauth(&provider).unwrap().access_token(),
            "oauth-access-token"
        );
        store.clear(&provider);
        assert!(store.get_oauth(&provider).is_none());
    }

    #[test]
    fn credential_debug_redacts_all_token_material() {
        let provider = ProviderId::new("openai").unwrap();
        let credential =
            OAuthCredential::new(provider, "access-secret", Some("refresh-secret"), None);
        let debug = format!("{credential:?}");
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("refresh-secret"));
    }
}
