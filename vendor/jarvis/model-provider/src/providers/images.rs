use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};

use super::{
    aborted, apply_headers, bounded_error_body, bounded_response_body, client_for_policy, dispatch,
    normalize_base_url, retry_after_from_headers, DispatchResult, EndpointPolicy,
};
use crate::{
    Api, ImageBackground, ImageGenerationProvider, ImageGenerationRequest, ImageGenerationResponse,
    ImageOutputFormat, ImageQuality, ImageResponseFormat, ImageSize, ProviderError,
    ProviderErrorKind, ProviderId, RequestOptions,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Standalone OpenAI Images API generation transport.
pub struct OpenAiImageProvider {
    provider_id: ProviderId,
    api_key: String,
    base_url: reqwest::Url,
    client: Client,
    request_timeout: Duration,
}

impl OpenAiImageProvider {
    pub fn new(provider_id: ProviderId, api_key: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            provider_id,
            api_key: api_key.into(),
            base_url: normalize_base_url(DEFAULT_BASE_URL, EndpointPolicy::SecureOrLoopback)?,
            client: client_for_policy(EndpointPolicy::SecureOrLoopback)?,
            request_timeout: DEFAULT_TIMEOUT,
        })
    }

    pub fn with_base_url(self, base_url: impl AsRef<str>) -> Result<Self, ProviderError> {
        self.with_base_url_and_policy(base_url, EndpointPolicy::SecureOrLoopback)
    }

    pub fn with_base_url_and_policy(
        mut self,
        base_url: impl AsRef<str>,
        policy: EndpointPolicy,
    ) -> Result<Self, ProviderError> {
        self.base_url = normalize_base_url(base_url.as_ref(), policy)?;
        self.client = client_for_policy(policy)?;
        Ok(self)
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    fn endpoint(&self) -> Result<reqwest::Url, ProviderError> {
        self.base_url
            .join("images/generations")
            .map_err(|_| image_invalid("invalid image generation endpoint"))
    }

    fn request(
        &self,
        body: &ImageGenerationBody<'_>,
    ) -> Result<reqwest::RequestBuilder, ProviderError> {
        let body = serde_json::to_vec(body)
            .map_err(|_| image_serialization("image request serialization failed"))?;
        let mut request = self
            .client
            .post(self.endpoint()?)
            .header("content-type", "application/json")
            .timeout(self.request_timeout)
            .body(body);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        Ok(request)
    }

    async fn dispatch_request(
        &self,
        body: &ImageGenerationBody<'_>,
        options: &RequestOptions,
    ) -> Result<Response, ProviderError> {
        let builder = apply_headers(
            self.request(body)?,
            &options.headers,
            (!self.api_key.is_empty()).then_some("authorization"),
        );
        match dispatch(builder, options.abort.as_ref()).await {
            DispatchResult::Aborted(phase) => Err(aborted(phase)),
            DispatchResult::Sent(result) => result.map_err(|error| self.transport_error(error)),
        }
    }

    async fn status_error(&self, response: Response) -> ProviderError {
        let retry_after = retry_after_from_headers(response.headers());
        let status = response.status();
        let body = match bounded_error_body(response).await {
            Ok(body) => body,
            Err(()) => {
                return ProviderError::new(
                    ProviderErrorKind::StreamInterrupted,
                    crate::FailurePhase::DuringStream,
                    "OpenAI image error body interrupted or exceeded the limit",
                )
                .with_status(status.as_u16())
            }
        };
        let message = serde_json::from_slice::<ApiErrorResponse>(&body)
            .ok()
            .map(|error| error.error.message)
            .unwrap_or_else(|| format!("HTTP {status} provider error"));
        let message = ProviderError::redacted_message(message, &self.api_key);
        let kind = match status.as_u16() {
            401 | 403 => ProviderErrorKind::Authentication,
            408 => ProviderErrorKind::Timeout,
            429 => ProviderErrorKind::RateLimit,
            400..=499 => ProviderErrorKind::InvalidRequest,
            500..=599 => ProviderErrorKind::Unavailable,
            _ => ProviderErrorKind::Other,
        };
        let mut error = ProviderError::new(kind, crate::FailurePhase::AfterDispatch, message)
            .with_status(status.as_u16());
        if let Some(retry_after) = retry_after {
            error = error.with_retry_after(retry_after);
        }
        error
    }

    fn transport_error(&self, error: reqwest::Error) -> ProviderError {
        let message = ProviderError::redacted_message(error.to_string(), &self.api_key);
        let (kind, phase) = if error.is_timeout() {
            (ProviderErrorKind::Timeout, crate::FailurePhase::Unknown)
        } else if error.is_connect() {
            (
                ProviderErrorKind::Unavailable,
                crate::FailurePhase::BeforeDispatch,
            )
        } else {
            (ProviderErrorKind::Other, crate::FailurePhase::Unknown)
        };
        ProviderError::new(kind, phase, message)
    }
}

#[async_trait]
impl ImageGenerationProvider for OpenAiImageProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn api(&self) -> &Api {
        static API: Api = Api::OpenAiImages;
        &API
    }

    async fn generate_with(
        &self,
        request: ImageGenerationRequest,
        options: RequestOptions,
    ) -> Result<ImageGenerationResponse, ProviderError> {
        self.validate_request(&request)?;
        let body = ImageGenerationBody::from_request(&request);
        let response = self.dispatch_request(&body, &options).await?;
        if !response.status().is_success() {
            return Err(self.status_error(response).await);
        }
        let bytes = bounded_response_body(response).await.map_err(|()| {
            ProviderError::new(
                ProviderErrorKind::StreamInterrupted,
                crate::FailurePhase::DuringStream,
                "OpenAI image response body interrupted or exceeded the limit",
            )
        })?;
        let response: ImageGenerationResponse = serde_json::from_slice(&bytes)
            .map_err(|_| image_protocol("invalid OpenAI image generation response"))?;
        if response.data.is_empty() {
            return Err(image_protocol(
                "OpenAI image generation response contained no images",
            ));
        }
        if response
            .data
            .iter()
            .any(|image| image.b64_json.is_none() && image.url.is_none())
        {
            return Err(image_protocol(
                "OpenAI image generation response contained an empty image",
            ));
        }
        Ok(response)
    }
}

#[derive(Serialize)]
struct ImageGenerationBody<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<ImageSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<ImageQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<ImageBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_format: Option<ImageOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ImageResponseFormat>,
}

impl<'a> ImageGenerationBody<'a> {
    fn from_request(request: &'a ImageGenerationRequest) -> Self {
        Self {
            model: &request.model.id,
            prompt: request.prompt.as_str(),
            n: request.n,
            size: request.size,
            quality: request.quality,
            background: request.background,
            output_format: request.output_format,
            response_format: request.response_format,
        }
    }
}

#[derive(Deserialize)]
struct ApiErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

fn image_invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        crate::FailurePhase::BeforeDispatch,
        message,
    )
}

fn image_serialization(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Serialization,
        crate::FailurePhase::BeforeDispatch,
        message,
    )
}

fn image_protocol(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Protocol,
        crate::FailurePhase::AfterDispatch,
        message,
    )
}
