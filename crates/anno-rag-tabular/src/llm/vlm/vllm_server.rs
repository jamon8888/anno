//! vLLM on-prem backend for vision-OCR.
//!
//! Talks to any OpenAI-compatible `/v1/chat/completions` endpoint — typically
//! a co-located vLLM process serving `lightonai/LightOnOCR-2-1B`.  No third-
//! party egress: the `base_url` must resolve inside the customer's trust boundary
//! (Spec B §4.3).

#[cfg(feature = "vlm-ocr")]
use async_trait::async_trait;
#[cfg(feature = "vlm-ocr")]
use liter_llm::{
    ClientConfigBuilder, DefaultClient, LlmClient,
    image::encode_data_url,
    types::{
        ChatCompletionRequest, ContentPart, ImageUrl, Message, UserContent, UserMessage,
    },
};

#[cfg(feature = "vlm-ocr")]
use super::{PageImage, Transcription, VLM_OCR_PROMPT_FR, VlmOcrClient, vlm_quality_score};

/// VLM OCR client backed by a co-located vLLM server (OpenAI-compatible).
///
/// Sends a two-part user message: the French OCR prompt first, then the page
/// image encoded as a base64 data URL. Requires no authentication because the
/// server is expected to be network-local.
#[cfg(feature = "vlm-ocr")]
pub struct VllmServerClient {
    client: DefaultClient,
    model: String,
}

#[cfg(feature = "vlm-ocr")]
impl VllmServerClient {
    /// Construct a client pointing at `base_url` and using `model` for every request.
    ///
    /// `base_url` is the server root, e.g. `"http://127.0.0.1:8000"` — the
    /// `/v1/chat/completions` path is appended automatically by `liter-llm`.
    ///
    /// # Errors
    ///
    /// Returns an error if `liter-llm` cannot construct the underlying HTTP client
    /// (e.g. TLS initialisation failure on non-Windows platforms).
    pub fn new(base_url: &str, model: impl Into<String>) -> crate::error::Result<Self> {
        let config = ClientConfigBuilder::new("")
            .base_url(base_url)
            .load_env(false)
            .max_retries(1) // 1 retry for transient network hiccup; no exponential backoff at local endpoints
            .build();
        let client = DefaultClient::new(config, None)
            .map_err(|e| crate::error::Error::Extract {
                doc: "vlm-init".into(),
                col: "client".into(),
                source: Box::new(e),
            })?;
        Ok(Self {
            client,
            model: model.into(),
        })
    }
}

#[cfg(feature = "vlm-ocr")]
#[async_trait]
impl VlmOcrClient for VllmServerClient {
    async fn transcribe(
        &self,
        image: &PageImage,
        _hint: &str,
    ) -> crate::error::Result<Transcription> {
        let data_url = encode_data_url(&image.bytes, Some(image.mime));

        let parts = vec![
            ContentPart::Text {
                text: VLM_OCR_PROMPT_FR.to_owned(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: data_url,
                    detail: None,
                },
            },
        ];

        let req = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![Message::User(UserMessage {
                content: UserContent::Parts(parts),
                name: None,
            })],
            ..Default::default()
        };

        let resp = self
            .client
            .chat(req)
            .await
            .map_err(|e| crate::error::Error::Extract {
                doc: image.doc_id.clone(),
                col: format!("page:{}", image.page),
                source: Box::new(e),
            })?;

        let first_choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| crate::error::Error::Extract {
                doc: image.doc_id.clone(),
                col: format!("page:{}", image.page),
                source: format!(
                    "VLM server returned no choices (doc: {}, page: {})",
                    image.doc_id, image.page
                )
                .into(),
            })?;
        let content = first_choice
            .message
            .content
            .ok_or_else(|| crate::error::Error::Extract {
                doc: image.doc_id.clone(),
                col: format!("page:{}", image.page),
                source: format!(
                    "VLM response has no content (doc: {}, page: {})",
                    image.doc_id, image.page
                )
                .into(),
            })?;
        let text = content
            .as_text()
            .ok_or_else(|| crate::error::Error::Extract {
                doc: image.doc_id.clone(),
                col: format!("page:{}", image.page),
                source: format!(
                    "VLM response content is not text (doc: {}, page: {})",
                    image.doc_id, image.page
                )
                .into(),
            })?
            .to_string();

        let confidence = if text.is_empty() {
            0.0
        } else {
            vlm_quality_score(&text)
        };

        tracing::debug!(
            doc_id = %image.doc_id,
            page = image.page,
            confidence,
            "vlm transcription complete"
        );

        Ok(Transcription { text, confidence })
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

// Compile-time assertion: VllmServerClient must be Send + Sync because
// VlmOcrClient requires it (async_trait + shared state across await points).
#[cfg(feature = "vlm-ocr")]
fn _assert_vlm_server_send_sync()
where
    VllmServerClient: Send + Sync,
{
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "vlm-ocr")]
    use super::*;

    #[cfg(feature = "vlm-ocr")]
    #[tokio::test]
    #[ignore = "requires co-located vLLM serving lightonai/LightOnOCR-2-1B at :8000"]
    async fn vllm_server_client_transcribes_fixture() {
        let client = VllmServerClient::new(
            "http://127.0.0.1:8000",
            "lightonai/LightOnOCR-2-1B",
        )
        .expect("client init");

        // Load a real fixture PNG from crates/anno-rag/tests/fixtures/vlm_ocr_eval/printed/
        // and assert transcription.confidence > 0.5
        todo!("load fixture PNG and call transcribe")
    }
}
