use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Api, ModelSpec, ProviderError, ProviderErrorKind, ProviderId, RequestOptions};

/// Image-generation request for the standalone Images API.
///
/// This seam intentionally models generation only. Image edits, variations,
/// and the Responses API image-generation tool remain separate capabilities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
    pub model: ModelSpec,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageQuality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageBackground>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageOutputFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ImageResponseFormat>,
}

impl ImageGenerationRequest {
    pub fn new(model: ModelSpec, prompt: impl Into<String>) -> Self {
        Self {
            model,
            prompt: prompt.into(),
            n: None,
            size: None,
            quality: None,
            background: None,
            output_format: None,
            response_format: None,
        }
    }

    pub fn with_n(mut self, n: u8) -> Self {
        self.n = Some(n);
        self
    }

    pub fn with_size(mut self, size: ImageSize) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_quality(mut self, quality: ImageQuality) -> Self {
        self.quality = Some(quality);
        self
    }

    pub fn with_background(mut self, background: ImageBackground) -> Self {
        self.background = Some(background);
        self
    }

    pub fn with_output_format(mut self, output_format: ImageOutputFormat) -> Self {
        self.output_format = Some(output_format);
        self
    }

    pub fn with_response_format(mut self, response_format: ImageResponseFormat) -> Self {
        self.response_format = Some(response_format);
        self
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.model.id.trim().is_empty() {
            return Err(image_before_dispatch("image model id must not be empty"));
        }
        if self.model.api != Api::OpenAiImages {
            return Err(image_before_dispatch(format!(
                "image request uses API {:?}, but standalone image generation requires OpenAiImages",
                self.model.api
            )));
        }
        if self.model.provider.as_str().trim().is_empty() {
            return Err(image_before_dispatch("image provider id must not be empty"));
        }
        if self.prompt.trim().is_empty() {
            return Err(image_before_dispatch("image prompt must not be empty"));
        }
        if self.n == Some(0) {
            return Err(image_before_dispatch(
                "image count must be greater than zero",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImageSize {
    #[serde(rename = "1024x1024")]
    Square,
    #[serde(rename = "1024x1536")]
    Portrait,
    #[serde(rename = "1536x1024")]
    Landscape,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageQuality {
    Low,
    Medium,
    High,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageBackground {
    Transparent,
    Opaque,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageOutputFormat {
    Png,
    Webp,
    Jpeg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImageResponseFormat {
    #[serde(rename = "b64_json")]
    B64Json,
    #[serde(rename = "url")]
    Url,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
    pub created: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageBackground>,
    #[serde(default)]
    pub data: Vec<GeneratedImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ImageUsage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratedImage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b64_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<ImageInputTokenDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<ImageOutputTokenDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInputTokenDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOutputTokenDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_tokens: Option<u64>,
}

/// Public seam for an independent image-generation endpoint.
#[async_trait]
pub trait ImageGenerationProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn api(&self) -> &Api;

    fn validate_request(&self, request: &ImageGenerationRequest) -> Result<(), ProviderError> {
        request.validate()?;
        if request.model.provider != *self.provider_id() {
            return Err(image_before_dispatch(format!(
                "image model/provider binding mismatch: model belongs to {}, provider is {}",
                request.model.provider,
                self.provider_id()
            )));
        }
        if request.model.api != *self.api() {
            return Err(image_before_dispatch(format!(
                "image model/API binding mismatch: model uses {:?}, provider uses {:?}",
                request.model.api,
                self.api()
            )));
        }
        Ok(())
    }

    async fn generate(
        &self,
        request: ImageGenerationRequest,
    ) -> Result<ImageGenerationResponse, ProviderError> {
        self.generate_with(request, RequestOptions::default()).await
    }

    async fn generate_with(
        &self,
        request: ImageGenerationRequest,
        options: RequestOptions,
    ) -> Result<ImageGenerationResponse, ProviderError>;
}

fn image_before_dispatch(message: impl Into<String>) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::InvalidRequest,
        crate::FailurePhase::BeforeDispatch,
        message,
    )
}
