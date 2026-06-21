//! Routing layer — selects VLM backend from runtime config.
//! No third-party-hosted backend: all inference stays in the customer's trust boundary
//! (Spec B §4.3). Use `vlm_backend = "off"` to disable VLM without recompiling.

#![cfg(feature = "vlm-ocr")]

use async_trait::async_trait;

use super::{PageImage, Transcription, VlmOcrClient};

/// Reject any VLM URL that does not resolve to a local loopback address.
///
/// VLM backends receive raw page images from client documents. Allowing arbitrary
/// URLs would route sensitive content outside the customer trust boundary (Spec B §4.3).
fn guard_local_url(url: &str) -> crate::error::Result<()> {
    let is_local = url.contains("127.0.0.1")
        || url.contains("localhost")
        || url.contains("[::1]")
        || url.contains("0.0.0.0");
    if !is_local {
        return Err(crate::error::Error::Extract {
            doc: "vlm-routing".into(),
            col: "url".into(),
            source: format!(
                "VLM URL {:?} is not a loopback address; only localhost/127.0.0.1/[::1] \
                 are permitted to keep page images within the trust boundary",
                url
            )
            .into(),
        });
    }
    Ok(())
}

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
            Some("vllm") | None => {
                let url = cfg
                    .vlm_vllm_url
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:8000");
                guard_local_url(url)?;
                Box::new(super::vllm_server::VllmServerClient::new(
                    url,
                    "lightonai/LightOnOCR-2-1B",
                )?)
            }
            Some("local") => {
                let url = cfg
                    .vlm_local_url
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:8080");
                guard_local_url(url)?;
                Box::new(super::local_gguf::LocalVlmClient::new(
                    url,
                    "LightOnOCR-1B-1025",
                )?)
            }
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
    ) -> anno_rag::error::Result<Transcription> {
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
