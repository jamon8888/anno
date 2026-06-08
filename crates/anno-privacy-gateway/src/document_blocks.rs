//! Expand Anthropic document blocks into gateway-safe text blocks.

use base64::Engine;
use serde_json::{json, Value};

use crate::privacy_mode::PrivacyMode;
use crate::{Error, PrivacyEngine, Result};

/// Counts produced while expanding document blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentExpansionReport {
    /// Number of document blocks replaced by text blocks.
    pub document_count: usize,
    /// Number of entities pseudonymized from inline base64 documents.
    pub entity_count: usize,
}

struct DocumentSourceText {
    text: String,
    entities: usize,
}

/// Replace supported document blocks with local text blocks before provider routing.
pub async fn expand_document_blocks(
    body: &mut Value,
    registry: &crate::file_registry::FileRegistry,
    privacy_mode: PrivacyMode,
    max_document_bytes: usize,
    privacy: &mut PrivacyEngine,
) -> Result<DocumentExpansionReport> {
    let mut report = DocumentExpansionReport::default();
    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            expand_message_content(
                message,
                registry,
                privacy_mode,
                max_document_bytes,
                privacy,
                &mut report,
            )
            .await?;
        }
    }
    Ok(report)
}

async fn expand_message_content(
    message: &mut Value,
    registry: &crate::file_registry::FileRegistry,
    privacy_mode: PrivacyMode,
    max_document_bytes: usize,
    privacy: &mut PrivacyEngine,
    report: &mut DocumentExpansionReport,
) -> Result<()> {
    let Some(content) = message.get_mut("content") else {
        return Ok(());
    };
    if let Some(text) = content.as_str() {
        *content = Value::Array(vec![json!({"type": "text", "text": text})]);
    }
    let Some(blocks) = content.as_array_mut() else {
        return Ok(());
    };

    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("document") {
            continue;
        }
        let title = block
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("document")
            .to_string();
        let source = block.get("source").ok_or_else(|| {
            Error::UnsupportedFeature("document block missing source".to_string())
        })?;
        let source_text = document_text_from_source(
            source,
            &title,
            registry,
            privacy_mode,
            max_document_bytes,
            privacy,
        )
        .await?;
        *block = json!({
            "type": "text",
            "text": format!("[Document: {title}]\n{}", source_text.text)
        });
        report.document_count += 1;
        report.entity_count += source_text.entities;
    }
    Ok(())
}

async fn document_text_from_source(
    source: &Value,
    title: &str,
    registry: &crate::file_registry::FileRegistry,
    privacy_mode: PrivacyMode,
    max_document_bytes: usize,
    privacy: &mut PrivacyEngine,
) -> Result<DocumentSourceText> {
    match source.get("type").and_then(Value::as_str) {
        Some("file") => {
            let id = source
                .get("file_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    Error::UnsupportedFeature("file document source missing file_id".to_string())
                })?;
            let text = match privacy_mode {
                PrivacyMode::Pseudonymized => registry.read_pseudonymized_text(id).await?,
                PrivacyMode::CleartextDpa | PrivacyMode::CleartextLocal => {
                    registry.read_cleartext_text(id).await?.ok_or_else(|| {
                        Error::UnsupportedFeature(
                            "cleartext file derivative is not retained".to_string(),
                        )
                    })?
                }
            };
            Ok(DocumentSourceText { text, entities: 0 })
        }
        Some("base64") => {
            let media_type = source
                .get("media_type")
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream");
            let encoded = source.get("data").and_then(Value::as_str).ok_or_else(|| {
                Error::UnsupportedFeature("base64 document source missing data".to_string())
            })?;
            reject_oversized_encoded_document(encoded, max_document_bytes)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| Error::Privacy(format!("decode base64 document: {e}")))?;
            if bytes.len() > max_document_bytes {
                return Err(Error::UnsupportedFeature(format!(
                    "base64 document exceeds {max_document_bytes} bytes"
                )));
            }
            let extracted =
                crate::document_extract::extract_uploaded_document(title, media_type, bytes)
                    .await?;
            if privacy_mode == PrivacyMode::Pseudonymized {
                let pseudonymized = privacy.pseudonymize_plain_text(&extracted.text)?;
                Ok(DocumentSourceText {
                    text: pseudonymized.text,
                    entities: pseudonymized.entities,
                })
            } else {
                Ok(DocumentSourceText {
                    text: extracted.text,
                    entities: 0,
                })
            }
        }
        Some("url") => Err(Error::UnsupportedFeature(
            "url document sources are not fetched by anno privacy gateway".to_string(),
        )),
        Some(other) => Err(Error::UnsupportedFeature(format!(
            "unsupported document source type: {other}"
        ))),
        None => Err(Error::UnsupportedFeature(
            "document source must include type".to_string(),
        )),
    }
}

fn reject_oversized_encoded_document(encoded: &str, max_document_bytes: usize) -> Result<()> {
    let max_encoded_len = max_document_bytes
        .saturating_add(2)
        .saturating_div(3)
        .saturating_mul(4)
        .saturating_add(4);
    if encoded.len() > max_encoded_len {
        return Err(Error::UnsupportedFeature(format!(
            "base64 document exceeds {max_document_bytes} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_mode::PrivacyMode;
    use base64::Engine;
    use serde_json::json;

    #[tokio::test]
    async fn rejects_url_document_sources() {
        let registry = test_registry().await;
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "url", "url": "https://example.com/a.pdf"}
                }]
            }]
        });
        let err = expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            usize::MAX,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect_err("url rejected");

        assert!(err.to_string().contains("url document sources"));
    }

    #[tokio::test]
    async fn expands_base64_document_to_text_block() {
        let registry = test_registry().await;
        let data = base64::engine::general_purpose::STANDARD.encode("Bonjour Marie Dupont");
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "base64", "media_type": "text/plain", "data": data},
                    "title": "notes.txt"
                }]
            }]
        });
        expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            usize::MAX,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect("expand");

        let text = body["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("PERSON_"));
        assert!(!text.contains("Marie Dupont"));
    }

    #[tokio::test]
    async fn rejects_oversized_base64_document() {
        let registry = test_registry().await;
        let data = base64::engine::general_purpose::STANDARD.encode("Bonjour Marie Dupont");
        let mut body = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "base64", "media_type": "text/plain", "data": data},
                    "title": "notes.txt"
                }]
            }]
        });
        let err = expand_document_blocks(
            &mut body,
            &registry,
            PrivacyMode::Pseudonymized,
            8,
            &mut crate::PrivacyEngine::from_config(&crate::GatewayConfig::default()).unwrap(),
        )
        .await
        .expect_err("oversized rejected");

        assert!(err.to_string().contains("exceeds"));
    }

    async fn test_registry() -> crate::file_registry::FileRegistry {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.keep();
        crate::file_registry::FileRegistry::new(crate::file_registry::FileRegistryConfig {
            root,
            retain_raw: false,
            retain_cleartext: true,
        })
    }
}
