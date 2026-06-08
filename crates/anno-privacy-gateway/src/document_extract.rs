//! Local document extraction for uploaded files and base64 document blocks.

use crate::{Error, Result};
use std::path::PathBuf;

/// Extracted document text and basic source metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    /// Original filename from the upload or document block.
    pub filename: String,
    /// Content type supplied by the caller.
    pub detected_content_type: String,
    /// Trimmed extracted text.
    pub text: String,
}

/// Extract local document text without sending file bytes to any provider.
pub async fn extract_uploaded_document(
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<ExtractedDocument> {
    let text = if is_text_like(filename, content_type) {
        String::from_utf8(bytes)
            .map_err(|e| Error::Privacy(format!("uploaded text is not valid UTF-8: {e}")))?
    } else {
        extract_with_kreuzberg(filename, bytes).await?
    };
    let normalized = text.trim().to_string();
    if normalized.is_empty() {
        return Err(Error::UnsupportedFeature(
            "uploaded document extracted empty text".to_string(),
        ));
    }
    Ok(ExtractedDocument {
        filename: filename.to_string(),
        detected_content_type: content_type.to_string(),
        text: normalized,
    })
}

fn is_text_like(filename: &str, content_type: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    content_type.starts_with("text/")
        || lower.ends_with(".txt")
        || lower.ends_with(".md")
        || lower.ends_with(".csv")
        || lower.ends_with(".json")
}

async fn extract_with_kreuzberg(filename: &str, bytes: Vec<u8>) -> Result<String> {
    let tmp_dir = tempfile::tempdir()
        .map_err(|e| Error::Privacy(format!("create temp extraction dir: {e}")))?;
    let path: PathBuf = tmp_dir.path().join(safe_temp_filename(filename));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| Error::Privacy(format!("write temp uploaded document: {e}")))?;

    let config = kreuzberg::core::config::ExtractionConfig {
        disable_ocr: true,
        ..Default::default()
    };
    let extracted = kreuzberg::extract_file(&path, None, &config)
        .await
        .map_err(|e| Error::Privacy(format!("extract uploaded document: {e}")))?;
    Ok(extracted.content)
}

fn safe_temp_filename(filename: &str) -> String {
    let sanitized: String = filename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "upload.bin".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extracts_utf8_text_plain() {
        let doc =
            extract_uploaded_document("notes.txt", "text/plain", b"Bonjour Marie Dupont".to_vec())
                .await
                .expect("extract");

        assert_eq!(doc.text, "Bonjour Marie Dupont");
        assert_eq!(doc.detected_content_type, "text/plain");
    }

    #[tokio::test]
    async fn rejects_empty_document_text() {
        let err = extract_uploaded_document("empty.txt", "text/plain", b"   ".to_vec())
            .await
            .expect_err("empty rejected");

        assert!(err.to_string().contains("empty"));
    }
}
