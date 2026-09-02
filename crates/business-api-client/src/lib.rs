//! Typed HTTP client for the versioned Business API.
//!
//! This crate contains transport concerns only: authentication, request IDs,
//! stable error parsing, pagination and safe retries.

use std::time::Duration;

use business_api_contracts as contracts;
use bytes::Bytes;
use reqwest::{Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub bearer_token: String,
    pub timeout: Duration,
}

impl ClientConfig {
    pub fn new(
        base_url: impl Into<String>,
        bearer_token: impl Into<String>,
    ) -> Result<Self, ClientError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let bearer_token = bearer_token.into();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(ClientError::InvalidConfiguration(
                "api-url must be an http(s) URL".to_string(),
            ));
        }
        if bearer_token.trim().is_empty() {
            return Err(ClientError::InvalidConfiguration(
                "token must not be empty".to_string(),
            ));
        }
        Ok(Self {
            base_url,
            bearer_token,
            timeout: Duration::from_secs(30),
        })
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid client configuration: {0}")]
    InvalidConfiguration(String),
    #[error("request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("API returned {status}: {code}: {message}")]
    Api {
        status: StatusCode,
        code: String,
        message: String,
        trace_id: Option<String>,
    },
    #[error("API response was malformed")]
    MalformedResponse,
}

#[derive(Clone)]
pub struct BusinessApiClient {
    http: reqwest::Client,
    base_url: String,
    bearer_token: String,
}

impl BusinessApiClient {
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        // The base URL is the (typically internal) Business API, so requests
        // must never inherit machine proxy settings (Windows system proxy,
        // `*_PROXY` environment variables): that would hand the bearer token
        // to a third-party proxy and break internal reachability.
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .no_proxy()
            .build()?;
        Ok(Self {
            http,
            base_url: config.base_url,
            bearer_token: config.bearer_token,
        })
    }

    pub async fn status(&self) -> Result<serde_json::Value, ClientError> {
        let response = self
            .send_safe(self.authorized(self.http.get(self.url("/health/ready"))))
            .await?;
        self.decode_raw(response).await
    }

    pub async fn documents_list(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<contracts::Page<contracts::Document>, ClientError> {
        let mut path = format!("/api/v1/documents?limit={limit}");
        if let Some(cursor) = cursor {
            path.push_str("&cursor=");
            path.push_str(&urlencoding::encode(cursor));
        }
        self.request_json(Method::GET, &path, None).await
    }

    pub async fn document_get(&self, id: Uuid) -> Result<contracts::Document, ClientError> {
        self.request_json(Method::GET, &format!("/api/v1/documents/{id}"), None)
            .await
    }

    pub async fn upload(&self, request: UploadRequest) -> Result<contracts::Document, ClientError> {
        let part = reqwest::multipart::Part::bytes(request.body.to_vec())
            .file_name(request.file_name)
            .mime_str(&request.content_type)
            .map_err(|_| ClientError::InvalidConfiguration("invalid content type".to_string()))?;
        let response = self
            .authorized(self.http.post(self.url("/api/v1/documents/upload")))
            .header("Idempotency-Key", request.idempotency_key)
            .multipart(reqwest::multipart::Form::new().part("file", part))
            .send()
            .await?;
        self.decode(response).await
    }

    pub async fn processing_list(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<contracts::Page<contracts::ProcessingJob>, ClientError> {
        let mut path = format!("/api/v1/processing-jobs?limit={limit}");
        if let Some(cursor) = cursor {
            path.push_str("&cursor=");
            path.push_str(&urlencoding::encode(cursor));
        }
        self.request_json(Method::GET, &path, None).await
    }

    pub async fn processing_for_document(
        &self,
        id: Uuid,
    ) -> Result<Vec<contracts::ProcessingJob>, ClientError> {
        self.request_json(
            Method::GET,
            &format!("/api/v1/documents/{id}/processing-jobs"),
            None,
        )
        .await
    }

    pub async fn processing_get(&self, id: Uuid) -> Result<contracts::ProcessingJob, ClientError> {
        self.request_json(Method::GET, &format!("/api/v1/processing-jobs/{id}"), None)
            .await
    }

    pub async fn processing_start(
        &self,
        document_id: Uuid,
        content_revision: i64,
        idempotency_key: &str,
    ) -> Result<contracts::ProcessingJob, ClientError> {
        let body = serde_json::json!({ "content_revision": content_revision });
        self.request_json(
            Method::POST,
            &format!("/api/v1/documents/{document_id}/processing-jobs"),
            Some((&body, idempotency_key)),
        )
        .await
    }

    pub async fn candidate_get(&self, job_id: Uuid) -> Result<contracts::Candidate, ClientError> {
        self.request_json(
            Method::GET,
            &format!("/api/v1/processing-jobs/{job_id}/candidate"),
            None,
        )
        .await
    }

    pub async fn audit_list(
        &self,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<contracts::Page<contracts::AuditEvent>, ClientError> {
        let mut path = format!("/api/v1/admin/audit-events?limit={limit}");
        if let Some(cursor) = cursor {
            path.push_str("&cursor=");
            path.push_str(&urlencoding::encode(cursor));
        }
        self.request_json(Method::GET, &path, None).await
    }

    pub async fn findings_list(
        &self,
        limit: u16,
    ) -> Result<contracts::Page<contracts::IntegrityFinding>, ClientError> {
        self.request_json(
            Method::GET,
            &format!("/api/v1/admin/integrity/findings?limit={limit}"),
            None,
        )
        .await
    }

    pub async fn finding_get(&self, id: Uuid) -> Result<contracts::IntegrityFinding, ClientError> {
        self.request_json(
            Method::GET,
            &format!("/api/v1/admin/integrity/findings/{id}"),
            None,
        )
        .await
    }

    pub async fn audit_get(&self, id: Uuid) -> Result<contracts::AuditEvent, ClientError> {
        self.request_json(
            Method::GET,
            &format!("/api/v1/admin/audit-events/{id}"),
            None,
        )
        .await
    }
    pub async fn operations_overview(&self) -> Result<contracts::OperationsOverview, ClientError> {
        self.request_json(Method::GET, "/api/v1/operations/overview", None)
            .await
    }

    async fn request_json<T>(
        &self,
        method: Method,
        path: &str,
        body: Option<(&serde_json::Value, &str)>,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        let mut request = self.authorized(self.http.request(method.clone(), self.url(path)));
        if let Some((body, idempotency_key)) = body {
            request = request
                .json(&body)
                .header("Idempotency-Key", idempotency_key);
        }
        let response = if method == Method::GET {
            self.send_safe(request).await?
        } else {
            request.send().await?
        };
        self.decode(response).await
    }

    async fn send_safe(&self, request: RequestBuilder) -> Result<reqwest::Response, ClientError> {
        let mut request = request;
        for attempt in 0..=2 {
            let cloned = request.try_clone().ok_or(ClientError::MalformedResponse)?;
            let response = cloned.send().await?;
            if attempt == 2
                || !matches!(
                    response.status(),
                    StatusCode::BAD_GATEWAY
                        | StatusCode::SERVICE_UNAVAILABLE
                        | StatusCode::GATEWAY_TIMEOUT
                )
            {
                return Ok(response);
            }
            request = self.authorized(self.http.get(response.url().clone()));
            tokio::time::sleep(Duration::from_millis(50 * (attempt + 1))).await;
        }
        Err(ClientError::MalformedResponse)
    }

    async fn decode<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ClientError> {
        let status = response.status();
        if !status.is_success() {
            let error = response.json::<contracts::ErrorResponse>().await.unwrap_or(
                contracts::ErrorResponse {
                    code: "upstream_error".to_string(),
                    message: "Business API request failed".to_string(),
                    request_id: "unknown".to_string(),
                    trace_id: None,
                    details: None,
                },
            );
            return Err(ClientError::Api {
                status,
                code: error.code,
                message: error.message,
                trace_id: error.trace_id,
            });
        }
        let envelope = response
            .json::<contracts::ApiResponse<T>>()
            .await
            .map_err(|_| ClientError::MalformedResponse)?;
        envelope.data.ok_or(ClientError::MalformedResponse)
    }

    async fn decode_raw<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ClientError> {
        let status = response.status();
        if !status.is_success() {
            return Err(self.api_error(status, response).await);
        }
        response
            .json::<T>()
            .await
            .map_err(|_| ClientError::MalformedResponse)
    }

    async fn api_error(&self, status: StatusCode, response: reqwest::Response) -> ClientError {
        let error =
            response
                .json::<contracts::ErrorResponse>()
                .await
                .unwrap_or(contracts::ErrorResponse {
                    code: "upstream_error".to_string(),
                    message: "Business API request failed".to_string(),
                    request_id: "unknown".to_string(),
                    trace_id: None,
                    details: None,
                });
        ClientError::Api {
            status,
            code: error.code,
            message: error.message,
            trace_id: error.trace_id,
        }
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        request
            .bearer_auth(&self.bearer_token)
            .header("X-Request-ID", Uuid::now_v7().to_string())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub file_name: String,
    pub content_type: String,
    pub body: Bytes,
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Plain-HTTP responder on an owned socket, so tests never depend on
    /// host port state or third-party servers.
    fn spawn_http_responder(status_line: &'static str, body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|_| unreachable!());
        let addr = listener.local_addr().unwrap_or_else(|_| unreachable!());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut scratch = [0_u8; 4096];
                let _unused = stream.read(&mut scratch);
                let head = format!(
                    "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _unused = stream
                    .write_all(head.as_bytes())
                    .and_then(|()| stream.write_all(body));
                let _unused = stream.flush();
            }
        });
        addr.to_string()
    }

    /// Serializes every test that mutates the process-global environment in
    /// this binary. The proxy variables are consumed at client-construction
    /// time, so an unrelated env-sensitive test must not observe or race with
    /// the mutation window.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Saves the original proxy-related environment variables and restores
    /// them exactly on drop, so a failing assertion cannot leak the test's
    /// `*_PROXY` values into the process environment for later tests.
    struct ProxyEnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    const PROXY_ENV_KEYS: [&str; 4] = ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"];

    impl ProxyEnvGuard {
        fn point_proxies_at(proxy: &str) -> Self {
            let saved = PROXY_ENV_KEYS
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect();
            for key in PROXY_ENV_KEYS {
                std::env::set_var(key, format!("http://{proxy}"));
            }
            Self { saved }
        }
    }

    impl Drop for ProxyEnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    // The lock is deliberately held across the awaited request so the request
    // executes strictly inside this test's env mutation window; a concurrent
    // env-sensitive test can neither observe nor flip the variables mid-test.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn requests_never_route_through_environment_proxies() {
        // A proxy-aware client would reach the 503 responder configured via
        // `*_PROXY` instead of the 200 responder at the API base URL.
        // Lock first so the mutation/restore window is exclusive, then restore
        // via the guard on every exit path (including assertion panics).
        let _env_guard = env_lock();
        let api = spawn_http_responder("HTTP/1.1 200 OK", br#"{"status":"ready"}"#);
        let proxy = spawn_http_responder("HTTP/1.1 503 Service Unavailable", b"");
        let _proxies = ProxyEnvGuard::point_proxies_at(&proxy);
        let config = ClientConfig::new(format!("http://{api}"), "test-token")
            .unwrap_or_else(|_| unreachable!());
        let client = BusinessApiClient::new(config).unwrap_or_else(|_| unreachable!());
        let ready = client.status().await;
        assert!(ready.is_ok(), "expected direct API response, got {ready:?}");
    }

    #[tokio::test]
    async fn proxy_environment_is_restored_after_the_client_construction_window() {
        // Regression for the env pollution itself: after the guarded test's
        // pattern completes, the original (absent) proxy environment must be
        // observable again, proving save/restore rather than leak.
        let _env_guard = env_lock();
        let had_before: Vec<bool> = PROXY_ENV_KEYS
            .iter()
            .map(|k| std::env::var_os(k).is_some())
            .collect();
        {
            let _proxies = ProxyEnvGuard::point_proxies_at("127.0.0.1:1");
            for key in PROXY_ENV_KEYS {
                assert!(
                    std::env::var_os(key).is_some(),
                    "{key} must be set inside the guard"
                );
            }
        }
        let had_after: Vec<bool> = PROXY_ENV_KEYS
            .iter()
            .map(|k| std::env::var_os(k).is_some())
            .collect();
        assert_eq!(
            had_before, had_after,
            "proxy environment must be exactly restored"
        );
    }
}
