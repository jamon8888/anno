//! Routing layer — selects VLM backend from runtime config.
//! No third-party-hosted backend: all inference stays in the customer's trust boundary
//! (Spec B §4.3). Use `vlm_backend = "off"` to disable VLM without recompiling.

#![cfg(feature = "vlm-ocr")]

use async_trait::async_trait;

use super::{PageImage, Transcription, VlmOcrClient};

/// Reject any VLM URL whose host is not a loopback address.
///
/// VLM backends receive raw page images from client documents. Allowing arbitrary
/// URLs would route sensitive content outside the customer trust boundary (Spec B §4.3).
///
/// We extract the host from the URL authority (scheme://[userinfo@]host[:port]/…) so
/// that substring tricks like `http://evil.com?x=127.0.0.1` or
/// `http://127.0.0.1.evil.com` cannot bypass the check.
fn guard_local_url(url: &str) -> crate::error::Result<()> {
    let host = vlm_url_host(url);
    let is_local = matches!(
        host.as_deref(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]")
    );
    if !is_local {
        return Err(crate::error::Error::Extract {
            doc: "vlm-routing".into(),
            col: "url".into(),
            source: format!(
                "VLM URL {:?} host {:?} is not a loopback address; \
                 only localhost/127.0.0.1/[::1] are permitted to keep \
                 page images within the trust boundary",
                url,
                host.as_deref().unwrap_or("<unparseable>")
            )
            .into(),
        });
    }
    Ok(())
}

/// Extract the lowercased host from a URL string without pulling in a URL-parsing crate.
///
/// Handles: `http://host/path`, `http://user:pass@host:port/path`, `http://[::1]:8080/`.
/// Returns `None` when the URL has no `://` or the authority is empty.
fn vlm_url_host(url: &str) -> Option<String> {
    // Strip scheme (everything up to and including "://")
    let after_scheme = url.split_once("://")?.1;
    // Strip path/query/fragment — authority ends at the first '/'
    let authority = after_scheme.split('/').next()?;
    // Strip userinfo ("user:pass@")
    let host_and_port = authority.split('@').last()?;
    if host_and_port.is_empty() {
        return None;
    }
    // IPv6 literal: "[::1]" or "[::1]:8080"
    if let Some(rest) = host_and_port.strip_prefix('[') {
        let end = rest.find(']')?;
        return Some(format!("[{}]", &rest[..end]));
    }
    // IPv4 or hostname: strip optional port and any fragment (e.g. "host:8000#frag")
    let host = host_and_port
        .split('#')
        .next()
        .unwrap_or(host_and_port)
        .split(':')
        .next()
        .unwrap_or(host_and_port);
    Some(host.to_ascii_lowercase())
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

#[cfg(test)]
mod tests {
    use super::vlm_url_host;

    #[test]
    fn host_extraction_covers_all_loopback_forms() {
        assert_eq!(
            vlm_url_host("http://127.0.0.1:8000").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            vlm_url_host("http://localhost:8000/v1").as_deref(),
            Some("localhost")
        );
        assert_eq!(vlm_url_host("http://[::1]:8080/").as_deref(), Some("[::1]"));
        assert_eq!(
            vlm_url_host("http://127.0.0.1").as_deref(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn host_extraction_rejects_bypass_attempts() {
        // query-string trick: "127.0.0.1" appears but host is evil.com
        assert_ne!(
            vlm_url_host("http://evil.com/proxy?host=127.0.0.1").as_deref(),
            Some("127.0.0.1")
        );
        // subdomain trick: "127.0.0.1" appears as a subdomain
        assert_ne!(
            vlm_url_host("http://127.0.0.1.evil.com").as_deref(),
            Some("127.0.0.1")
        );
        // fragment trick
        assert_ne!(
            vlm_url_host("http://evil.com:8000#localhost").as_deref(),
            Some("localhost")
        );
        // 0.0.0.0 is NOT a loopback address — must not pass guard
        assert_eq!(
            vlm_url_host("http://0.0.0.0:8000").as_deref(),
            Some("0.0.0.0"),
            "0.0.0.0 must parse as itself so guard_local_url can reject it"
        );
    }
}
