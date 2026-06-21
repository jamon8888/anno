//! Routing layer — selects VLM backend from runtime config.
//! No third-party-hosted backend: all inference stays in the customer's trust boundary
//! (Spec B §4.3). Use `vlm_backend = "off"` to disable VLM without recompiling.

#![cfg(feature = "vlm-ocr")]

use async_trait::async_trait;

use super::{PageImage, Transcription, VlmOcrClient};

/// Selects and delegates to a VLM backend based on `AnnoRagConfig.vlm_backend`.
///
/// Constructed via [`RoutingVlmClient::from_config`]; returns `None` when the
/// backend is `"off"` so the caller can fall through to Tesseract without
/// holding a live client.
pub struct RoutingVlmClient {
    backend: Box<dyn VlmOcrClient>,
}

impl RoutingVlmClient {
    /// Build a routing client from the runtime config.
    ///
    /// Returns `Ok(None)` when `vlm_backend = "off"` — callers should fall
    /// through to Tesseract in that case.
    ///
    /// # Errors
    ///
    /// Returns an error if `vlm_backend` is an unrecognised value, or if the
    /// selected backend fails to initialise (e.g. HTTP client construction).
    pub fn from_config(cfg: &anno_rag::AnnoRagConfig) -> crate::error::Result<Option<Self>> {
        let backend: Box<dyn VlmOcrClient> = match cfg.vlm_backend.as_deref() {
            Some("vllm") | None => Box::new(super::vllm_server::VllmServerClient::new(
                cfg.vlm_vllm_url
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:8000"),
                "lightonai/LightOnOCR-2-1B",
            )?),
            Some("local") => Box::new(super::local_gguf::LocalVlmClient::new(
                cfg.vlm_local_url
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:8080"),
                "LightOnOCR-1B-1025",
            )?),
            Some("off") => return Ok(None),
            Some(other) => {
                return Err(crate::error::Error::Extract {
                    doc: "vlm-routing".into(),
                    col: "vlm_backend".into(),
                    source: format!(
                        "unsupported vlm_backend value {:?}; expected \"vllm\", \"local\", or \"off\"",
                        other
                    )
                    .into(),
                });
            }
        };
        Ok(Some(Self { backend }))
    }
}

#[async_trait]
impl VlmOcrClient for RoutingVlmClient {
    async fn transcribe(
        &self,
        image: &PageImage,
        hint: &str,
    ) -> crate::error::Result<Transcription> {
        self.backend.transcribe(image, hint).await
    }

    fn model_id(&self) -> &str {
        self.backend.model_id()
    }
}

// Compile-time assertion: RoutingVlmClient must be Send + Sync.
fn _assert_routing_send_sync()
where
    RoutingVlmClient: Send + Sync,
{
}
