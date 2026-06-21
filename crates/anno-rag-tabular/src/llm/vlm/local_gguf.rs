//! llama-server GGUF backend for vision-OCR.
//!
//! `LocalVlmClient` wraps [`VllmServerClient`] — both speak the same
//! OpenAI-compatible `/v1/chat/completions` HTTP protocol.  The only difference
//! is the default port: llama.cpp's `llama-server` (or `server`) typically
//! listens on `:8080` rather than the vLLM default of `:8000`.
//!
//! Users point `vlm_local_url` at the llama-server process; the VlmOcrClient
//! contract is identical to the vLLM path.

#[cfg(feature = "vlm-ocr")]
use async_trait::async_trait;

#[cfg(feature = "vlm-ocr")]
use super::vllm_server::VllmServerClient;
#[cfg(feature = "vlm-ocr")]
use super::{PageImage, Transcription, VlmOcrClient};

/// VLM OCR client backed by a local llama.cpp `llama-server` process.
///
/// Delegates entirely to [`VllmServerClient`]; the only behavioural difference
/// is the default base URL (`127.0.0.1:8080` vs `:8000`).
#[cfg(feature = "vlm-ocr")]
pub struct LocalVlmClient {
    inner: VllmServerClient,
}

#[cfg(feature = "vlm-ocr")]
impl LocalVlmClient {
    /// Construct a client pointing at `base_url` with the given `model` name.
    ///
    /// The typical call for a default llama-server install is:
    /// ```ignore
    /// LocalVlmClient::new("http://127.0.0.1:8080", "lightonai/LightOnOCR-2-1B")
    /// ```
    ///
    /// # Errors
    ///
    /// Propagates errors from [`VllmServerClient::new`].
    pub fn new(base_url: &str, model: impl Into<String>) -> crate::error::Result<Self> {
        Ok(Self {
            inner: VllmServerClient::new(base_url, model)?,
        })
    }
}

#[cfg(feature = "vlm-ocr")]
#[async_trait]
impl VlmOcrClient for LocalVlmClient {
    async fn transcribe(
        &self,
        image: &PageImage,
        hint: &str,
    ) -> crate::error::Result<Transcription> {
        self.inner.transcribe(image, hint).await
    }

    fn model_id(&self) -> &str {
        self.inner.model_id()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "vlm-ocr")]
    use super::*;

    #[cfg(feature = "vlm-ocr")]
    #[tokio::test]
    #[ignore = "requires co-located llama-server serving lightonai/LightOnOCR-2-1B at :8080"]
    async fn local_vlm_client_transcribes_fixture() {
        let client = LocalVlmClient::new("http://127.0.0.1:8080", "lightonai/LightOnOCR-2-1B")
            .expect("client init");

        // Load a real fixture PNG from crates/anno-rag/tests/fixtures/vlm_ocr_eval/printed/
        // and assert transcription.confidence > 0.5
        todo!("load fixture PNG and call transcribe")
    }
}
