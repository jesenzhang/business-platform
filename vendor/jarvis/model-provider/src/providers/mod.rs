mod anthropic;
mod images;
mod mock;
mod openai;
mod responses;

use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{AbortSignal, FailurePhase, ImageContent, ProviderError, ProviderErrorKind};

pub use anthropic::AnthropicProvider;
pub use images::OpenAiImageProvider;
pub use mock::{MockProvider, ScriptedProvider};
pub use openai::OpenAiCompatibleProvider;
pub use responses::OpenAiResponsesProvider;

pub(crate) const MAX_STREAM_BUFFER_BYTES: usize = 8 * 1024 * 1024;

pub(crate) enum DispatchResult {
    Aborted(FailurePhase),
    Sent(Result<reqwest::Response, reqwest::Error>),
}

pub(crate) async fn dispatch(
    builder: reqwest::RequestBuilder,
    abort: Option<&AbortSignal>,
) -> DispatchResult {
    let Some(abort) = abort else {
        return DispatchResult::Sent(builder.send().await);
    };
    if abort.is_aborted() {
        return DispatchResult::Aborted(FailurePhase::BeforeDispatch);
    }
    tokio::select! {
        _ = abort.cancelled() => DispatchResult::Aborted(FailurePhase::Unknown),
        result = builder.send() => DispatchResult::Sent(result),
    }
}

pub(crate) fn aborted(phase: FailurePhase) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Aborted,
        phase,
        "provider request aborted",
    )
}

pub(crate) fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get("retry-after")?.to_str().ok()?;
    value
        .parse::<u64>()
        .ok()
        .map(|seconds| Duration::from_secs(seconds.min(86_400)))
}

pub(crate) fn apply_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &[(String, String)],
    protected_header: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut extra = reqwest::header::HeaderMap::new();
    let mut invalid = None;
    for (name, value) in headers {
        if protected_header.is_some_and(|protected| name.eq_ignore_ascii_case(protected)) {
            continue;
        }
        let original_name = name;
        let original_value = value;
        let name = match reqwest::header::HeaderName::try_from(original_name.as_str()) {
            Ok(name) => name,
            Err(_) => {
                invalid = Some((original_name, original_value));
                break;
            }
        };
        let value = match reqwest::header::HeaderValue::try_from(original_value.as_str()) {
            Ok(value) => value,
            Err(_) => {
                invalid = Some((original_name, original_value));
                break;
            }
        };
        extra.insert(name, value);
    }
    builder = builder.headers(extra);
    if let Some((name, value)) = invalid {
        builder = builder.header(name, value);
    }
    builder
}

pub(crate) fn normalize_image(image: &ImageContent) -> Result<(&str, &str), ProviderError> {
    let media_type = image.media_type.trim();
    let media_type = match media_type {
        "image/jpg" => "image/jpeg",
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" => media_type,
        _ => {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                FailurePhase::BeforeDispatch,
                format!("unsupported image media type {media_type}"),
            ))
        }
    };
    let data = image.data.trim();
    if data.is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::InvalidRequest,
            FailurePhase::BeforeDispatch,
            "image data must not be empty",
        ));
    }
    Ok((media_type, data))
}
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;

pub(crate) async fn bounded_error_body(response: reqwest::Response) -> Result<Vec<u8>, ()> {
    bounded_response_body_with_limit(response, MAX_ERROR_BODY_BYTES).await
}

pub(crate) async fn bounded_response_body(response: reqwest::Response) -> Result<Vec<u8>, ()> {
    bounded_response_body_with_limit(response, MAX_RESPONSE_BODY_BYTES).await
}

async fn bounded_response_body_with_limit(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ()> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointPolicy {
    #[default]
    SecureOrLoopback,
    TrustedPrivateHttp,
}

impl EndpointPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::SecureOrLoopback => "secure-or-loopback",
            Self::TrustedPrivateHttp => "trusted-private-http",
        }
    }

    pub fn plaintext_warning(self) -> Option<&'static str> {
        match self {
            Self::SecureOrLoopback => None,
            Self::TrustedPrivateHttp => Some(
                "warning: plaintext private-network transport; API credentials, prompts, source code, tool data and model responses are not encrypted in transit",
            ),
        }
    }
}

/// Build the HTTP client for one endpoint policy.
///
/// Endpoint policy is a direct-destination boundary. Ambient proxy settings
/// must never silently change where a provider request goes. Trusted private
/// HTTP additionally rejects redirects so a credential-bearing request cannot
/// be redirected outside the explicitly trusted destination.
pub(crate) fn client_for_policy(policy: EndpointPolicy) -> Result<Client, ProviderError> {
    let builder = Client::builder().no_proxy();
    let builder = match policy {
        EndpointPolicy::SecureOrLoopback => builder,
        EndpointPolicy::TrustedPrivateHttp => builder.redirect(reqwest::redirect::Policy::none()),
    };
    builder
        .build()
        .map_err(|_| invalid("failed to build provider HTTP client"))
}

/// Build a policy-aware client that rejects redirects for all policies.
///
/// Catalog refresh uses this variant whenever it sends credentials. The
/// stricter redirect boundary prevents a provider-controlled redirect from
/// receiving the credential-bearing catalog request.
pub(crate) fn client_for_policy_without_redirects(
    _policy: EndpointPolicy,
) -> Result<Client, ProviderError> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| invalid("failed to build provider HTTP client"))
}

pub fn normalize_base_url(
    input: &str,
    policy: EndpointPolicy,
) -> Result<reqwest::Url, ProviderError> {
    let mut url = reqwest::Url::parse(input).map_err(|_| invalid("invalid provider base URL"))?;
    if url.cannot_be_a_base() || url.host_str().is_none() {
        return Err(invalid(
            "provider base URL must be hierarchical and include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("provider base URL must not contain credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid(
            "provider base URL must not contain a query or fragment",
        ));
    }
    let host = url.host_str().unwrap_or_default();
    let address_class = classify_host(host);
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match (url.scheme(), policy, loopback, is_private_ip(host)) {
        ("https", EndpointPolicy::SecureOrLoopback, _, _) => {}
        ("https", EndpointPolicy::TrustedPrivateHttp, _, _) => {
            return Err(invalid(
                "--allow-private-http is not valid for HTTPS; HTTPS already uses the secure-or-loopback policy",
            ));
        }
        ("http", EndpointPolicy::SecureOrLoopback, true, _) => {}
        ("http", EndpointPolicy::TrustedPrivateHttp, true, _) => {
            return Err(invalid(
                "--allow-private-http is not valid for loopback; loopback HTTP already uses the secure-or-loopback policy",
            ));
        }
        ("http", EndpointPolicy::TrustedPrivateHttp, _, true) => {}
        ("http", EndpointPolicy::SecureOrLoopback, _, _) => {
            return Err(invalid(format!(
                "plain HTTP provider base URL detected {}; allowed modes are HTTPS or loopback HTTP",
                address_class.label()
            )));
        }
        ("http", EndpointPolicy::TrustedPrivateHttp, _, _) => {
            return Err(invalid(format!(
                "trusted private HTTP provider detected {}; only IP literals in 10/8, 172.16/12, 192.168/16, or fc00::/7 are allowed",
                address_class.label()
            )));
        }
        (scheme, _, _, _) => {
            return Err(invalid(format!(
                "provider base URL scheme {scheme} is unsupported"
            )));
        }
    }
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!("{path}/"));
    Ok(url)
}

fn is_private_ip(host: &str) -> bool {
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let [a, b, _, _] = ip.octets();
            a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
        }
        Ok(std::net::IpAddr::V6(ip)) => ip.segments()[0] & 0xfe00 == 0xfc00,
        Err(_) => false,
    }
}

#[derive(Clone, Copy)]
enum AddressClass {
    Hostname,
    Loopback,
    Private,
    Cgnat,
    LinkLocal,
    PublicOrReserved,
}

impl AddressClass {
    fn label(self) -> &'static str {
        match self {
            Self::Hostname => "an HTTP hostname",
            Self::Loopback => "a loopback address",
            Self::Private => "a private-network IP literal",
            Self::Cgnat => "a CGNAT address",
            Self::LinkLocal => "a link-local address",
            Self::PublicOrReserved => "a public, broadcast, or reserved address",
        }
    }
}

fn classify_host(host: &str) -> AddressClass {
    if host.eq_ignore_ascii_case("localhost") {
        return AddressClass::Loopback;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let [first, second, _, _] = ip.octets();
            if ip.is_loopback() {
                AddressClass::Loopback
            } else if first == 10
                || (first == 172 && (16..=31).contains(&second))
                || (first == 192 && second == 168)
            {
                AddressClass::Private
            } else if first == 100 && (64..=127).contains(&second) {
                AddressClass::Cgnat
            } else if first == 169 && second == 254 {
                AddressClass::LinkLocal
            } else {
                AddressClass::PublicOrReserved
            }
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            if ip.is_loopback() {
                AddressClass::Loopback
            } else if first & 0xfe00 == 0xfc00 {
                AddressClass::Private
            } else if first & 0xffc0 == 0xfe80 {
                AddressClass::LinkLocal
            } else {
                AddressClass::PublicOrReserved
            }
        }
        Err(_) => AddressClass::Hostname,
    }
}

pub(crate) struct SseRecord {
    pub event: Option<String>,
    pub data: String,
}

pub(crate) struct SseReader {
    body: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: Vec<u8>,
    eof: bool,
}

impl SseReader {
    pub(crate) fn new<S>(body: S) -> Self
    where
        S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
    {
        Self {
            body: Box::pin(body),
            buffer: Vec::new(),
            eof: false,
        }
    }

    pub(crate) async fn next_record(&mut self) -> Result<Option<SseRecord>, ProviderError> {
        loop {
            if let Some((end, delimiter_len)) = record_end(&self.buffer) {
                if end > 8 * 1024 * 1024 {
                    return Err(stream_error("SSE record exceeds the stream limit"));
                }
                let bytes: Vec<u8> = self.buffer.drain(..end + delimiter_len).collect();
                let record = parse_record(&bytes)?;
                if record.data.is_empty() {
                    continue;
                }
                return Ok(Some(record));
            }
            if self.eof {
                if self.buffer.is_empty() {
                    return Ok(None);
                }
                return Err(stream_error("truncated SSE record"));
            }
            match futures::StreamExt::next(&mut self.body).await {
                Some(Ok(bytes)) => {
                    self.buffer.extend_from_slice(&bytes);
                    if self.buffer.len() > 8 * 1024 * 1024 && record_end(&self.buffer).is_none() {
                        return Err(stream_error("SSE record exceeds the stream limit"));
                    }
                }
                Some(Err(_)) => return Err(stream_error("provider response stream failed")),
                None => self.eof = true,
            }
        }
    }
}

fn record_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4));
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2));
    match (crlf, lf) {
        (Some(left), Some(right)) if left.0 <= right.0 => Some(left),
        (Some(_left), Some(right)) => Some(right),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_record(bytes: &[u8]) -> Result<SseRecord, ProviderError> {
    let text = std::str::from_utf8(bytes).map_err(|_| stream_error("SSE record is not UTF-8"))?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value).to_owned());
        }
    }
    Ok(SseRecord {
        event,
        data: data.join("\n"),
    })
}

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        FailurePhase::BeforeDispatch,
        message,
    )
}

pub(crate) fn protocol(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Protocol,
        FailurePhase::DuringStream,
        message,
    )
}

pub(crate) fn stream_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::StreamInterrupted,
        FailurePhase::DuringStream,
        message,
    )
}
